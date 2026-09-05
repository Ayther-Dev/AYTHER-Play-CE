//! pack_install.rs — instalar, actualizar y REVERTIR un pack (#315).
//!
//! El pack instalado es un archivo suelto en `packs_install_dir`: `<id>.ay`. Lo
//! que faltaba no era copiarlo —eso ya lo hacía la descarga del Hub— sino el
//! ciclo de vida alrededor:
//!
//! ## Instalar no puede destruir lo que funciona
//!
//! El flujo del Hub descargaba **encima** del pack instalado y, si la firma no
//! validaba, lo BORRABA (`remove_file(&dest)`). Es decir: una descarga cortada
//! o un pack mal firmado dejaban al jugador sin el pack que venía usando, que
//! estaba bien. Acá la escritura va siempre a `<id>.ay.new`, se verifica ahí, y
//! recién entonces se rota. Un fallo en cualquier punto deja lo instalado
//! intacto.
//!
//! ## La versión anterior se conserva UNA generación
//!
//! Al rotar, lo que estaba pasa a `<id>.ay.prev`. Una sola: guardar N versiones
//! de un pack de cientos de MB para un jugador que va a revertir una vez —si es
//! que revierte— es un costo que nadie pidió. `rollback` las intercambia, así
//! que revertir dos veces vuelve al punto de partida en vez de dejar al jugador
//! sin salida.
//!
//! ## Los savestates no se tocan
//!
//! Viven en `saves_dir` (`config.rs`), otro árbol. Este módulo sólo escribe
//! dentro del directorio de packs, y su test lo comprueba sobre un `saves/`
//! poblado: la garantía no es una promesa del comentario, es un assert.

use std::path::{Path, PathBuf};

use crate::verifier::{self, Audit};

/// Portable file identifier (at most 128 bytes), including ordinary pack stems
/// with internal spaces or dots. No leading/trailing dots or spaces.
/// Windows device names are excluded even when followed by an extension.
pub fn validate_pack_id(id: &str) -> Result<(), String> {
    let upper = id
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end()
        .to_ascii_uppercase();
    let reserved = matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (upper.len() == 4
            && (upper.starts_with("COM") || upper.starts_with("LPT"))
            && matches!(upper.as_bytes()[3], b'1'..=b'9'));
    if id.is_empty()
        || id.len() > 128
        || id.starts_with(['.', ' '])
        || id.ends_with(['.', ' '])
        || reserved
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.' | b' '))
    {
        return Err(format!("identificador de pack inválido: {id:?}"));
    }
    Ok(())
}

/// Check every lifecycle destination before any mutation, including stale staging.
/// Existing links/reparse points and directories are rejected, not followed.
/// The directory must not be concurrently modified by another process.
fn validate_paths(packs_dir: &Path, id: &str) -> Result<(), String> {
    validate_pack_id(id)?;
    for suffix in ["ay", "ay.prev", "ay.new", "ay.swap", "ay.part"] {
        let path = packs_dir.join(format!("{id}.{suffix}"));
        if path.parent() != Some(packs_dir) {
            return Err(format!(
                "ruta fuera del directorio de packs: {}",
                path.display()
            ));
        }
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) => {
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    if metadata.file_attributes() & 0x400 != 0 {
                        return Err(format!("reparse point no permitido: {}", path.display()));
                    }
                }
                if !metadata.file_type().is_file() {
                    return Err(format!("archivo de pack no regular: {}", path.display()));
                }
                let root = packs_dir.canonicalize().map_err(|e| e.to_string())?;
                let resolved = path.canonicalize().map_err(|e| e.to_string())?;
                if resolved.parent() != Some(root.as_path()) {
                    return Err(format!(
                        "ruta fuera del directorio de packs: {}",
                        path.display()
                    ));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(())
}

/// Qué pasó al instalar.
#[derive(Debug, Clone, PartialEq)]
pub enum InstallResult {
    /// Instalado. `replaced` = había una versión previa, que quedó conservada.
    Installed { replaced: bool },
    /// No es un pack legible, o su índice no cierra: NO se tocó lo instalado.
    ///
    /// **Es el único rechazo que CE sabe emitir.** Play tiene además dos por
    /// procedencia —firma inválida y sello que el registro no avala—; acá no
    /// existen, y no como una simplificación: un tipo que ofreciera decir «la
    /// firma no valida» invitaría a que alguien lo devolviera, y este binario
    /// no tiene con qué juzgar una firma (ADR-005, `verifier.rs`).
    Invalid(String),
    /// Error de disco (copiar, renombrar).
    Io(String),
}

/// Ruta del pack instalado con ese id.
pub fn installed_path(packs_dir: &Path, pack_id: &str) -> PathBuf {
    packs_dir.join(format!("{pack_id}.ay"))
}

/// Ruta de la versión anterior conservada (una generación).
pub fn previous_path(packs_dir: &Path, pack_id: &str) -> PathBuf {
    packs_dir.join(format!("{pack_id}.ay.prev"))
}

/// Ruta de staging: donde se escribe lo que todavía no se verificó.
pub fn staging_path(packs_dir: &Path, pack_id: &str) -> PathBuf {
    packs_dir.join(format!("{pack_id}.ay.new"))
}

/// ¿Hay una versión anterior a la que volver?
pub fn has_previous(packs_dir: &Path, pack_id: &str) -> bool {
    validate_paths(packs_dir, pack_id).is_ok() && previous_path(packs_dir, pack_id).is_file()
}

/// Instala un `.ay` que ya está en el disco del jugador (o el que acaba de
/// bajar el Hub, una vez escrito en staging).
///
/// `src` se COPIA: un pack que el jugador tiene en Descargas sigue estando ahí
/// después de instalarlo. Mover un archivo que el usuario no puso donde
/// nosotros queremos es una sorpresa, no una optimización.
pub fn install_local(packs_dir: &Path, pack_id: &str, src: &Path) -> InstallResult {
    if let Err(error) = validate_paths(packs_dir, pack_id) {
        return InstallResult::Invalid(error);
    }
    if let Err(e) = std::fs::create_dir_all(packs_dir) {
        return InstallResult::Io(e.to_string());
    }
    let staging = staging_path(packs_dir, pack_id);
    // Un staging viejo de un intento anterior no se hereda.
    let _ = std::fs::remove_file(&staging);
    if let Err(e) = std::fs::copy(src, &staging) {
        return InstallResult::Io(e.to_string());
    }
    promote(packs_dir, pack_id)
}

/// Verifica lo que haya en staging y, si vale, lo pone en su lugar
/// conservando la versión anterior. El paso que la descarga del Hub tiene que
/// llamar DESPUÉS de escribir `<id>.ay.new` — nunca sobre el pack instalado.
pub fn promote(packs_dir: &Path, pack_id: &str) -> InstallResult {
    if let Err(error) = validate_paths(packs_dir, pack_id) {
        return InstallResult::Invalid(error);
    }
    let staging = staging_path(packs_dir, pack_id);
    if !staging.is_file() {
        return InstallResult::Io("no hay nada en staging".into());
    }

    // Verificar ANTES de tocar nada: el pack instalado sigue siendo el bueno
    // hasta que el nuevo demuestre que se puede confiar en él.
    // CE audita SIEMPRE, y acá está la diferencia con la edición paga: allá un
    // pack del Hub llega con el sello del servicio y ése decide, así que la
    // auditoría por entrada es redundante. Acá no hay sello — la auditoría es
    // lo único que hay, así que se hace antes de decir «instalado».
    let audit = verifier::audit(&staging);
    if !audit.is_installable() {
        let _ = std::fs::remove_file(&staging);
        return match audit {
            Audit::InvalidPack(m) | Audit::Mismatch(m) => InstallResult::Invalid(m),
            _ => unreachable!("se_puede_instalar() ya cubrió el resto"),
        };
    }

    // Y que el MOTOR pueda abrirlo (#312). La firma dice que nadie lo tocó;
    // esto dice que SIRVE. Son cosas distintas: un pack con el manifest
    // incompleto pasa la verificación de integridad sin problemas y después no
    // abre — instalarlo sería dejarle al jugador un archivo que descubre roto
    // recién al querer jugar.
    {
        let path = staging.to_string_lossy().to_string();
        if let Err(e) = ayther_core::archive_vfs::AyArchive::open_verbose(&path) {
            let _ = std::fs::remove_file(&staging);
            return InstallResult::Invalid(format!("{e:?}"));
        }
    }

    let current = installed_path(packs_dir, pack_id);
    let previous = previous_path(packs_dir, pack_id);
    let replaced = current.is_file();
    if replaced {
        // La generación anterior a la anterior se descarta acá y no antes: si
        // el rename siguiente falla, el jugador se queda con DOS versiones
        // buenas en vez de con ninguna.
        let _ = std::fs::remove_file(&previous);
        if let Err(e) = std::fs::rename(&current, &previous) {
            return InstallResult::Io(e.to_string());
        }
    }
    if let Err(e) = std::fs::rename(&staging, &current) {
        // Deshacer: volver a poner la anterior en su lugar. Sin esto, un fallo
        // acá dejaría al jugador sin pack instalado y con el suyo escondido
        // bajo un nombre que la biblioteca no mira.
        if replaced {
            let _ = std::fs::rename(&previous, &current);
        }
        return InstallResult::Io(e.to_string());
    }
    InstallResult::Installed { replaced }
}

/// Vuelve a la versión anterior. Las INTERCAMBIA (no descarta la actual): un
/// jugador que revierte para probar tiene que poder volver a adelantar, y un
/// rollback que quema la versión nueva convierte una prueba en una decisión.
pub fn rollback(packs_dir: &Path, pack_id: &str) -> Result<(), String> {
    validate_paths(packs_dir, pack_id)?;
    let current = installed_path(packs_dir, pack_id);
    let previous = previous_path(packs_dir, pack_id);
    if !previous.is_file() {
        return Err("no hay versión anterior".into());
    }
    let tmp = packs_dir.join(format!("{pack_id}.ay.swap"));
    let _ = std::fs::remove_file(&tmp);
    if current.is_file() {
        std::fs::rename(&current, &tmp).map_err(|e| e.to_string())?;
    }
    if let Err(e) = std::fs::rename(&previous, &current) {
        if tmp.is_file() {
            let _ = std::fs::rename(&tmp, &current);
        }
        return Err(e.to_string());
    }
    if tmp.is_file() {
        std::fs::rename(&tmp, &previous).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Borra la versión anterior conservada (liberar espacio a pedido). No hay
/// borrado automático por tiempo: un pack que ocupa lugar es visible y se
/// borra cuando el jugador quiere, y una limpieza silenciosa le sacaría el
/// rollback justo cuando lo necesita.
/// Sin botón que lo llame todavía (#320); lo que lo mantiene honesto es su
/// oráculo, y el `allow` no cubre `cargo test`: si el oráculo se va, el aviso vuelve.
#[cfg_attr(not(test), allow(dead_code))]
pub fn discard_previous(packs_dir: &Path, pack_id: &str) -> Result<(), String> {
    validate_paths(packs_dir, pack_id)?;
    let previous = previous_path(packs_dir, pack_id);
    if !previous.is_file() {
        return Ok(());
    }
    std::fs::remove_file(&previous).map_err(|e| e.to_string())
}

/// Qué pasó al desinstalar.
#[derive(Debug, Clone, PartialEq)]
pub enum UninstallResult {
    /// Se sacó el pack. `tenia_anterior` = también había una versión previa
    /// conservada, que se fue con él.
    Uninstalled {
        had_previous: bool,
    },
    /// No había nada con ese id.
    NotInstalled,
    Io(String),
}

/// Saca un pack instalado — **y sólo el pack** (#614).
///
/// La ROM es del jugador y vive donde él la puso; el core también; los
/// guardados viven en `saves_dir`, otro árbol. Desinstalar un pack no puede
/// llevarse ninguna de las tres.
#[cfg_attr(not(test), allow(dead_code))]
pub fn uninstall(packs_dir: &Path, pack_id: &str) -> UninstallResult {
    if let Err(error) = validate_paths(packs_dir, pack_id) {
        return UninstallResult::Io(error);
    }
    let current = installed_path(packs_dir, pack_id);
    let previous = previous_path(packs_dir, pack_id);
    let had_previous = previous.is_file();
    if !current.is_file() && !had_previous {
        return UninstallResult::NotInstalled;
    }
    // El orden importa poco pero el criterio no: si algo falla, se dice cuál,
    // en vez de dejar la mitad borrada en silencio.
    for p in [
        current,
        previous,
        staging_path(packs_dir, pack_id),
        packs_dir.join(format!("{pack_id}.ay.part")),
    ] {
        if p.exists()
            && let Err(e) = std::fs::remove_file(&p)
        {
            return UninstallResult::Io(format!("{}: {e}", p.display()));
        }
    }
    UninstallResult::Uninstalled { had_previous }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn invalid_ids_cannot_mutate_any_lifecycle_path() {
        let tmp = tempfile::tempdir().unwrap();
        let packs = tmp.path().join("packs");
        let source = valid_pack(tmp.path(), "source.ay", "source");
        let absolute = tmp.path().join("witness").to_string_lossy().into_owned();
        for suffix in ["ay", "ay.prev", "ay.new", "ay.swap", "ay.part"] {
            std::fs::write(tmp.path().join(format!("witness.{suffix}")), b"keep").unwrap();
        }
        for id in [
            "",
            ".",
            "..",
            "...",
            "../witness",
            "..\\witness",
            "/witness",
            "C:\\witness",
            "C:witness",
            "a/b",
            "a\\b",
            "CON",
            "lpt1",
            "id:stream",
            absolute.as_str(),
        ] {
            assert!(
                matches!(
                    install_local(&packs, id, &source),
                    InstallResult::Invalid(_)
                ),
                "{id}"
            );
            assert!(
                matches!(promote(&packs, id), InstallResult::Invalid(_)),
                "{id}"
            );
            assert!(rollback(&packs, id).is_err(), "{id}");
            assert!(discard_previous(&packs, id).is_err(), "{id}");
            assert!(
                matches!(uninstall(&packs, id), UninstallResult::Io(_)),
                "{id}"
            );
            assert!(!packs.exists(), "invalid id must not create directories");
            for suffix in ["ay", "ay.prev", "ay.new", "ay.swap", "ay.part"] {
                assert_eq!(
                    std::fs::read(tmp.path().join(format!("witness.{suffix}"))).unwrap(),
                    b"keep"
                );
            }
        }
        for id in ["sonic", "Sonic-USA_01", "123", "Golden Axe HD", "sonic.v1"] {
            assert!(validate_pack_id(id).is_ok());
        }
    }

    #[test]
    fn lifecycle_rejects_nonregular_destinations_before_mutation() {
        let tmp = tempfile::tempdir().unwrap();
        let packs = tmp.path().join("packs");
        let source = valid_pack(tmp.path(), "source.ay", "source");
        std::fs::create_dir_all(packs.join("ga.ay.swap")).unwrap();
        std::fs::write(packs.join("ga.ay"), b"keep").unwrap();
        assert!(matches!(
            install_local(&packs, "ga", &source),
            InstallResult::Invalid(_)
        ));
        assert!(matches!(promote(&packs, "ga"), InstallResult::Invalid(_)));
        assert!(rollback(&packs, "ga").is_err());
        assert!(discard_previous(&packs, "ga").is_err());
        assert!(matches!(uninstall(&packs, "ga"), UninstallResult::Io(_)));
        assert_eq!(std::fs::read(packs.join("ga.ay")).unwrap(), b"keep");
    }

    /// Un `.ay` mínimo SIN firma: el verificador lo acepta como `Unsigned`, que
    /// es lo que hace que estos tests prueben el ciclo de vida y no la firma
    /// (que tiene su propio test en `verifier`).
    fn valid_pack(dir: &Path, name: &str, marker: &str) -> PathBuf {
        let p = dir.join(name);
        let f = std::fs::File::create(&p).unwrap();
        let mut z = zip::ZipWriter::new(f);
        let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default();
        z.start_file("manifest.toml", opts).unwrap();
        // Un manifest COMPLETO, no el mínimo que un parser laxo aceptaría:
        // desde que `promote` exige que el motor pueda abrir el pack, un
        // fixture a medias probaría el rechazo en vez del camino bueno.
        write!(
            z,
            "[pack]\nname = \"{marker}\"\nversion = \"1.0.0\"\n\
                   game_id = \"x\"\nayther_min = \"0.1.0\"\nschema = 2\n\n\
                   [regions]\ndefault = \"NTSC\"\nsupported = [\"NTSC\"]\n"
        )
        .unwrap();
        z.finish().unwrap();
        p
    }

    fn mark_of(p: &Path) -> String {
        crate::pack_meta::read(p)
            .1
            .map(|i| i.name)
            .unwrap_or_default()
    }

    #[test]
    fn the_full_cycle_touches_nothing_foreign() {
        let tmp = tempfile::tempdir().unwrap();
        let packs = tmp.path().join("packs");
        let saves = tmp.path().join("saves");
        let roms = tmp.path().join("roms");

        // Lo que NO es del pack: la ROM del jugador y sus guardados.
        std::fs::create_dir_all(saves.join("ga")).unwrap();
        std::fs::create_dir_all(&roms).unwrap();
        std::fs::write(saves.join("ga/partida.srm"), b"progreso").unwrap();
        std::fs::write(roms.join("ga.md"), b"la rom del jugador").unwrap();

        let inventory = |d: &Path| -> Vec<String> {
            let mut v: Vec<String> = std::fs::read_dir(d)
                .map(|rd| {
                    rd.flatten()
                        .map(|e| e.file_name().to_string_lossy().into_owned())
                        .collect()
                })
                .unwrap_or_default();
            v.sort();
            v
        };

        let v1 = valid_pack(tmp.path(), "v1.ay", "v1");
        let v2 = valid_pack(tmp.path(), "v2.ay", "v2");

        install_local(&packs, "ga", &v1);
        assert_eq!(inventory(&packs), vec!["ga.ay"], "instalar deja UN archivo");

        install_local(&packs, "ga", &v2);
        assert_eq!(
            inventory(&packs),
            vec!["ga.ay", "ga.ay.prev"],
            "actualizar conserva una generación y NO deja staging"
        );
        assert_eq!(mark_of(&installed_path(&packs, "ga")), "v2");

        rollback(&packs, "ga").unwrap();
        assert_eq!(
            inventory(&packs),
            vec!["ga.ay", "ga.ay.prev"],
            "revertir INTERCAMBIA, no borra: se puede volver a adelantar"
        );
        assert_eq!(mark_of(&installed_path(&packs, "ga")), "v1");

        // Y un resto de descarga interrumpida, para que desinstalar lo barra.
        std::fs::write(packs.join("ga.ay.part"), b"a medio bajar").unwrap();

        assert_eq!(
            uninstall(&packs, "ga"),
            UninstallResult::Uninstalled { had_previous: true }
        );
        assert!(
            inventory(&packs).is_empty(),
            "desinstalar no deja ni el pack, ni la anterior, ni el parcial"
        );

        // Lo ajeno, intacto.
        assert_eq!(
            std::fs::read(saves.join("ga/partida.srm")).unwrap(),
            b"progreso"
        );
        assert_eq!(
            std::fs::read(roms.join("ga.md")).unwrap(),
            b"la rom del jugador"
        );

        // Y desinstalar dos veces no es un error de disco: no estaba.
        assert_eq!(uninstall(&packs, "ga"), UninstallResult::NotInstalled);
    }

    /// El caso completo de la issue: instalar, actualizar, revertir — y volver
    /// a adelantar.
    #[test]
    fn install_update_revert() {
        let tmp = tempfile::tempdir().unwrap();
        let packs = tmp.path().join("packs");
        let v1 = valid_pack(tmp.path(), "v1.ay", "v1");
        let v2 = valid_pack(tmp.path(), "v2.ay", "v2");

        assert_eq!(
            install_local(&packs, "ga", &v1),
            InstallResult::Installed { replaced: false }
        );
        assert_eq!(mark_of(&installed_path(&packs, "ga")), "v1");
        assert!(
            !has_previous(&packs, "ga"),
            "la primera instalación no deja previa"
        );

        assert_eq!(
            install_local(&packs, "ga", &v2),
            InstallResult::Installed { replaced: true }
        );
        assert_eq!(mark_of(&installed_path(&packs, "ga")), "v2");
        assert!(has_previous(&packs, "ga"), "la anterior se conserva");
        assert_eq!(mark_of(&previous_path(&packs, "ga")), "v1");

        rollback(&packs, "ga").unwrap();
        assert_eq!(mark_of(&installed_path(&packs, "ga")), "v1");
        // Revertir INTERCAMBIA: la v2 sigue ahí y se puede volver a adelantar.
        assert_eq!(mark_of(&previous_path(&packs, "ga")), "v2");
        rollback(&packs, "ga").unwrap();
        assert_eq!(mark_of(&installed_path(&packs, "ga")), "v2");

        // El original que el jugador tenía en el disco no se movió.
        assert!(v1.is_file() && v2.is_file(), "install COPIA, no mueve");
    }

    /// EL DEFECTO que esto cierra: el flujo viejo bajaba encima del pack
    /// instalado y, si la firma no validaba, lo borraba. Una descarga corrupta
    /// dejaba al jugador sin el pack que venía usando.
    #[test]
    fn an_invalid_pack_does_not_touch_what_is_installed() {
        let tmp = tempfile::tempdir().unwrap();
        let packs = tmp.path().join("packs");
        let v1 = valid_pack(tmp.path(), "v1.ay", "v1");
        install_local(&packs, "ga", &v1);

        // Un archivo que no es un ZIP: el verificador lo rechaza.
        let invalid_archive = tmp.path().join("basura.ay");
        std::fs::write(&invalid_archive, b"no soy un pack").unwrap();
        match install_local(&packs, "ga", &invalid_archive) {
            InstallResult::Invalid(_) => {}
            other => panic!("se esperaba rechazo, salió {other:?}"),
        }

        assert!(
            installed_path(&packs, "ga").is_file(),
            "lo instalado sigue estando"
        );
        assert_eq!(
            mark_of(&installed_path(&packs, "ga")),
            "v1",
            "y sigue siendo la versión que funcionaba"
        );
        assert!(
            !staging_path(&packs, "ga").exists(),
            "el staging fallido no queda tirado"
        );
    }

    /// La firma dice que NADIE LO TOCÓ; que el motor pueda abrirlo dice que
    /// SIRVE. Son cosas distintas, y confundirlas deja instalado un archivo que
    /// el jugador descubre roto recién al querer jugar.
    ///
    /// Este pack está perfectamente firmado y tiene el manifest incompleto: pasa
    /// la verificación de integridad y no abre.
    #[test]
    fn a_pack_that_signs_well_but_does_not_open_is_not_installed() {
        let tmp = tempfile::tempdir().unwrap();
        let packs = tmp.path().join("packs");
        let valid_archive = valid_pack(tmp.path(), "v1.ay", "v1");
        install_local(&packs, "ga", &valid_archive);

        // Manifest sin `game_id` ni `[regions]`: el ZIP está sano y la firma
        // no es el problema.
        let incomplete_archive = tmp.path().join("flojo.ay");
        {
            let f = std::fs::File::create(&incomplete_archive).unwrap();
            let mut z = zip::ZipWriter::new(f);
            let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default();
            z.start_file("manifest.toml", opts).unwrap();
            z.write_all(
                b"[pack]
name = \"a medias\"
",
            )
            .unwrap();
            z.finish().unwrap();
        }
        match install_local(&packs, "ga", &incomplete_archive) {
            InstallResult::Invalid(_) => {}
            other => panic!("se esperaba rechazo por no abrir, salió {other:?}"),
        }
        assert_eq!(
            mark_of(&installed_path(&packs, "ga")),
            "v1",
            "lo instalado sigue siendo lo que funcionaba"
        );
    }

    /// Los savestates viven en otro árbol y este módulo no los mira. Se prueba
    /// sobre un `saves/` poblado porque «no los toca» es un CA de la issue, y
    /// un comentario no es una garantía.
    #[test]
    fn updating_does_not_touch_the_savestates() {
        let tmp = tempfile::tempdir().unwrap();
        let packs = tmp.path().join("packs");
        let saves = tmp.path().join("saves");
        std::fs::create_dir_all(&saves).unwrap();
        std::fs::write(saves.join("ga.srm"), b"partida").unwrap();

        let v1 = valid_pack(tmp.path(), "v1.ay", "v1");
        let v2 = valid_pack(tmp.path(), "v2.ay", "v2");
        install_local(&packs, "ga", &v1);
        install_local(&packs, "ga", &v2);
        rollback(&packs, "ga").unwrap();

        assert_eq!(std::fs::read(saves.join("ga.srm")).unwrap(), b"partida");
        assert_eq!(std::fs::read_dir(&saves).unwrap().count(), 1);
    }

    /// Sin versión anterior, revertir FALLA con un motivo — no deja el pack a
    /// medio camino ni borra el instalado.
    #[test]
    fn reverting_without_a_previous_breaks_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let packs = tmp.path().join("packs");
        let v1 = valid_pack(tmp.path(), "v1.ay", "v1");
        install_local(&packs, "ga", &v1);

        assert!(rollback(&packs, "ga").is_err());
        assert_eq!(mark_of(&installed_path(&packs, "ga")), "v1");
    }

    /// Descartar la anterior libera el espacio y deja el instalado intacto.
    #[test]
    fn discard_the_previous_one() {
        let tmp = tempfile::tempdir().unwrap();
        let packs = tmp.path().join("packs");
        let v1 = valid_pack(tmp.path(), "v1.ay", "v1");
        let v2 = valid_pack(tmp.path(), "v2.ay", "v2");
        install_local(&packs, "ga", &v1);
        install_local(&packs, "ga", &v2);

        discard_previous(&packs, "ga").unwrap();
        assert!(!has_previous(&packs, "ga"));
        assert_eq!(mark_of(&installed_path(&packs, "ga")), "v2");
        // Idempotente: descartar dos veces no es un error.
        discard_previous(&packs, "ga").unwrap();
    }
}
