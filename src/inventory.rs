//! Directory-scoped reconciliation. Notifications are hints; only a complete
//! enumeration can confirm absence. No asset contents are read here.
use crate::{
    catalog::{Result, Store},
    file_revision::Revision,
};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::{Path, PathBuf},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

static SERVICE: OnceLock<Mutex<Inventory>> = OnceLock::new();
static PENDING: AtomicBool = AtomicBool::new(false);
static LOST: AtomicBool = AtomicBool::new(false);
static EPOCH: OnceLock<Instant> = OnceLock::new();
static LAST_EVENT: AtomicU64 = AtomicU64::new(0);
static FIRST_EVENT: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Entry {
    directory: bool,
    revision: Option<Revision>,
}
type Directory = BTreeMap<PathBuf, Entry>;

pub struct Changes {
    pub changed: BTreeSet<PathBuf>,
    pub removed: BTreeSet<PathBuf>,
    pub files: BTreeSet<PathBuf>,
    pub revisions: BTreeMap<PathBuf, Revision>,
    pub unavailable: Vec<String>,
    pub cancelled: bool,
    /// Only successful enumerations of the expected volume can prove absence.
    pub confirmed: BTreeMap<PathBuf, BTreeSet<PathBuf>>,
}

impl Changes {
    pub fn confirms_absence(&self, path: &Path) -> bool {
        path.ancestors()
            .filter_map(|child| child.parent().map(|parent| (parent, child)))
            .any(|(parent, child)| {
                self.confirmed
                    .get(parent)
                    .is_some_and(|entries| !entries.contains(child))
            })
    }
}

pub struct Inventory {
    store: Store,
    directories: BTreeMap<PathBuf, Directory>,
    roots: BTreeSet<PathBuf>,
    dirty: BTreeSet<PathBuf>,
    watcher: Option<RecommendedWatcher>,
    events: mpsc::Receiver<notify::Result<notify::Event>>,
    last_fallback: Instant,
    unobserved: BTreeSet<PathBuf>,
}

pub fn pending() -> bool {
    let now = EPOCH.get_or_init(Instant::now).elapsed().as_millis() as u64;
    (PENDING.load(Ordering::Acquire) || LOST.load(Ordering::Acquire))
        && (now >= LAST_EVENT.load(Ordering::Acquire).saturating_add(250)
            || now >= FIRST_EVENT.load(Ordering::Acquire).saturating_add(1000))
}

pub fn retry() {
    LOST.store(true, Ordering::Release);
}

pub fn reconcile(store: Store, roots: &[PathBuf], cancel: &AtomicBool) -> Result<Changes> {
    let service = SERVICE.get_or_init(|| Mutex::new(Inventory::new(store)));
    service
        .lock()
        .map_err(|e| e.to_string())?
        .scan(roots, cancel)
}

impl Inventory {
    fn new(store: Store) -> Self {
        let (tx, events) = mpsc::sync_channel(4096);
        let watcher = notify::recommended_watcher(move |event| {
            let now = EPOCH.get_or_init(Instant::now).elapsed().as_millis() as u64;
            LAST_EVENT.store(now, Ordering::Release);
            if !PENDING.swap(true, Ordering::AcqRel) {
                FIRST_EVENT.store(now, Ordering::Release);
            }
            if tx.try_send(event).is_err() {
                LOST.store(true, Ordering::Release);
            }
        })
        .ok();
        Self {
            store,
            directories: BTreeMap::new(),
            roots: BTreeSet::new(),
            dirty: BTreeSet::new(),
            watcher,
            events,
            last_fallback: Instant::now(),
            unobserved: BTreeSet::new(),
        }
    }

    pub fn scan(&mut self, roots: &[PathBuf], cancel: &AtomicBool) -> Result<Changes> {
        let mut out = Changes {
            changed: BTreeSet::new(),
            removed: BTreeSet::new(),
            files: BTreeSet::new(),
            revisions: BTreeMap::new(),
            unavailable: Vec::new(),
            cancelled: false,
            confirmed: BTreeMap::new(),
        };
        // Canonicalize accessible roots to collapse aliases. Keep inaccessible
        // roots registered, so disconnecting a drive never retires its games.
        let requested: BTreeSet<_> = roots.iter().map(|p| canonical(p)).collect();
        let requested: BTreeSet<_> = requested
            .iter()
            .filter(|p| {
                !requested
                    .iter()
                    .any(|other| other != *p && p.starts_with(other))
            })
            .cloned()
            .collect();
        for root in self.roots.difference(&requested) {
            if let Some(watcher) = &mut self.watcher {
                let _ = watcher.unwatch(root);
            }
        }
        for root in requested.difference(&self.roots) {
            if let Some(watcher) = &mut self.watcher
                && let Err(error) = watcher.watch(root, RecursiveMode::Recursive)
            {
                out.unavailable.push(format!("{}: {error}", root.display()));
                self.unobserved.insert(root.clone());
            }
            self.dirty.insert(root.clone());
        }
        self.roots = requested;
        let mut unavailable_roots = BTreeSet::new();
        for root in &self.roots {
            match crate::file_revision::directory_identity(root) {
                Ok(identity) => {
                    let expected = self
                        .store
                        .get::<String>("root-identities", &root.to_string_lossy())?;
                    if expected.as_ref().is_some_and(|old| old != &identity) {
                        unavailable_roots.insert(root.clone());
                        out.unavailable.push(format!("{}: el directorio o volumen cambió de identidad; se conserva su biblioteca anterior",root.display()));
                    } else if expected.is_none() {
                        self.store
                            .put("root-identities", &root.to_string_lossy(), &identity)?;
                    }
                }
                Err(error) => {
                    unavailable_roots.insert(root.clone());
                    out.unavailable.push(format!("{}: {error}", root.display()));
                }
            }
        }
        PENDING.store(false, Ordering::Release);
        for event in self.events.try_iter() {
            match event {
                Ok(event) => {
                    if event.need_rescan() {
                        LOST.store(true, Ordering::Release);
                    }
                    // Read/access notifications must not feed scans back into
                    // themselves (not all backends emit these).
                    if matches!(event.kind, notify::EventKind::Access(_)) {
                        continue;
                    }
                    for path in event.paths {
                        if let Some(parent) = path.parent() {
                            self.dirty.insert(parent.to_path_buf());
                        }
                    }
                }
                Err(_) => LOST.store(true, Ordering::Release),
            }
        }
        if LOST.swap(false, Ordering::AcqRel) || self.watcher.is_none() {
            self.dirty.extend(self.roots.iter().cloned());
        }
        if self.last_fallback.elapsed() >= Duration::from_secs(60) {
            self.dirty.extend(self.unobserved.iter().cloned());
            if let Some(watcher) = &mut self.watcher {
                self.unobserved
                    .retain(|root| watcher.watch(root, RecursiveMode::Recursive).is_err());
            }
            self.last_fallback = Instant::now();
        }
        self.dirty
            .retain(|p| self.roots.iter().any(|r| p.starts_with(r)));
        // An interrupted/crashed scan cannot leave a persisted clean claim.
        self.store.put("inventory", "dirty", &self.dirty)?;
        let recursive: Vec<_> = self
            .dirty
            .iter()
            .filter(|p| self.roots.contains(*p))
            .cloned()
            .collect();
        let mut queue: VecDeque<_> = self.dirty.iter().cloned().collect();
        let mut visited = BTreeSet::new();
        while let Some(dir) = queue.pop_front() {
            if cancel.load(Ordering::Relaxed) {
                out.cancelled = true;
                break;
            }
            if !visited.insert(dir.clone()) {
                continue;
            }
            if unavailable_roots.iter().any(|root| dir.starts_with(root)) {
                self.dirty.insert(dir);
                continue;
            }
            let previous = match self.directories.get(&dir) {
                Some(d) => d.clone(),
                None => self
                    .store
                    .get::<Directory>("directory-inventory", &dir.to_string_lossy())?
                    .unwrap_or_default(),
            };
            let current = match enumerate(&dir, cancel) {
                Ok(d) => d,
                Err(error) => {
                    self.directories.entry(dir.clone()).or_insert(previous);
                    out.unavailable.push(format!("{}: {error}", dir.display()));
                    self.dirty.insert(dir);
                    continue;
                }
            };
            for (path, entry) in &current {
                if entry.directory {
                    // A root reconciliation is recursive; a notification on a
                    // known parent only descends into newly discovered folders.
                    if recursive.iter().any(|root| dir.starts_with(root))
                        || !self.directories.contains_key(path)
                        || previous.get(path) != Some(entry)
                    {
                        queue.push_back(path.clone());
                    }
                } else if previous.get(path) != Some(entry) {
                    out.changed.insert(path.clone());
                }
            }
            for (path, entry) in &previous {
                if !current.contains_key(path) {
                    if entry.directory {
                        for (child, entries) in &self.directories {
                            if child.starts_with(path) {
                                out.removed.extend(
                                    entries
                                        .iter()
                                        .filter(|(_, e)| !e.directory)
                                        .map(|(p, _)| p.clone()),
                                );
                            }
                        }
                        self.directories.retain(|child, _| !child.starts_with(path));
                    } else {
                        out.removed.insert(path.clone());
                    }
                }
            }
            self.store
                .put("directory-inventory", &dir.to_string_lossy(), &current)?;
            out.confirmed
                .insert(dir.clone(), current.keys().cloned().collect());
            self.directories.insert(dir.clone(), current);
            self.dirty.remove(&dir);
        }
        if out.cancelled {
            self.dirty.extend(visited);
            self.dirty.extend(queue);
        }
        self.store.put("inventory", "dirty", &self.dirty)?;
        for (dir, entries) in &self.directories {
            if self.roots.iter().any(|r| dir.starts_with(r)) {
                out.files.extend(
                    entries
                        .iter()
                        .filter(|(_, e)| !e.directory)
                        .map(|(p, _)| p.clone()),
                );
                out.revisions.extend(
                    entries
                        .iter()
                        .filter_map(|(p, e)| e.revision.clone().map(|r| (p.clone(), r))),
                );
            }
        }
        Ok(out)
    }
}

pub fn canonical(path: &Path) -> PathBuf {
    let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    #[cfg(windows)]
    if let Some(local) = path.to_str().and_then(|s| s.strip_prefix(r"\\?\"))
        && !local.starts_with("UNC\\")
    {
        return PathBuf::from(local);
    }
    path
}

fn enumerate(dir: &Path, cancel: &AtomicBool) -> std::io::Result<Directory> {
    crate::cache_metrics::DIRECTORIES_ENUMERATED.fetch_add(1, Ordering::Relaxed);
    let mut entries = Directory::new();
    for entry in std::fs::read_dir(dir)? {
        if cancel.load(Ordering::Relaxed) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "Exploración incompleta",
            ));
        }
        let entry = entry?;
        let kind = entry.file_type()?;
        // Do not follow links outside authorized roots or into cycles.
        if kind.is_symlink() {
            continue;
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if entry.metadata()?.file_attributes() & 0x400 != 0 {
                continue;
            }
        }
        let revision = if kind.is_file() {
            Some(Revision::observe(&entry.path())?)
        } else {
            None
        };
        entries.insert(
            entry.path(),
            Entry {
                directory: kind.is_dir(),
                revision,
            },
        );
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restart_confirms_missing_descendants_even_after_snapshot_was_saved() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("roms");
        let child = root.join("child");
        std::fs::create_dir_all(&child).unwrap();
        let rom = child.join("one.md");
        std::fs::write(&rom, b"rom").unwrap();
        let rom = canonical(&rom);
        let store = Store::open(&d.path().join("catalog.sqlite3")).unwrap();
        let cancel = AtomicBool::new(false);
        let mut inventory = Inventory::new(store.clone());
        inventory
            .scan(std::slice::from_ref(&root), &cancel)
            .unwrap();
        std::fs::remove_file(&rom).unwrap();
        std::fs::remove_dir(&child).unwrap();
        inventory.dirty.insert(canonical(&root));
        inventory
            .scan(std::slice::from_ref(&root), &cancel)
            .unwrap();
        drop(inventory); // Simulate crash before the game deletion was committed.
        let mut restarted = Inventory::new(store);
        let result = restarted.scan(&[root], &cancel).unwrap();
        assert!(result.confirms_absence(&rom));
    }
    #[test]
    fn a_change_only_enumerates_its_directory_and_offline_is_not_deletion() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("roms");
        let child = root.join("child");
        std::fs::create_dir_all(&child).unwrap();
        std::fs::write(root.join("a.md"), b"rom").unwrap();
        std::fs::write(child.join("b.md"), b"rom").unwrap();
        let mut inventory = Inventory::new(Store::open(&d.path().join("catalog.sqlite3")).unwrap());
        let cancel = AtomicBool::new(false);
        let roots = [root.clone()];
        assert_eq!(inventory.scan(&roots, &cancel).unwrap().changed.len(), 2);
        // Inject a precise change independently of backend delivery timing.
        let child = canonical(&child);
        std::fs::remove_file(child.join("b.md")).unwrap();
        inventory.dirty.insert(child.clone());
        let result = inventory.scan(&roots, &cancel).unwrap();
        assert!(result.removed.contains(&child.join("b.md")));
        std::fs::rename(&root, d.path().join("disconnected")).unwrap();
        let result = inventory.scan(&roots, &cancel).unwrap();
        assert!(result.removed.is_empty());
    }
}
