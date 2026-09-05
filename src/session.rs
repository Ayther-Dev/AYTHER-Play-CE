//! session.rs — cerrar, recuperar y no lanzar dos veces la misma partida (#601).
//!
//! # Las tres cosas que faltaban
//!
//! #596 dejó el manifest: qué se lanzó. Lo que no había era el otro extremo —
//! qué pasó después— y sin eso tres situaciones quedan sin salida.
//!
//! **El crash.** El juego se cierra solo y Play dice «terminó de forma anormal».
//! Volver a apretar A relanza exactamente lo mismo, así que vuelve a fallar. El
//! camino es uno solo: el modo seguro, que es el mismo manifest **sin** lo que
//! pudo causarlo — sin pack, sin shaders, sin overrides. La ROM y los guardados
//! no se tocan, y eso se dice en el diálogo, porque «modo seguro» sin esa
//! promesa suena a que algo se va a perder.
//!
//! **La sesión huérfana.** Si Play muere con el juego abierto, la carpeta de la
//! sesión queda sin cerrar y su savestate queda ahí, sin dueño. Al arrancar hay
//! que encontrarla y ofrecer recuperarla o descartarla — descartar borra la
//! CARPETA DE SESIÓN y nada más.
//!
//! **El doble lanzamiento.** Dos instancias del mismo juego escriben en los
//! mismos guardados y la última en cerrar gana. El lock lo impide y el aviso
//! dice quién lo tiene.
//!
//! # Por qué el cierre es otro archivo
//!
//! El manifest es INMUTABLE (#596): se escribe una vez y describe una partida.
//! El cierre va en `exit.toml`, al lado. Escribirlo dentro del manifest lo
//! convertiría en un archivo que cambia mientras corre — y entonces ya no
//! describe la sesión, describe la última vez que alguien lo miró.
//!
//! Y por eso mismo «sin cerrar» es una condición del sistema de archivos y no
//! una bandera en memoria: es lo único que sobrevive a que Play deje de existir,
//! que es exactamente el caso que hay que detectar.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::launch_manifest::{self, LaunchManifest};

/// Cómo terminó una sesión. Vive en `<sesión>/exit.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionExit {
    /// Cuándo (UTC ISO-8601).
    #[serde(alias = "terminada")]
    pub ended_at: String,
    /// `true` = el proceso salió por su cuenta con código 0.
    #[serde(alias = "limpia")]
    pub clean_exit: bool,
    /// El código de salida, cuando lo hubo. `None` = lo mató una señal o no se
    /// pudo leer — que no es lo mismo que salir con 0 y hay que poder
    /// distinguirlo en el diagnóstico.
    #[serde(default, alias = "codigo")]
    pub exit_code: Option<i32>,
    /// El savestate que dejó, si dejó alguno.
    #[serde(default)]
    pub savestate: String,
    #[serde(default, alias = "segundos")]
    pub elapsed_seconds: u64,
}

/// Escribe el cierre de una sesión.
///
/// Atómico como el manifest: un `exit.toml` truncado no parsea, y una sesión
/// cuyo cierre no parsea se lee como huérfana para siempre — el jugador vería
/// el diálogo de recuperación en cada arranque por una partida que terminó
/// bien.
pub fn close(dir: &Path, c: &SessionExit) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let destination = dir.join("exit.toml");
    let tmp = dir.join("exit.toml.tmp");
    let text = toml::to_string_pretty(c)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, &destination)?;
    Ok(())
}

/// El cierre de una sesión, si lo tiene.
pub fn read_exit(dir: &Path) -> Option<SessionExit> {
    toml::from_str(&std::fs::read_to_string(dir.join("exit.toml")).ok()?).ok()
}

/// Una sesión que quedó sin cerrar.
#[derive(Debug, Clone)]
pub struct OrphanedSession {
    pub id: String,
    pub dir: PathBuf,
    pub manifest: LaunchManifest,
    /// El savestate de salida, si el runtime alcanzó a escribirlo. Es lo que
    /// hace que «recuperar» valga la pena: sin él, recuperar es sólo volver a
    /// lanzar el mismo juego.
    pub savestate: Option<PathBuf>,
}

/// Las sesiones sin cerrar que quedaron en `<datos>/sessions/`.
///
/// Ordenadas de la más reciente a la más vieja: si hay varias, la que le
/// importa al jugador es la última, y una lista en orden de directorio pondría
/// primero la que el sistema de archivos quiera.
pub fn orphaned_sessions(data_dir: &Path) -> Vec<OrphanedSession> {
    let root = data_dir.join("sessions");
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Vec::new();
    };
    let mut v: Vec<OrphanedSession> = Vec::new();
    for e in entries.flatten() {
        let dir = e.path();
        if !dir.is_dir() {
            continue;
        }
        // Un manifest que no se entiende NO es una huérfana: es una sesión que
        // este build no puede reproducir, y ofrecer recuperarla sería ofrecer
        // relanzarla con la mitad de las opciones.
        let Some(m) = launch_manifest::read(&dir.join("launch.toml")) else {
            continue;
        };
        if read_exit(&dir).is_some() {
            continue;
        }
        let save = dir.join("exit.state");
        v.push(OrphanedSession {
            id: m.id.clone(),
            savestate: save.is_file().then_some(save),
            manifest: m,
            dir,
        });
    }
    v.sort_by(|a, b| b.manifest.created.cmp(&a.manifest.created));
    v
}

/// Descarta una sesión huérfana: borra SU CARPETA y nada más.
///
/// **Es el CA de la issue.** Lo que se borra es el manifest, el cierre que no
/// llegó a escribirse y el savestate de salida de esa sesión. La ROM, la
/// carpeta de guardados y la SRAM del juego viven en otro árbol y esta función
/// no puede alcanzarlos: recibe el directorio de la sesión, no una raíz.
pub fn discard(h: &OrphanedSession) -> std::io::Result<()> {
    // Cinturón: sólo se borra algo que parece una carpeta de sesión. Sin esto,
    // un `Huerfana` armado con la raíz equivocada borraría el árbol entero — y
    // el modo de fallo de una función que borra recursivo es demasiado caro
    // para confiarlo a que nadie la llame mal.
    if !h.dir.join("launch.toml").is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "no es una carpeta de sesión: no tiene launch.toml",
        ));
    }
    std::fs::remove_dir_all(&h.dir)
}

/// El manifest del modo seguro: el mismo juego, sin nada de lo que pudo fallar.
///
/// Se apaga TODO lo que Play agrega encima del emulador —pack, shaders,
/// subsistemas, perfil, salida, parche— y se conservan las tres cosas que
/// definen la partida: la ROM, su hash y el core. Apagar selectivamente sería
/// tener que acertar cuál de los seis causó el crash; el modo seguro existe
/// justamente para no tener que acertar.
///
/// Es una sesión NUEVA, con su propio id: el crash es parte de la historia de
/// la anterior y sobrescribirlo perdería el manifest que hay que diagnosticar.
pub fn safe_mode(m: &LaunchManifest, id: &str) -> LaunchManifest {
    LaunchManifest {
        format: launch_manifest::FORMAT,
        id: id.to_string(),
        created: crate::date::now_utc(),
        play_version: env!("CARGO_PKG_VERSION").to_string(),
        // Lo que NO se toca.
        rom: m.rom.clone(),
        rom_crc32: m.rom_crc32.clone(),
        core: m.core.clone(),
        // Lo que se apaga.
        pack: String::new(),
        profile: "original".to_string(),
        subsystems: Some(0),
        muted_buses: 0,
        shaders: false,
        output: String::new(),
        no_flicker: false,
        patch: String::new(),
        // #604: el modo seguro NO reanuda. Si el guardado es lo que colgaba el
        // juego, cargarlo repetiría el crash — y el modo seguro existe para
        // arrancar sin lo que pudo haberlo causado.
        save: String::new(),
    }
}

// ---------------------------------------------------------------------------
// El lock por juego
// ---------------------------------------------------------------------------

/// Quién tiene tomado un juego. Vive en `<datos>/sessions/locks/<clave>.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameLockOwner {
    pub pid: u32,
    /// La sesión que lo tomó, para poder ir a ella desde el aviso.
    #[serde(alias = "sesion")]
    pub session: String,
    /// Cómo se llama el juego, para poder decirlo sin volver a la biblioteca.
    #[serde(alias = "juego")]
    pub game: String,
}

/// El resultado de intentar tomar un juego.
#[derive(Debug, Clone, PartialEq)]
pub enum GameLockResult {
    /// Es tuyo. Llevá el path para soltarlo al terminar.
    Acquired(PathBuf),
    /// Ya lo tiene otro proceso VIVO.
    AlreadyRunning(GameLockOwner),
}

/// La clave de lock de una ROM.
///
/// Por RUTA y no por CRC: dos copias de la misma ROM en dos carpetas son dos
/// archivos, y lo que se protege es el archivo de guardado, que se deriva de la
/// ruta. Y por hash de la ruta y no por la ruta misma, porque una ruta de
/// Windows no es un nombre de archivo válido.
pub fn key(rom: &Path) -> String {
    // FNV-1a de 64 bits: alcanza de sobra para nombrar un archivo de lock y no
    // agrega una dependencia por algo que no es criptográfico.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in rom.to_string_lossy().to_lowercase().bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

fn locks_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("sessions").join("locks")
}

/// Intenta tomar el juego.
///
/// `vivo` se inyecta a propósito: la pregunta «¿ese pid sigue existiendo?» es
/// del sistema operativo y no se puede escribir un test determinista alrededor
/// de ella. Lo que sí se prueba —y es lo que puede salir mal— es la DECISIÓN:
/// un lock de un proceso muerto se toma, uno de un proceso vivo no.
pub fn take_focus(
    data_dir: &Path,
    rom: &Path,
    game: &str,
    session: &str,
    pid: u32,
    is_process_alive: &dyn Fn(u32) -> bool,
) -> GameLockResult {
    let dir = locks_dir(data_dir);
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(format!("{}.toml", key(rom)));

    if let Some(d) = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| toml::from_str::<GameLockOwner>(&t).ok())
    {
        // Un lock del PROPIO proceso no bloquea: sería Play impidiéndose a sí
        // mismo relanzar un juego que ya cerró.
        if d.pid != pid && is_process_alive(d.pid) {
            return GameLockResult::AlreadyRunning(d);
        }
    }

    let d = GameLockOwner {
        pid,
        session: session.to_string(),
        game: game.to_string(),
    };
    if let Ok(t) = toml::to_string_pretty(&d) {
        let _ = std::fs::write(&path, t);
    }
    GameLockResult::Acquired(path)
}

/// Suelta el lock. Que falle no es grave: el siguiente arranque lo encuentra
/// apuntando a un pid muerto y lo pisa.
pub fn release(path: &Path) {
    let _ = std::fs::remove_file(path);
}

/// ¿Ese proceso sigue existiendo?
///
/// Sin dependencias nuevas y por el camino más barato de cada plataforma. Un
/// error de permisos cuenta como VIVO: no poder mirar un proceso ajeno no es lo
/// mismo que que no exista, y equivocarse hacia «vivo» deja al jugador con un
/// aviso de más — equivocarse hacia «muerto» le deja dos sesiones escribiendo
/// los mismos guardados.
#[cfg(windows)]
pub fn is_process_alive(pid: u32) -> bool {
    // `OpenProcess` sin la crate `windows`: el enlazador ya trae kernel32.
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit_handle: i32, pid: u32) -> *mut std::ffi::c_void;
        fn GetExitCodeProcess(h: *mut std::ffi::c_void, code: *mut u32) -> i32;
        fn CloseHandle(h: *mut std::ffi::c_void) -> i32;
        fn GetLastError() -> u32;
    }
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const ERROR_INVALID_PARAMETER: u32 = 87;
    const STILL_ACTIVE: u32 = 259;
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            // Sólo «parámetro inválido» significa que el pid no existe; un
            // acceso denegado significa que existe y no es nuestro.
            return GetLastError() != ERROR_INVALID_PARAMETER;
        }
        let mut code: u32 = 0;
        let ok = GetExitCodeProcess(h, &mut code) != 0;
        CloseHandle(h);
        // Un handle abierto de un proceso que ya salió sigue siendo válido: lo
        // que dice si terminó es el código, no el handle.
        !ok || code == STILL_ACTIVE
    }
}

#[cfg(not(windows))]
pub fn is_process_alive(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

// ---------------------------------------------------------------------------
// El diagnóstico
// ---------------------------------------------------------------------------

/// El nombre de un archivo, sin su ruta.
///
/// Toda ruta de Windows lleva el nombre de usuario adentro
/// (`C:\Users\david\...`). El diagnóstico es algo que el jugador puede llegar a
/// mandar, así que las rutas se recortan al nombre — que para el core es su dll
/// y para el pack es su hash de contenido (#365), o sea justo lo que sirve para
/// diagnosticar y nada de lo que identifica a una persona.
fn file_name_only(p: &str) -> String {
    if p.is_empty() {
        return "—".to_string();
    }
    Path::new(p)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "—".to_string())
}

/// El diagnóstico redactado de una sesión.
///
/// **Es el CA de la issue**: después de un crash tiene que quedar algo que se
/// pueda leer y mandar. Lleva con qué corrió y cómo terminó; no lleva rutas, ni
/// el nombre del usuario, ni el nombre de la ROM —que en un dump de
/// diagnósticos diría qué juegos tiene alguien—. La ROM se identifica por su
/// CRC32, que es lo que sirve para reproducir el caso.
pub fn diagnostic(m: &LaunchManifest, c: Option<&SessionExit>) -> String {
    let mut s = String::new();
    s.push_str("[ayther-play] diagnostico de sesion\n");
    s.push_str(&format!("play        = {}\n", m.play_version));
    s.push_str(&format!("formato     = {}\n", m.format));
    s.push_str(&format!("so          = {}\n", std::env::consts::OS));
    s.push_str(&format!("creada      = {}\n", m.created));
    s.push_str(&format!(
        "rom_crc32   = {}\n",
        if m.rom_crc32.is_empty() {
            "—"
        } else {
            &m.rom_crc32
        }
    ));
    s.push_str(&format!("core        = {}\n", file_name_only(&m.core)));
    s.push_str(&format!("pack        = {}\n", file_name_only(&m.pack)));
    s.push_str(&format!(
        "perfil      = {}\n",
        if m.profile.is_empty() {
            "—"
        } else {
            &m.profile
        }
    ));
    s.push_str(&format!(
        "subsistemas = {}\n",
        m.subsystems
            .map(|x| x.to_string())
            .unwrap_or_else(|| "(del pack)".into())
    ));
    s.push_str(&format!("buses_mudos = {}\n", m.muted_buses));
    s.push_str(&format!("shaders     = {}\n", m.shaders));
    s.push_str(&format!(
        "salida      = {}\n",
        if m.output.is_empty() {
            "—"
        } else {
            &m.output
        }
    ));
    s.push_str(&format!("parche      = {}\n", file_name_only(&m.patch)));
    match c {
        Some(c) => {
            s.push_str(&format!("termino     = {}\n", c.ended_at));
            s.push_str(&format!("limpia      = {}\n", c.clean_exit));
            s.push_str(&format!(
                "codigo      = {}\n",
                c.exit_code
                    .map(|x| x.to_string())
                    .unwrap_or_else(|| "(sin codigo: lo mataron)".into())
            ));
            s.push_str(&format!("segundos    = {}\n", c.elapsed_seconds));
        }
        None => s.push_str("termino     = (sin cerrar: la sesion quedo huerfana)\n"),
    }
    s
}

// ---------------------------------------------------------------------------
// El balance del fin de sesión (#602)
// ---------------------------------------------------------------------------

/// Lo que quedó de una partida.
///
/// # Por qué se CUENTA y no se recuerda
///
/// Play no ve nada mientras se juega: el Runtime tiene la pantalla y es el que
/// guarda. Preguntarle cuántas veces guardó querría decir un evento nuevo por
/// cada guardado y un contador que Play tendría que mantener sincronizado —
/// dos estados para el mismo hecho, y el que se desincroniza es el que se
/// muestra.
///
/// Contar los archivos que cambiaron DURANTE la sesión no puede desincronizarse
/// de nada: mira el resultado, que es lo que el jugador va a encontrar mañana
/// cuando busque su partida.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SessionSummary {
    pub elapsed_seconds: u64,
    /// La SRAM del juego se escribió en esta sesión.
    pub sram: bool,
    /// Cuántos savestates quedaron nuevos o pisados.
    pub savestates: u32,
    /// Cuántas capturas A/B se tomaron. Se cuentan por su `-split.png`: cada
    /// captura escribe tres PNG y un JSON, y contar archivos diría cuatro.
    pub captures: u32,
}

impl SessionSummary {
    /// ¿Hubo algo que contar además del tiempo? Con todo en cero, las tres
    /// baldosas dirían «0 · nada · 0», que es ruido: se juega sin guardar todo
    /// el tiempo.
    pub fn has_activity(&self) -> bool {
        self.sram || self.savestates > 0 || self.captures > 0
    }
}

/// Cuenta lo que la sesión dejó.
///
/// `desde` es el arranque de la sesión: lo que ya estaba no se cuenta, porque
/// «Guardados: SRAM + 3 estados» tiene que hablar de ESTA partida y no del
/// historial del juego. Un archivo sin fecha legible se cuenta como viejo —
/// equivocarse hacia abajo muestra de menos, y equivocarse hacia arriba le
/// atribuye a esta sesión guardados que no hizo.
pub fn summarize_session(
    saves: &Path,
    captures: &Path,
    game_id: &str,
    profile: &str,
    elapsed_seconds: u64,
    since: std::time::SystemTime,
) -> SessionSummary {
    let new = |p: &Path| -> bool {
        std::fs::metadata(p)
            .and_then(|m| m.modified())
            .map(|t| t >= since)
            .unwrap_or(false)
    };

    let mut b = SessionSummary {
        elapsed_seconds,
        ..Default::default()
    };
    b.sram = new(&crate::saves::sram_path(saves, game_id, profile));

    // #603: las versiones las enumera `guardados`, que es el dueño del layout.
    // Listar los archivos acá sería un segundo lugar que sabe cómo se llaman —
    // y el que se queda atrás cuando el nombre cambie.
    b.savestates = crate::saves::versions(saves, game_id, profile)
        .iter()
        .filter(|v| new(&v.path))
        .count() as u32;

    if let Ok(entries) = std::fs::read_dir(captures) {
        for e in entries.flatten() {
            let p = e.path();
            if p.file_name()
                .is_some_and(|n| n.to_string_lossy().ends_with("-split.png"))
                && new(&p)
            {
                b.captures += 1;
            }
        }
    }
    b
}

/// Dónde escribe sus capturas el Runtime.
///
/// `SDL_GetPrefPath("Ayther", "runtime")` — o sea el árbol de config de Ayther
/// con `runtime/` adentro. Se deriva de la config de Play y no se vuelve a
/// preguntar al sistema: si el jugador movió su carpeta, preguntar de nuevo
/// devolvería la de fábrica y la baldosa diría siempre cero.
pub fn captures_dir(config_dir: &Path) -> PathBuf {
    config_dir.join("runtime").join("capturas")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launch_config::LaunchConfig;

    fn manifest(id: &str) -> LaunchManifest {
        let cfg = LaunchConfig {
            profile: "enhanced".into(),
            subsystems: Some(7),
            muted_buses: 2,
            shaders: true,
            output: "crt".into(),
            no_flicker: true,
            patch: "C:/Users/david/hacks/parche.ips".into(),
        };
        LaunchManifest::new(
            id,
            Path::new("C:/Users/david/ROMs/Golden Axe (USA).md"),
            "a1b2c3d4",
            Path::new("C:/Users/david/cores/genesis_plus_gx_libretro.dll"),
            Some(Path::new("C:/Users/david/packs/9f8e7d.ay")),
            &cfg,
        )
    }

    fn write_session(data_dir: &Path, id: &str) -> PathBuf {
        let m = manifest(id);
        launch_manifest::write(data_dir, &m).expect("se escribe");
        launch_manifest::session_dir(data_dir, id)
    }

    /// EL CA: el modo seguro apaga todo lo que Play agrega y NO toca la ROM, su
    /// hash ni el core. Si tocara la ROM, «modo seguro» sería el nombre de otra
    /// partida.
    #[test]
    fn safe_mode_keeps_the_rom_and_turns_off_the_rest() {
        let m = manifest("s1");
        let s = safe_mode(&m, "s2");

        assert_eq!(s.rom, m.rom, "la ROM es la misma partida");
        assert_eq!(s.rom_crc32, m.rom_crc32);
        assert_eq!(s.core, m.core, "sin core no arranca nada");

        assert_eq!(s.pack, "", "el pack es lo primero que se sospecha");
        assert!(!s.shaders);
        assert_eq!(s.subsystems, Some(0));
        assert_eq!(s.profile, "original");
        assert_eq!(s.output, "");
        assert_eq!(s.patch, "", "un parche mal aplicado cuelga el juego");
        assert!(!s.no_flicker);
        assert_eq!(s.muted_buses, 0);

        assert_ne!(
            s.id, m.id,
            "es una sesion nueva: el crash es historia de la otra"
        );
    }

    /// Y sus argumentos no llevan NADA de lo que se apagó. Es lo que hace cierta
    /// la promesa del diálogo: no alcanza con vaciar los campos si el derivador
    /// de args igual manda `--profile` o `--patch`.
    #[test]
    fn safe_mode_args_carry_nothing_turned_on() {
        let args = safe_mode(&manifest("s1"), "s2").to_args().join(" ");
        for forbidden in ["--pack", "--patch", "--output"] {
            assert!(
                !args.contains(forbidden),
                "el modo seguro manda {forbidden}: {args}"
            );
        }
        assert!(
            args.contains("--subsystems 0"),
            "hay que APAGARLOS, no omitirlos: {args}"
        );
        assert!(args.contains("--no-shaders"), "{args}");
    }

    /// Una sesión sin `exit.toml` es huérfana; una con él, no. Es la única
    /// condición que sobrevive a que Play deje de existir.
    #[test]
    fn orphan_is_the_one_that_did_not_record_its_close() {
        let t = tempfile::tempdir().unwrap();
        let d1 = write_session(t.path(), "20260827T200000-aaaa");
        let _d2 = write_session(t.path(), "20260827T210000-bbbb");

        assert_eq!(orphaned_sessions(t.path()).len(), 2);

        close(
            &d1,
            &SessionExit {
                ended_at: "2026-08-27T20:30:00Z".into(),
                clean_exit: true,
                exit_code: Some(0),
                savestate: String::new(),
                elapsed_seconds: 1800,
            },
        )
        .expect("se cierra");

        let h = orphaned_sessions(t.path());
        assert_eq!(h.len(), 1, "la cerrada dejo de ser huerfana");
        assert_eq!(h[0].id, "20260827T210000-bbbb");
    }

    /// Y se listan de la más nueva a la más vieja: si hay varias, la que le
    /// importa al jugador es la última, y el orden del directorio no es un
    /// orden.
    #[test]
    fn orphans_come_newest_to_oldest() {
        let t = tempfile::tempdir().unwrap();
        for (id, created) in [
            ("a", "2026-08-27T10:00:00Z"),
            ("b", "2026-08-27T22:00:00Z"),
            ("c", "2026-08-27T15:00:00Z"),
        ] {
            let mut m = manifest(id);
            m.created = created.to_string();
            launch_manifest::write(t.path(), &m).unwrap();
        }
        let ids: Vec<String> = orphaned_sessions(t.path())
            .into_iter()
            .map(|h| h.id)
            .collect();
        assert_eq!(ids, vec!["b", "c", "a"]);
    }

    /// EL CA que más importa: descartar borra la carpeta de la SESIÓN y no
    /// puede alcanzar la ROM ni los guardados — viven en otro árbol.
    #[test]
    fn discarding_erases_the_session_and_nothing_else() {
        let t = tempfile::tempdir().unwrap();
        let saves = t.path().join("saves");
        std::fs::create_dir_all(&saves).unwrap();
        std::fs::write(saves.join("golden_axe.srm"), b"partida").unwrap();

        let d = write_session(t.path(), "s1");
        std::fs::write(d.join("exit.state"), b"estado").unwrap();

        let h = orphaned_sessions(t.path()).remove(0);
        assert!(h.savestate.is_some(), "el savestate de salida se encuentra");
        discard(&h).expect("se descarta");

        assert!(!d.exists(), "la carpeta de la sesion se fue");
        assert!(
            saves.join("golden_axe.srm").is_file(),
            "LOS GUARDADOS NO SE TOCAN"
        );
        assert!(
            t.path().join("sessions").is_dir(),
            "la raiz de sesiones sigue"
        );
    }

    /// Y descartar algo que no es una sesión falla en vez de borrar. Una
    /// función que borra recursivo no puede confiar en que nadie la llame mal.
    #[test]
    fn discarding_refuses_if_there_is_no_manifest() {
        let t = tempfile::tempdir().unwrap();
        let invalid_entry = t.path().join("no_es_una_sesion");
        std::fs::create_dir_all(&invalid_entry).unwrap();
        std::fs::write(invalid_entry.join("importante.txt"), b"x").unwrap();

        let h = OrphanedSession {
            id: "x".into(),
            dir: invalid_entry.clone(),
            manifest: manifest("x"),
            savestate: None,
        };
        assert!(discard(&h).is_err());
        assert!(
            invalid_entry.join("importante.txt").is_file(),
            "no se borro nada"
        );
    }

    /// El lock: si el dueño sigue vivo, no se toma y el aviso sabe quién es.
    #[test]
    fn a_game_already_in_play_does_not_launch_twice() {
        let t = tempfile::tempdir().unwrap();
        let rom = Path::new("C:/ROMs/Golden Axe (USA).md");
        let is_process_alive = |_: u32| true;

        let GameLockResult::Acquired(p) =
            take_focus(t.path(), rom, "Golden Axe", "s1", 100, &is_process_alive)
        else {
            panic!("el primero lo toma")
        };
        assert!(p.is_file());

        match take_focus(t.path(), rom, "Golden Axe", "s2", 200, &is_process_alive) {
            GameLockResult::AlreadyRunning(d) => {
                assert_eq!(d.pid, 100);
                assert_eq!(d.game, "Golden Axe");
                assert_eq!(d.session, "s1", "el aviso puede llevar a la sesion");
            }
            other => panic!("se lanzo dos veces: {other:?}"),
        }
    }

    /// EL DEFECTO QUE ESTO EVITA: un lock que sobrevive a un crash bloquearía
    /// el juego para siempre. Si el dueño ya no existe, se toma.
    #[test]
    fn a_lock_from_a_dead_process_blocks_nothing() {
        let t = tempfile::tempdir().unwrap();
        let rom = Path::new("C:/ROMs/Golden Axe (USA).md");

        let _ = take_focus(t.path(), rom, "Golden Axe", "s1", 100, &|_| true);
        assert!(
            matches!(
                take_focus(t.path(), rom, "Golden Axe", "s2", 200, &|_| false),
                GameLockResult::Acquired(_)
            ),
            "el dueno murio: el juego tiene que poder lanzarse"
        );
    }

    /// Soltar libera de verdad, y dos ROMs distintas no se estorban.
    #[test]
    fn releasing_frees_and_each_rom_has_its_lock() {
        let t = tempfile::tempdir().unwrap();
        let a = Path::new("C:/ROMs/a.md");
        let b = Path::new("C:/ROMs/b.md");

        let GameLockResult::Acquired(pa) = take_focus(t.path(), a, "A", "s1", 100, &|_| true)
        else {
            panic!()
        };
        assert!(
            matches!(
                take_focus(t.path(), b, "B", "s2", 200, &|_| true),
                GameLockResult::Acquired(_)
            ),
            "otra ROM es otra partida"
        );

        release(&pa);
        assert!(matches!(
            take_focus(t.path(), a, "A", "s3", 300, &|_| true),
            GameLockResult::Acquired(_)
        ));
    }

    /// EL CA: el diagnóstico queda disponible y NO lleva rutas ni el nombre del
    /// usuario. Es algo que el jugador puede llegar a mandar.
    #[test]
    fn the_diagnostic_goes_redacted() {
        let m = manifest("s1");
        let c = SessionExit {
            ended_at: "2026-08-27T21:00:00Z".into(),
            clean_exit: false,
            exit_code: None,
            savestate: String::new(),
            elapsed_seconds: 42,
        };
        let d = diagnostic(&m, Some(&c));

        for sensitive_fragment in [
            "C:/Users",
            "C:\\Users",
            "david",
            "/home/",
            "Golden Axe",
            "ROMs",
        ] {
            assert!(
                !d.contains(sensitive_fragment),
                "el diagnostico filtra «{sensitive_fragment}»:\n{d}"
            );
        }

        // Y lo que SÍ tiene que llevar, porque es con lo que se reproduce.
        assert!(
            d.contains("a1b2c3d4"),
            "sin el CRC no se sabe que ROM era:\n{d}"
        );
        assert!(d.contains("genesis_plus_gx_libretro.dll"), "{d}");
        assert!(
            d.contains("9f8e7d.ay"),
            "el pack se nombra por su hash de contenido:\n{d}"
        );
        assert!(d.contains("parche.ips"), "{d}");
        assert!(d.contains("sin codigo"), "matado != salir con 0:\n{d}");
    }

    /// Una sesión huérfana también se diagnostica: es justo la que hay que
    /// mirar, y decir «terminó bien» por no tener el dato sería mentir.
    #[test]
    fn an_orphan_is_diagnosed_as_unclosed() {
        let d = diagnostic(&manifest("s1"), None);
        assert!(d.contains("huerfana"), "{d}");
    }

    // ---- #602: el balance del fin de sesión --------------------------------

    fn touch(p: &Path) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"x").unwrap();
    }

    /// Se cuenta lo de ESTA sesión. Lo que ya estaba no entra: «Guardados: SRAM
    /// + 3 estados» tiene que hablar de la partida que acaba de terminar, no
    ///   del historial del juego.
    #[test]
    fn the_balance_counts_only_what_this_session_did() {
        let t = tempfile::tempdir().unwrap();
        let saves = t.path().join("saves");
        let caps = t.path().join("capturas");

        // Lo viejo, antes del corte.
        crate::saves::write_version(&saves, "golden", "hd", "20260827T100000Z", "a1", b"vieja")
            .unwrap();
        touch(&caps.join("captura-vieja-split.png"));
        std::thread::sleep(std::time::Duration::from_millis(20));
        let since = std::time::SystemTime::now();
        std::thread::sleep(std::time::Duration::from_millis(20));

        // Lo de la sesión.
        touch(&crate::saves::sram_path(&saves, "golden", "hd"));
        for when in ["20260827T200000Z", "20260827T210000Z"] {
            crate::saves::write_version(&saves, "golden", "hd", when, "a1", b"nueva").unwrap();
        }
        touch(&caps.join("captura-nueva-split.png"));
        // Los otros dos PNG y el JSON de la MISMA captura no suman.
        touch(&caps.join("captura-nueva-original.png"));
        touch(&caps.join("captura-nueva-ayther.png"));
        touch(&caps.join("captura-nueva.json"));

        let b = summarize_session(&saves, &caps, "golden", "hd", 2520, since);
        assert_eq!(b.elapsed_seconds, 2520);
        assert!(b.sram, "la SRAM se escribio en esta sesion");
        assert_eq!(b.savestates, 2, "la ranura vieja no cuenta");
        assert_eq!(
            b.captures, 1,
            "una captura son cuatro archivos, no cuatro capturas"
        );
        assert!(b.has_activity());
    }

    /// Y no se cuentan los guardados de OTRO juego. Los saves viven todos en la
    /// misma carpeta.
    #[test]
    fn the_balance_does_not_count_saves_from_another_game() {
        let t = tempfile::tempdir().unwrap();
        let saves = t.path().join("saves");
        let caps = t.path().join("capturas");
        std::fs::create_dir_all(&caps).unwrap();
        let since = std::time::SystemTime::now();
        std::thread::sleep(std::time::Duration::from_millis(20));

        touch(&crate::saves::sram_path(&saves, "sonic", "hd"));
        crate::saves::write_version(&saves, "sonic", "hd", "20260827T200000Z", "a1", b"x").unwrap();
        crate::saves::write_version(&saves, "golden", "hd", "20260827T200000Z", "a1", b"x")
            .unwrap();

        let b = summarize_session(&saves, &caps, "golden", "hd", 60, since);
        assert!(!b.sram, "la SRAM de sonic no es la de golden");
        assert_eq!(b.savestates, 1);

        // Y tampoco los de OTRO PERFIL del mismo juego: el estado de una
        // partida en HD no es el de la misma partida en original.
        assert_eq!(
            summarize_session(&saves, &caps, "golden", "original", 60, since).savestates,
            0
        );
    }

    /// Una sesión sin guardar ni capturar es lo NORMAL, y no tiene nada que
    /// mostrar además del tiempo. Tres baldosas en cero serían ruido.
    #[test]
    fn a_session_with_nothing_says_so() {
        let t = tempfile::tempdir().unwrap();
        let b = summarize_session(
            &t.path().join("saves"),
            &t.path().join("caps"),
            "golden",
            "hd",
            300,
            std::time::SystemTime::now(),
        );
        assert_eq!(
            b,
            SessionSummary {
                elapsed_seconds: 300,
                ..Default::default()
            }
        );
        assert!(!b.has_activity());
    }

    /// Y las capturas salen del árbol de config del jugador, no del de fábrica:
    /// preguntándole de nuevo al sistema, alguien que movió su carpeta vería
    /// siempre cero.
    #[test]
    fn the_captures_dir_hangs_off_the_runtime_tree() {
        let d = captures_dir(Path::new("D:/mis datos/Ayther"));
        assert!(
            d.ends_with("runtime/capturas") || d.ends_with("runtime\\capturas"),
            "{}",
            d.display()
        );
    }
}
