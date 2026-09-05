//! explorer.rs — elegir una carpeta o un archivo con el mando (#588).
//!
//! El onboarding pide dos cosas que en un escritorio se resuelven con el
//! diálogo del sistema: la carpeta de ROM y el archivo del core. Play no puede
//! usarlo. Un `GetOpenFileName` de Windows es una ventana de escritorio: se
//! maneja con mouse, no tiene foco visible a tres metros, y sobre una
//! aplicación a pantalla completa aparece detrás o la saca del modo — el
//! jugador queda mirando una pantalla que no responde. La spec es
//! gamepad-first (§6, §14) y el onboarding tiene que poder completarse **sólo
//! con el mando**.
//!
//! Así que el explorador es propio y mínimo: una lista de entradas, con la
//! carpeta de arriba como primera fila. Todo lo que decide qué se ve está en
//! `listar()`, que es una función del sistema de archivos hacia un `Vec` y por
//! eso se puede probar sin ventana.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    /// `true` para la fila «..». Se dibuja distinto y nunca se puede elegir
    /// como resultado, sólo navegar.
    pub is_parent: bool,
}

/// Qué se está eligiendo. Cambia qué archivos se muestran: en modo carpeta los
/// archivos sólo estorban.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Search {
    Folder,
    /// Archivos con estas extensiones (en minúsculas, sin punto).
    File(&'static [&'static str]),
}

/// Las extensiones de un core, por plataforma de compilación.
pub const EXT_CORE: &[&str] = &["dll", "so", "dylib"];

/// Lista el contenido de `dir` para mostrarlo.
///
/// Ordena carpetas antes que archivos y alfabéticamente sin distinguir
/// mayúsculas, que es como la gente espera leer una lista; el orden del sistema
/// de archivos no es ninguno de los dos y cambia entre máquinas.
///
/// Los errores no se propagan: una carpeta que no se puede leer —permisos, una
/// unidad que se desconectó— devuelve sólo «..», que es lo único que el jugador
/// puede hacer ahí. Un `Result` obligaría a la UI a decidir qué hacer con un
/// error del que no se puede hacer nada.
pub fn list_entries(dir: &Path, search: Search) -> Vec<Entry> {
    let mut out = Vec::new();
    if let Some(parent) = dir.parent() {
        out.push(Entry {
            name: "..".into(),
            path: parent.to_path_buf(),
            is_dir: true,
            is_parent: true,
        });
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };

    let mut dirs: Vec<Entry> = Vec::new();
    let mut files: Vec<Entry> = Vec::new();
    for e in rd.flatten() {
        let path = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        // Los ocultos de Unix y los de sistema de Windows sólo son ruido acá.
        if name.starts_with('.') {
            continue;
        }
        let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if is_dir {
            dirs.push(Entry {
                name,
                path,
                is_dir: true,
                is_parent: false,
            });
        } else if let Search::File(exts) = search {
            let ext = path
                .extension()
                .and_then(|x| x.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if exts.contains(&ext.as_str()) {
                files.push(Entry {
                    name,
                    path,
                    is_dir: false,
                    is_parent: false,
                });
            }
        }
    }
    let key = |e: &Entry| e.name.to_lowercase();
    dirs.sort_by_key(key);
    files.sort_by_key(key);
    out.extend(dirs);
    out.extend(files);
    out
}

/// Por dónde empezar cuando todavía no hay nada elegido.
///
/// La carpeta del usuario y no la raíz del disco: desde `C:\` hacen falta
/// cuatro niveles de navegación con la cruceta para llegar a algo propio.
pub fn starting_path() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// Cuántas ROM conocidas hay en `dir` (sin recursión, sin hashear).
///
/// Es el «212 archivos» que el paso de carpetas muestra al lado de cada una.
/// Sin ese número, agregar la carpeta equivocada se descubre recién al terminar
/// el escaneo completo; con él, se ve en el acto.
pub fn count_roms(dir: &Path) -> usize {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return 0;
    };
    rd.flatten()
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|x| x.to_str())
                .map(|x| crate::library::platform_for_ext(x).is_some())
                .unwrap_or(false)
        })
        .count()
}

/// #678 — cuántos `.ay` hay a la vista en una carpeta, para la fila de
/// Ajustes. Plano, sin recursión, como la lee el escaneo: contar distinto de
/// lo que después aparece sería prometer packs que no llegan.
pub fn count_packs(dir: &Path) -> usize {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return 0;
    };
    rd.flatten()
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("ay"))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tree() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        fs::create_dir(d.path().join("Mega Drive")).unwrap();
        fs::create_dir(d.path().join("aaa")).unwrap();
        fs::write(d.path().join("core.dll"), b"x").unwrap();
        fs::write(d.path().join("Sonic.md"), b"x").unwrap();
        fs::write(d.path().join("notas.txt"), b"x").unwrap();
        fs::write(d.path().join(".oculto"), b"x").unwrap();
        d
    }

    #[test]
    fn when_picking_a_folder_no_files_are_shown() {
        let d = tree();
        let l = list_entries(d.path(), Search::Folder);
        assert!(l.iter().all(|e| e.is_dir), "se colaron archivos: {l:?}");
        assert!(l.iter().any(|e| e.name == "Mega Drive"));
    }

    #[test]
    fn when_picking_a_core_only_the_requested_extensions_are_shown() {
        let d = tree();
        let l = list_entries(d.path(), Search::File(EXT_CORE));
        let names: Vec<&str> = l.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"core.dll"), "{names:?}");
        assert!(!names.contains(&"notas.txt"), "{names:?}");
        assert!(!names.contains(&"Sonic.md"), "{names:?}");
        // Las carpetas siguen estando: hay que poder navegar hasta el core.
        assert!(names.contains(&"aaa"));
    }

    /// «..» primero, después carpetas, después archivos, y alfabético sin
    /// distinguir mayúsculas. Con el orden del sistema de archivos, la misma
    /// carpeta se ve distinta en dos máquinas.
    #[test]
    fn the_order_is_what_a_reader_expects() {
        let d = tree();
        let l = list_entries(d.path(), Search::File(EXT_CORE));
        assert!(l[0].is_parent, "«..» tiene que ser la primera fila");
        assert_eq!(l[1].name, "aaa");
        assert_eq!(l[2].name, "Mega Drive");
        assert_eq!(l[3].name, "core.dll");
    }

    #[test]
    fn hidden_entries_are_not_shown() {
        let d = tree();
        let l = list_entries(d.path(), Search::File(&["oculto", "dll"]));
        // «..» es la salida, no un oculto.
        assert!(
            !l.iter().any(|e| !e.is_parent && e.name.starts_with('.')),
            "{l:?}"
        );
    }

    /// Una carpeta ilegible deja al jugador con «..» y nada más — nunca con una
    /// lista vacía de la que no se puede salir.
    #[test]
    fn an_unreadable_folder_keeps_the_way_out() {
        let l = list_entries(Path::new("Z:/no/existe/seguro"), Search::Folder);
        assert_eq!(l.len(), 1);
        assert!(l[0].is_parent);
    }

    /// Y la raíz de una unidad no tiene «..»: subir de `C:\` no lleva a ningún
    /// lado, y una fila que no hace nada es peor que ninguna.
    #[test]
    fn at_the_root_there_is_nowhere_up() {
        let root = if cfg!(windows) { "C:\\" } else { "/" };
        let l = list_entries(Path::new(root), Search::Folder);
        assert!(
            !l.first().map(|e| e.is_parent).unwrap_or(false),
            "{:?}",
            l.first()
        );
    }

    #[test]
    fn the_rom_count_uses_the_library_table() {
        let d = tree();
        // Sonic.md cuenta; core.dll y notas.txt no.
        assert_eq!(count_roms(d.path()), 1);
    }
}
