//! Regla compartida para localizar el checkout fuente de AYTHER Runtime en tests.

use std::path::{Path, PathBuf};

pub const ENV_VAR: &str = "AYTHER_RUNTIME_SOURCE";
const SIBLING_DIRECTORY: &str = "AYTHER-Runtime";

fn resolve(manifest_dir: &Path, configured: Option<PathBuf>) -> PathBuf {
    match configured.filter(|path| !path.as_os_str().is_empty()) {
        Some(configured) if configured.is_absolute() => configured,
        Some(configured) => manifest_dir.join(configured),
        None => manifest_dir
            .parent()
            .expect("AYTHER Play CE debe tener un directorio padre")
            .join(SIBLING_DIRECTORY),
    }
}

/// Devuelve la raíz fuente de Runtime.
///
/// `AYTHER_RUNTIME_SOURCE` puede ser absoluta o relativa a este repositorio.
/// Sin override, se usa el checkout hermano canónico `../AYTHER-Runtime`.
pub fn root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    resolve(manifest_dir, std::env::var_os(ENV_VAR).map(PathBuf::from))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_the_canonical_sibling_checkout() {
        let manifest_dir = Path::new("projects").join("AYTHER-Play-CE");
        assert_eq!(
            resolve(&manifest_dir, None),
            Path::new("projects").join("AYTHER-Runtime")
        );
    }

    #[test]
    fn a_relative_override_is_relative_to_play_ce() {
        let manifest_dir = Path::new("projects").join("AYTHER-Play-CE");
        assert_eq!(
            resolve(&manifest_dir, Some(PathBuf::from("vendor/runtime"))),
            manifest_dir.join("vendor/runtime")
        );
    }
}
