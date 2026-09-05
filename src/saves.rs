//! saves.rs — dónde vive una partida guardada, y cuántas versiones (#603).
//!
//! # El layout
//!
//! ```text
//! <datos>/saves/<game_id>/<perfil>/
//!     sram.srm                                 la pila del cartucho
//!     estado-20260827T211500Z-a1b2c3d4.bin     una versión, con su revisión de ROM
//!     estado-20260827T211500Z-a1b2c3d4.png     su miniatura
//! ```
//!
//! Antes era un archivo por juego —`<saves>/<game_id>.bin`— escrito con
//! `fopen`/`fwrite`, y eso deja tres agujeros.
//!
//! **Un apagón lo corrompe.** Escribir encima del guardado bueno significa que
//! entre el primer byte y el último no hay partida: cortar la luz ahí la borra.
//! Acá se escribe a un temporal y se reemplaza el destino sin borrarlo primero.
//! Esto evita publicar datos parciales; no garantiza persistencia ante apagones.
//!
//! **Una versión sola no perdona.** Guardar arriba de un guardado es la forma
//! más común de perder veinte horas, y es irreversible por definición. Con N
//! versiones fechadas, «me guardé en el lugar equivocado» deja de ser el final.
//!
//! **El perfil y la ROM importan.** Un savestate es una foto de la memoria del
//! emulador: cargarlo sobre otra revisión de la ROM no da un error, da un juego
//! que se comporta raro media hora después. La revisión viaja en el NOMBRE —no
//! en un índice aparte— porque así sobrevive a copiar la carpeta, que es lo que
//! la gente hace con sus guardados. Y el perfil separa las carpetas porque el
//! estado de una partida en HD no es el de la misma partida en original.
//!
//! # Por qué no hay ranuras numeradas
//!
//! `savestate_slots.rs` implementaba ranuras 0..N sobre el layout plano y no lo
//! usaba nadie. Una ranura obliga a elegir DÓNDE guardar antes de saber si el
//! guardado va a servir, y el error clásico —pisar la ranura buena— es
//! exactamente lo que este layout hace imposible. Se guarda siempre en una
//! versión nueva, y elegir es cosa de restaurar.

use std::path::{Path, PathBuf};

/// El perfil que se asume cuando no hay ninguno.
///
/// `original` y no vacío: es el perfil que SIEMPRE existe (#293, implícito en
/// todo pack), así que una carpeta `original/` es una que el juego va a volver
/// a mirar. Una carpeta con nombre vacío no la mira nadie.
pub const DEFAULT_PROFILE: &str = "original";

/// Cuántas versiones se conservan por juego y perfil.
///
/// Doce y no «todas»: un savestate de Mega Drive pesa cientos de kilobytes y
/// guardar mucho es normal, así que sin tope la carpeta crece sin fin. Doce
/// cubre una tarde entera de idas y vueltas, que es el horizonte en el que
/// alguien se acuerda de que tenía algo mejor.
pub const MAX_VERSIONS: usize = 12;

/// Una versión guardada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub path: PathBuf,
    /// UTC compacto: `20260827T211500Z`. Ordena alfabéticamente igual que
    /// cronológicamente, que es lo que deja ordenar sin parsear.
    pub when: String,
    /// CRC32 de la ROM con la que se hizo, en hexadecimal.
    pub rom_crc32: String,
    pub bytes: u64,
}

impl Version {
    /// ¿Sirve para esta ROM?
    ///
    /// Una versión SIN revisión —las migradas del layout viejo, que no la
    /// guardaba— se acepta: no saberlo no es lo mismo que saber que está mal, y
    /// rechazarla dejaría a quien viene de una versión anterior de Play sin sus
    /// partidas. Lo que no se hace es afirmar que coincide.
    pub fn is_compatible_with(&self, rom_crc32: &str) -> bool {
        self.rom_crc32.is_empty()
            || rom_crc32.is_empty()
            || self.rom_crc32.eq_ignore_ascii_case(rom_crc32)
    }

    /// La miniatura, si el Runtime la dejó.
    pub fn thumbnail(&self) -> Option<PathBuf> {
        let p = self.path.with_extension("png");
        p.is_file().then_some(p)
    }
}

/// `<datos>/saves/<game_id>/<perfil>/`.
pub fn dir(saves: &Path, game_id: &str, profile: &str) -> PathBuf {
    let p = if profile.is_empty() {
        DEFAULT_PROFILE
    } else {
        profile
    };
    saves.join(sanitize(game_id)).join(sanitize(p))
}

/// La SRAM del cartucho. Una sola: es la pila del juego, no una foto.
pub fn sram_path(saves: &Path, game_id: &str, profile: &str) -> PathBuf {
    dir(saves, game_id, profile).join("sram.srm")
}

/// Un `game_id` o un perfil no son nombres de archivo. Se recortan a lo que
/// cualquier sistema acepta, sin inventar: lo que no pasa se vuelve `_`.
fn sanitize(s: &str) -> String {
    let sanitized: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if sanitized.is_empty() {
        "sin_nombre".to_string()
    } else {
        sanitized
    }
}

/// El nombre de una versión: `estado-<cuando>-<crc>.bin`.
pub fn version_name(when: &str, rom_crc32: &str) -> String {
    let crc = if rom_crc32.is_empty() {
        "sinrev".to_string()
    } else {
        sanitize(rom_crc32).to_lowercase()
    };
    format!("estado-{}-{crc}.bin", sanitize(when))
}

fn parse(path: &Path) -> Option<Version> {
    let n = path.file_name()?.to_str()?;
    let body = n.strip_prefix("estado-")?.strip_suffix(".bin")?;
    // La revisión es lo que sigue al ÚLTIMO guion: el timestamp lleva los
    // suyos, así que partir por el primero se comería la fecha.
    let (when, crc) = body.rsplit_once('-')?;
    Some(Version {
        when: when.to_string(),
        rom_crc32: if crc == "sinrev" {
            String::new()
        } else {
            crc.to_string()
        },
        bytes: std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
        path: path.to_path_buf(),
    })
}

/// Las versiones guardadas, **de la más nueva a la más vieja**.
///
/// Ese orden y no el del directorio: quien abre esta lista busca la última, y
/// el sistema de archivos entrega en el orden que quiere.
pub fn versions(saves: &Path, game_id: &str, profile: &str) -> Vec<Version> {
    let d = dir(saves, game_id, profile);
    let Ok(entries) = std::fs::read_dir(&d) else {
        return Vec::new();
    };
    let mut v: Vec<Version> = entries.flatten().filter_map(|e| parse(&e.path())).collect();
    v.sort_by(|a, b| b.when.cmp(&a.when));
    v
}

/// La última versión, que es la partida en curso.
pub fn last(saves: &Path, game_id: &str, profile: &str) -> Option<Version> {
    versions(saves, game_id, profile).into_iter().next()
}

/// Escribe una versión mediante [`write_atomic`], propagando sus errores.
pub fn write_version(
    saves: &Path,
    game_id: &str,
    profile: &str,
    when: &str,
    rom_crc32: &str,
    data: &[u8],
) -> std::io::Result<Version> {
    let d = dir(saves, game_id, profile);
    std::fs::create_dir_all(&d)?;
    let destination = d.join(version_name(when, rom_crc32));
    write_atomic(&destination, data)?;
    parse(&destination).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "el nombre de la versión no se pudo leer",
        )
    })
}

/// Publica un guardado completo con reemplazo atómico en el mismo directorio.
/// Sincroniza el temporal antes de reemplazar; nunca borra el destino primero.
/// Los errores de escritura, sincronización y reemplazo llegan al llamador.
/// Se intenta limpiar el temporal ante errores (incluido al cerrar por RAII).
/// Si el proceso muere o la limpieza falla puede quedar `.save-*`; no se carga
/// automáticamente y se puede eliminar con Play cerrado. No se sincroniza el
/// directorio: no se garantiza durabilidad ante cortes de energía.
pub fn write_atomic(destination: &Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    write_atomic_with(
        destination,
        data,
        |file, bytes| file.write_all(bytes),
        |tmp, path| tmp.persist(path).map(|_| ()).map_err(|error| error.error),
    )
}

fn write_atomic_with(
    destination: &Path,
    data: &[u8],
    write: impl FnOnce(&mut std::fs::File, &[u8]) -> std::io::Result<()>,
    replace: impl FnOnce(tempfile::NamedTempFile, &Path) -> std::io::Result<()>,
) -> std::io::Result<()> {
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let mut tmp = tempfile::Builder::new()
        .prefix(".save-")
        .tempfile_in(parent)?;
    write(tmp.as_file_mut(), data)?;
    tmp.as_file().sync_all()?;
    replace(tmp, destination)
}

/// Restaura una versión anterior: **la copia como versión NUEVA**.
///
/// **Es el CA de la issue**: restaurar conserva una copia del actual. No pisa
/// nada — la versión que estaba en curso sigue en la carpeta, con su fecha, y
/// se puede volver a ella. Restaurar sobrescribiendo convertiría «me equivoqué
/// de guardado» en una pérdida definitiva, que es exactamente el problema que
/// las versiones vienen a resolver.
pub fn restore(
    saves: &Path,
    game_id: &str,
    profile: &str,
    v: &Version,
    when: &str,
) -> std::io::Result<Version> {
    let data = std::fs::read(&v.path)?;
    let new_version = write_version(saves, game_id, profile, when, &v.rom_crc32, &data)?;
    // La miniatura viaja con su estado: sin ella, la versión restaurada se
    // vería vacía en una lista donde todas las demás tienen imagen.
    if let Some(m) = v.thumbnail() {
        let _ = std::fs::copy(&m, new_version.path.with_extension("png"));
    }
    Ok(new_version)
}

/// Conserva las `max` versiones más nuevas y borra el resto. Devuelve cuántas
/// borró.
///
/// Se poda DESPUÉS de escribir y nunca antes: podar primero podría dejar la
/// carpeta sin nada si la escritura que sigue falla.
pub fn prune(saves: &Path, game_id: &str, profile: &str, max: usize) -> usize {
    let v = versions(saves, game_id, profile);
    let mut n = 0;
    for old_version in v.into_iter().skip(max) {
        if std::fs::remove_file(&old_version.path).is_ok() {
            n += 1;
            let _ = std::fs::remove_file(old_version.path.with_extension("png"));
        }
    }
    n
}

/// Migra los guardados del layout plano al nuevo. Devuelve cuántos movió.
///
/// El layout viejo era `<saves>/<game_id>.bin` y `<saves>/<game_id>.<n>.bin`, y
/// no guardaba ni perfil ni revisión de ROM. Van a `original/` porque es el
/// perfil que siempre existe: un guardado hecho antes de que hubiera perfiles
/// no puede pertenecer a ninguno en particular, y `original` es el único que
/// cualquier pack —o la ausencia de pack— va a volver a mirar.
///
/// Se MUEVE y no se copia: dejar el archivo viejo en su lugar significa que la
/// próxima versión de Play lo vuelve a migrar, y la partida que el jugador
/// siguió jugando aparecería duplicada y desactualizada.
pub fn migrate_legacy(saves: &Path, game_id: &str, when: &str) -> usize {
    let mut n = 0;
    let Ok(entries) = std::fs::read_dir(saves) else {
        return 0;
    };
    let mut old_entries: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let Some(name) = p.file_name().and_then(|x| x.to_str()) else {
                return false;
            };
            p.is_file()
                && name.ends_with(".bin")
                && (name == format!("{game_id}.bin") || name.starts_with(&format!("{game_id}.")))
        })
        .collect();
    // El orden es EXPLÍCITO y no alfabético: en el layout viejo,
    // `<game_id>.bin` era la ranura 0 —la principal, la que sincronizaba con la
    // nube— y las numeradas eran las secundarias. Así que la 0 es la partida en
    // curso y va última (la más nueva), y las numeradas quedan detrás en orden
    // descendente. Dejarlo al orden alfabético daría lo mismo por casualidad
    // —'1' ordena antes que 'b'— y esa clase de casualidad se rompe el día que
    // alguien renombra algo.
    let slot = |p: &PathBuf| -> u32 {
        p.file_name()
            .and_then(|x| x.to_str())
            .and_then(|n| n.strip_prefix(&format!("{game_id}."))?.strip_suffix(".bin"))
            .and_then(|r| r.parse::<u32>().ok())
            .unwrap_or(0)
    };
    old_entries.sort_by_key(|p| std::cmp::Reverse(slot(p)));

    for (i, p) in old_entries.iter().enumerate() {
        let Ok(data) = std::fs::read(p) else { continue };
        // Cada uno con su propio instante para que no colisionen los nombres y
        // el orden se conserve.
        let timestamp = format!("{when}{i:02}");
        if write_version(saves, game_id, DEFAULT_PROFILE, &timestamp, "", &data).is_ok() {
            let _ = std::fs::remove_file(p);
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn windows_replacement_denied_by_reader_keeps_old_save() {
        use std::os::windows::fs::OpenOptionsExt;
        let t = saves();
        let destination = t.path().join("save.bin");
        std::fs::write(&destination, b"old").unwrap();
        // Allow readers but deny delete/replace while this handle is held.
        let reader = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&destination)
            .unwrap();
        assert!(write_atomic(&destination, b"new").is_err());
        assert_eq!(std::fs::read(&destination).unwrap(), b"old");
        assert_eq!(std::fs::read_dir(t.path()).unwrap().count(), 1);
        drop(reader);
        write_atomic(&destination, b"new").unwrap();
        assert_eq!(std::fs::read(&destination).unwrap(), b"new");
    }

    #[test]
    fn partial_write_failure_preserves_destination_and_cleans_temporary() {
        use std::io::Write;
        let t = saves();
        let destination = t.path().join("save.bin");
        std::fs::write(&destination, b"old").unwrap();
        let result = write_atomic_with(
            &destination,
            b"new data",
            |file, bytes| {
                file.write_all(&bytes[..2])?;
                Err(std::io::Error::other("injected write failure"))
            },
            |_, _| panic!("replacement must not run after failed write"),
        );
        assert!(result.unwrap_err().to_string().contains("injected write"));
        assert_eq!(std::fs::read(&destination).unwrap(), b"old");
        assert_eq!(std::fs::read_dir(t.path()).unwrap().count(), 1);
    }

    #[test]
    fn replacement_failure_preserves_destination_and_cleans_temporary() {
        use std::io::Write;
        let t = saves();
        let destination = t.path().join("save.bin");
        std::fs::write(&destination, b"old").unwrap();
        let result = write_atomic_with(
            &destination,
            b"new data",
            |file, bytes| file.write_all(bytes),
            |tmp, path| {
                assert_eq!(std::fs::read(tmp.path()).unwrap(), b"new data");
                assert_eq!(std::fs::read(path).unwrap(), b"old");
                Err(std::io::Error::other("injected replacement failure"))
            },
        );
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("injected replacement")
        );
        assert_eq!(std::fs::read(&destination).unwrap(), b"old");
        assert_eq!(std::fs::read_dir(t.path()).unwrap().count(), 1);
    }

    fn saves() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    /// EL CA: un apagado a mitad de escritura no corrompe.
    ///
    /// Lo que se prueba es la propiedad que lo garantiza — que el destino no se
    /// abre nunca para escribir, sino que aparece entero por un `rename`. Un
    /// test no puede cortar la luz; lo que sí puede es exigir que el guardado
    /// bueno siga intacto mientras el nuevo se escribe, y que ningún temporal
    /// quede tirado.
    #[test]
    fn writing_does_not_touch_the_good_save_until_the_end() {
        let t = saves();
        let v1 = write_version(
            t.path(),
            "golden",
            "hd",
            "20260827T200000Z",
            "a1b2c3d4",
            b"partida buena",
        )
        .unwrap();
        assert_eq!(std::fs::read(&v1.path).unwrap(), b"partida buena");

        let v2 = write_version(
            t.path(),
            "golden",
            "hd",
            "20260827T210000Z",
            "a1b2c3d4",
            b"partida nueva",
        )
        .unwrap();
        assert_eq!(
            std::fs::read(&v1.path).unwrap(),
            b"partida buena",
            "escribir una version nueva no puede tocar la anterior"
        );
        assert_eq!(std::fs::read(&v2.path).unwrap(), b"partida nueva");

        let leftovers: Vec<_> = std::fs::read_dir(dir(t.path(), "golden", "hd"))
            .unwrap()
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "quedo un temporal tirado: {leftovers:?}"
        );
    }

    /// Y pisar un archivo existente tampoco lo deja a medias: se escribe al
    /// lado y recién entonces se reemplaza.
    #[test]
    fn atomic_write_replaces_whole() {
        let t = saves();
        let p = t.path().join("sub").join("sram.srm");
        write_atomic(&p, b"viejo").unwrap();
        write_atomic(&p, b"nuevo mas largo").unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"nuevo mas largo");
        assert!(!p.with_extension("tmp").exists());
    }

    /// EL OTRO CA: restaurar CONSERVA una copia del actual.
    ///
    /// Restaurar sobrescribiendo convertiría «me equivoqué de guardado» en una
    /// pérdida definitiva — justo el problema que las versiones vienen a
    /// resolver.
    #[test]
    fn restoring_keeps_the_current_one() {
        let t = saves();
        let old_version = write_version(
            t.path(),
            "golden",
            "hd",
            "20260827T100000Z",
            "a1b2c3d4",
            b"antes del jefe",
        )
        .unwrap();
        write_version(
            t.path(),
            "golden",
            "hd",
            "20260827T200000Z",
            "a1b2c3d4",
            b"despues del jefe",
        )
        .unwrap();

        let new_version =
            restore(t.path(), "golden", "hd", &old_version, "20260827T210000Z").unwrap();

        let v = versions(t.path(), "golden", "hd");
        assert_eq!(v.len(), 3, "restaurar AGREGA, no reemplaza: {v:?}");
        assert_eq!(
            v[0].path, new_version.path,
            "la restaurada queda como la actual"
        );
        assert_eq!(std::fs::read(&new_version.path).unwrap(), b"antes del jefe");
        assert_eq!(
            std::fs::read(&v[1].path).unwrap(),
            b"despues del jefe",
            "LA QUE ESTABA EN CURSO SIGUE AHI"
        );
    }

    /// Las versiones vienen de la más nueva a la más vieja: quien abre la lista
    /// busca la última, y el directorio entrega en el orden que quiere.
    #[test]
    fn versions_come_newest_to_oldest() {
        let t = saves();
        for when in ["20260827T100000Z", "20260827T220000Z", "20260827T150000Z"] {
            write_version(t.path(), "golden", "hd", when, "a1b2c3d4", b"x").unwrap();
        }
        let v = versions(t.path(), "golden", "hd");
        assert_eq!(
            v.iter().map(|x| x.when.as_str()).collect::<Vec<_>>(),
            ["20260827T220000Z", "20260827T150000Z", "20260827T100000Z"]
        );
        assert_eq!(
            last(t.path(), "golden", "hd").unwrap().when,
            "20260827T220000Z"
        );
    }

    /// El perfil separa las carpetas: el estado de una partida en HD no es el
    /// de la misma partida en original.
    #[test]
    fn each_profile_has_its_saves() {
        let t = saves();
        write_version(t.path(), "golden", "hd", "20260827T100000Z", "a1", b"hd").unwrap();
        write_version(
            t.path(),
            "golden",
            "original",
            "20260827T110000Z",
            "a1",
            b"og",
        )
        .unwrap();

        assert_eq!(versions(t.path(), "golden", "hd").len(), 1);
        assert_eq!(versions(t.path(), "golden", "original").len(), 1);
        assert_eq!(
            std::fs::read(&last(t.path(), "golden", "hd").unwrap().path).unwrap(),
            b"hd"
        );

        // Y un perfil vacío cae en `original`, que es el que siempre existe.
        assert_eq!(
            dir(t.path(), "golden", ""),
            dir(t.path(), "golden", "original")
        );
    }

    /// La revisión de ROM viaja EN EL NOMBRE, así que sobrevive a copiar la
    /// carpeta — que es lo que la gente hace con sus guardados.
    #[test]
    fn the_rom_revision_travels_in_the_name() {
        let t = saves();
        let v = write_version(
            t.path(),
            "golden",
            "hd",
            "20260827T100000Z",
            "A1B2C3D4",
            b"x",
        )
        .unwrap();
        assert!(
            v.path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .contains("a1b2c3d4")
        );

        // Y se lee de vuelta, incluso desde otra carpeta.
        let read_back = &versions(t.path(), "golden", "hd")[0];
        assert_eq!(read_back.rom_crc32, "a1b2c3d4");
        assert_eq!(
            read_back.when, "20260827T100000Z",
            "el timestamp lleva guiones: partir por el primero se comeria la fecha"
        );

        assert!(read_back.is_compatible_with("a1b2c3d4"));
        assert!(
            read_back.is_compatible_with("A1B2C3D4"),
            "la revision no distingue mayusculas"
        );
        assert!(
            !read_back.is_compatible_with("ffffffff"),
            "otra revision de ROM"
        );
    }

    /// Una versión sin revisión —las migradas del layout viejo— se acepta: no
    /// saberlo no es lo mismo que saber que está mal, y rechazarla dejaría sin
    /// partidas a quien viene de una versión anterior de Play.
    #[test]
    fn a_version_without_revision_is_not_rejected() {
        let t = saves();
        write_version(t.path(), "golden", "hd", "20260827T100000Z", "", b"x").unwrap();
        let v = &versions(t.path(), "golden", "hd")[0];
        assert_eq!(v.rom_crc32, "");
        assert!(v.is_compatible_with("a1b2c3d4"));
    }

    /// Podar conserva las más nuevas. Un tope existe porque sin él la carpeta
    /// crece sin fin; conservar las viejas sería conservar lo que no sirve.
    #[test]
    fn pruning_keeps_the_newest_ones() {
        let t = saves();
        for i in 0..8 {
            write_version(
                t.path(),
                "golden",
                "hd",
                &format!("20260827T1{i:05}Z"),
                "a1",
                b"x",
            )
            .unwrap();
        }
        assert_eq!(prune(t.path(), "golden", "hd", 3), 5);
        let v = versions(t.path(), "golden", "hd");
        assert_eq!(v.len(), 3);
        assert_eq!(v[0].when, "20260827T100007Z");
        assert_eq!(v[2].when, "20260827T100005Z");
    }

    /// La migración MUEVE, no copia: dejar el viejo en su lugar haría que la
    /// próxima versión de Play lo migrara otra vez, y la partida que el jugador
    /// siguió jugando aparecería duplicada y desactualizada.
    #[test]
    fn the_migration_moves_the_old_saves() {
        let t = saves();
        std::fs::write(t.path().join("golden.bin"), b"principal").unwrap();
        std::fs::write(t.path().join("golden.1.bin"), b"ranura 1").unwrap();
        std::fs::write(t.path().join("sonic.bin"), b"otro juego").unwrap();

        assert_eq!(migrate_legacy(t.path(), "golden", "20260827T2100"), 2);

        assert!(!t.path().join("golden.bin").exists(), "el viejo se MUEVE");
        assert!(!t.path().join("golden.1.bin").exists());
        assert!(
            t.path().join("sonic.bin").is_file(),
            "el de otro juego no se toca"
        );

        let v = versions(t.path(), "golden", "original");
        assert_eq!(v.len(), 2);
        // La ranura 0 (`golden.bin`) era la principal —la partida en curso— así
        // que queda como la versión más nueva; las numeradas, detrás.
        assert_eq!(std::fs::read(&v[0].path).unwrap(), b"principal");
        assert_eq!(std::fs::read(&v[1].path).unwrap(), b"ranura 1");

        // Y correrla de nuevo no hace nada: ya no queda nada que migrar.
        assert_eq!(migrate_legacy(t.path(), "golden", "20260827T2200"), 0);
        assert_eq!(versions(t.path(), "golden", "original").len(), 2);
    }

    /// Un `game_id` con caracteres que no son nombre de archivo no rompe nada
    /// ni escapa de la carpeta de guardados.
    #[test]
    fn a_weird_game_id_does_not_escape_the_folder() {
        let t = saves();
        let d = dir(t.path(), "../../etc", "..");
        assert!(d.starts_with(t.path()), "{}", d.display());
        assert!(!d.to_string_lossy().contains(".."));
    }
}
