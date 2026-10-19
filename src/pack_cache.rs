//! One persisted inspection per confirmed pack revision; no catalog-wide audit.
use crate::{
    catalog::{Result, Store},
    file_revision::Revision,
    verifier::Audit,
};
use ayther_core::{
    archive_vfs::{AyArchive, CatalogMetadata},
    pack_validate::{self, ValidationMetadata},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

// 2: snapshots carry pose_substitutions.toml and its asset headers (Engine rc.17).
const INSPECTION_VERSION: u32 = 2;
const MAX_METADATA_CACHE_BYTES: usize = 32 * 1024 * 1024;
static GLOBAL: OnceLock<Repository> = OnceLock::new();

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub revision: Revision,
    pub version: u32,
    pub metadata: ValidationMetadata,
    pub catalog: Option<CatalogMetadata>,
    pub parse_error: Option<String>,
    pub audit: Option<Audit>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub crc32: Option<String>,
}

impl Snapshot {
    pub fn crc32(&self) -> Option<String> {
        let meta = self.catalog.as_ref()?;
        meta.compat
            .rom_crc32
            .as_deref()
            .or_else(|| meta.meta.game_id.strip_prefix("crc32:"))
            .map(|s| s.trim().trim_start_matches("0x").to_ascii_lowercase())
            .filter(|s| !s.is_empty())
    }
}

pub struct Repository {
    store: Store,
    memory: Mutex<HashMap<PathBuf, Arc<Snapshot>>>,
    gates: Mutex<HashMap<PathBuf, std::sync::Weak<Mutex<()>>>>,
    confirmed: Mutex<HashMap<PathBuf, Revision>>,
    catalog: Mutex<HashMap<PathBuf, CatalogEntry>>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Promotion {
    pub staging: PathBuf,
    pub previous: PathBuf,
    pub installed_before: Option<Revision>,
    pub snapshot: Snapshot,
}

pub fn init(store: Store) {
    let _ = GLOBAL.set(Repository::new(store));
}
pub fn global() -> Option<&'static Repository> {
    GLOBAL.get()
}

impl Repository {
    pub fn new(store: Store) -> Self {
        Self {
            store,
            memory: Mutex::new(HashMap::new()),
            gates: Mutex::new(HashMap::new()),
            confirmed: Mutex::new(HashMap::new()),
            catalog: Mutex::new(HashMap::new()),
        }
    }

    pub fn require_writable(&self) -> Result<()> {
        if self.store.read_only()? {
            Err("La biblioteca está en modo de solo lectura. No se pueden modificar packs.".into())
        } else {
            Ok(())
        }
    }

    pub fn operation(&self, path: &Path, operation: &Promotion) -> Result<()> {
        self.store
            .put("pack-operations", &path.to_string_lossy(), operation)
    }

    /// A crash cannot turn a promoted file into an implicitly trusted pack.
    /// Recover the filesystem layout first; recovered bytes require first-use audit.
    pub fn recover_promotions(&self) -> Result<()> {
        for (key, operation) in self.store.records::<Promotion>("pack-operations")? {
            let current = PathBuf::from(&key);
            let present = Revision::observe(&current).ok();
            if present
                .as_ref()
                .is_some_and(|r| r.same_file(&operation.snapshot.revision))
            {
                let mut snapshot = operation.snapshot;
                snapshot.revision = present.unwrap();
                snapshot.audit = None;
                self.publish(&current, snapshot)?;
                self.invalidate(&operation.staging)?;
                self.finish_operation(&current)?;
                continue;
            }
            if present
                .as_ref()
                .zip(operation.installed_before.as_ref())
                .is_some_and(|(a, b)| a.same_file(b))
            {
                // Promotion did not replace the original, or its rollback succeeded.
                self.invalidate(&current)?;
                self.finish_operation(&current)?;
                continue;
            }
            if present.is_none() && !current.try_exists().map_err(|e| e.to_string())? {
                if let Some(previous) = &operation.installed_before {
                    let reserved = crate::file_revision::reserve_for_mutation(&operation.previous)
                        .map_err(|e| e.to_string())?;
                    if !Revision::from_file(&reserved)
                        .map_err(|e| e.to_string())?
                        .same_file(previous)
                    {
                        return Err("La versión anterior del pack cambió; la recuperación requiere revisión".into());
                    }
                    std::fs::rename(&operation.previous, &current).map_err(|e| e.to_string())?;
                } else {
                    let reserved = crate::file_revision::reserve_for_mutation(&operation.staging)
                        .map_err(|e| e.to_string())?;
                    if !Revision::from_file(&reserved)
                        .map_err(|e| e.to_string())?
                        .same_file(&operation.snapshot.revision)
                    {
                        return Err(
                            "El pack pendiente cambió; no se completó su instalación".into()
                        );
                    }
                    std::fs::rename(&operation.staging, &current).map_err(|e| e.to_string())?;
                }
                self.invalidate(&current)?;
                self.invalidate(&operation.staging)?;
                self.finish_operation(&current)?;
                continue;
            }
            return Err(format!(
                "No se pudo recuperar {}: los archivos cambiaron. Se conserva la operación pendiente.",
                current.display()
            ));
        }
        Ok(())
    }

    pub fn finish_operation(&self, path: &Path) -> Result<()> {
        self.store
            .remove("pack-operations", &path.to_string_lossy())
    }

    fn gate(&self, path: &Path) -> Arc<Mutex<()>> {
        let mut gates = self.gates.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(gate) = gates.get(path).and_then(std::sync::Weak::upgrade) {
            return gate;
        }
        gates.retain(|_, gate| gate.strong_count() > 0);
        let gate = Arc::new(Mutex::new(()));
        gates.insert(path.to_path_buf(), Arc::downgrade(&gate));
        gate
    }

    pub fn peek(&self, path: &Path) -> Option<Arc<Snapshot>> {
        self.memory.lock().ok()?.get(path).cloned()
    }

    /// Catalog association queries use a tiny projection, never the asset index.
    pub fn catalog_entry(&self, path: &Path) -> Result<CatalogEntry> {
        if let Some(entry) = self
            .catalog
            .lock()
            .map_err(|e| e.to_string())?
            .get(path)
            .cloned()
        {
            return Ok(entry);
        }
        let entry = match self
            .store
            .get::<CatalogEntry>("pack-catalog", &path.to_string_lossy())?
        {
            Some(entry) => entry,
            None => CatalogEntry {
                crc32: self.discover(path)?.crc32(),
            },
        };
        self.catalog
            .lock()
            .map_err(|e| e.to_string())?
            .insert(path.to_path_buf(), entry.clone());
        Ok(entry)
    }

    fn remember(&self, path: &Path, snapshot: Arc<Snapshot>) {
        let mut memory = self.memory.lock().unwrap_or_else(|e| e.into_inner());
        let size = |snapshot: &Snapshot| {
            snapshot
                .metadata
                .texts
                .values()
                .map(|s| s.len())
                .sum::<usize>()
                + snapshot
                    .metadata
                    .names
                    .iter()
                    .map(|s| s.len())
                    .sum::<usize>()
        };
        let incoming = size(&snapshot);
        memory.remove(path);
        while !memory.is_empty()
            && (memory.len() >= 64
                || memory
                    .values()
                    .map(|s| size(s))
                    .sum::<usize>()
                    .saturating_add(incoming)
                    > MAX_METADATA_CACHE_BYTES)
        {
            let victim = memory.keys().next().cloned().unwrap();
            memory.remove(&victim);
        }
        memory.insert(path.to_path_buf(), snapshot);
    }

    pub fn is_confirmed(&self, path: &Path, revision: &Revision) -> bool {
        self.confirmed
            .lock()
            .is_ok_and(|confirmed| confirmed.get(path) == Some(revision))
    }

    fn load(&self, path: &Path) -> Result<Option<Arc<Snapshot>>> {
        if let Some(snapshot) = self.peek(path) {
            return Ok(Some(snapshot));
        }
        let snapshot = self
            .store
            .get::<Snapshot>("pack-snapshots", &path.to_string_lossy())?;
        Ok(snapshot
            .filter(|s| s.version == INSPECTION_VERSION)
            .map(|s| {
                let s = Arc::new(s);
                self.remember(path, s.clone());
                s
            }))
    }

    pub fn publish(&self, path: &Path, snapshot: Snapshot) -> Result<Arc<Snapshot>> {
        let catalog = CatalogEntry {
            crc32: snapshot.crc32(),
        };
        if !self.store.read_only()? {
            self.store.put_records(vec![
                (
                    "pack-snapshots".into(),
                    path.to_string_lossy().into_owned(),
                    serde_json::to_string(&snapshot).map_err(|e| e.to_string())?,
                ),
                (
                    "pack-catalog".into(),
                    path.to_string_lossy().into_owned(),
                    serde_json::to_string(&catalog).map_err(|e| e.to_string())?,
                ),
            ])?;
        }
        self.catalog
            .lock()
            .map_err(|e| e.to_string())?
            .insert(path.to_path_buf(), catalog);
        if snapshot.audit.is_some() {
            self.confirmed
                .lock()
                .map_err(|e| e.to_string())?
                .insert(path.to_path_buf(), snapshot.revision.clone());
        }
        let snapshot = Arc::new(snapshot);
        self.remember(path, snapshot.clone());
        Ok(snapshot)
    }

    /// Existing packs keep cached presentation until selected; only a new file
    /// needs initial metadata. A changed snapshot never authorizes launch.
    pub fn discover(&self, path: &Path) -> Result<Arc<Snapshot>> {
        let gate = self.gate(path);
        let _guard = gate.lock().map_err(|e| e.to_string())?;
        if let Some(snapshot) = self.load(path)? {
            return Ok(snapshot);
        }
        let reserve = crate::file_revision::reserve(path).map_err(|e| e.to_string())?;
        let revision = Revision::from_file(&reserve).map_err(|e| e.to_string())?;
        let snapshot = read_metadata(path, revision.clone())?;
        if Revision::from_file(&reserve).map_err(|e| e.to_string())? != revision {
            return Err("El pack cambió mientras se leía".into());
        }
        self.publish(path, snapshot)
    }

    pub fn inspect(
        &self,
        path: &Path,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<Arc<Snapshot>> {
        let gate = self.gate(path);
        let _guard = loop {
            if cancel.load(Ordering::Acquire) {
                return Err("Comprobación cancelada".into());
            }
            match gate.try_lock() {
                Ok(guard) => break guard,
                Err(std::sync::TryLockError::WouldBlock) => {
                    std::thread::sleep(std::time::Duration::from_millis(20))
                }
                Err(error) => return Err(error.to_string()),
            }
        };
        if cancel.load(Ordering::Relaxed) {
            return Err("Comprobación cancelada".into());
        }
        if self
            .store
            .get::<serde_json::Value>("pack-operations", &path.to_string_lossy())?
            .is_some()
        {
            return Err("La instalación de este pack está pendiente de recuperación. Reiniciá Play CE antes de usarlo.".into());
        }
        let reserved = crate::file_revision::reserve(path)
            .map_err(|e| format!("Pack no disponible o en modificación: {e}"))?;
        let revision = Revision::from_file(&reserved).map_err(|e| e.to_string())?;
        let previous = self.load(path)?;
        if let Some(snapshot) = &previous
            && snapshot.revision.proves_unchanged(&revision)
            && snapshot.audit.is_some()
            && self.confirmed.lock().map_err(|e| e.to_string())?.get(path)
                == Some(&snapshot.revision)
        {
            return Ok(snapshot.clone());
        }
        let mut snapshot = match previous.filter(|s| s.revision.proves_unchanged(&revision)) {
            Some(s) => (*s).clone(),
            None => read_metadata(path, revision.clone())?,
        };
        let _permit = crate::io_budget::acquire(&revision.identity, cancel)?;
        snapshot.audit = Some(match &snapshot.parse_error {
            Some(error) => Audit::InvalidPack(error.clone()),
            None => crate::verifier::audit_with_progress(path, cancel, progress)?,
        });
        if cancel.load(Ordering::Relaxed) {
            return Err("Comprobación cancelada".into());
        }
        let after = Revision::from_file(&reserved).map_err(|e| e.to_string())?;
        let at_path = Revision::observe(path).map_err(|e| e.to_string())?;
        if revision != after || revision != at_path {
            return Err("El pack cambió; la comprobación anterior fue descartada".into());
        }
        self.publish(path, snapshot)
    }

    pub fn invalidate(&self, path: &Path) -> Result<()> {
        self.catalog.lock().map_err(|e| e.to_string())?.remove(path);
        self.store.remove("pack-catalog", &path.to_string_lossy())?;
        self.confirmed
            .lock()
            .map_err(|e| e.to_string())?
            .remove(path);
        self.memory.lock().map_err(|e| e.to_string())?.remove(path);
        self.store.remove("pack-snapshots", &path.to_string_lossy())
    }
}

fn read_metadata(path: &Path, revision: Revision) -> Result<Snapshot> {
    crate::cache_metrics::PACK_METADATA_READS.fetch_add(1, Ordering::Relaxed);
    crate::cache_metrics::PACK_CONTENT_OPENS.fetch_add(1, Ordering::Relaxed);
    let metadata = match pack_validate::read_metadata(&path.to_string_lossy()) {
        Ok(metadata) => metadata,
        Err(report) => {
            let message = report
                .findings
                .iter()
                .map(|f| f.message.as_str())
                .collect::<Vec<_>>()
                .join("; ");
            if report.findings.iter().any(|f| f.code == "io") {
                return Err(message);
            }
            return Ok(Snapshot {
                revision,
                version: INSPECTION_VERSION,
                metadata: ValidationMetadata {
                    names: Vec::new(),
                    texts: Default::default(),
                    asset_heads: Default::default(),
                },
                catalog: None,
                parse_error: Some(message),
                audit: None,
            });
        }
    };
    let catalog = metadata
        .texts
        .get("manifest.toml")
        .ok_or_else(|| "Falta manifest.toml".to_string())
        .and_then(|text| {
            AyArchive::catalog_metadata(text.as_bytes()).map_err(|e| format!("{e:?}"))
        });
    let (catalog, parse_error) = match catalog {
        Ok(m) => (Some(m), None),
        Err(e) => (None, Some(e)),
    };
    Ok(Snapshot {
        revision,
        version: INSPECTION_VERSION,
        metadata,
        catalog,
        parse_error,
        audit: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    fn make_pack(path: &Path, name: &str) {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        zip.start_file("manifest.toml", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(format!("[pack]\nname='{name}'\nversion='1'\ngame_id='crc32:12345678'\nayther_min='0.1.0'\nschema=2\n[regions]\ndefault='US'\nsupported=['US']\n").as_bytes()).unwrap();
        zip.finish().unwrap();
    }

    #[test]
    fn interrupted_rotation_restores_old_pack_without_losing_staging() {
        let d = tempfile::tempdir().unwrap();
        let repo = Repository::new(Store::open(&d.path().join("catalog.sqlite3")).unwrap());
        let current = d.path().join("pack.ay");
        let previous = d.path().join("pack.ay.prev");
        let staging = d.path().join("pack.ay.new");
        make_pack(&current, "Old");
        make_pack(&staging, "New");
        let old = Revision::observe(&current).unwrap();
        let snapshot = repo
            .inspect(&staging, &AtomicBool::new(false), &mut |_, _| {})
            .unwrap();
        repo.operation(
            &current,
            &Promotion {
                staging: staging.clone(),
                previous: previous.clone(),
                installed_before: Some(old.clone()),
                snapshot: (*snapshot).clone(),
            },
        )
        .unwrap();
        std::fs::rename(&current, &previous).unwrap();
        assert!(
            repo.inspect(&current, &AtomicBool::new(false), &mut |_, _| {})
                .unwrap_err()
                .contains("recuperación")
        );
        repo.recover_promotions().unwrap();
        assert_eq!(Revision::observe(&current).unwrap().identity, old.identity);
        assert!(staging.is_file());
        assert!(
            repo.store
                .records::<Promotion>("pack-operations")
                .unwrap()
                .is_empty()
        );
        repo.recover_promotions().unwrap();
    }

    /// A crash right after `rename(staging, current)` leaves `current` with the
    /// creation time of the pack it replaced (NTFS tunneling, see
    /// `Revision::same_file`). Recovery must still recognise it as the promoted
    /// file instead of keeping the operation pending forever.
    #[cfg(windows)]
    #[test]
    fn recovery_recognises_the_promoted_file_despite_a_tunneled_creation_time() {
        use std::os::windows::fs::FileTimesExt;
        let d = tempfile::tempdir().unwrap();
        let repo = Repository::new(Store::open(&d.path().join("catalog.sqlite3")).unwrap());
        let current = d.path().join("pack.ay");
        let previous = d.path().join("pack.ay.prev");
        let staging = d.path().join("pack.ay.new");
        make_pack(&current, "Old");
        make_pack(&staging, "New");
        std::fs::File::options()
            .write(true)
            .open(&staging)
            .unwrap()
            .set_times(
                std::fs::FileTimes::new().set_created(
                    std::time::SystemTime::now() + std::time::Duration::from_secs(600),
                ),
            )
            .unwrap();
        let old = Revision::observe(&current).unwrap();
        let snapshot = repo
            .inspect(&staging, &AtomicBool::new(false), &mut |_, _| {})
            .unwrap();
        repo.operation(
            &current,
            &Promotion {
                staging: staging.clone(),
                previous: previous.clone(),
                installed_before: Some(old),
                snapshot: (*snapshot).clone(),
            },
        )
        .unwrap();
        std::fs::rename(&current, &previous).unwrap();
        std::fs::rename(&staging, &current).unwrap();
        let promoted = Revision::observe(&current).unwrap();
        assert_ne!(
            promoted.identity, snapshot.revision.identity,
            "tunneling changed the creation time"
        );

        repo.recover_promotions().unwrap();
        assert!(
            repo.store
                .records::<Promotion>("pack-operations")
                .unwrap()
                .is_empty(),
            "the promotion is finished, not kept pending"
        );
        assert!(previous.is_file() && current.is_file() && !staging.exists());
        assert!(repo.discover(&current).unwrap().audit.is_none());
    }

    #[test]
    fn recovered_promotion_is_not_implicitly_audited() {
        let d = tempfile::tempdir().unwrap();
        let repo = Repository::new(Store::open(&d.path().join("catalog.sqlite3")).unwrap());
        let current = d.path().join("pack.ay");
        let staging = d.path().join("pack.ay.new");
        make_pack(&staging, "New");
        let snapshot = repo
            .inspect(&staging, &AtomicBool::new(false), &mut |_, _| {})
            .unwrap();
        repo.operation(
            &current,
            &Promotion {
                staging: staging.clone(),
                previous: d.path().join("pack.ay.prev"),
                installed_before: None,
                snapshot: (*snapshot).clone(),
            },
        )
        .unwrap();
        std::fs::rename(&staging, &current).unwrap();
        repo.recover_promotions().unwrap();
        assert!(repo.discover(&current).unwrap().audit.is_none());
        let confirmed = repo
            .inspect(&current, &AtomicBool::new(false), &mut |_, _| {})
            .unwrap();
        if confirmed.revision.change_token.is_some() {
            assert!(Arc::ptr_eq(
                &confirmed,
                &repo
                    .inspect(&current, &AtomicBool::new(false), &mut |_, _| panic!(
                        "cache hit must not audit"
                    ))
                    .unwrap()
            ));
        }
    }

    #[test]
    fn discovery_does_not_audit_and_first_use_is_persisted() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("one.ay");
        make_pack(&path, "One");
        let store = Store::open(&d.path().join("catalog.sqlite3")).unwrap();
        let repo = Repository::new(store.clone());
        assert!(repo.discover(&path).unwrap().audit.is_none());
        let snapshot = repo
            .inspect(&path, &AtomicBool::new(false), &mut |_, _| {})
            .unwrap();
        assert_eq!(snapshot.audit, Some(Audit::MissingIndex));
        assert_eq!(
            Repository::new(store).load(&path).unwrap().unwrap().audit,
            snapshot.audit
        );
    }
    #[test]
    fn changed_pack_is_reinspected_and_cancel_does_not_publish() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("one.ay");
        make_pack(&path, "One");
        let repo = Repository::new(Store::open(&d.path().join("catalog.sqlite3")).unwrap());
        repo.inspect(&path, &AtomicBool::new(false), &mut |_, _| {})
            .unwrap();
        make_pack(&path, "Replacement");
        assert!(
            repo.inspect(&path, &AtomicBool::new(true), &mut |_, _| {})
                .is_err()
        );
        assert_eq!(
            repo.peek(&path)
                .unwrap()
                .catalog
                .as_ref()
                .unwrap()
                .meta
                .name,
            "One"
        );
        let updated = repo
            .inspect(&path, &AtomicBool::new(false), &mut |_, _| {})
            .unwrap();
        assert_eq!(updated.catalog.as_ref().unwrap().meta.name, "Replacement");
    }
}
