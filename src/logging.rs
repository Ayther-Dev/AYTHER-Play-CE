//! logging.rs — el log de Play, con niveles y con **redacción** (PLAY-EP12.1, #621).
//!
//! # Por qué la redacción vive acá y no en cada `eprintln!`
//!
//! El §10 dice que en el log no van ni las rutas del disco del jugador ni los
//! nombres de los archivos de su biblioteca: un log se copia y se pega en un
//! ticket, y ahí «D:\\Juegos\\ROMs\\Sonic 2 (mi copia).md» cuenta dos cosas que
//! nadie preguntó —cómo se llama la carpeta y qué ROM tiene—.
//!
//! Poner la regla en cada punto de llamada es confiar en que nadie se olvide, y
//! son ochenta y tres. Acá la regla se aplica UNA vez, al escribir, y el
//! oráculo `ninguna_linea_de_log_lleva_una_ruta` lee el propio fuente para que
//! un `eprintln!` nuevo con una ruta no compile en verde.
//!
//! # Qué se conserva
//!
//! La extensión y el hecho de que *hay* una ruta. `<ruta:.md>` dice «acá iba un
//! archivo de Mega Drive» sin decir cuál ni dónde: alcanza para entender el
//! error y no alcanza para reconstruir la biblioteca de nadie.
//!
//! Las URLs quedan enteras. La del Hub no es del jugador —la configuró él, pero
//! apunta a un servicio— y sin ella un problema de red no se diagnostica.

use std::sync::atomic::{AtomicU8, Ordering};

/// Cuánto se dice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// Algo falló y el jugador lo va a notar.
    Error = 0,
    /// Algo no salió como se esperaba y siguió andando.
    Warn = 1,
    /// El curso normal: qué se abrió, qué se instaló.
    Info = 2,
    /// Para diagnosticar: tiempos, decisiones intermedias.
    Verbose = 3,
}

impl Level {
    pub fn of(s: &str) -> Option<Level> {
        match s.trim().to_lowercase().as_str() {
            "error" => Some(Level::Error),
            "aviso" | "warn" => Some(Level::Warn),
            "info" => Some(Level::Info),
            "detalle" | "debug" | "trace" => Some(Level::Verbose),
            _ => None,
        }
    }

    pub fn text(self) -> &'static str {
        match self {
            Level::Error => "error",
            Level::Warn => "aviso",
            Level::Info => "info",
            Level::Verbose => "detalle",
        }
    }
}

static LEVEL: AtomicU8 = AtomicU8::new(Level::Info as u8);

/// Fija el nivel. La variable de entorno gana sobre la configuración: quien
/// está diagnosticando un problema no debería tener que editar un TOML —y
/// menos el de la máquina de otro— para ver más.
pub fn configure(configured_level: &str) {
    let n = std::env::var("AYTHER_PLAY_LOG")
        .ok()
        .and_then(|v| Level::of(&v))
        .or_else(|| Level::of(configured_level))
        .unwrap_or(Level::Info);
    LEVEL.store(n as u8, Ordering::Relaxed);
}

pub fn level() -> Level {
    match LEVEL.load(Ordering::Relaxed) {
        0 => Level::Error,
        1 => Level::Warn,
        3 => Level::Verbose,
        _ => Level::Info,
    }
}

/// Escribe una línea, ya redactada. La usan las macros; llamarla a mano
/// también sirve.
pub fn line(n: Level, module: &str, message: &str) {
    if n > level() {
        return;
    }
    eprintln!("[{}] [{module}] {}", n.text(), redact(message));
}

/// Las extensiones que delatan la biblioteca del jugador: sus ROMs y sus
/// guardados. Un `.ay` no está: el pack es contenido publicado, con nombre
/// público, y ocultarlo haría ilegible el log de una instalación.
const PRIVATE_EXTS: [&str; 12] = [
    "md", "smd", "gen", "bin", "sms", "gg", "nes", "sfc", "smc", "gba", "srm", "state",
];

/// Saca del texto lo que no tiene por qué viajar.
///
/// Redacta: rutas absolutas de Windows (`C:\\…`), UNC (`\\\\servidor\\…`),
/// rutas POSIX (`/home/…`) y nombres sueltos de archivos de la biblioteca.
/// Conserva: URLs, ids de pack, nombres de juego del catálogo, y la extensión
/// de lo redactado.
pub fn redact(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let b: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < b.len() {
        // Una URL se copia entera: es de un servicio, no del disco de nadie.
        if let Some(end) = url_end(&b, i) {
            out.extend(&b[i..end]);
            i = end;
            continue;
        }
        if let Some((end, ext)) = path_end(&b, i) {
            // Un `.rs` es CODIGO, no biblioteca: el archivo y la linea son lo
            // unico que sirve de un panico y no dicen nada del jugador. Su
            // carpeta si —la del registro de cargo lleva su nombre de usuario—,
            // asi que se va la carpeta y se queda el archivo.
            if ext.as_deref() == Some("rs") {
                let segment: String = b[i..end].iter().collect();
                let file = segment.rsplit(['/', '\\']).next().unwrap_or("");
                out.push_str("<ruta>/");
                out.push_str(file);
            } else {
                out.push_str(&redaction_marker("ruta", ext.as_deref()));
            }
            i = end;
            continue;
        }
        if let Some((end, ext)) = private_file_from(&b, i) {
            out.push_str(&redaction_marker("archivo", Some(&ext)));
            i = end;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

fn redaction_marker(kind: &str, ext: Option<&str>) -> String {
    match ext {
        Some(e) if !e.is_empty() => format!("<{kind}:.{e}>"),
        _ => format!("<{kind}>"),
    }
}

fn is_separator(c: char) -> bool {
    c == '/' || c == '\\'
}

/// Un carácter que puede formar parte de una ruta. Deliberadamente ancho: los
/// nombres de las carpetas de la gente tienen espacios, paréntesis y acentos, y
/// cortar en el primer espacio dejaría media ruta afuera de la redacción.
fn is_path_character(c: char) -> bool {
    !matches!(c, '"' | '\'' | ',' | ';' | ')' | ']' | '}' | '\n' | '\t')
}

fn url_end(b: &[char], i: usize) -> Option<usize> {
    let remaining: String = b[i..].iter().take(8).collect();
    if !(remaining.starts_with("http://") || remaining.starts_with("https://")) {
        return None;
    }
    let mut j = i;
    while j < b.len() && !b[j].is_whitespace() && !matches!(b[j], '"' | '\'' | ',' | ';') {
        j += 1;
    }
    Some(j)
}

/// ¿Empieza acá una ruta? Devuelve dónde termina y su extensión.
fn path_end(b: &[char], i: usize) -> Option<(usize, Option<String>)> {
    let windows_drive =
        i + 2 < b.len() && b[i].is_ascii_alphabetic() && b[i + 1] == ':' && is_separator(b[i + 2]);
    let unc = i + 1 < b.len() && b[i] == '\\' && b[i + 1] == '\\';
    // Una ruta POSIX tiene que tener al menos dos separadores para no confundir
    // con una fracción o el principio de una frase: `/home/x` sí, `y/o` no.
    let posix = b[i] == '/'
        && b[i + 1..]
            .iter()
            .take_while(|c| is_path_character(**c) && !c.is_whitespace())
            .filter(|c| **c == '/')
            .count()
            >= 1;
    if !(windows_drive || unc || posix) {
        return None;
    }
    let mut j = i;
    while j < b.len() && is_path_character(b[j]) {
        // Un espacio corta la ruta sólo si lo que sigue no parece seguir siendo
        // parte de ella: «C:\Mis Juegos\rom.md» es una ruta, «abrió C:\a y
        // falló» no incluye «y falló».
        if b[j].is_whitespace() {
            let path_continues = b[j + 1..]
                .iter()
                .take_while(|c| !c.is_whitespace())
                .any(|c| is_separator(*c) || *c == '.');
            if !path_continues {
                break;
            }
        }
        j += 1;
    }
    // Los signos de puntuación finales no son de la ruta.
    while j > i && matches!(b[j - 1], '.' | ':' | '!' | '?') && !is_separator(b[j - 1]) {
        j -= 1;
    }
    Some((j, extension(&b[i..j])))
}

/// Un nombre suelto de archivo de la biblioteca (sin carpeta): `Sonic 2.md`.
fn private_file_from(b: &[char], i: usize) -> Option<(usize, String)> {
    // Sólo al principio de una palabra.
    if i > 0 && !b[i - 1].is_whitespace() && !matches!(b[i - 1], '"' | '\'' | '(' | '[') {
        return None;
    }
    let mut j = i;
    while j < b.len() && !b[j].is_whitespace() && is_path_character(b[j]) {
        j += 1;
    }
    let ext = extension(&b[i..j])?;
    if !PRIVATE_EXTS.contains(&ext.as_str()) {
        return None;
    }
    Some((j, ext))
}

fn extension(segment: &[char]) -> Option<String> {
    let s: String = segment.iter().collect();
    let last_component = s.rsplit(['/', '\\']).next()?;
    // Un pánico escribe `archivo.rs:1584:32:`, y esa cola no es la extensión.
    let last_component = last_component.split(':').next().unwrap_or(last_component);
    let (_, ext) = last_component.rsplit_once('.')?;
    let ext = ext.trim().to_lowercase();
    if ext.is_empty() || ext.len() > 6 || !ext.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some(ext)
}

#[macro_export]
macro_rules! log_error {
    ($m:expr, $($a:tt)*) => { $crate::logging::line($crate::logging::Level::Error, $m, &format!($($a)*)) };
}
#[macro_export]
macro_rules! log_warning {
    ($m:expr, $($a:tt)*) => { $crate::logging::line($crate::logging::Level::Warn, $m, &format!($($a)*)) };
}
#[macro_export]
macro_rules! log_info {
    ($m:expr, $($a:tt)*) => { $crate::logging::line($crate::logging::Level::Info, $m, &format!($($a)*)) };
}
#[macro_export]
macro_rules! log_verbose {
    ($m:expr, $($a:tt)*) => { $crate::logging::line($crate::logging::Level::Verbose, $m, &format!($($a)*)) };
}

#[cfg(test)]
mod tests;
