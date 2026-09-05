//! onboarding.rs — el primer arranque, en seis pasos (#588, PLAY-EP01.1).
//!
//! Antes de esto, Play no tenía primer arranque: leía `play_config.toml` si
//! existía y, si no, inventaba rutas junto al ejecutable. Nadie escribía ese
//! archivo — `Config::save()` no tenía un solo llamador—, así que la única
//! manera de configurar Play era editar TOML a mano. Para un launcher que se
//! usa desde el sillón, eso es no tener configuración.
//!
//! Los seis pasos son los de `docs/design/play-ce.pen` (00 y 00b–00f): idioma ·
//! mandos · carpetas de ROM · core · BYOR/BYOC · resumen.
//!
//! El paso 1 elige **sólo el idioma**. El diseño mostraba también un tema
//! (oscuro / alto contraste) y llegó a estar en pantalla, pero no había paleta
//! de alto contraste que aplicar ni campo donde guardarlo: era un control que
//! no hacía nada. El alto contraste entra con su paleta en EP11 (#582); hasta
//! entonces no se ofrece, porque elegirlo y no ver cambio se lee como que Play
//! está roto.
//!
//! **Todo lo que decide vive acá y es puro.** La UI dibuja `Estado` y le manda
//! `Accion`; no sabe qué paso sigue ni cuándo se puede avanzar. Es el mismo
//! reparto que `pad_router` (#586), y por el mismo motivo: las reglas que
//! importan —no se avanza sin core válido, «Atrás» nunca deja al jugador
//! encerrado— se prueban sin abrir una ventana.

use crate::config::Config;
use crate::core_probe::{self, Verdict};
use crate::tr;
use std::path::PathBuf;

/// Versión de los términos BYOR/BYOC que se muestran. Si cambia, la aceptación
/// vieja deja de valer y el paso se vuelve a mostrar: por eso se guarda la
/// versión y no un booleano.
pub const TERMS_VERSION: &str = "1.0";

/// El texto de los términos (#590, diseño 00e).
///
/// **Neutral y sin enlaces, por regla de la spec (§3, §11): «legalidad visible,
/// sin enlaces a fuentes no autorizadas».** Play no dice de dónde sacar una ROM
/// ni un core, ni con una URL, ni con un nombre de sitio, ni con un «buscá». Un
/// test recorre estos párrafos buscando cualquier cosa que parezca un enlace,
/// porque el día que alguien agregue uno «para ayudar» nadie va a estar mirando.
///
/// Vive acá y no en el `.slint` para que sea uno solo: el texto que se acepta,
/// el que se traduce y el que se prueba tienen que ser el mismo. Y así el día
/// que cambie, cambia al lado de la constante de versión que lo gobierna — que
/// es lo que hace que nadie se olvide de subirla.
pub fn terms_paragraphs() -> Vec<String> {
    vec![
        tr!(
            "AYTHER no distribuye ROMs ni cores, y no ayuda a conseguirlos: no incluye ninguno, no los descarga y no enlaza a ningún sitio que los ofrezca."
        ),
        tr!(
            "BYOR — la ROM la ponés vos. Play lee los archivos de las carpetas que le indiques y nunca los copia, los mueve ni los sube. Tener derecho a usar esas copias es tuyo."
        ),
        tr!(
            "BYOC — el core lo ponés vos. El emulador que ejecuta el juego es un programa de terceros que elegís e instalás vos; Play lo lanza en un proceso aparte y no lo modifica."
        ),
        tr!(
            "Un pack de remasterización no contiene partes del juego original. Aporta material propio de su autor —gráficos, sonido, video— que se aplica sobre tu copia mientras jugás: sin tu ROM no hay nada que jugar."
        ),
        tr!(
            "Nada de esto sale de tu máquina. En modo local, Play no envía tu biblioteca, tus guardados ni qué estás jugando."
        ),
    ]
}

/// ¿La aceptación que hay registrada vale para los términos de hoy?
///
/// Las dos cosas o ninguna: una versión sin fecha no es un registro —no se
/// puede decir cuándo se aceptó— y una fecha sin versión no dice qué se aceptó.
/// Un registro a medias vuelve a pedir los términos, que es el lado seguro del
/// error: molesta una vez, en vez de dar por aceptado algo que nadie firmó.
pub fn terms_are_current(cfg: &Config) -> bool {
    cfg.terms_accepted.as_deref() == Some(TERMS_VERSION)
        && cfg.terms_date.as_deref().is_some_and(|f| !f.is_empty())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    Welcome = 0,
    Gamepads = 1,
    Folders = 2,
    Core = 3,
    Terms = 4,
    Summary = 5,
}

/// Cuántos pasos tiene el recorrido. La UI no lo lee —cada paso se dibuja
/// solo—, pero el oráculo cuenta con él para saber si los recorrió todos: el `allow` no cubre `cargo test`: si el oráculo se va, el aviso vuelve.
#[cfg_attr(not(test), allow(dead_code))]
pub const STEPS: usize = 6;

impl Step {
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn from_index(i: usize) -> Step {
        match i {
            0 => Step::Welcome,
            1 => Step::Gamepads,
            2 => Step::Folders,
            3 => Step::Core,
            4 => Step::Terms,
            _ => Step::Summary,
        }
    }

    /// El título del paso, tal como lo escribe el `.pen`.
    ///
    /// #627 — es el ÚNICO título que queda. El wizard era un diálogo con su
    /// Head («MANDOS», «CARPETAS DE ROMS») y el encabezado adentro, dos títulos
    /// para la misma pantalla; el diseño nuevo lo abre a pantalla completa y
    /// deja uno solo.
    pub fn heading(self) -> String {
        match self {
            Step::Welcome => {
                tr!("Tu biblioteca retro, remasterizada. Vos ponés la ROM y nosotros el resto.")
            }
            Step::Gamepads => tr!("PROBÁ TU MANDO"),
            Step::Folders => tr!("¿DÓNDE ESTÁN TUS JUEGOS?"),
            Step::Core => tr!("ELEGÍ TU CORE"),
            Step::Terms => tr!("TÉRMINOS DE USO"),
            Step::Summary => tr!("TODO LISTO"),
        }
    }
}

/// Los idiomas del paso 1, en el orden en que rotan.
///
/// `auto` está en el ciclo y no afuera porque es el valor que trae una
/// instalación nueva: sin él, el paso mostraría «Español» sobre una
/// configuración que dice `auto`, y en una máquina en inglés el jugador leería
/// «Español» en el wizard y vería la biblioteca en inglés.
pub const LANGUAGES: [&str; 3] = ["auto", "es", "en"];

/// El siguiente idioma del ciclo, en la dirección pedida.
///
/// Libre y no sólo método del wizard: Ajustes (#599) rota exactamente por la
/// misma lista. Dos ciclos separados es como una pantalla ofrece tres idiomas
/// y la otra dos.
pub fn cycle_language(current_language: &str, d: i32) -> String {
    let n = LANGUAGES.len() as i32;
    let i = LANGUAGES
        .iter()
        .position(|x| *x == current_language)
        .unwrap_or(0) as i32;
    let d = if d == 0 { 1 } else { d };
    LANGUAGES[(((i + d) % n + n) % n) as usize].to_string()
}

/// Cómo se lee un código de idioma. Uno solo para el paso y el resumen: dos
/// listas de nombres es cómo terminan diciendo cosas distintas.
pub fn language_name(language_code: &str) -> String {
    match language_code {
        "es" => tr!("Español"),
        "en" => tr!("English"),
        // #627 — «Predeterminado», que es como lo llama el .pen. Antes decía
        // «El del sistema»: la misma idea con más palabras, y en un chip a tres
        // metros las palabras de más son las que no se leen.
        _ => tr!("Predeterminado"),
    }
}

/// Lo que el jugador lleva elegido.
///
/// `PartialEq` no es decorativo: es lo que deja probar que una acción del
/// wizard hizo *algo*, que es como se atrapan los botones que prometen y no
/// cumplen.
#[derive(Debug, Clone, PartialEq)]
pub struct State {
    pub step: Step,
    pub language: String,
    /// Nombre del mando detectado, vacío si no hay ninguno.
    pub gamepad_name: String,
    pub folders: Vec<PathBuf>,
    /// Cuántas ROM se vieron en cada carpeta, en el mismo orden.
    pub roms_per_folder: Vec<usize>,
    pub core: Option<PathBuf>,
    pub verdict: Option<Verdict>,
    pub terms_accepted: bool,
    /// Cuándo se aceptaron los términos vigentes, si ya estaban aceptados
    /// (#590). Se muestra en el paso y es lo que NO se pisa al re-correr el
    /// wizard: volver a pasar por la pantalla no es volver a aceptar.
    pub terms_accepted_at: Option<String>,
    /// Este wizard es el PRIMER ARRANQUE de verdad: no hay nada configurado
    /// detrás.
    ///
    /// Decide lo único que puede cerrar Play desde adentro. «B» en la portada
    /// sale de la aplicación —la única excepción a «Atrás nunca cierra» (§9)—
    /// y la razón escrita es que *en la primera pantalla del primer arranque no
    /// hay a dónde volver y no hay nada guardado que perder*. Las dos mitades
    /// de esa frase dejan de ser ciertas cuando el wizard se re-abre desde
    /// Ajustes («Volver a ver la bienvenida») o con `--onboarding`: ahí SÍ hay
    /// una biblioteca detrás, y «B» cerraba Play de golpe sin que nadie pidiera
    /// salir.
    pub first_run: bool,
    /// #599 — el wizard se abrió desde Ajustes para editar UN paso.
    ///
    /// Cambia tres cosas y ninguna más: «A» GUARDA en vez de continuar, «B»
    /// vuelve a Ajustes en vez de retroceder un paso, y la pantalla no dibuja
    /// «Paso 3 de 6» —no es un recorrido de seis pasos—. El resto del paso es
    /// exactamente el mismo, que es el punto: dos selectores de carpeta
    /// distintos serían dos comportamientos distintos para la misma decisión.
    pub single_step_mode: bool,
    /// El explorador está abierto encima del paso.
    pub browsing: bool,
    /// Dónde está parado el explorador, qué busca y qué se ve.
    pub explorer_dir: PathBuf,
    pub explorer_folder_mode: bool,
    pub explorer_entries: Vec<crate::explorer::Entry>,
}

impl Default for State {
    fn default() -> Self {
        State {
            step: Step::Welcome,
            language: "auto".into(),
            gamepad_name: String::new(),
            folders: Vec::new(),
            roms_per_folder: Vec::new(),
            core: None,
            verdict: None,
            terms_accepted: false,
            terms_accepted_at: None,
            first_run: false,
            single_step_mode: false,
            browsing: false,
            explorer_dir: PathBuf::new(),
            explorer_folder_mode: true,
            explorer_entries: Vec::new(),
        }
    }
}

impl State {
    /// Arranca desde lo que ya hay configurado, para que re-correr el
    /// onboarding no obligue a rehacer lo que estaba bien.
    pub fn of(cfg: &Config) -> State {
        // Sólo las que siguen existiendo. Arrastrar una carpeta que se borró
        // —o la ruta por defecto, que en una instalación nueva no existe— haría
        // que el paso arranque con una fila que no se puede escanear y con
        // «Continuar» habilitado: el jugador terminaría el wizard y se
        // encontraría una biblioteca vacía.
        let folders: Vec<PathBuf> = crate::config::rom_folders(cfg)
            .into_iter()
            .filter(|d| d.exists())
            .collect();
        let roms = folders
            .iter()
            .map(|d| crate::explorer::count_roms(d))
            .collect();
        // Un core ya configurado se VALIDA de entrada, no cuando el jugador lo
        // vuelve a elegir. Es el caso de quien re-corre el wizard porque algo
        // dejó de andar: mostrarle el semáforo de lo que ya tiene es la mitad
        // de la respuesta, y obligarlo a re-elegir un archivo que no cambió
        // para enterarse es la forma larga de decírselo.
        let core = Some(cfg.default_core.clone()).filter(|p| p.exists());
        let verdict = core
            .as_ref()
            .map(|c| core_probe::validate(&cfg.runtime(), c));
        State {
            // Lo dice la configuración, no quien abre el wizard: es el mismo
            // dato con el que `hace_falta` decide mostrarlo la primera vez.
            first_run: !cfg.onboarding_complete,
            language: cfg.language.clone(),
            folders,
            roms_per_folder: roms,
            core,
            verdict,
            terms_accepted: terms_are_current(cfg),
            terms_accepted_at: if terms_are_current(cfg) {
                cfg.terms_date.clone()
            } else {
                None
            },
            ..Default::default()
        }
    }

    /// Por qué no se puede avanzar todavía, o `None` si se puede.
    ///
    /// Devuelve el motivo en vez de un `bool` porque el botón deshabilitado sin
    /// explicación es la forma más frecuente de dejar a alguien trabado: el
    /// jugador ve «Continuar» apagado y no tiene manera de saber qué le falta.
    pub fn blocker(&self) -> Option<String> {
        match self.step {
            Step::Welcome => None,
            // El mando es opcional a propósito: quien juega con teclado tiene
            // que poder terminar el wizard igual.
            Step::Gamepads => None,
            Step::Folders => {
                if self.folders.is_empty() {
                    Some(tr!("Agregá al menos una carpeta con tus ROM."))
                } else {
                    None
                }
            }
            Step::Core => match &self.verdict {
                Some(v) if v.is_usable() => None,
                Some(v) => Some(v.remedy()),
                None => Some(tr!("Elegí el archivo del core que vas a usar.")),
            },
            Step::Terms => {
                if self.terms_accepted {
                    None
                } else {
                    Some(tr!(
                        "Marcá que entendés que las ROM y el core los ponés vos."
                    ))
                }
            }
            Step::Summary => None,
        }
    }

    pub fn can_advance(&self) -> bool {
        self.blocker().is_none()
    }

    /// La siguiente opción de idioma, en la dirección pedida. Rota en vez de
    /// topar en los extremos: con tres opciones, toparse obliga a saber en qué
    /// punta de la lista está uno parado.
    pub fn cycle_language(&mut self, d: i32) {
        self.language = cycle_language(&self.language, d);
    }

    /// Avanza si se puede. Devuelve `false` si estaba trabado (la UI muestra
    /// entonces `traba()`).
    pub fn advance(&mut self) -> bool {
        if !self.can_advance() || self.step == Step::Summary {
            return false;
        }
        self.step = Step::from_index(self.step.index() + 1);
        true
    }

    /// ¿«B» acá cierra Play?
    ///
    /// Sólo en la portada del PRIMER arranque, y con el explorador cerrado. Es
    /// la única excepción a «Atrás nunca cierra la aplicación» (§9) y vive en
    /// este módulo —y no en el callback— para que se pueda probar sin abrir una
    /// ventana: es la respuesta que, si sale mal, cierra la aplicación.
    pub fn back_closes_app(&self) -> bool {
        self.first_run && !self.browsing && !self.single_step_mode && self.step == Step::Welcome
    }

    /// Retrocede. Desde el primer paso no hace nada: en el onboarding, «Atrás»
    /// no puede cerrar Play — es la misma regla del chasis (§9) y la razón por
    /// la que este método no devuelve «cerrar».
    pub fn go_back(&mut self) -> bool {
        if self.browsing {
            self.browsing = false;
            return true;
        }
        // #599 — abierto desde Ajustes no hay paso anterior al que volver: el
        // anterior es Ajustes, y quien cierra el wizard es el llamador. Sin
        // esto, «B» sobre «Carpetas de ROMs» dejaba al jugador en el paso del
        // mando, dentro de un wizard que él nunca abrió.
        if self.single_step_mode {
            return false;
        }
        if self.step == Step::Welcome {
            return false;
        }
        self.step = Step::from_index(self.step.index() - 1);
        true
    }

    pub fn add_folder(&mut self, dir: PathBuf) {
        if self.folders.contains(&dir) {
            return;
        }
        let n = crate::explorer::count_roms(&dir);
        self.folders.push(dir);
        self.roms_per_folder.push(n);
    }

    pub fn remove_folder(&mut self, i: usize) {
        if i < self.folders.len() {
            self.folders.remove(i);
            self.roms_per_folder.remove(i);
        }
    }

    pub fn total_roms(&self) -> usize {
        self.roms_per_folder.iter().sum()
    }

    /// Elige el core y lo valida en el acto, contra el runtime configurado.
    ///
    /// En el acto y no al apretar «Continuar»: el jugador tiene que ver el
    /// semáforo sobre el archivo que acaba de elegir, con el explorador todavía
    /// fresco en la cabeza. Un error que aparece un paso después ya no se
    /// asocia con la elección.
    pub fn select_core(&mut self, cfg: &Config, core: PathBuf) {
        self.verdict = Some(core_probe::validate(&cfg.runtime(), &core));
        self.core = Some(core);
        self.browsing = false;
    }

    // -- el explorador -------------------------------------------------------

    /// Abre el explorador buscando una carpeta o un core.
    ///
    /// Arranca donde ya estuvo, si estuvo: quien agrega tres carpetas de la
    /// misma unidad no tiene que volver a navegar desde su casa cada vez.
    pub fn open_explorer(&mut self, folder: bool) {
        let since = if self.explorer_dir.exists() && self.explorer_folder_mode == folder {
            self.explorer_dir.clone()
        } else if folder {
            self.folders
                .last()
                .cloned()
                .filter(|p| p.exists())
                .unwrap_or_else(crate::explorer::starting_path)
        } else {
            self.core
                .as_ref()
                .and_then(|c| c.parent().map(PathBuf::from))
                .filter(|p| p.exists())
                .unwrap_or_else(crate::explorer::starting_path)
        };
        self.explorer_folder_mode = folder;
        self.navigate_to(since);
        self.browsing = true;
    }

    fn navigate_to(&mut self, dir: PathBuf) {
        let search = if self.explorer_folder_mode {
            crate::explorer::Search::Folder
        } else {
            crate::explorer::Search::File(crate::explorer::EXT_CORE)
        };
        self.explorer_entries = crate::explorer::list_entries(&dir, search);
        self.explorer_dir = dir;
    }

    /// «A» sobre la fila `i` del explorador: entrar en la carpeta, o elegir el
    /// archivo. Devuelve `true` si el explorador se cerró (se eligió algo).
    pub fn activate_entry(&mut self, cfg: &Config, i: usize) -> bool {
        let Some(e) = self.explorer_entries.get(i).cloned() else {
            return false;
        };
        if e.is_dir {
            self.navigate_to(e.path);
            false
        } else {
            self.select_core(cfg, e.path);
            true
        }
    }

    /// «Esta carpeta»: se elige la que se está mirando. Una carpeta se elige
    /// **estando adentro** y no señalándola desde afuera, que es lo que evita
    /// la duda de si «Mega Drive» seleccionado significa esa carpeta o la que
    /// la contiene.
    pub fn select_current_folder(&mut self) -> bool {
        if !self.browsing || !self.explorer_folder_mode {
            return false;
        }
        let d = self.explorer_dir.clone();
        self.add_folder(d);
        self.browsing = false;
        true
    }

    // -- las acciones del paso ------------------------------------------------

    /// Qué dice el botón secundario de este paso, o vacío si no hay ninguno.
    ///
    /// La etiqueta y la acción salen del **mismo** `match` (`secundaria()` está
    /// justo abajo y las prueba juntas un test). Antes vivían en dos lugares
    /// distintos —el texto en `main.rs`, la acción en otro `match`— y se
    /// desincronizaron enseguida: el paso del core ofrecía «Elegir core» y el de
    /// carpetas vacío «Agregar carpeta», y ninguno de los dos hacía nada.
    pub fn secondary_action_label(&self) -> String {
        if self.browsing {
            return String::new();
        }
        match self.step {
            // Agregar es «A»; el secundario saca la que está enfocada. Con la
            // lista vacía no hay nada que sacar y el botón no aparece.
            Step::Folders if !self.folders.is_empty() => tr!("Quitar"),
            _ => String::new(),
        }
    }

    /// ¿El core elegido sirve? Una pregunta y no tres `matches!` repartidos:
    /// de ella dependen qué dice «A», qué dice «X» y si el paso traba.
    pub fn core_is_usable(&self) -> bool {
        matches!(&self.verdict, Some(v) if v.is_usable())
    }

    /// Qué dice «A» en este paso.
    ///
    /// #627 — con el diseño nuevo «A» es CONTINUAR, y sólo se desvía cuando el
    /// paso todavía no tiene con qué continuar: sin carpetas, sin core, sin
    /// aceptar. Es el mismo `match` que `activar()` —están pegados a propósito—
    /// porque una etiqueta que se escribe lejos de la acción termina anunciando
    /// otra cosa.
    pub fn primary_action_label(&self) -> String {
        match self.step {
            Step::Folders if self.folders.is_empty() => tr!("Agregar carpeta"),
            Step::Core if !self.core_is_usable() => tr!("Elegir core"),
            Step::Terms if !self.terms_accepted => tr!("Aceptar"),
            Step::Summary => tr!("Comenzar"),
            // #599 — desde Ajustes, «A» guarda y vuelve. Anunciar «Continuar»
            // prometería un recorrido que no existe, y el jugador que entró a
            // cambiar una carpeta esperaría cuatro pantallas más.
            _ if self.single_step_mode => tr!("Guardar"),
            _ => tr!("Continuar"),
        }
    }

    /// Qué dice «X»: la acción del paso cuando «A» ya está ocupado en
    /// continuar. Vacío cuando no hay ninguna, o cuando «A» ya hace eso mismo
    /// —anunciar dos botones para una sola acción es un pie que sobra.
    pub fn step_action_label(&self) -> String {
        match self.step {
            Step::Folders if !self.folders.is_empty() => tr!("Agregar"),
            Step::Core if self.core_is_usable() => tr!("Elegir otro"),
            _ => String::new(),
        }
    }

    /// Ejecuta lo que `etiqueta_secundaria()` promete.
    pub fn secondary_action(&mut self, row: usize) {
        if self.browsing {
            return;
        }
        if self.step == Step::Folders {
            self.remove_folder(row);
        }
    }

    /// «A» en este paso. Devuelve `true` cuando lo que «A» hace es CONTINUAR.
    ///
    /// Devuelve en vez de avanzar solo porque el final del wizard —guardar la
    /// configuración y salir a escanear— lo sabe el llamador y no este módulo:
    /// duplicarlo acá sería tener dos finales.
    ///
    /// #627 — antes «A» actuaba sobre el paso y «Start» continuaba. El diseño
    /// nuevo pone «A Continuar» en el pie de las seis pantallas, así que A
    /// continúa y sólo se desvía cuando el paso no tiene con qué: ahí hace lo
    /// único que destraba (agregar la carpeta, elegir el core, aceptar), que es
    /// lo que su etiqueta dice.
    pub fn activate(&mut self, cfg: &Config, row: usize) -> bool {
        if self.browsing {
            self.activate_entry(cfg, row);
            return false;
        }
        match self.step {
            Step::Folders if self.folders.is_empty() => {
                self.open_explorer(true);
                false
            }
            Step::Core if !self.core_is_usable() => {
                self.open_explorer(false);
                false
            }
            Step::Terms if !self.terms_accepted => {
                self.terms_accepted = true;
                false
            }
            _ => true,
        }
    }

    /// «X»: la acción del paso, la que `etiqueta_x()` anuncia.
    ///
    /// Funciona aunque el pie no la anuncie (con la lista vacía «A» ya agrega):
    /// el botón «Agregar carpeta» de la pantalla llama acá, y un botón visible
    /// que no responde es peor que un atajo que no se anuncia.
    pub fn step_action(&mut self) {
        // Explorando, «X» es «usar esta carpeta»: elegir aquella en la que uno
        // está parado, sin tener que buscar una fila que diga «ésta». Es lo que
        // el pie anuncia, y vive acá —y no en el ruteo ni en el `.slint`— para
        // que el mando y el teclado no puedan hacer cosas distintas con el
        // mismo botón. (Lo hacían: el mando elegía la carpeta y la tecla `x` no
        // hacía nada.)
        if self.browsing {
            self.select_current_folder();
            return;
        }
        match self.step {
            Step::Folders => self.open_explorer(true),
            Step::Core => self.open_explorer(false),
            Step::Welcome | Step::Gamepads | Step::Terms | Step::Summary => {}
        }
    }

    /// Los botones que este paso usa, para el pie de pantalla: `(tecla, qué
    /// hace)`.
    ///
    /// #627 — PARES y no una frase. El pie del diseño dibuja cada botón como su
    /// PadGlyph —la píldora clara con la letra— seguido de lo que hace, así que
    /// la tecla es un DATO y la etiqueta es el único texto que se traduce.
    /// Antes el pie era una frase entera («A agregar carpeta   Start continuar
    /// B volver») y el traductor tenía que arrastrar las letras de los botones
    /// adentro del idioma — un «B back» en la mitad de una línea en español.
    ///
    /// Sin las flechas: moverse con el stick o la cruceta es la gramática del
    /// chasis y el pie del diseño no la enumera en ninguna pantalla. Lo que el
    /// pie anuncia es lo que NO se adivina.
    pub fn hints(&self) -> Vec<(String, String)> {
        if self.browsing {
            return if self.explorer_folder_mode {
                vec![
                    ("A".into(), tr!("Entrar")),
                    ("X".into(), tr!("Usar esta carpeta")),
                    ("B".into(), tr!("Cancelar")),
                ]
            } else {
                vec![("A".into(), tr!("Elegir")), ("B".into(), tr!("Cancelar"))]
            };
        }
        let mut v = vec![("A".to_string(), self.primary_action_label())];
        let x = self.step_action_label();
        if !x.is_empty() {
            v.push(("X".to_string(), x));
        }
        let y = self.secondary_action_label();
        if !y.is_empty() {
            v.push(("Y".to_string(), y));
        }
        // «B» en la portada SALE de Play, y en los demás pasos vuelve. Es la
        // única excepción a «Atrás nunca cierra la aplicación» (§9) y la trae el
        // diseño: en la primera pantalla del primer arranque no hay a dónde
        // volver, no hay nada guardado que perder, y un launcher de televisor
        // del que no se puede salir con el mando es peor que la regla.
        v.push((
            "B".to_string(),
            if self.step == Step::Welcome {
                tr!("Salir")
            } else {
                tr!("Volver")
            },
        ));
        v
    }

    /// Las filas del paso 6, tal como las dibuja `play-ce.pen`.
    pub fn summary(&self) -> Vec<(String, String)> {
        let language = language_name(&self.language);
        let folders = tr!("{} · {} juegos", self.folders.len(), self.total_roms());
        let core = match &self.verdict {
            Some(Verdict::Valid(i)) => {
                let p: Vec<String> = i.platforms().into_iter().collect();
                if p.is_empty() {
                    i.name.clone()
                } else {
                    p.join(", ")
                }
            }
            _ => tr!("Sin core"),
        };
        // TRES filas y no cuatro. La cuarta era «Hub y cuenta — Omitidos, modo
        // local», que en AYTHER Play tiene sentido: ahí el Hub existe y el
        // wizard cuenta que lo salteó. Acá nombrarlo sería anunciar algo que
        // esta edición no tiene, en la primera pantalla que el jugador ve.
        vec![
            (tr!("Idioma"), language),
            (tr!("Carpetas"), folders),
            (tr!("Core"), core),
        ]
    }

    /// Vuelca lo elegido en la configuración, sellando la aceptación con la
    /// hora de ahora. El llamador guarda.
    pub fn apply(&self, cfg: &mut Config) {
        self.apply_at(cfg, &crate::date::now_utc());
    }

    /// Lo mismo, con la fecha dada. Existe para que el registro de aceptación
    /// se pueda probar: un test que dependiera del reloj sólo podría afirmar
    /// que la fecha «parece una fecha», que es justo lo que no alcanza para un
    /// registro que tiene que sobrevivir al jugador.
    ///
    /// Este es el primer escritor de `play_config.toml` que existe en Play.
    pub fn apply_at(&self, cfg: &mut Config, timestamp: &str) {
        cfg.language = self.language.clone();
        cfg.rom_dirs = self.folders.clone();
        // `rom_dir` sigue existiendo y apunta a la primera: es lo que leen las
        // partes que todavía piensan en una sola carpeta. Dejarlo desactualizado
        // haría que Play escanee una carpeta que el jugador ya quitó.
        if let Some(first_item) = self.folders.first() {
            cfg.rom_dir = first_item.clone();
        }
        if let Some(core) = &self.core {
            cfg.default_core = core.clone();
            // Y el core queda anotado para cada plataforma que declara servir:
            // es lo que hace que una biblioteca de varias consolas arranque sin
            // volver a preguntar (#589).
            if let Some(Verdict::Valid(info)) = &self.verdict {
                for p in info.platforms() {
                    cfg.cores.insert(p, core.clone());
                }
            }
        }
        // La aceptación se sella con fecha, y la fecha de una aceptación que ya
        // valía NO se pisa: re-correr el wizard —para cambiar una carpeta, por
        // ejemplo— no es volver a aceptar nada, y mover la fecha borraría el
        // único dato que dice cuándo pasó de verdad.
        if self.terms_accepted && !terms_are_current(cfg) {
            cfg.terms_accepted = Some(TERMS_VERSION.to_string());
            cfg.terms_date = Some(timestamp.to_string());
        }
        cfg.onboarding_complete = true;
    }
}

/// ¿Hay que mostrar el onboarding al arrancar?
///
/// No alcanza con «ya se corrió una vez»: una configuración que apunta a un
/// core que se borró o se movió deja a Play sin poder lanzar nada, y el error
/// aparecería recién al apretar Jugar. En ese caso el wizard vuelve, pero
/// **directo al paso del core**: rehacer los seis pasos para arreglar uno es
/// castigar al jugador por un archivo que movió.
pub fn required_step(cfg: &Config) -> Option<Step> {
    if !cfg.onboarding_complete {
        return Some(Step::Welcome);
    }
    if !terms_are_current(cfg) {
        return Some(Step::Terms);
    }
    if !cfg.default_core.exists() {
        return Some(Step::Core);
    }
    if crate::config::rom_folders(cfg).iter().all(|d| !d.exists()) {
        return Some(Step::Folders);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Una fecha fija: el registro de aceptación se prueba con la fecha
    /// puesta a mano, no con el reloj. Un test que dependiera de la hora sólo
    /// podría afirmar que la fecha «parece una fecha».
    const WHEN: &str = "2026-08-26T22:14:03Z";

    fn cfg() -> Config {
        let mut c = Config::defaults();
        c.onboarding_complete = true;
        c.terms_accepted = Some(TERMS_VERSION.into());
        c.terms_date = Some(WHEN.into());
        c
    }

    fn ready() -> State {
        let mut e = State::default();
        e.add_folder(std::env::temp_dir());
        e.terms_accepted = true;
        e.verdict = Some(Verdict::Valid(crate::core_probe::CoreInfo {
            api: 1,
            name: "Genesis Plus GX".into(),
            version: "v1".into(),
            extensions: vec!["md".into(), "sms".into()],
        }));
        e.core = Some(PathBuf::from("core.dll"));
        e
    }

    #[test]
    fn the_steps_go_back_and_forth_without_skipping() {
        let mut e = ready();
        let mut seen = vec![e.step];
        while e.advance() {
            seen.push(e.step);
        }
        assert_eq!(seen.len(), STEPS, "recorrido: {seen:?}");
        assert_eq!(e.step, Step::Summary);
        while e.go_back() {}
        assert_eq!(e.step, Step::Welcome);
    }

    /// La regla del chasis: «Atrás» nunca cierra la aplicación. En el primer
    /// paso del wizard no hay a dónde volver, y devolver «cerrar» sería la
    /// única forma de que el primer arranque termine en un escritorio.
    #[test]
    fn back_on_the_first_step_closes_nothing() {
        let mut e = State::default();
        assert!(!e.go_back());
        assert_eq!(e.step, Step::Welcome);
    }

    /// Y cierra primero el explorador: si «Atrás» saltara al paso anterior con
    /// el explorador abierto, el jugador perdería el paso por querer cerrar una
    /// lista.
    #[test]
    fn back_closes_the_explorer_before_the_step() {
        let mut e = ready();
        e.step = Step::Core;
        e.browsing = true;
        assert!(e.go_back());
        assert!(!e.browsing);
        assert_eq!(e.step, Step::Core);
    }

    #[test]
    fn no_advancing_without_folders_and_the_reason_is_readable() {
        let mut e = State {
            step: Step::Folders,
            ..Default::default()
        };
        let t = e.blocker().expect("debería estar trabado");
        assert!(t.len() > 15, "el motivo no dice qué hacer: «{t}»");
        assert!(!e.advance());
        e.add_folder(std::env::temp_dir());
        assert!(e.blocker().is_none());
        assert!(e.advance());
    }

    /// Un core que no valida traba el paso, y el motivo que se muestra es el
    /// remedio del veredicto — no un «no se puede continuar» genérico.
    #[test]
    fn an_invalid_core_jams_the_step_with_its_own_remedy() {
        let mut e = ready();
        e.step = Step::Core;
        e.verdict = Some(Verdict::ForeignArchitecture {
            core: crate::core_probe::Architecture::X86,
            runtime: crate::core_probe::Architecture::X64,
        });
        let t = e.blocker().unwrap();
        assert!(t.contains("x64"), "{t}");
        assert!(!e.advance());
    }

    #[test]
    fn without_accepting_the_terms_there_is_no_finishing() {
        let mut e = ready();
        e.step = Step::Terms;
        e.terms_accepted = false;
        assert!(e.blocker().is_some());
        e.terms_accepted = true;
        assert!(e.advance());
    }

    /// El mando NO es obligatorio: quien juega con teclado tiene que poder
    /// terminar. Es lo contrario del resto del wizard y por eso se fija.
    #[test]
    fn finishing_without_a_gamepad_is_possible() {
        let mut e = ready();
        e.step = Step::Gamepads;
        e.gamepad_name = String::new();
        assert!(e.can_advance());
    }

    // -- lo que el paso anuncia y lo que el paso hace ------------------------

    /// Un botón secundario que no hace nada es peor que no tenerlo: el jugador
    /// lo aprieta, no pasa nada y deja de confiar en los otros. Este test
    /// recorre los seis pasos y exige que, si hay etiqueta, apretarla CAMBIE el
    /// estado. Es el oráculo del defecto que tenía: el paso del core ofrecía
    /// «Elegir core» y el de carpetas vacío «Agregar carpeta», y los dos eran
    /// adornos —la etiqueta se escribía en `main.rs` y la acción en otro lado.
    #[test]
    fn the_secondary_button_does_what_it_promises() {
        for i in 0..STEPS {
            let mut e = ready();
            e.step = Step::from_index(i);
            let label = e.secondary_action_label();
            if label.is_empty() {
                continue;
            }
            let before = e.clone();
            e.secondary_action(0);
            assert_ne!(e, before, "«{label}» en {:?} no hizo nada", before.step);
        }
    }

    /// Cada tecla que el pie anuncia HACE algo (#627).
    ///
    /// Con los hints en pares, el oráculo se puede escribir como corresponde:
    /// se recorre el pie tecla por tecla y se aprieta cada una. Antes era un
    /// `contains("A ")` sobre una frase, que sólo podía preguntar por una.
    #[test]
    fn every_announced_key_does_something() {
        let cfg = cfg();
        for i in 0..STEPS {
            let base = {
                let mut e = ready();
                e.step = Step::from_index(i);
                e
            };
            for (key, label) in base.hints() {
                let mut e = base.clone();
                let before = e.clone();
                let performed_action = match key.as_str() {
                    // «A» o cambia el estado o continúa: las dos son hacer algo.
                    "A" => e.activate(&cfg, 0) || e != before,
                    "X" => {
                        e.step_action();
                        e != before
                    }
                    "Y" => {
                        e.secondary_action(0);
                        e != before
                    }
                    // En la portada «B» sale de Play: no cambia el estado del
                    // wizard y por eso no se le puede pedir que lo cambie.
                    "B" => {
                        let r = e.go_back();
                        r || e.step == Step::Welcome
                    }
                    other_key => panic!("el pie anuncia «{other_key}», que nadie rutea"),
                };
                assert!(
                    performed_action,
                    "{:?}: el pie dice «{key} {label}» y no hace nada",
                    before.step
                );
            }
        }
    }

    /// Y lo mismo con el pie del EXPLORADOR, que anuncia otras tres teclas.
    ///
    /// El defecto que cierra: el pie decía «X Usar esta carpeta», el mando la
    /// elegía y la tecla `x` no hacía nada — dos gramáticas para el mismo botón
    /// anunciado, y la del teclado era la muda.
    #[test]
    fn every_explorer_key_does_something() {
        let cfg = cfg();
        let mut base = ready();
        base.step = Step::Folders;
        base.open_explorer(true);
        assert!(base.browsing, "no se pudo abrir el explorador");

        for (key, label) in base.hints() {
            let mut e = base.clone();
            let before = e.clone();
            let performed_action = match key.as_str() {
                "A" => {
                    e.activate(&cfg, 0);
                    e != before
                }
                "X" => {
                    e.step_action();
                    e != before
                }
                "B" => e.go_back(),
                other_key => {
                    panic!("el pie del explorador anuncia «{other_key}», que nadie rutea")
                }
            };
            assert!(
                performed_action,
                "explorando: el pie dice «{key} {label}» y no hace nada"
            );
        }
    }

    /// Lo que «A» y «X» prometen es lo que hacen: las etiquetas salen del mismo
    /// `match` que las acciones, y este test las lee juntas. El defecto que
    /// cierra: el paso del core ofrecía «Elegir core» y el de carpetas vacío
    /// «Agregar carpeta», y los dos eran adornos.
    #[test]
    fn the_label_of_a_says_what_a_does() {
        let cfg = cfg();
        for i in 0..STEPS {
            let mut e = ready();
            e.step = Step::from_index(i);
            let label = e.primary_action_label();
            let advances = e.clone().activate(&cfg, 0);
            // «Continuar» y «Comenzar» son las dos formas de seguir; cualquier
            // otra etiqueta promete una acción, y entonces A no puede avanzar.
            let promises_to_continue = label == tr!("Continuar") || label == tr!("Comenzar");
            assert_eq!(
                advances,
                promises_to_continue,
                "{:?}: el pie dice «A {label}» y A {}",
                e.step,
                if advances {
                    "continúa"
                } else {
                    "hace otra cosa"
                }
            );
        }
    }

    /// El pie nunca queda vacío, ni en el paso ni en el explorador: una
    /// pantalla sin botones anunciados deja al jugador probando teclas. Y «B»
    /// está SIEMPRE: es la salida, y una pantalla sin salida anunciada es la
    /// que deja a alguien apagando el televisor.
    #[test]
    fn no_step_is_left_without_a_footer() {
        for i in 0..STEPS {
            let mut e = ready();
            e.step = Step::from_index(i);
            let keys: Vec<String> = e.hints().into_iter().map(|(t, _)| t).collect();
            assert!(keys.contains(&"A".to_string()), "{:?}: {keys:?}", e.step);
            assert!(keys.contains(&"B".to_string()), "{:?}: {keys:?}", e.step);
            for (_, label) in e.hints() {
                assert!(!label.is_empty(), "{:?}: una tecla sin etiqueta", e.step);
            }
            e.browsing = true;
            assert!(!e.hints().is_empty());
        }
    }

    /// El idioma rota en las dos direcciones e incluye «el del sistema», que es
    /// lo que trae una instalación nueva. Sin él en el ciclo, el paso mostraba
    /// «Español» sobre una configuración que decía `auto`: en una máquina en
    /// inglés, el wizard decía una cosa y la biblioteca hacía otra.
    #[test]
    fn the_language_rotates_and_the_system_one_is_in_the_cycle() {
        let mut e = State::default();
        assert_eq!(
            e.language, "auto",
            "una instalación nueva no arranca en `auto`"
        );
        let mut seen = vec![e.language.clone()];
        for _ in 1..LANGUAGES.len() {
            e.cycle_language(1);
            seen.push(e.language.clone());
        }
        seen.sort();
        assert_eq!(seen, vec!["auto", "en", "es"]);
        // Y vuelve al principio.
        e.cycle_language(1);
        assert_eq!(e.language, "auto");
        // Para atrás también.
        e.cycle_language(-1);
        assert_eq!(e.language, "en");
    }

    /// El resumen dice el idioma que se va a aplicar, no uno inventado.
    #[test]
    fn the_summary_does_not_lie_about_the_language() {
        let mut e = ready();
        e.language = "auto".into();
        assert_eq!(e.summary()[0].1, language_name("auto"));
        e.language = "en".into();
        assert_eq!(e.summary()[0].1, language_name("en"));
    }

    #[test]
    fn a_folder_is_not_added_twice() {
        let mut e = State::default();
        let d = std::env::temp_dir();
        e.add_folder(d.clone());
        e.add_folder(d);
        assert_eq!(e.folders.len(), 1);
        assert_eq!(e.roms_per_folder.len(), 1);
    }

    /// Quitar una carpeta tiene que llevarse su conteo: si los dos vectores se
    /// desalinean, el resumen atribuye los juegos de una carpeta a otra.
    #[test]
    fn removing_a_folder_does_not_misalign_the_counts() {
        let mut e = State {
            folders: vec!["a".into(), "b".into(), "c".into()],
            ..Default::default()
        };
        e.roms_per_folder = vec![1, 2, 3];
        e.remove_folder(1);
        assert_eq!(e.folders, vec![PathBuf::from("a"), PathBuf::from("c")]);
        assert_eq!(e.roms_per_folder, vec![1, 3]);
        assert_eq!(e.total_roms(), 4);
        // Y un índice de más no puede hacer panic: viene de una lista de la UI.
        e.remove_folder(99);
        assert_eq!(e.folders.len(), 2);
    }

    #[test]
    fn the_summary_has_the_three_rows_from_the_design() {
        let r = ready().summary();
        assert_eq!(r.len(), 3);
        for (k, v) in &r {
            assert!(!k.is_empty() && !v.is_empty(), "fila vacía: {k:?} = {v:?}");
        }
    }

    /// Aplicar deja la config lista para lanzar: idioma, carpetas, core, y el
    /// core anotado para CADA plataforma que sirve.
    #[test]
    fn apply_leaves_the_config_ready_to_launch() {
        let mut c = cfg();
        let mut e = ready();
        e.language = "en".into();
        e.core = Some(PathBuf::from("C:/cores/gpgx.dll"));
        e.apply_at(&mut c, WHEN);

        assert_eq!(c.language, "en");
        assert_eq!(c.rom_dirs.len(), 1);
        assert_eq!(
            c.rom_dir, e.folders[0],
            "rom_dir quedó apuntando a otro lado"
        );
        assert_eq!(c.default_core, PathBuf::from("C:/cores/gpgx.dll"));
        assert_eq!(
            c.cores.get("Mega Drive"),
            Some(&PathBuf::from("C:/cores/gpgx.dll"))
        );
        assert_eq!(
            c.cores.get("Master System"),
            Some(&PathBuf::from("C:/cores/gpgx.dll"))
        );
        assert!(
            !c.cores.contains_key("SNES"),
            "se anotó un core para una consola que no sirve"
        );
        assert_eq!(c.terms_accepted.as_deref(), Some(TERMS_VERSION));
        assert!(c.onboarding_complete);
    }

    /// El wizard completo, de punta a punta: se recorren los seis pasos y al
    /// final el archivo EXISTE y dice lo que el jugador eligió.
    ///
    /// Es la razón de ser de #588. Antes, `Config::save()` no tenía un solo
    /// llamador: la única manera de configurar Play era editar TOML a mano, y
    /// nada probaba que ese archivo se pudiera escribir.
    #[test]
    fn walking_the_wizard_writes_the_config_file() {
        let tmp = tempfile::tempdir().unwrap();
        let destination = tmp.path().join("play_config.toml");

        // Paso 1: idioma.
        let mut e = State {
            language: "en".into(),
            ..Default::default()
        };
        assert!(e.advance()); // → mandos
        assert!(e.advance()); // → carpetas (el mando es opcional)
        // Paso 3: una carpeta de verdad.
        e.add_folder(tmp.path().to_path_buf());
        assert!(e.advance()); // → core
        // Paso 4: un core que valida. El archivo se crea de verdad porque
        // `hace_falta` comprueba que siga existiendo, y esa comprobación es
        // parte de lo que este test verifica.
        let core = tmp.path().join("gpgx.dll");
        std::fs::write(&core, b"MZ").unwrap();
        e.core = Some(core.clone());
        e.verdict = Some(Verdict::Valid(crate::core_probe::CoreInfo {
            api: 1,
            name: "Genesis Plus GX".into(),
            version: "v1".into(),
            extensions: vec!["md".into()],
        }));
        assert!(e.advance()); // → términos
        // Paso 5: aceptar.
        assert!(!e.advance(), "se pudo pasar los términos sin aceptarlos");
        e.terms_accepted = true;
        assert!(e.advance()); // → resumen
        assert_eq!(e.step, Step::Summary);

        let mut c = Config::defaults();
        e.apply_at(&mut c, WHEN);
        c.save_to(&destination);

        assert!(destination.exists(), "el wizard terminó y no escribió nada");
        let text = std::fs::read_to_string(&destination).unwrap();
        let read_back: Config =
            toml::from_str(&text).expect("lo que escribió no se puede volver a leer");
        assert_eq!(read_back.language, "en");
        assert_eq!(read_back.rom_dirs, vec![tmp.path().to_path_buf()]);
        assert_eq!(read_back.default_core, core);
        assert_eq!(read_back.cores.get("Mega Drive"), Some(&core));
        assert_eq!(read_back.terms_accepted.as_deref(), Some(TERMS_VERSION));
        assert_eq!(read_back.terms_date.as_deref(), Some(WHEN));
        assert!(read_back.onboarding_complete);
        // Y con eso, el wizard no vuelve a aparecer.
        assert_eq!(required_step(&read_back), None);
    }

    // -- #590: los términos, su texto y su registro --------------------------

    /// **La regla de la spec (§3, §11): legalidad visible, sin enlaces a
    /// fuentes no autorizadas.** El texto de los términos no puede llevar a
    /// ningún lado a buscar una ROM o un core, ni con una URL, ni con un
    /// dominio suelto. Nadie va a estar mirando el día que alguien agregue uno
    /// «para ayudar al jugador»; por eso lo mira un test.
    #[test]
    fn the_terms_lead_to_no_source() {
        const TRACES: &[&str] = &["http", "://", "www.", ".com", ".net", ".org", ".io", "@"];
        for p in terms_paragraphs() {
            let lowercase_paragraph = p.to_lowercase();
            for r in TRACES {
                assert!(
                    !lowercase_paragraph.contains(r),
                    "los términos contienen «{r}»: {p}"
                );
            }
            assert!(p.len() > 40, "un párrafo que no dice nada: «{p}»");
        }
    }

    /// Y dicen las dos reglas que el jugador está aceptando. Un texto legal que
    /// no nombra lo que rige es un texto que se marca sin leer.
    #[test]
    fn the_text_names_the_two_rules_that_govern() {
        let complete_text = terms_paragraphs().join(" ");
        assert!(complete_text.contains("BYOR"), "{complete_text}");
        assert!(complete_text.contains("BYOC"), "{complete_text}");
        assert!(terms_paragraphs().len() >= 4);
    }

    /// La aceptación se guarda con versión Y fecha. Sin fecha no es un
    /// registro: no se puede decir cuándo se aceptó.
    #[test]
    fn accepting_leaves_version_and_date() {
        let mut c = Config::defaults();
        let mut e = ready();
        e.terms_accepted = true;
        e.apply_at(&mut c, WHEN);
        assert_eq!(c.terms_accepted.as_deref(), Some(TERMS_VERSION));
        assert_eq!(c.terms_date.as_deref(), Some(WHEN));
        assert!(terms_are_current(&c));
    }

    /// Sin aceptar no se escribe nada: `onboarding_completo` puede quedar en
    /// true —el wizard se recorrió— pero el registro no existe, y por eso el
    /// paso vuelve a aparecer en el próximo arranque.
    #[test]
    fn without_accepting_nothing_is_recorded() {
        let mut c = Config::defaults();
        let mut e = ready();
        e.terms_accepted = false;
        e.apply_at(&mut c, WHEN);
        assert_eq!(c.terms_accepted, None);
        assert_eq!(c.terms_date, None);
        assert_eq!(required_step(&c), Some(Step::Terms));
    }

    /// Re-correr el wizard —para cambiar una carpeta, por ejemplo— NO es
    /// volver a aceptar: la fecha de la aceptación original se queda donde
    /// está. Moverla borraría el único dato que dice cuándo pasó de verdad.
    #[test]
    fn re_running_the_wizard_does_not_stomp_the_original_date() {
        let mut c = cfg();
        let e = State::of(&c);
        assert!(e.terms_accepted, "una aceptación vigente se vuelve a pedir");
        assert_eq!(e.terms_accepted_at.as_deref(), Some(WHEN));
        e.apply_at(&mut c, "2027-01-01T00:00:00Z");
        assert_eq!(
            c.terms_date.as_deref(),
            Some(WHEN),
            "se pisó la fecha original"
        );
    }

    /// Unos términos nuevos, en cambio, sí sellan fecha nueva: es otra
    /// aceptación, de otro texto.
    #[test]
    fn new_terms_renew_the_date() {
        let mut c = cfg();
        c.terms_accepted = Some("0.9".into());
        assert_eq!(required_step(&c), Some(Step::Terms));
        let e = State::of(&c);
        assert!(
            !e.terms_accepted,
            "una aceptación de otra versión se dio por buena"
        );
        assert_eq!(
            e.terms_accepted_at, None,
            "se mostraría la fecha de un texto que ya no rige"
        );

        let mut e = e;
        e.terms_accepted = true;
        e.apply_at(&mut c, "2027-01-01T00:00:00Z");
        assert_eq!(c.terms_accepted.as_deref(), Some(TERMS_VERSION));
        assert_eq!(c.terms_date.as_deref(), Some("2027-01-01T00:00:00Z"));
    }

    /// Un registro a medias —versión sin fecha, o al revés— vuelve a pedir los
    /// términos. Es el lado seguro del error: molesta una vez, en vez de dar
    /// por aceptado algo que no se puede probar que se aceptó. Pasa de verdad:
    /// un `play_config.toml` escrito por #588, antes de que hubiera fecha,
    /// tiene exactamente esa forma.
    #[test]
    fn a_half_done_record_asks_for_the_terms_again() {
        let mut c = cfg();
        c.terms_date = None;
        assert!(!terms_are_current(&c));
        assert_eq!(required_step(&c), Some(Step::Terms));

        // Y una fecha vacía no cuenta como fecha.
        c.terms_date = Some(String::new());
        assert!(!terms_are_current(&c));

        c.terms_date = Some(WHEN.into());
        c.terms_accepted = None;
        assert!(!terms_are_current(&c));
    }

    // -- cuándo vuelve el wizard --------------------------------------------

    #[test]
    fn the_first_time_it_shows_whole() {
        let mut c = cfg();
        c.onboarding_complete = false;
        assert_eq!(required_step(&c), Some(Step::Welcome));
    }

    /// Un core que se borró o se movió no puede descubrirse al apretar Jugar.
    /// Y volver al paso del core, no al primero: rehacer los seis pasos para
    /// arreglar uno es castigar al jugador por haber movido un archivo.
    #[test]
    fn if_the_core_is_gone_it_goes_back_to_the_core_step() {
        let mut c = cfg();
        c.default_core = PathBuf::from("Z:/se/borro.dll");
        assert_eq!(required_step(&c), Some(Step::Core));
    }

    #[test]
    fn new_terms_are_shown_again() {
        let mut c = cfg();
        c.default_core = std::env::current_exe().unwrap();
        c.rom_dirs = vec![std::env::temp_dir()];
        assert_eq!(required_step(&c), None);
        c.terms_accepted = Some("0.9".into());
        assert_eq!(required_step(&c), Some(Step::Terms));
    }

    #[test]
    fn with_everything_in_place_the_wizard_does_not_bother() {
        let mut c = cfg();
        c.default_core = std::env::current_exe().unwrap();
        c.rom_dirs = vec![std::env::temp_dir()];
        assert_eq!(required_step(&c), None);
    }

    /// #599 — abierto desde Ajustes para editar UN paso, «A» dice GUARDAR y
    /// «B» no retrocede: no hay paso anterior al que volver, el anterior es
    /// Ajustes. Sin esto, quien entra a cambiar una carpeta termina en el paso
    /// del mando de un wizard que nunca abrió.
    #[test]
    fn from_settings_a_step_is_edited_and_saved() {
        let mut c = cfg();
        c.rom_dirs = vec![std::env::temp_dir()];
        let mut e = State::of(&c);
        e.step = Step::Folders;
        e.single_step_mode = true;

        assert_eq!(
            e.primary_action_label(),
            "Guardar",
            "«Continuar» prometería un recorrido"
        );
        assert!(!e.go_back(), "no hay paso anterior: lo cierra el llamador");
        assert_eq!(
            e.step,
            Step::Folders,
            "«B» no puede mudar al jugador de paso"
        );

        // Y el explorador sigue cerrándose con «B», que es lo de siempre: la
        // regla nueva es sobre el PASO, no sobre lo que se abre encima.
        e.open_explorer(true);
        assert!(e.go_back());
        assert!(!e.browsing);
        assert_eq!(e.step, Step::Folders);
    }

    /// Y sin el modo, nada cambia: el wizard del primer arranque sigue
    /// continuando y retrocediendo como antes.
    #[test]
    fn the_normal_wizard_does_not_notice() {
        let mut c = cfg();
        c.rom_dirs = vec![std::env::temp_dir()];
        let mut e = State::of(&c);
        e.step = Step::Folders;
        assert_eq!(e.primary_action_label(), "Continuar");
        assert!(e.go_back());
        assert_eq!(e.step, Step::Gamepads);
    }

    /// El ciclo de idiomas es UNO: el del wizard y el de Ajustes son la misma
    /// lista, y por eso no pueden ofrecer opciones distintas.
    #[test]
    fn the_language_cycle_is_the_same_for_both() {
        let mut e = State::default();
        for _ in 0..LANGUAGES.len() * 2 {
            let expected = cycle_language(&e.language, 1);
            e.cycle_language(1);
            assert_eq!(e.language, expected);
        }
        // Rota en los dos sentidos y vuelve al mismo lugar.
        assert_eq!(cycle_language(&cycle_language("es", 1), -1), "es");
        // Un código que no está en la lista no traba el ciclo.
        assert!(LANGUAGES.contains(&cycle_language("xx", 1).as_str()));
    }

    /// «ATRÁS NUNCA CIERRA PLAY» salvo en la portada del PRIMER arranque.
    ///
    /// Re-abrir la bienvenida desde Ajustes deja al jugador en esa misma
    /// portada con su biblioteca detrás, y ahí «B» cerraba Play de golpe: la
    /// justificación de la excepción —«no hay a dónde volver ni nada guardado
    /// que perder»— es falsa en cuanto hay algo configurado.
    #[test]
    fn back_only_closes_play_on_first_boot() {
        let mut c = cfg();
        c.rom_dirs = vec![std::env::temp_dir()];

        // Primer arranque de verdad: no hay nada configurado.
        c.onboarding_complete = false;
        let e = State::of(&c);
        assert!(e.step == Step::Welcome && e.first_run);
        assert!(
            e.back_closes_app(),
            "en la portada del primer arranque, B sale"
        );

        // Re-abierto desde Ajustes, con todo ya configurado: NO cierra.
        c.onboarding_complete = true;
        let e2 = State::of(&c);
        assert!(
            !e2.back_closes_app(),
            "con biblioteca detrás, «B» no puede cerrar Play"
        );

        // Y en los otros pasos tampoco, ni con el explorador encima.
        let mut e3 = State::of(&c);
        e3.first_run = true;
        e3.step = Step::Folders;
        assert!(!e3.back_closes_app(), "fuera de la portada, B vuelve");
        e3.step = Step::Welcome;
        e3.browsing = true;
        assert!(
            !e3.back_closes_app(),
            "con el explorador abierto, B lo cierra a él"
        );
        e3.browsing = false;
        e3.single_step_mode = true;
        assert!(
            !e3.back_closes_app(),
            "abierto desde Ajustes, B vuelve a Ajustes"
        );
    }

    /// Re-correrlo no obliga a rehacer lo que ya estaba bien.
    #[test]
    fn re_running_it_starts_with_what_was_already_there() {
        let mut c = cfg();
        c.language = "en".into();
        c.rom_dirs = vec![std::env::temp_dir()];
        c.default_core = std::env::current_exe().unwrap();
        let e = State::of(&c);
        assert_eq!(e.language, "en");
        assert_eq!(e.folders, vec![std::env::temp_dir()]);
        assert!(e.core.is_some());
        assert!(
            e.terms_accepted,
            "los términos ya aceptados se vuelven a pedir"
        );
    }
}
