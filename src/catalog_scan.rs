//! Incremental catalog projection from directory inventories.
use crate::{
    catalog::{Result, Store},
    config::Config,
    inventory,
    library::{self, Game, InstalledPack, PackStatus},
};
use std::{
    collections::HashMap,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

pub struct Outcome {
    pub games: Vec<Game>,
    pub removed: Vec<String>,
    pub cancelled: bool,
    pub unavailable: Vec<String>,
    pub artwork_changes: Vec<std::path::PathBuf>,
}

pub fn scan(
    store: Store,
    config: &Config,
    known: &HashMap<String, Game>,
    dat: Option<&crate::nointro::DatFile>,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<Outcome> {
    let rom_roots: Vec<_> = crate::config::rom_folders(config)
        .iter()
        .map(|p| inventory::canonical(p))
        .collect();
    let covers_error = prepare_covers_directory(&store, &config.covers_dir).err();
    let mut roots = crate::config::rom_folders(config);
    roots.push(config.packs_install_dir.clone());
    roots.extend(config.pack_dirs.iter().cloned());
    roots.push(config.covers_dir.clone());
    let mut changes = inventory::reconcile(store, &roots, cancel)?;
    if let Some(error) = covers_error {
        changes.unavailable.push(error);
    }
    let mut games = Vec::new();
    let roms: Vec<_> = changes
        .files
        .iter()
        .filter(|path| {
            rom_roots.iter().any(|root| path.starts_with(root))
                && path
                    .extension()
                    .and_then(|s| s.to_str())
                    .and_then(library::platform_for_ext)
                    .is_some()
        })
        .collect();
    progress(0, roms.len());
    for (index, path) in roms.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let old = known.get(path.to_string_lossy().as_ref());
        // Imported entries remain unconfirmed until selected. Migration must
        // not turn startup into identification of the entire ROM collection.
        if old.is_some_and(|g| g.sha256.is_empty()) {
            continue;
        }
        if (changes.changed.contains(*path)
            || old.is_none_or(|g| g.revision.as_ref() != changes.revisions.get(*path)))
            && let Some(game) = library::scan_one_cancelable(path, known, cancel)
        {
            games.push(game);
        }
        progress(index + 1, roms.len());
    }
    if let Some(dat) = dat {
        for game in known.values() {
            if dat
                .lookup(&game.crc32)
                .is_some_and(|entry| entry.name != game.name || entry.region != game.region)
                && !games.iter().any(|g| g.path == game.path)
            {
                games.push(game.clone());
            }
        }
        library::enrich_with_dat(&mut games, dat);
    }
    let pack_changed = changes
        .changed
        .iter()
        .chain(changes.removed.iter())
        .any(|p| is_pack(p));
    let cover_changed = changes
        .changed
        .iter()
        .chain(changes.removed.iter())
        .any(|p| {
            matches!(
                p.extension().and_then(|s| s.to_str()),
                Some("png" | "jpg" | "jpeg" | "webp")
            )
        });
    let mut packs = Vec::new();
    if let Some(repo) = crate::pack_cache::global() {
        for path in changes.files.iter().filter(|p| is_pack(p)) {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            match repo.catalog_entry(path) {
                Ok(snapshot) => packs.push(InstalledPack {
                    id: path
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    path: path.to_string_lossy().into_owned(),
                    rom_crc32: snapshot.crc32,
                }),
                Err(error) => crate::log_warning!(
                    "catalog",
                    "No se pudieron leer los metadatos de {}: {}",
                    path.display(),
                    error
                ),
            }
        }
    }
    // Association changes use in-memory metadata, never open unrelated ROMs or
    // audit packs. Persist only changed projections.
    packs.sort_by(|a, b| a.id.cmp(&b.id).then_with(|| a.path.cmp(&b.path)));
    if pack_changed || cover_changed {
        for game in known.values() {
            if !games.iter().any(|g| g.path == game.path)
                && !changes.removed.contains(Path::new(&game.path))
            {
                games.push(game.clone());
            }
        }
    }
    for game in &mut games {
        let selected = known
            .get(&game.path)
            .map(|g| g.pack_path.clone())
            .unwrap_or_default();
        let previous_paths = game.available_packs.clone();
        // An unavailable historical selection is not an installed sidecar.
        // Otherwise an unrelated scan can reinsert an explicitly removed pack.
        if !previous_paths.contains(&game.pack_path) {
            game.has_pack = false;
            game.pack_path.clear();
        }
        library::attach_installed_packs(std::slice::from_mut(game), &packs);
        for path in previous_paths.iter().filter(|p| {
            !changes.files.contains(Path::new(p)) && !changes.confirms_absence(Path::new(p))
        }) {
            if !game.available_packs.contains(path) {
                game.available_packs.push(path.clone());
            }
        }
        game.available_packs
            .retain(|p| !changes.confirms_absence(Path::new(p)));
        if !selected.is_empty() {
            game.pack_path = selected.clone();
            game.has_pack = true;
            if !game.available_packs.contains(&selected) {
                game.pack_status = PackStatus::Incompatible;
                game.compat_reason =
                    "Pack seleccionado no disponible. Elegí otro pack o jugá al original.".into();
            } else if changes.changed.contains(Path::new(&selected)) {
                game.pack_status = PackStatus::Unverified;
                game.compat_reason = "El pack cambió. Se comprobará al abrir el juego.".into();
            }
        } else if previous_paths != game.available_packs {
            game.pack_status = if game.has_pack {
                PackStatus::Unverified
            } else {
                PackStatus::NoPack
            };
        }
    }
    let covers_dir = inventory::canonical(&config.covers_dir);
    let covers = library::CoverIndex::new(&changes.files);
    for game in &mut games {
        game.cover_path = covers.find(game, &covers_dir).unwrap_or_default();
    }
    games.retain(|g| {
        known
            .get(&g.path)
            .is_none_or(|old| serde_json::to_value(old).ok() != serde_json::to_value(g).ok())
    });
    Ok(Outcome {
        games,
        removed: known
            .keys()
            .filter(|p| {
                changes.confirms_absence(Path::new(p))
                    || !rom_roots.iter().any(|root| Path::new(p).starts_with(root))
            })
            .cloned()
            .collect(),
        cancelled: changes.cancelled || cancel.load(Ordering::Relaxed),
        unavailable: changes.unavailable,
        artwork_changes: changes
            .changed
            .iter()
            .chain(changes.removed.iter())
            .filter(|p| {
                matches!(
                    p.extension().and_then(|s| s.to_str()),
                    Some("png" | "jpg" | "jpeg" | "webp")
                )
            })
            .cloned()
            .collect(),
    })
}

fn is_pack(path: &Path) -> bool {
    matches!(path.extension().and_then(|s| s.to_str()), Some("ay" | "ae"))
}

/// Runs on the scan worker. Creation failures affect covers only, not the ROM
/// inventory, and are reported alongside the other availability diagnostics.
fn prepare_covers_directory(store: &Store, path: &Path) -> Result<()> {
    let covers = inventory::canonical(path);
    match std::fs::symlink_metadata(&covers) {
        Ok(metadata) if metadata.is_dir() => return Ok(()),
        Ok(_) => {
            return Err(format!(
                "La ruta de portadas no es una carpeta: {}",
                path.display()
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "No se pudo acceder a la carpeta de portadas {}: {error}",
                path.display()
            ));
        }
    }
    let previous = store.get::<String>("root-identities", &covers.to_string_lossy())?;
    std::fs::create_dir_all(path).map_err(|error| {
        format!(
            "No se pudo crear la carpeta de portadas {}: {error}",
            path.display()
        )
    })?;
    if let Some(previous) = previous {
        let identity =
            crate::file_revision::directory_identity(path).map_err(|error| error.to_string())?;
        // A folder we recreate has a new file ID. Accept that change on the
        // same volume, while retaining the guard against a reused drive letter.
        if !previous.is_empty() && previous.split(':').next() == identity.split(':').next() {
            store.put("root-identities", &covers.to_string_lossy(), &identity)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_covers_directory_is_created_and_existing_contents_are_preserved() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(&directory.path().join("catalog.sqlite3")).unwrap();
        let covers = directory.path().join("new-parent").join("covers");
        prepare_covers_directory(&store, &covers).unwrap();
        assert!(covers.is_dir());
        let image = covers.join("cover.png");
        std::fs::write(&image, b"existing cover").unwrap();
        prepare_covers_directory(&store, &covers).unwrap();
        assert_eq!(std::fs::read(image).unwrap(), b"existing cover");
    }

    #[test]
    fn recreated_covers_update_the_directory_identity_on_the_same_volume() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(&directory.path().join("catalog.sqlite3")).unwrap();
        let covers = directory.path().join("covers");
        std::fs::create_dir(&covers).unwrap();
        let previous = crate::file_revision::directory_identity(&covers).unwrap();
        let key = inventory::canonical(&covers).to_string_lossy().into_owned();
        store.put("root-identities", &key, &previous).unwrap();
        std::fs::remove_dir(&covers).unwrap();
        prepare_covers_directory(&store, &covers).unwrap();
        assert_eq!(
            store.get::<String>("root-identities", &key).unwrap(),
            Some(crate::file_revision::directory_identity(&covers).unwrap())
        );
    }

    #[test]
    fn covers_creation_reports_obstructions_without_overwriting_them() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(&directory.path().join("catalog.sqlite3")).unwrap();
        let covers = directory.path().join("covers");
        std::fs::write(&covers, b"not a directory").unwrap();
        assert!(prepare_covers_directory(&store, &covers).is_err());
        assert!(prepare_covers_directory(&store, &covers.join("nested")).is_err());
        assert_eq!(std::fs::read(covers).unwrap(), b"not a directory");
    }
}
