//! porting.rs — llevarse los datos y volver a traerlos (PLAY-EP08.3, #620).
//!
//! # Irse tiene que ser fácil
//!
//! Un jugador que no puede sacar sus partidas está atrapado, y un reproductor
//! que atrapa no es un reproductor. El export es **un ZIP normal**: se abre con
//! el explorador de archivos, se copia a un pendrive, se manda por correo y se
//! importa en otra máquina sin Play de por medio. Nada de un formato propio que
//! sólo nosotros sabemos leer.
//!
//! # Importar AGREGA, nunca pisa
//!
//! Es la misma regla que restaurar (#604): lo importado entra como una versión
//! más, con su fecha, y lo que había sigue estando. Importar sobrescribiendo
//! convertiría «me traje las partidas de la otra PC» en «perdí las de ésta», y
//! el jugador se enteraría cuando ya no hay vuelta.
//!
//! # Desactivar la nube no borra nada
//!
//! Ni local ni remoto. Deja de sincronizar y punto: borrar lo remoto es OTRA
//! acción, explícita y con constancia (`/v1/guardados/eliminar-todo`). Juntarlas
//! haría que apagar una preferencia destruyera datos.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Qué se llevó el export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportSummary {
    pub file: PathBuf,
    /// Cuántos guardados entraron.
    pub saves: usize,
    /// Si la configuración entró.
    pub preferences: bool,
}

/// Qué entró al importar.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImportSummary {
    /// Cuántos guardados se agregaron.
    pub saves: usize,
    /// Los que ya estaban idénticos y no se duplicaron.
    pub already_present: usize,
    /// Lo que el ZIP traía y no se pudo interpretar. Se cuenta y se sigue: un
    /// archivo raro no puede impedir que entren los otros veinte.
    pub skipped: usize,
}

const INSIDE_SAVES: &str = "saves/";
const INSIDE_CONFIG: &str = "play_config.toml";

/// Empaqueta los guardados y —si se pide— la configuración.
///
/// El `.zip` lleva el árbol tal cual: `saves/<juego>/<perfil>/<versión>`. Que
/// la estructura sea la misma adentro y afuera es lo que hace que se pueda
/// mirar y entender sin herramientas nuestras.
pub fn export(
    saves: &Path,
    config: Option<&Path>,
    destination: &Path,
) -> std::io::Result<ExportSummary> {
    if let Some(p) = destination.parent() {
        std::fs::create_dir_all(p)?;
    }
    let f = std::fs::File::create(destination)?;
    let mut z = zip::ZipWriter::new(f);
    let options: zip::write::FileOptions<()> = zip::write::FileOptions::default();
    let mut n = 0usize;

    for (rel, bytes) in collect_files(saves)? {
        z.start_file(format!("{INSIDE_SAVES}{rel}"), options)
            .map_err(std::io::Error::other)?;
        z.write_all(&bytes)?;
        n += 1;
    }

    let mut prefs = false;
    if let Some(c) = config
        && let Ok(t) = std::fs::read(c)
    {
        z.start_file(INSIDE_CONFIG, options)
            .map_err(std::io::Error::other)?;
        z.write_all(&t)?;
        prefs = true;
    }
    z.finish().map_err(std::io::Error::other)?;
    Ok(ExportSummary {
        file: destination.to_path_buf(),
        saves: n,
        preferences: prefs,
    })
}

/// Trae un export a este árbol. **Agrega**: nunca reemplaza un archivo que ya
/// esté.
///
/// `cuando` es el sello que llevan las versiones importadas cuyo nombre ya
/// exista acá: entran con su propia marca de tiempo para que se distingan de
/// las de esta máquina en la lista, en vez de desaparecer detrás de ellas.
pub fn import(zip_path: &Path, saves: &Path, when: &str) -> std::io::Result<ImportSummary> {
    let f = std::fs::File::open(zip_path)?;
    let mut z = zip::ZipArchive::new(f).map_err(std::io::Error::other)?;
    let mut r = ImportSummary::default();

    for i in 0..z.len() {
        let mut e = match z.by_index(i) {
            Ok(e) => e,
            Err(_) => {
                r.skipped += 1;
                continue;
            }
        };
        if e.is_dir() {
            continue;
        }
        let name = e.name().to_string();
        let Some(rel) = name.strip_prefix(INSIDE_SAVES) else {
            // La configuración NO se importa sola: cambiar las preferencias de
            // alguien porque trajo sus partidas sería una sorpresa. Se cuenta
            // como salteada para que el resumen no mienta sobre lo que había.
            r.skipped += 1;
            continue;
        };
        // Nada de rutas que salgan del árbol: un ZIP es un archivo que trajo
        // otro, y `../../` adentro de un nombre es el ataque más viejo que hay.
        if rel.contains("..") || rel.starts_with('/') || rel.contains(':') {
            r.skipped += 1;
            continue;
        }
        let mut bytes = Vec::new();
        if e.read_to_end(&mut bytes).is_err() {
            r.skipped += 1;
            continue;
        }

        let destination = saves.join(rel);
        if destination.exists() {
            // Ya está y es idéntico: no se duplica.
            if std::fs::read(&destination)
                .map(|d| d == bytes)
                .unwrap_or(false)
            {
                r.already_present += 1;
                continue;
            }
            // Mismo nombre y distinto contenido: entra AL LADO, con su sello.
            // Pisar acá sería exactamente lo que este módulo promete no hacer.
            let alternate_destination = with_badge(&destination, when);
            if let Some(p) = alternate_destination.parent() {
                std::fs::create_dir_all(p)?;
            }
            crate::saves::write_atomic(&alternate_destination, &bytes)?;
            r.saves += 1;
            continue;
        }
        if let Some(p) = destination.parent() {
            std::fs::create_dir_all(p)?;
        }
        crate::saves::write_atomic(&destination, &bytes)?;
        r.saves += 1;
    }
    Ok(r)
}

/// `partida.srm` → `partida-importado-20260829T120000Z.srm`.
fn with_badge(p: &Path, when: &str) -> PathBuf {
    let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("");
    let base = p
        .file_stem()
        .and_then(|x| x.to_str())
        .unwrap_or("importado");
    let name = if ext.is_empty() {
        format!("{base}-importado-{when}")
    } else {
        format!("{base}-importado-{when}.{ext}")
    };
    p.with_file_name(name)
}

/// Todos los archivos bajo `raiz`, con su ruta relativa en formato ZIP.
fn collect_files(root: &Path) -> std::io::Result<Vec<(String, Vec<u8>)>> {
    let mut out = Vec::new();
    if !root.is_dir() {
        return Ok(out);
    }
    let mut pending = vec![root.to_path_buf()];
    while let Some(d) = pending.pop() {
        for e in std::fs::read_dir(&d)?.flatten() {
            let p = e.path();
            if p.is_dir() {
                pending.push(p);
                continue;
            }
            let Ok(rel) = p.strip_prefix(root) else {
                continue;
            };
            let rel = rel.to_string_lossy().replace('\\', "/");
            out.push((rel, std::fs::read(&p)?));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

#[cfg(test)]
mod tests;
