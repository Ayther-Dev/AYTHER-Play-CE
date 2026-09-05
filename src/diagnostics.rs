//! diagnostics.rs — el paquete que el jugador puede LEER antes de mandarlo
//! (PLAY-EP12.2, #622).
//!
//! # Dos reportes que nadie unía
//!
//! El Runtime escribe su `diagnostico.md` con F9 —GPU, backend, por qué sugiere
//! lo que sugiere— y Play arma el suyo desde #602, con el manifest de la sesión
//! y cómo cerró. Son las dos mitades de la misma pregunta: cuando alguien dice
//! «se me cerró», la respuesta está repartida entre un archivo que Play no lee
//! y otro que el jugador no sabe que existe.
//!
//! # Que se vea antes de mandarlo no es una promesa: es la forma
//!
//! Este módulo **no manda nada**. Devuelve texto. Quien lo llama lo muestra o
//! lo guarda, y adjuntarlo es del jugador. Un paquete de diagnóstico que se
//! envía solo obliga a confiar en que no lleva nada de más; uno que se lee
//! antes no necesita que nadie confíe.
//!
//! Y va redactado con las reglas de #621, que es lo que hace que leerlo sea
//! rápido: el jugador no tiene que revisar si se le coló el nombre de una
//! carpeta, porque el nombre de la carpeta no está.

use crate::logging::redact;
use std::path::{Path, PathBuf};

/// Lo que trae el paquete, ya armado.
pub struct DiagnosticPackage {
    /// El texto entero, redactado. Es lo que se muestra y lo que se guarda:
    /// **el mismo**, para que lo que el jugador leyó sea lo que mandó.
    pub text: String,
    /// Qué partes se encontraron y cuáles no, para que la pantalla pueda
    /// decirlo sin volver a mirar el disco.
    pub sections: Vec<(&'static str, bool)>,
}

/// Dónde deja el Runtime su informe (`SDL_GetPrefPath("Ayther", "runtime")`).
///
/// Se deriva igual que la del Runtime en vez de pasarla por argumento porque es
/// exactamente lo que hay que poder encontrar SIN que el Runtime esté corriendo
/// — que es el caso: se lee después de que se cerró.
pub fn runtime_report_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Ayther")
        .join("runtime")
        .join("diagnostico.md")
}

/// Arma el paquete.
///
/// `sesion` es el directorio de una sesión concreta (`sessions/<id>`), o `None`
/// para armar uno sin sesión — que es el caso de «Play no abre», donde igual
/// sirven la versión, el sistema y el último pánico.
pub fn package(
    play_version: &str,
    session: Option<&Path>,
    diagnostics_dir: &Path,
) -> DiagnosticPackage {
    let mut s = String::new();
    let mut sections = Vec::new();

    s.push_str("# Diagnóstico de Ayther Play\n\n");
    s.push_str("Esto es todo lo que se va a mandar. Nada más, y nada se manda solo.\n\n");
    s.push_str(&format!("play    = {play_version}\n"));
    s.push_str(&format!("so      = {}\n", std::env::consts::OS));
    s.push_str(&format!("arq     = {}\n\n", std::env::consts::ARCH));

    // --- La sesión ---------------------------------------------------------
    let has_session = match session {
        Some(dir) => match session_text(dir) {
            Some(t) => {
                s.push_str("## La sesión\n\n```\n");
                s.push_str(&t);
                s.push_str("```\n\n");
                true
            }
            None => {
                s.push_str("## La sesión\n\nNo se pudo leer el manifest de esa sesión.\n\n");
                false
            }
        },
        None => {
            s.push_str(
                "## La sesión\n\nSin sesión: este paquete se armó fuera de una partida.\n\n",
            );
            false
        }
    };
    sections.push(("sesión", has_session));

    // --- El informe del Runtime -------------------------------------------
    let rp = runtime_report_path();
    let has_runtime_report = match std::fs::read_to_string(&rp) {
        Ok(t) => {
            s.push_str("## El informe del Runtime\n\n");
            s.push_str(&t);
            if !t.ends_with('\n') {
                s.push('\n');
            }
            s.push('\n');
            true
        }
        Err(_) => {
            // Decir que no está es parte del CA: un paquete al que le falta
            // algo sin avisar se lee como un paquete completo que no explica
            // nada.
            s.push_str(
                "## El informe del Runtime\n\n\
                 No hay ninguno. Lo escribe el Runtime con **F9** durante una partida: \
                 si el problema se ve jugando, abrí el juego, apretá F9 y volvé a armar \
                 este paquete.\n\n",
            );
            false
        }
    };
    sections.push(("informe del Runtime", has_runtime_report));

    // --- El último pánico --------------------------------------------------
    let has_panic = match latest_panic(diagnostics_dir) {
        Some((name, t)) => {
            s.push_str(&format!("## El último cierre inesperado ({name})\n\n```\n"));
            s.push_str(&t);
            if !t.ends_with('\n') {
                s.push('\n');
            }
            s.push_str("```\n\n");
            true
        }
        None => {
            s.push_str("## Cierres inesperados\n\nNinguno registrado.\n\n");
            false
        }
    };
    sections.push(("cierre inesperado", has_panic));

    // La redacción es lo ÚLTIMO y sobre el texto entero: así alcanza también a
    // lo que escribieron el Runtime y el hook de pánico, que no pasaron por el
    // log de Play.
    DiagnosticPackage {
        text: redact(&s),
        sections,
    }
}

/// Guarda el paquete y devuelve dónde quedó. No manda nada: adjuntarlo es del
/// jugador.
pub fn save(diagnostics_dir: &Path, text: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(diagnostics_dir)?;
    let destination = diagnostics_dir.join("diagnostico-play.txt");
    std::fs::write(&destination, text)?;
    Ok(destination)
}

fn session_text(dir: &Path) -> Option<String> {
    let m = crate::launch_manifest::read(&dir.join("launch.toml"))?;
    let c = crate::session::read_exit(dir);
    Some(crate::session::diagnostic(&m, c.as_ref()))
}

/// El pánico más reciente, si lo hay. El más reciente y no todos: el paquete es
/// para leerlo, y diez volcados viejos es exactamente lo que hace que nadie lo
/// lea.
fn latest_panic(diagnostics_dir: &Path) -> Option<(String, String)> {
    let mut candidates: Vec<PathBuf> = std::fs::read_dir(diagnostics_dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("play-panico"))
        })
        .collect();
    candidates.sort();
    let p = candidates.pop()?;
    let name = p.file_name()?.to_str()?.to_string();
    Some((name, std::fs::read_to_string(&p).ok()?))
}

#[cfg(test)]
mod tests;
