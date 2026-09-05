//! launch_config.rs — configuración previa al lanzamiento (#311).
//!
//! Lo que el jugador elige ANTES de arrancar —perfil, qué se sustituye, volumen
//! por bus, shaders— y cómo eso llega al runtime.
//!
//! El riesgo que anota la issue es «desalinear las capacidades ofrecidas con
//! las reales del pack», y eso decide la forma del módulo: las opciones no se
//! escriben a mano, se **derivan** de lo que el pack declara (`[systems]` y
//! `[[profile]]`, #294/#293). Una lista fija de casillas ofrecería «Música» en
//! un pack que no trae audio, y el jugador la marcaría esperando algo.
//!
//! La configuración se guarda por **combinación ROM + pack**: los subsistemas
//! no significan lo mismo en dos packs distintos, y arrastrar la de uno al otro
//! haría que el segundo arrancara a medias sin que nadie lo haya pedido. Es la
//! misma clave que usa el panel in-game (#299) — a propósito: son la misma
//! preferencia vista desde dos lados.

use std::path::{Path, PathBuf};

/// Lo que este pack PUEDE ofrecer. Sale del pack, no de una lista fija.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PackCapabilities {
    /// Perfiles ofrecibles, `(id, nombre)`. Siempre trae al menos «original».
    pub profiles: Vec<(String, String)>,
    /// Índice del perfil recomendado por el autor (el `default` del pack).
    pub default_profile: usize,
    /// Máscara de subsistemas que el pack trae (bit i = `SUBSYSTEMS[i]`).
    pub systems: u32,
    /// ¿El pack declara `[systems]`? Ausente ≠ vacío (#294): un pack viejo no
    /// dice nada, y ahí lo honesto es ofrecer todo en vez de nada.
    pub declares_systems: bool,
    /// #296: perfil de SALIDA que el pack RECOMIENDA. Vacío = no recomienda.
    pub output_recommended: String,
}

impl PackCapabilities {
    /// Los subsistemas que se pueden OFRECER. Un pack legacy no declara nada, y
    /// ahí ofrecer una lista vacía dejaría al jugador sin controles sobre un
    /// pack que sí sustituye cosas.
    pub fn offerable_systems(&self) -> u32 {
        if self.declares_systems {
            self.systems
        } else {
            0xFF
        }
    }
}

/// Lo que el jugador eligió.
#[derive(Debug, Clone, PartialEq)]
pub struct LaunchConfig {
    pub profile: String,
    /// `None` = no tocó nada: manda el perfil. Un 0 no puede distinguir «apagó
    /// todo» de «no eligió», y esa diferencia decide si el juego arranca
    /// remasterizado.
    pub subsystems: Option<u32>,
    /// Bits de bus silenciado (0 sin clasificar · 1 música · 2 efectos · 3 voces).
    pub muted_buses: u32,
    pub shaders: bool,
    /// #296: perfil de SALIDA elegido. Vacío = no eligió, y ahí manda lo que el
    /// pack recomienda. Guardar el resuelto convertiría la recomendación del
    /// pack en la elección del usuario a la primera partida, y el pack
    /// siguiente ya no podría recomendar nada.
    pub output: String,
    /// EM-7.1 (#230): «Modo suave» — le pide al core que quite el límite de
    /// sprites por línea, que es lo que produce el parpadeo del original.
    ///
    /// Es una preferencia del JUGADOR y no del pack: quien lo apaga quiere ver
    /// el juego como era, parpadeo incluido, y ninguna remasterización debería
    /// decidir eso por él. Por eso vive acá y no en `PackCapabilities`.
    ///
    /// `false` es el default a propósito: arrancar con emulación modificada
    /// haría que el primer vistazo no fuera el juego. La opción existe, no se
    /// impone.
    ///
    /// Que FUNCIONE depende del core (BYOC): un core que no declare la opción
    /// simplemente no la pregunta nunca, y esto no hace nada. No es un error —
    /// es lo que significa traer tu propio core.
    ///
    /// Se llama `no_flicker` y no `smooth` porque «smooth» ya es un perfil de
    /// SALIDA (#296, «Suavizado», que es un filtro de escalado). Dos cosas
    /// distintas con el mismo nombre en el mismo archivo es cómo se termina
    /// aplicando una donde va la otra.
    pub no_flicker: bool,
    /// EM-7.4 (#230): parche IPS/BPS del PROPIO usuario — una fan-translation o
    /// un romhack. Ruta absoluta; vacío = ninguno.
    ///
    /// BYOR-safe: el parche describe cómo transformar una ROM que el usuario ya
    /// tiene, no la lleva adentro. AYTHER no redistribuye nada, y el archivo del
    /// disco no se toca — se aplica al buffer que recibe el core.
    ///
    /// Vive en la config de lanzamiento y no en la entrada de biblioteca porque
    /// es una elección de ESTA partida: el mismo juego se puede jugar con la
    /// traducción o sin ella, y la biblioteca no debería tener dos entradas
    /// para la misma ROM.
    pub patch: String,
}

impl Default for LaunchConfig {
    fn default() -> Self {
        LaunchConfig {
            profile: String::new(),
            subsystems: None,
            muted_buses: 0,
            shaders: true,
            output: String::new(),
            no_flicker: false,
            patch: String::new(),
        }
    }
}

/// El id del perfil que apaga TODO lo del pack.
///
/// Es una constante y no una cadena suelta porque tres lugares la comparan —el
/// toggle, la fila de la configuración previa y el manifest— y una de las tres
/// escrita distinta significaría «HD» donde dice «Original», en silencio.
pub const ORIGINAL_PROFILE: &str = "original";

impl LaunchConfig {
    /// #598 — ¿esta configuración va a arrancar en HD?
    ///
    /// «Original» es el perfil sin nada, y hay dos maneras de estar ahí: elegir
    /// el perfil `original`, o apagar todos los subsistemas a mano. Las dos
    /// tienen que contestar lo mismo, porque el jugador ve una sola cosa —el
    /// juego como era— y un toggle que dice «HD» sobre una pantalla sin nada
    /// remasterizado es una mentira que además no se puede corregir tocándolo.
    pub fn is_hd(&self) -> bool {
        if self.profile == ORIGINAL_PROFILE {
            return false;
        }
        !matches!(self.subsystems, Some(0))
    }

    /// #598 — pasa a Original, o vuelve a lo que el pack recomienda.
    ///
    /// Volver NO restaura «todo encendido» sino el perfil del PACK: es la misma
    /// razón por la que `recommended` existe. «Todo encendido» restauraría algo
    /// que nadie decidió, y en un pack con tres perfiles sería el equivalente a
    /// elegir por el autor.
    ///
    /// Es REVERSIBLE y no toca nada del disco (§6): sólo cambia qué perfil se
    /// escribe en el manifest de la próxima sesión. La ROM y los guardados son
    /// los mismos en las dos posiciones — el jugador puede ir y volver entre
    /// partidas sin perder nada.
    pub fn set_hd(&mut self, hd: bool, caps: &PackCapabilities) {
        if hd {
            self.profile = Self::recommended(caps).profile;
            // La elección manual de subsistemas se limpia: quedó de la posición
            // anterior y volver a HD significa «lo que el pack propone».
            self.subsystems = None;
        } else {
            self.profile = ORIGINAL_PROFILE.to_string();
            // Y la máscara en CERO, explícita. Con `None` mandaría el perfil, y
            // un pack que no declare `original` dejaría todo encendido — que es
            // exactamente lo contrario de lo que el toggle acaba de pedir.
            self.subsystems = Some(0);
        }
    }

    /// La configuración RECOMENDADA por el autor del pack: su perfil
    /// predeterminado y nada tocado a mano. Es lo que restaura el botón
    /// «recomendada» — y es del PACK, no de fábrica: volver a «todo encendido»
    /// restauraría algo que nadie decidió.
    pub fn recommended(caps: &PackCapabilities) -> Self {
        LaunchConfig {
            profile: caps
                .profiles
                .get(caps.default_profile)
                .map(|p| p.0.clone())
                .unwrap_or_default(),
            subsystems: None,
            muted_buses: 0,
            shaders: true,
            // Vacío a propósito: «recomendado» incluye dejar que el pack
            // recomiende la salida. Poner el default acá lo pisaría.
            output: String::new(),
            // EM-7.1: «recomendado» es lo del PACK, y el parpadeo no es cosa
            // del pack sino del jugador. Restaurar la recomendacion no le
            // apaga una preferencia suya... salvo que este metodo construye
            // una config nueva: el caller conserva la del jugador si quiere.
            no_flicker: false,
            // El parche es del jugador y no del pack: «restaurar lo
            // recomendado» no puede sacarle su traduccion.
            patch: String::new(),
        }
    }

    /// Recorta la elección a lo que este pack puede: un perfil que no existe se
    /// cae al recomendado y los subsistemas se intersectan con los que trae.
    ///
    /// Se aplica al CARGAR una configuración guardada, porque puede venir de
    /// otra versión del pack. Encender un subsistema ausente no lo hace
    /// aparecer — sólo dejaría la pantalla mostrando algo que no pasa.
    pub fn clamp_to(&mut self, caps: &PackCapabilities) {
        if !self.profile.is_empty() && !caps.profiles.iter().any(|p| p.0 == self.profile) {
            self.profile = Self::recommended(caps).profile;
        }
        if let Some(m) = self.subsystems {
            self.subsystems = Some(m & caps.offerable_systems());
        }
    }

    /// Los argumentos para `ayther_runtime`. Sólo se pasa lo que el jugador
    /// eligió: sin `--subsystems`, el runtime usa el perfil, y eso es distinto
    /// de pasarle la máscara del perfil (que lo dejaría fuera de todo perfil en
    /// cuanto el autor publique una versión nueva).
    pub fn to_args(&self) -> Vec<String> {
        let mut a = Vec::new();
        if !self.profile.is_empty() {
            a.push("--profile".into());
            a.push(self.profile.clone());
        }
        if let Some(m) = self.subsystems {
            a.push("--subsystems".into());
            a.push(m.to_string());
        }
        if self.muted_buses != 0 {
            a.push("--mute-buses".into());
            a.push(self.muted_buses.to_string());
        }
        if !self.shaders {
            a.push("--no-shaders".into());
        }
        if !self.output.is_empty() {
            a.push("--output".into());
            a.push(self.output.clone());
        }
        // EM-7.1: la clave es la de Genesis Plus GX. Se manda SÓLO cuando está
        // encendida: mandar `disabled` sería fijar el default del core como si
        // fuera una elección del jugador, y un core que lo tenga distinto
        // dejaría de usar el suyo.
        if self.no_flicker {
            a.push("--core-option".into());
            a.push("no_sprite_limit=enabled".into());
        }
        // EM-7.4: el parche del usuario. El runtime lo aplica en RAM; si no se
        // puede aplicar, NO abre — arrancar el juego sin parchear parece que el
        // parche no hizo nada.
        if !self.patch.is_empty() {
            a.push("--patch".into());
            a.push(self.patch.clone());
        }
        a
    }
}

/// Limpia un texto para usarlo como nombre de archivo.
fn key_sanitize(s: &str) -> String {
    let mut o: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    while o.ends_with('.') {
        o.pop();
    }
    if o.is_empty() {
        o.push_str("sin_nombre");
    }
    o
}

/// Dónde vive la configuración de esta combinación ROM + pack.
///
/// Por el NOMBRE del archivo y no por su ruta: mover la biblioteca de carpeta no
/// tiene por qué borrar las preferencias de todos los juegos.
pub fn config_path(dir: &Path, rom: &Path, pack: Option<&Path>) -> PathBuf {
    let rom_key = rom
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut key = key_sanitize(&rom_key);
    if let Some(p) = pack
        && let Some(n) = p.file_name()
    {
        key.push_str("__");
        key.push_str(&key_sanitize(&n.to_string_lossy()));
    }
    dir.join(format!("{key}.toml"))
}

/// Si la pantalla previa (#311) tiene que abrirse para este lanzamiento.
///
/// #654 — la pantalla es de la PRIMERA vez y no de cada vez: «Jugar» sobre un
/// juego que ya se configuró arranca con lo guardado, y quien quiera cambiarlo
/// usa «Configurar». Los dos entran por el mismo `launch_entry`, así que la
/// diferencia la hace `forzada`.
///
/// Está acá arriba, y no escrito en el `if` del callback, porque las dos
/// condiciones se leen igual de bien al revés y equivocarse no rompe nada
/// visible: `!ya_configurado` a secas deja al botón «Configurar» como el único
/// que ya no puede abrir la configuración, y eso no falla — simplemente no
/// hace nada cuando lo apretás.
pub fn open_prelaunch(already_configured: bool, forced: bool) -> bool {
    forced || !already_configured
}

pub fn load(file: &Path) -> LaunchConfig {
    let mut c = LaunchConfig::default();
    let Ok(text) = std::fs::read_to_string(file) else {
        return c;
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let k = k.trim();
        let v = v.trim().trim_matches('"');
        match k {
            "profile" => c.profile = v.to_string(),
            "subsystems" => c.subsystems = v.parse::<u32>().ok(),
            "muted_buses" => c.muted_buses = v.parse::<u32>().unwrap_or(0),
            "shaders" => c.shaders = v == "true",
            "output" => c.output = v.to_string(),
            "no_flicker" => c.no_flicker = v == "true", // EM-7.1
            "patch" => c.patch = v.to_string(),         // EM-7.4
            _ => {}
        }
    }
    c
}

pub fn save(file: &Path, c: &LaunchConfig) -> bool {
    if let Some(parent) = file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut s = String::from(
        "# Configuración previa de AYTHER para esta combinación de ROM y pack (#311).\n\n",
    );
    s.push_str(&format!("profile = \"{}\"\n", c.profile));
    // `subsystems` sólo se escribe si el jugador tocó algo: su AUSENCIA es lo
    // que dice «manda el perfil», y un 0 no puede decir eso.
    if let Some(m) = c.subsystems {
        s.push_str(&format!("subsystems = {m}\n"));
    }
    s.push_str(&format!("muted_buses = {}\n", c.muted_buses));
    s.push_str(&format!("shaders = {}\n", c.shaders));
    // #296: se guarda VACÍO si no eligió. Guardar el perfil resuelto
    // convertiría la recomendación del pack en la elección del usuario a la
    // primera partida, y el pack siguiente ya no podría recomendar nada.
    s.push_str(&format!("output = \"{}\"\n", c.output));
    // EM-7.1: la preferencia del jugador sobre el parpadeo. Persiste como
    // las demas — quien la eligio una vez no tiene que volver a elegirla.
    s.push_str(&format!("no_flicker = {}\n", c.no_flicker));
    s.push_str(&format!("patch = \"{}\"\n", c.patch));
    std::fs::write(file, s).is_ok()
}

// ---------------------------------------------------------------------------
// La pantalla, como MENU DE FILAS (#311)
// ---------------------------------------------------------------------------
//
// No es un formulario con casillas y sliders sueltos: es una lista donde
// arriba/abajo mueve y izquierda/derecha cambia el valor de la fila.
//
// La razon es el mando. En un formulario, cada control toma botones distintos
// —una casilla se marca con A, un slider se mueve con el stick, un desplegable
// se abre y se cierra— y el jugador tiene que saber cual es cual antes de
// tocarlo. Una lista uniforme se maneja sin mirar, que es como se usa un
// launcher desde el sillon. Es tambien lo que hace que «funciona
// completamente con gamepad» sea comprobable: son cuatro direcciones y un
// boton, no una superficie de controles.

#[derive(Debug, Clone, PartialEq)]
pub enum RowKind {
    Profile,
    /// Un grupo VISUAL, con los subsistemas que enciende. Agrupados como el
    /// jugador los nombra —Personajes, Escenario, Interfaz— porque nadie sabe
    /// que es un metasprite ni por que «tiles» y «planes» son dos cosas.
    Systems {
        mask: u32,
    },
    /// Un bus de audio (indice de AudioBus).
    Bus(u8),
    Shaders,
    /// #296: perfil de SALIDA — cómo se ve en ESTA pantalla. No confundir con
    /// `Profile`, que es qué se sustituye: uno lo decide el autor del pack y el
    /// otro quien mira el monitor.
    Output,
    /// EM-7.1 (#230): «Modo suave» — sin límite de sprites por línea, que es
    /// lo que produce el parpadeo del original. No es un shader ni un perfil:
    /// es una opción del CORE, y por eso puede no existir según cuál se use.
    NoFlicker,
    /// Volver a lo que el autor del pack recomienda.
    Restore,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfigRow {
    pub label: String,
    pub value: String,
    /// false = el pack no lo trae. La fila se MUESTRA igual, apagada: una que
    /// desaparece se lee como «esta version no lo tiene»; una gris dice «este
    /// pack no lo trae», que es la verdad.
    pub available: bool,
    pub kind: RowKind,
}

const VISUAL_GROUPS: [(&str, u32); 3] = [
    ("Personajes", 0b0000_0011), // sprites + metasprites
    ("Escenario", 0b0000_1100),  // tiles + planes
    ("Interfaz", 0b0001_0000),   // ui
];
const BUS_NAMES: [&str; 4] = ["", "Musica", "Efectos", "Voces"];

/// #296: los perfiles de SALIDA, en el mismo orden que el Engine. Es una lista
/// paralela y por eso está acá el comentario: si el Engine agrega uno, esta se
/// queda corta y el jugador no lo puede elegir desde Play. La alternativa —
/// pedírsela al Engine por FFI— cruzaría el límite Rust/C++ para leer cinco
/// cadenas constantes, que es más maquinaria de la que el problema pide.
const OUTPUT_IDS: [(&str, &str); 6] = [
    ("lcd", "LCD nativo"),
    ("crt", "CRT simulado"),
    ("pixel", "Pixel-perfect"),
    ("smooth", "Suavizado"),
    ("cinema", "Cinematografica"),
    // #230 EM-7.2: el CRT mas el sangrado de croma del cable compuesto, que es
    // como se vieron estos juegos. Va aparte de "crt" porque son dos cosas: uno
    // es el tubo y el otro el cable.
    ("ntsc", "NTSC compuesto"),
];

pub fn output_name(id: &str) -> Option<&'static str> {
    OUTPUT_IDS.iter().find(|(i, _)| *i == id).map(|(_, n)| *n)
}

/// La configuración con la que ARRANCA esta partida: lo que el jugador eligió
/// para ESTE juego y, si no eligió nada, el perfil de salida GLOBAL de Ajustes
/// (#599).
///
/// Se resuelve al lanzar y **no se guarda**. Escribir el global en la config
/// del juego lo convertiría en una elección por juego, y el próximo cambio en
/// Ajustes ya no alcanzaría a ese juego — es la misma razón por la que tampoco
/// se guarda lo que recomienda el pack.
///
/// El global le gana al pack y pierde contra el juego, que es la precedencia de
/// `settings.rs`: el pack RECOMIENDA, no impone. Como el Runtime resuelve
/// «lo que eligió el usuario > lo que recomienda el pack > el default»,
/// mandarle el global por `--output` es exactamente eso.
///
/// Sin ninguno de los dos no se manda nada, y ahí sí manda el pack.
pub fn with_global_output(cfg: &LaunchConfig, global: &str) -> LaunchConfig {
    let mut c = cfg.clone();
    if c.output.is_empty() {
        c.output = global.to_string();
    }
    c
}

/// El siguiente perfil de salida GLOBAL (#599, Ajustes).
///
/// El ciclo incluye **«sin elegir»** —la cadena vacía—, que la fila de la
/// pantalla del juego no tiene: ahí ya hay un pack que recomienda y elegir es
/// justamente pisarlo. En lo global, en cambio, «que lo decida el pack» es una
/// opción legítima y la de fábrica; sin ella el jugador que probó un perfil una
/// vez no podría volver nunca a la recomendación del pack.
pub fn cycle_global_output(current: &str, d: i32) -> String {
    let n = OUTPUT_IDS.len() as i32 + 1; // +1 = el vacío
    // El vacío es la posición 0 y los perfiles van corridos uno.
    let i = OUTPUT_IDS
        .iter()
        .position(|(id, _)| *id == current)
        .map(|p| p as i32 + 1)
        .unwrap_or(0);
    let d = if d == 0 { 1 } else { d };
    let j = ((i + d) % n + n) % n;
    if j == 0 {
        String::new()
    } else {
        OUTPUT_IDS[(j - 1) as usize].0.to_string()
    }
}

/// Qué mostrar en la fila: lo elegido, o lo que el pack recomienda.
fn output_display(chosen: &str, pack: &str) -> String {
    if let Some(n) = output_name(chosen) {
        return n.to_string();
    }
    if let Some(n) = output_name(pack) {
        return format!("{n} (del pack)");
    }
    output_name(OUTPUT_IDS[0].0).unwrap_or("—").to_string()
}

/// Las filas de la pantalla, derivadas del pack y del estado actual.
pub fn config_rows(caps: &PackCapabilities, cfg: &LaunchConfig) -> Vec<ConfigRow> {
    let pack_output: &str = &caps.output_recommended;
    let mut rows = Vec::new();

    let prof_name = caps
        .profiles
        .iter()
        .find(|p| p.0 == cfg.profile)
        .map(|p| p.1.clone())
        .unwrap_or_else(|| {
            // Vacio = manda el predeterminado del pack. Se NOMBRA en vez de
            // mostrar un hueco: el jugador tiene que saber con que va a jugar.
            caps.profiles
                .get(caps.default_profile)
                .map(|p| p.1.clone())
                .unwrap_or_else(|| "—".into())
        });
    rows.push(ConfigRow {
        label: "Perfil".into(),
        value: prof_name,
        available: caps.profiles.len() > 1,
        kind: RowKind::Profile,
    });

    let avail = caps.offerable_systems();
    // Sin eleccion propia, se muestra lo que trae el perfil: la pantalla no
    // puede decir «apagado» sobre algo que va a arrancar encendido.
    let effective = cfg.subsystems.unwrap_or(avail);
    for (name, mask) in VISUAL_GROUPS {
        let here = avail & mask;
        rows.push(ConfigRow {
            label: name.into(),
            value: if here == 0 {
                "no incluido".into()
            } else if effective & here != 0 {
                "AYTHER".into()
            } else {
                "original".into()
            },
            available: here != 0,
            kind: RowKind::Systems { mask },
        });
    }

    for b in 1..4u8 {
        let muted = cfg.muted_buses & (1 << b) != 0;
        rows.push(ConfigRow {
            label: BUS_NAMES[b as usize].into(),
            value: if muted {
                "silenciado".into()
            } else {
                "suena".into()
            },
            available: true,
            kind: RowKind::Bus(b),
        });
    }

    // El nombre del perfil de salida, o el del pack si no eligió. Se NOMBRA
    // en vez de mostrar «(automático)»: el jugador tiene que saber con qué va a
    // ver el juego, y «automático» no le dice nada sobre eso.
    rows.push(ConfigRow {
        label: "Salida".into(),
        value: output_display(&cfg.output, pack_output),
        available: true,
        kind: RowKind::Output,
    });
    // EM-7.1: entre la salida y los shaders porque es de la misma familia —
    // cómo se ve— aunque el mecanismo sea otro. `available: true` siempre: si
    // el core no ofrece la opción, encenderla no hace nada, y eso NO se puede
    // saber hasta abrir la sesión. Mostrarla gris por las dudas mentiría en el
    // caso normal, que es el core que sí la tiene.
    rows.push(ConfigRow {
        label: "Modo suave".into(),
        value: if cfg.no_flicker {
            "si".into()
        } else {
            "no".into()
        },
        available: true,
        kind: RowKind::NoFlicker,
    });
    rows.push(ConfigRow {
        label: "Shaders".into(),
        value: if cfg.shaders {
            "si".into()
        } else {
            "no".into()
        },
        available: true,
        kind: RowKind::Shaders,
    });
    // «Valores predeterminados» NO es una fila: es un botón del pie del
    // diálogo (#627). Como fila era la única sin valor a la derecha —un hueco
    // donde las otras diez muestran con qué se va a jugar— y estaba al final de
    // una lista que hay que desplazar para llegar. `RowKind::Restore` sigue
    // existiendo porque la acción es la misma: la invoca el botón.
    rows
}

/// Izquierda/derecha (o A) sobre una fila. `delta` = -1 / +1; para las filas de
/// dos estados el signo da igual y alterna.
pub fn adjust(caps: &PackCapabilities, cfg: &mut LaunchConfig, kind: &RowKind, delta: i32) {
    match kind {
        RowKind::Profile => {
            if caps.profiles.is_empty() {
                return;
            }
            let cur = caps
                .profiles
                .iter()
                .position(|p| p.0 == cfg.profile)
                .unwrap_or(caps.default_profile);
            let n = caps.profiles.len() as i32;
            let next = ((cur as i32 + delta) % n + n) % n;
            cfg.profile = caps.profiles[next as usize].0.clone();
            // Cambiar de perfil DESCARTA los ajustes sueltos: el perfil los
            // pisa igual al aplicarse, y dejarlos haria que la pantalla
            // mostrara una mezcla que el runtime no va a reproducir.
            cfg.subsystems = None;
        }
        RowKind::Systems { mask } => {
            let avail = caps.offerable_systems();
            let here = avail & mask;
            if here == 0 {
                return;
            } // el pack no lo trae: no hace nada
            let mut m = cfg.subsystems.unwrap_or(avail);
            if m & here != 0 {
                m &= !here;
            } else {
                m |= here;
            }
            cfg.subsystems = Some(m);
        }
        RowKind::Bus(b) => {
            cfg.muted_buses ^= 1 << b;
        }
        RowKind::Shaders => {
            cfg.shaders = !cfg.shaders;
        }
        // EM-7.1: un toggle y no un ciclo — son dos estados y ninguno necesita
        // explicacion mas alla del nombre.
        RowKind::NoFlicker => {
            cfg.no_flicker = !cfg.no_flicker;
        }
        RowKind::Output => {
            // Ciclar es ELEGIR: a partir de acá manda el usuario y no la
            // recomendación del pack. Se arranca desde la que se está viendo
            // —la recomendada si no eligió— para que el primer paso mueva UNO
            // y no salte a otra parte de la lista.
            let cur = OUTPUT_IDS
                .iter()
                .position(|(i, _)| *i == cfg.output)
                .or_else(|| {
                    OUTPUT_IDS
                        .iter()
                        .position(|(i, _)| *i == caps.output_recommended)
                })
                .unwrap_or(0);
            let n = OUTPUT_IDS.len() as i32;
            let next = ((cur as i32 + delta) % n + n) % n;
            cfg.output = OUTPUT_IDS[next as usize].0.to_string();
        }
        RowKind::Restore => {
            *cfg = LaunchConfig::recommended(caps);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #633 — SACAR EL PACK NO TOCA NADA MAS QUE EL PERFIL.
    ///
    /// Es la garantia que la seccion promete con su verbo: «Desactivar» deja el
    /// juego en original y no roza la ROM, los guardados ni el pack en disco. La
    /// promesa no puede quedar en el comentario del callback — se comprueba
    /// sobre un arbol de guardados poblado, igual que `pack_install` prueba que
    /// instalar no los toca.
    #[test]
    fn disabling_the_pack_does_not_touch_the_saves() {
        let dir = tempfile::tempdir().unwrap();
        let saves = dir.path().join("saves");
        std::fs::create_dir_all(saves.join("aladdin/default")).unwrap();
        let sram = saves.join("aladdin/default/sram.srm");
        std::fs::write(&sram, b"la partida de alguien").unwrap();
        let rom = dir.path().join("juego.md");
        std::fs::write(&rom, b"la rom").unwrap();

        let caps = PackCapabilities {
            profiles: vec![
                ("original".into(), "Original".into()),
                ("hd".into(), "HD".into()),
            ],
            // El pack RECOMIENDA el suyo: con el default (0) el recomendado
            // seria «original» y poner el pack no cambiaria nada.
            default_profile: 1,
            ..Default::default()
        };
        let f = config_path(
            &dir.path().join("launch"),
            &rom,
            Some(&dir.path().join("p.ay")),
        );

        let mut cfg = load(&f);
        cfg.set_hd(true, &caps);
        save(&f, &cfg);
        assert!(load(&f).is_hd(), "el fixture arranca con el pack puesto");

        let mut cfg = load(&f);
        cfg.set_hd(false, &caps);
        save(&f, &cfg);

        let restored_config = load(&f);
        assert!(
            !restored_config.is_hd(),
            "desactivar deja el juego en original"
        );
        assert_eq!(restored_config.profile, ORIGINAL_PROFILE);

        assert_eq!(
            std::fs::read(&sram).unwrap(),
            b"la partida de alguien",
            "el guardado se toco: sacar un pack no puede pisar una partida"
        );
        assert!(rom.exists(), "la ROM sigue donde estaba");
    }

    /// EL PERFIL GLOBAL LLEGA AL LANZAMIENTO. Era el defecto: Ajustes lo
    /// guardaba en `play_config.toml` y nadie lo leía nunca — sólo se pasaba
    /// `--output` cuando se elegía por juego, así que el ajuste global no hacía
    /// absolutamente nada.
    #[test]
    fn the_global_profile_reaches_the_runtime() {
        let unchosen = LaunchConfig::default();
        assert!(!unchosen.to_args().iter().any(|a| a == "--output"));

        let with_global = with_global_output(&unchosen, "crt");
        assert_eq!(with_global.output, "crt");
        assert!(with_global.to_args().join(" ").contains("--output crt"));
    }

    /// Y pierde contra la elección POR JUEGO, que es más específica. Al revés,
    /// tocar Ajustes le cambiaría la salida a un juego que el jugador ya había
    /// configurado a mano.
    #[test]
    fn the_per_game_choice_beats_the_global_one() {
        let per_game = LaunchConfig {
            output: "pixel".into(),
            ..Default::default()
        };
        assert_eq!(with_global_output(&per_game, "crt").output, "pixel");
    }

    /// Sin ninguno de los dos no se manda nada: es lo que deja que el pack
    /// recomiende. Mandar el default de Play acá convertiría «no elegí» en una
    /// elección y ningún pack podría volver a recomendar.
    #[test]
    fn with_no_choice_nor_global_the_pack_rules() {
        let c = with_global_output(&LaunchConfig::default(), "");
        assert!(c.output.is_empty());
        assert!(!c.to_args().iter().any(|a| a == "--output"));
    }

    /// El ciclo global pasa por «sin elegir», que es el valor de fábrica y el
    /// único que deja que el pack siga recomendando. Sin él, probar un perfil
    /// una vez sería una decisión sin vuelta atrás.
    #[test]
    fn the_global_cycle_includes_not_choosing() {
        let mut v = String::new();
        let mut seen = vec![v.clone()];
        for _ in 0..OUTPUT_IDS.len() {
            v = cycle_global_output(&v, 1);
            assert!(!v.is_empty(), "todavía quedan perfiles por recorrer");
            assert!(output_name(&v).is_some(), "«{v}» no es un perfil conocido");
            seen.push(v.clone());
        }
        // Una vuelta entera vuelve al vacío, y pasó por todos.
        assert_eq!(cycle_global_output(&v, 1), "");
        assert_eq!(seen.len(), OUTPUT_IDS.len() + 1);

        // Y para atrás también: desde el vacío se llega al último.
        assert_eq!(
            cycle_global_output("", -1),
            OUTPUT_IDS[OUTPUT_IDS.len() - 1].0
        );
        assert_eq!(cycle_global_output(OUTPUT_IDS[0].0, -1), "");
    }

    fn caps() -> PackCapabilities {
        PackCapabilities {
            profiles: vec![
                ("original".into(), "Original".into()),
                ("faithful".into(), "Fiel".into()),
                ("enhanced".into(), "Mejorado".into()),
            ],
            default_profile: 2,
            systems: 0b0000_0111, // sprites + metasprites + tiles
            declares_systems: true,
            output_recommended: "crt".into(),
        }
    }

    /// El riesgo de la issue: ofrecer algo que el pack no tiene. Las opciones
    /// se DERIVAN del pack, no de una lista fija.    /// #598 — EL CA: «original» apaga TODO lo del pack.
    ///
    /// Se comprueba de las dos maneras de llegar ahí, porque el jugador ve una
    /// sola cosa —el juego como era— y un toggle que dijera «HD» sobre una
    /// pantalla sin nada remasterizado sería una mentira que además no se
    /// corrige tocándolo.
    #[test]
    fn original_turns_everything_off_and_the_toggle_knows_it() {
        let c = caps();
        let mut cfg = LaunchConfig::recommended(&c);
        assert!(cfg.is_hd(), "lo recomendado por el pack es HD");

        cfg.set_hd(false, &c);
        assert!(!cfg.is_hd());
        assert_eq!(cfg.profile, ORIGINAL_PROFILE);
        // La máscara va en CERO explícito: con `None` mandaría el perfil, y un
        // pack que no declare `original` dejaría todo encendido — lo contrario
        // de lo que el toggle acaba de pedir.
        assert_eq!(cfg.subsystems, Some(0));
        let a = cfg.to_args().join(" ");
        assert!(a.contains("--profile original"));
        assert!(
            a.contains("--subsystems 0"),
            "sin la mascara explicita el runtime usaria el perfil: {a}"
        );
    }

    /// Y es REVERSIBLE: volver deja el perfil del PACK, no «todo encendido».
    /// Restaurar todo sería elegir por el autor en un pack con tres perfiles.
    #[test]
    fn going_back_to_hd_restores_what_the_pack_proposes() {
        let c = caps();
        let mut cfg = LaunchConfig::recommended(&c);
        let recommended_profile = cfg.profile.clone();

        cfg.set_hd(false, &c);
        cfg.set_hd(true, &c);
        assert_eq!(cfg.profile, recommended_profile, "vuelve a lo del pack");
        assert_eq!(
            cfg.subsystems, None,
            "y sin eleccion manual pegada de antes"
        );
        assert!(cfg.is_hd());
    }

    /// Apagar todos los subsistemas a mano también es «original», aunque el
    /// perfil siga diciendo otra cosa: el toggle mira el EFECTO, no la etiqueta.
    #[test]
    fn turning_everything_off_by_hand_is_also_original() {
        let mut cfg = LaunchConfig::recommended(&caps());
        cfg.subsystems = Some(0);
        assert!(
            !cfg.is_hd(),
            "sin un solo subsistema no hay nada HD que mostrar"
        );
    }

    #[test]
    fn what_the_pack_does_not_bring_is_not_offered() {
        let c = caps();
        assert_eq!(c.offerable_systems(), 0b111);

        // Un pack LEGACY no declara nada, y ahí ofrecer una lista vacía dejaría
        // al jugador sin controles sobre un pack que sí sustituye cosas.
        // `[systems]` ausente no es `[systems]` vacío (#294).
        let legacy = PackCapabilities {
            declares_systems: false,
            ..c
        };
        assert_eq!(legacy.offerable_systems(), 0xFF);
    }

    /// La recomendada es la del AUTOR, no «todo encendido»: volver a todo
    /// restauraría algo que nadie decidió.
    #[test]
    fn the_recommended_one_comes_from_the_pack() {
        let r = LaunchConfig::recommended(&caps());
        assert_eq!(r.profile, "enhanced");
        assert!(r.subsystems.is_none(), "recomendada = manda el perfil");
        assert_eq!(r.muted_buses, 0);
    }

    /// Una configuración guardada puede venir de otra versión del pack.
    #[test]
    fn what_was_saved_is_trimmed_to_what_the_pack_can_do() {
        let c = caps();
        let mut g = LaunchConfig {
            profile: "reinterpreted".into(), // ya no existe
            subsystems: Some(0b1111_1111),   // más de lo que trae
            muted_buses: 0b10,
            shaders: false,
            output: String::new(),
            no_flicker: false,
            patch: String::new(),
        };
        g.clamp_to(&c);
        assert_eq!(
            g.profile, "enhanced",
            "un perfil que no existe cae al recomendado"
        );
        assert_eq!(g.subsystems, Some(0b111), "los subsistemas se intersectan");
        assert_eq!(g.muted_buses, 0b10, "lo que no depende del pack no se toca");
        assert!(!g.shaders);
    }

    /// `None` y `Some(0)` no son lo mismo: uno dice «manda el perfil», el otro
    /// «apagué todo». Confundirlos decide si el juego arranca remasterizado.
    #[test]
    fn not_choosing_is_not_turning_everything_off() {
        let no_selection = LaunchConfig {
            profile: "faithful".into(),
            ..Default::default()
        };
        assert!(
            !no_selection.to_args().iter().any(|a| a == "--subsystems"),
            "sin elección, el runtime usa el perfil"
        );

        let disabled_subsystems = LaunchConfig {
            profile: "faithful".into(),
            subsystems: Some(0),
            ..Default::default()
        };
        let args = disabled_subsystems.to_args();
        let i = args
            .iter()
            .position(|a| a == "--subsystems")
            .expect("va la flag");
        assert_eq!(args[i + 1], "0", "apagar todo SÍ se transmite");
    }

    #[test]
    fn the_arguments_reach_the_runtime() {
        let c = LaunchConfig {
            profile: "enhanced".into(),
            subsystems: Some(5),
            muted_buses: 2,
            shaders: false,
            output: String::new(),
            no_flicker: false,
            patch: String::new(),
        };
        let a = c.to_args().join(" ");
        assert!(a.contains("--profile enhanced"));
        assert!(a.contains("--subsystems 5"));
        assert!(a.contains("--mute-buses 2"));
        assert!(a.contains("--no-shaders"));

        // Con shaders, la flag NO va: pasar `--shaders` siempre haría que el
        // runtime nunca pudiera usar su propio default.
        let d = LaunchConfig {
            shaders: true,
            ..Default::default()
        };
        assert!(d.to_args().is_empty(), "sin nada elegido, no se pasa nada");
    }

    /// Por combinación ROM + pack: los subsistemas no significan lo mismo en
    /// dos packs distintos.
    #[test]
    fn one_config_per_rom_and_pack() {
        let d = Path::new("/cfg");
        let rom = Path::new("/roms/Golden Axe (U).md");
        let p1 = Path::new("/packs/ga-hd.ay");
        let p2 = Path::new("/packs/otro.ay");
        assert_ne!(config_path(d, rom, Some(p1)), config_path(d, rom, Some(p2)));
        assert_ne!(config_path(d, rom, Some(p1)), config_path(d, rom, None));
        // Por NOMBRE y no por ruta: mover la biblioteca de carpeta no puede
        // borrar las preferencias de todos los juegos.
        let relocated_rom = Path::new("/otra/carpeta/Golden Axe (U).md");
        assert_eq!(
            config_path(d, rom, Some(p1)),
            config_path(d, relocated_rom, Some(p1))
        );
        // Y el nombre no puede romper la ruta: los parentesis y espacios se
        // sanean, y si no la config no se guardaria — en silencio.
        let f = config_path(d, rom, Some(p1));
        let name = f.file_name().unwrap().to_string_lossy().to_string();
        assert!(!name.contains(' ') && !name.contains('('));
    }

    // -- El menu de filas (la pantalla) ----------------------------------

    fn find<'a>(rows: &'a [ConfigRow], label: &str) -> &'a ConfigRow {
        rows.iter()
            .find(|r| r.label == label)
            .expect("la fila existe")
    }

    /// Las filas salen del PACK. Y lo que el pack no trae se muestra APAGADO,
    /// no se esconde: una fila que desaparece se lee como «esta version no lo
    /// tiene»; una gris dice «este pack no lo trae», que es la verdad.
    #[test]
    fn the_rows_come_from_the_pack() {
        let c = caps(); // sprites + metasprites + tiles, sin ui
        let rows = config_rows(&c, &LaunchConfig::recommended(&c));
        assert!(find(&rows, "Personajes").available);
        assert!(find(&rows, "Escenario").available, "trae tiles");
        let ui = find(&rows, "Interfaz");
        assert!(!ui.available, "el pack no trae ui");
        assert_eq!(ui.value, "no incluido");
        // La fila del perfil NOMBRA con que se va a jugar aunque el jugador no
        // haya elegido: mostrar un hueco dejaria al jugador sin saberlo.
        assert_eq!(find(&rows, "Perfil").value, "Mejorado");
    }

    /// Sin eleccion propia, la pantalla muestra lo que trae el PERFIL. No puede
    /// decir «original» sobre algo que va a arrancar sustituido.
    #[test]
    fn with_no_choice_what_the_profile_will_do_is_shown() {
        let c = caps();
        let cfg = LaunchConfig::recommended(&c);
        assert!(cfg.subsystems.is_none());
        let rows = config_rows(&c, &cfg);
        assert_eq!(find(&rows, "Personajes").value, "AYTHER");
    }

    /// Izquierda/derecha alterna, y el grupo mueve TODOS sus subsistemas.
    #[test]
    fn adjusting_a_row_changes_the_state() {
        let c = caps();
        let mut cfg = LaunchConfig::recommended(&c);
        adjust(&c, &mut cfg, &RowKind::Systems { mask: 0b11 }, 1);
        assert_eq!(cfg.subsystems, Some(0b100), "apaga sprites Y metasprites");
        assert_eq!(find(&config_rows(&c, &cfg), "Personajes").value, "original");
        adjust(&c, &mut cfg, &RowKind::Systems { mask: 0b11 }, 1);
        assert_eq!(cfg.subsystems, Some(0b111), "y los vuelve a encender");

        adjust(&c, &mut cfg, &RowKind::Bus(1), 1);
        assert_eq!(cfg.muted_buses, 0b10);
        adjust(&c, &mut cfg, &RowKind::Shaders, -1);
        assert!(!cfg.shaders, "para dos estados el signo da igual");
    }

    /// Una fila que el pack no trae no hace NADA. Sin esto, el jugador la
    /// mueve, ve cambiar el texto y espera algo que no va a pasar.
    #[test]
    fn an_unavailable_row_does_nothing() {
        let c = caps();
        let mut cfg = LaunchConfig::recommended(&c);
        let before = cfg.clone();
        adjust(&c, &mut cfg, &RowKind::Systems { mask: 0b1_0000 }, 1); // ui
        assert_eq!(cfg, before);
    }

    /// Cambiar de perfil DESCARTA los ajustes sueltos: el perfil los pisa igual
    /// al aplicarse, y dejarlos haria que la pantalla mostrara una mezcla que
    /// el runtime no va a reproducir.
    #[test]
    fn switching_profiles_discards_what_was_hand_touched() {
        let c = caps();
        let mut cfg = LaunchConfig::recommended(&c);
        adjust(&c, &mut cfg, &RowKind::Systems { mask: 0b11 }, 1);
        assert!(cfg.subsystems.is_some());
        adjust(&c, &mut cfg, &RowKind::Profile, 1);
        assert!(cfg.subsystems.is_none(), "manda el perfil nuevo");
    }

    /// El perfil CICLA en los dos sentidos: en un mando no hay donde hacer clic
    /// para volver al primero.
    #[test]
    fn the_profile_cycles() {
        let c = caps(); // original, faithful, enhanced (default = enhanced)
        let mut cfg = LaunchConfig::recommended(&c);
        adjust(&c, &mut cfg, &RowKind::Profile, 1);
        assert_eq!(cfg.profile, "original", "desde el ultimo vuelve al primero");
        adjust(&c, &mut cfg, &RowKind::Profile, -1);
        assert_eq!(cfg.profile, "enhanced", "y al reves");
    }

    /// #296: la fila de SALIDA muestra lo que el pack recomienda mientras el
    /// jugador no elija — y lo dice, en vez de un «(automático)» que no informa
    /// con qué va a ver el juego.
    #[test]
    fn the_output_shows_the_pack_choice_until_the_player_picks() {
        let c = caps(); // el pack recomienda "crt"
        let mut cfg = LaunchConfig::recommended(&c);
        assert!(
            cfg.output.is_empty(),
            "«recomendado» incluye no elegir la salida"
        );
        let r0 = config_rows(&c, &cfg);
        let v = &find(&r0, "Salida").value;
        assert!(
            v.contains("CRT") && v.contains("del pack"),
            "se ve la del pack, y se dice que es del pack: {v}"
        );

        // Ciclar es ELEGIR: a partir de acá manda el usuario.
        adjust(&c, &mut cfg, &RowKind::Output, 1);
        assert!(!cfg.output.is_empty());
        let r1 = config_rows(&c, &cfg);
        let v2 = &find(&r1, "Salida").value;
        assert!(!v2.contains("del pack"), "ya no es del pack: {v2}");
        // Y arranca desde la que se estaba VIENDO: el primer paso mueve uno y
        // no salta a otra parte de la lista.
        assert_eq!(cfg.output, "pixel", "crt → pixel, el siguiente de la lista");

        // Y lo elegido viaja al runtime.
        assert!(c.output_recommended == "crt");
        let args = cfg.to_args().join(" ");
        assert!(args.contains("--output pixel"));
        // Sin elección NO se pasa la flag: pasarla con el valor recomendado
        // haría que el runtime no pudiera distinguir «el usuario eligió esto»
        // de «el pack lo sugiere», que es toda la diferencia.
        let no_selection = LaunchConfig::recommended(&c);
        assert!(!no_selection.to_args().iter().any(|a| a == "--output"));
    }

    #[test]
    fn restore_goes_back_to_the_pack_choice() {
        let c = caps();
        let mut cfg = LaunchConfig {
            profile: "faithful".into(),
            subsystems: Some(1),
            muted_buses: 6,
            shaders: false,
            output: "crt".into(),
            no_flicker: false,
            patch: String::new(),
        };
        adjust(&c, &mut cfg, &RowKind::Restore, 1);
        assert_eq!(cfg, LaunchConfig::recommended(&c));
        assert_eq!(cfg.profile, "enhanced", "el del PACK, no «todo encendido»");
    }

    #[test]
    fn it_round_trips() {
        let dir = std::env::temp_dir().join("ayther_launch_cfg_test");
        let _ = std::fs::create_dir_all(&dir);
        let f = dir.join("x.toml");
        let c = LaunchConfig {
            profile: "faithful".into(),
            subsystems: Some(3),
            muted_buses: 4,
            shaders: false,
            output: "pixel".into(),
            no_flicker: true,
            patch: "C:/hacks/traduccion.ips".into(),
        };
        assert!(save(&f, &c));
        assert_eq!(load(&f), c);

        // Sin archivo: los defaults, y `subsystems` en None — el runtime no
        // tiene que aplicar ninguna máscara en la primera partida.
        let default_config = load(&dir.join("no-existe.toml"));
        assert!(default_config.subsystems.is_none() && default_config.shaders);

        // Y guardar «no elegí» no puede volver como «apagué todo».
        let no_selection = LaunchConfig {
            profile: "a".into(),
            ..Default::default()
        };
        assert!(save(&f, &no_selection));
        assert!(load(&f).subsystems.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // -- EM-7.1 (#230): «Modo suave» ----------------------------------------
    //
    // Es una opción del CORE, no un shader ni un perfil, y eso decide cómo se
    // comporta: se manda sólo cuando el jugador la enciende, y su ausencia no
    // es «apagada» sino «que mande el core».

    #[test]
    fn soft_mode_only_travels_if_the_player_turns_it_on() {
        let mut c = LaunchConfig::default();
        assert!(
            !c.to_args().join(" ").contains("core-option"),
            "apagado NO manda la opcion: fijar `disabled` pisaria el default del core"
        );
        c.no_flicker = true;
        assert!(
            c.to_args()
                .join(" ")
                .contains("--core-option no_sprite_limit=enabled")
        );
    }

    /// Arranca apagado. Un remaster que modifica la emulación por su cuenta
    /// hace que el primer vistazo no sea el juego — la opción existe, no se
    /// impone.
    #[test]
    fn soft_mode_starts_off() {
        assert!(!LaunchConfig::default().no_flicker);
        assert!(
            !LaunchConfig::recommended(&caps()).no_flicker,
            "«recomendado» es lo del PACK, y el parpadeo no es cosa del pack"
        );
    }

    /// Persiste como las demás: quien la eligió una vez no vuelve a elegirla.
    #[test]
    fn soft_mode_persists() {
        let f = tempfile::NamedTempFile::new().unwrap();
        let mut c = LaunchConfig {
            no_flicker: true,
            ..Default::default()
        };
        assert!(save(f.path(), &c));
        assert!(load(f.path()).no_flicker, "se guardo y se leyo");

        c.no_flicker = false;
        assert!(save(f.path(), &c));
        assert!(!load(f.path()).no_flicker, "y el apagado tambien persiste");
    }

    /// Tiene su fila y es un TOGGLE, no un ciclo: son dos estados.
    #[test]
    fn soft_mode_has_a_row_and_toggles() {
        let c = caps();
        let mut cfg = LaunchConfig::recommended(&c);
        let row = config_rows(&c, &cfg)
            .into_iter()
            .find(|r| matches!(r.kind, RowKind::NoFlicker))
            .expect("la fila existe");
        assert_eq!(row.value, "no");
        // `available` es true aunque no sepamos si el core la ofrece: eso no se
        // puede saber hasta abrir la sesion, y mostrarla gris por las dudas
        // mentiria en el caso normal.
        assert!(row.available);

        adjust(&c, &mut cfg, &RowKind::NoFlicker, 1);
        assert!(cfg.no_flicker);
        adjust(&c, &mut cfg, &RowKind::NoFlicker, 1);
        assert!(!cfg.no_flicker, "alterna, no cicla");
    }

    /// NO VACUIDAD: la clave que se manda es la que el core entiende. Un typo
    /// acá compila, corre, y no hace nada — que es exactamente el defecto que
    /// esta tarea vino a arreglar, sólo que un escalón más abajo.
    #[test]
    fn the_key_is_the_core_one() {
        let c = LaunchConfig {
            no_flicker: true,
            ..Default::default()
        };
        let a = c.to_args();
        let i = a
            .iter()
            .position(|x| x == "--core-option")
            .expect("va la bandera");
        assert_eq!(
            a[i + 1],
            "no_sprite_limit=enabled",
            "la clave de Genesis Plus GX, con `=` y sin espacios"
        );
    }

    // -- EM-7.4 (#230): parches del usuario ---------------------------------

    /// El parche viaja al runtime, que lo aplica en RAM. Vacío = ninguno, y
    /// entonces no se manda la bandera: un `--patch ""` haría que el runtime
    /// intentara abrir un archivo sin nombre.
    #[test]
    fn the_user_patch_travels_to_the_runtime() {
        let mut c = LaunchConfig::default();
        assert!(
            !c.to_args().join(" ").contains("--patch"),
            "sin parche no se manda nada"
        );
        c.patch = "C:/hacks/traduccion.ips".into();
        let a = c.to_args();
        let i = a
            .iter()
            .position(|x| x == "--patch")
            .expect("va la bandera");
        assert_eq!(a[i + 1], "C:/hacks/traduccion.ips", "la ruta, tal cual");
    }

    /// El parche persiste con la config del juego: quien juega una traducción
    /// no tiene que volver a elegirla cada vez.
    #[test]
    fn the_patch_persists() {
        let f = tempfile::NamedTempFile::new().unwrap();
        let c = LaunchConfig {
            patch: "D:/roms/hack.bps".into(),
            ..Default::default()
        };
        assert!(save(f.path(), &c));
        assert_eq!(load(f.path()).patch, "D:/roms/hack.bps");
    }

    /// «Restaurar lo recomendado» NO le saca el parche al jugador. Lo
    /// recomendado es del PACK, y una fan-translation es del usuario: es la
    /// misma frontera que separa el perfil (del autor) del Modo suave (suyo).
    #[test]
    fn restore_does_not_take_the_patch_away_from_the_player() {
        let rec = LaunchConfig::recommended(&caps());
        assert!(rec.patch.is_empty(), "recommended() no INVENTA un parche");
        assert!(!rec.no_flicker, "ni una preferencia de emulacion");
    }

    /// #654 — las cuatro combinaciones, porque la que importa es la que no se
    /// deduce sola: un juego YA configurado abriendo la pantalla porque lo
    /// pidió «Configurar».
    #[test]
    fn the_pre_screen_is_first_time_only_unless_configure_asks_for_it() {
        assert!(
            open_prelaunch(false, false),
            "sin nada configurado, Jugar TIENE que ofrecer la pantalla: es la unica vez                  que el jugador ve lo que el pack permite"
        );
        assert!(
            !open_prelaunch(true, false),
            "ya configurado, Jugar arranca directo — que es el punto de #654"
        );
        assert!(
            open_prelaunch(true, true),
            "ya configurado pero pedido por «Configurar»: si esto diera false, ese boton                  seria el unico que no puede abrir la configuracion"
        );
        assert!(
            open_prelaunch(false, true),
            "forzada sobre un juego sin configurar: la pantalla igual"
        );
    }

    /// La combinacion ROM+pack es la clave, y por eso «ya configurado» no es
    /// global: estrenar un pack sobre el mismo juego vuelve a ofrecer opciones
    /// que el jugador nunca vio.
    #[test]
    fn the_config_file_tells_rom_and_pack_apart() {
        let dir = Path::new("/tmp/launch");
        let rom = Path::new("/roms/golden_axe.md");
        let a = config_path(dir, rom, None);
        let b = config_path(dir, rom, Some(Path::new("/packs/hd.ay")));
        let c = config_path(dir, rom, Some(Path::new("/packs/otro.ay")));
        assert_ne!(a, b, "con pack y sin pack no son la misma configuracion");
        assert_ne!(b, c, "dos packs distintos tampoco");
        assert_eq!(
            b,
            config_path(dir, rom, Some(Path::new("/packs/hd.ay"))),
            "la misma combinacion da SIEMPRE el mismo archivo, o «ya configurado»                     seria falso todas las veces y la pantalla volveria a aparecer siempre"
        );
    }
}
