//! SQLite persistence. A bounded actor owns the only database connection.
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
    time::Duration,
};

pub type Result<T> = std::result::Result<T, String>;
type Job = Box<dyn FnOnce(&mut Result<Connection>) + Send>;
const SCHEMA: i64 = 1;

#[derive(Clone)]
pub struct Store {
    tx: mpsc::SyncSender<Job>,
    path: Arc<PathBuf>,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let store = Self::open_deferred(path)?;
        store.call(|_| Ok(()))?;
        Ok(store)
    }

    /// Starts the owner thread without waiting for disk or migrations.
    pub fn open_deferred(path: &Path) -> Result<Self> {
        let path = path.to_path_buf();
        let worker_path = path.clone();
        let (tx, rx) = mpsc::sync_channel::<Job>(256);
        std::thread::Builder::new()
            .name("catalog-db".into())
            .spawn(move || {
                let writer = writer_lock(&worker_path);
                let mut db = match &writer {
                    Ok(Some(_)) => open_database(&worker_path),
                    Ok(None) => open_read_only(&worker_path),
                    Err(error) => Err(error.clone()),
                };
                for job in rx {
                    job(&mut db);
                }
                if let Ok(db) = db {
                    let _ = db.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)");
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            tx,
            path: Arc::new(path),
        })
    }

    /// Call only on workers. UI callbacks dispatch their operation off-thread.
    pub fn call<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Connection) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let (tx, rx) = mpsc::sync_channel(1);
        self.tx
            .try_send(Box::new(move |db| {
                let _ = tx.send(match db {
                    Ok(db) => f(db),
                    Err(error) => Err(error.clone()),
                });
            }))
            .map_err(|e| format!("La cola de biblioteca no está disponible: {e}"))?;
        rx.recv()
            .map_err(|e| format!("La biblioteca dejó de responder: {e}"))?
    }

    pub fn read_only(&self) -> Result<bool> {
        self.call(|db| db.is_readonly("main").map_err(|e| e.to_string()))
    }

    pub fn checkpoint_and_backup(&self) -> Result<()> {
        if self.read_only()? {
            return Ok(());
        }
        let copy = self.path.with_extension("backup-new.sqlite3");
        self.backup(&copy)?;
        // Only rotate after a complete, verified SQLite backup exists.
        let third = self.path.with_extension("backup-3.sqlite3");
        if third.try_exists().map_err(|e| e.to_string())? {
            std::fs::remove_file(&third).map_err(|e| e.to_string())?;
        }
        for index in (1..3).rev() {
            let source = self.path.with_extension(format!("backup-{index}.sqlite3"));
            if source.try_exists().map_err(|e| e.to_string())? {
                std::fs::rename(
                    source,
                    self.path
                        .with_extension(format!("backup-{}.sqlite3", index + 1)),
                )
                .map_err(|e| e.to_string())?;
            }
        }
        std::fs::rename(copy, self.path.with_extension("backup-1.sqlite3"))
            .map_err(|e| e.to_string())?;
        self.call(|db| {
            db.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
                .map_err(|e| e.to_string())
        })
    }

    pub fn get<T: DeserializeOwned + Send + 'static>(
        &self,
        namespace: &str,
        key: &str,
    ) -> Result<Option<T>> {
        let (namespace, key) = (namespace.to_owned(), key.to_owned());
        self.call(move |db| {
            let body: Option<String> = db
                .query_row(
                    "SELECT payload FROM records WHERE namespace=?1 AND key=?2",
                    params![namespace, key],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            body.map(|v| serde_json::from_str(&v).map_err(|e| format!("Caché inválida: {e}")))
                .transpose()
        })
    }

    pub fn put<T: Serialize>(&self, namespace: &str, key: &str, value: &T) -> Result<()> {
        let (namespace, key) = (namespace.to_owned(), key.to_owned());
        let body = serde_json::to_string(value).map_err(|e| e.to_string())?;
        self.call(move |db| {
            let changed = db.execute("INSERT INTO records(namespace,key,payload) VALUES(?1,?2,?3) ON CONFLICT(namespace,key) DO UPDATE SET payload=excluded.payload WHERE payload != excluded.payload", params![namespace, key, body]).map_err(|e| e.to_string())?;
            crate::cache_metrics::DB_ROWS_CHANGED.fetch_add(changed as u64, std::sync::atomic::Ordering::Relaxed);
            Ok(())
        })
    }

    pub fn put_records(&self, records: Vec<(String, String, String)>) -> Result<()> {
        self.call(move |db| {
            let tx = db.transaction().map_err(|e|e.to_string())?;
            let mut count = 0;
            for (namespace,key,body) in records {
                count += tx.execute("INSERT INTO records(namespace,key,payload) VALUES(?1,?2,?3) ON CONFLICT(namespace,key) DO UPDATE SET payload=excluded.payload WHERE payload != excluded.payload", params![namespace,key,body]).map_err(|e|e.to_string())?;
            }
            tx.commit().map_err(|e|e.to_string())?;
            crate::cache_metrics::DB_ROWS_CHANGED.fetch_add(count as u64, std::sync::atomic::Ordering::Relaxed);
            Ok(())
        })
    }

    pub fn remove(&self, namespace: &str, key: &str) -> Result<()> {
        let (namespace, key) = (namespace.to_owned(), key.to_owned());
        self.call(move |db| {
            db.execute(
                "DELETE FROM records WHERE namespace=?1 AND key=?2",
                params![namespace, key],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })
    }

    pub fn records<T: DeserializeOwned + Send + 'static>(
        &self,
        namespace: &str,
    ) -> Result<Vec<(String, T)>> {
        let namespace = namespace.to_owned();
        self.call(move |db| {
            let mut query = db
                .prepare("SELECT key,payload FROM records WHERE namespace=?1 ORDER BY key")
                .map_err(|e| e.to_string())?;
            let rows = query
                .query_map([namespace], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })
                .map_err(|e| e.to_string())?;
            rows.map(|row| {
                let (key, value) = row.map_err(|e| e.to_string())?;
                Ok((
                    key,
                    serde_json::from_str(&value).map_err(|e| e.to_string())?,
                ))
            })
            .collect()
        })
    }

    pub fn load_games(&self) -> Result<Vec<crate::library::Game>> {
        self.call(|db| {
            let mut statement = db
                .prepare("SELECT payload FROM games WHERE active=1 ORDER BY sort_title,id")
                .map_err(|e| e.to_string())?;
            let rows = statement
                .query_map([], |r| r.get::<_, String>(0))
                .map_err(|e| e.to_string())?;
            rows.map(|r| {
                serde_json::from_str(&r.map_err(|e| e.to_string())?).map_err(|e| e.to_string())
            })
            .collect()
        })
    }

    pub fn historical_game(&self, id: &str) -> Result<Option<crate::library::Game>> {
        let id = id.to_owned();
        self.call(move |db| {
            let body: Option<String> = db
                .query_row(
                    "SELECT payload FROM games WHERE id=?1 AND active=0",
                    [id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            body.map(|value| serde_json::from_str(&value).map_err(|e| e.to_string()))
                .transpose()
        })
    }

    pub fn save_games(&self, games: &[crate::library::Game]) -> Result<()> {
        for chunk in games.chunks(100) {
            let rows = chunk.to_vec();
            self.call(move |db| {
                let tx = db.transaction().map_err(|e| e.to_string())?;
                for game in rows {
                    write_game(&tx, &game)?;
                }
                tx.commit().map_err(|e| e.to_string())
            })?;
        }
        Ok(())
    }

    pub fn save_pack_associations(&self, games: &[crate::library::Game]) -> Result<()> {
        let games = games.to_vec();
        self.call(move |db| {
            let tx = db.transaction().map_err(|e| e.to_string())?;
            for game in games {
                write_game(&tx, &game)?;
            }
            tx.commit().map_err(|e| e.to_string())
        })
    }

    pub fn retire(&self, paths: Vec<String>) -> Result<()> {
        self.call(move |db| {
            let tx = db.transaction().map_err(|e| e.to_string())?;
            for path in paths {
                tx.execute(
                    "UPDATE games SET active=0 WHERE path=?1 AND active=1",
                    [path],
                )
                .map_err(|e| e.to_string())?;
            }
            tx.commit().map_err(|e| e.to_string())
        })
    }

    /// Migration is all-or-nothing. Invalid JSON never becomes an empty library.
    pub fn import_legacy(&self, path: &Path) -> Result<()> {
        if self
            .get::<bool>("migration", "legacy-imported")?
            .unwrap_or(false)
        {
            return Ok(());
        }
        #[derive(serde::Deserialize)]
        struct Legacy {
            games: std::collections::HashMap<String, crate::library::Game>,
        }
        let games = match std::fs::read(path) {
            Ok(bytes) => {
                serde_json::from_slice::<Legacy>(&bytes)
                    .map_err(|e| {
                        format!(
                            "No se pudo importar {}: {e}. El original está intacto.",
                            path.display()
                        )
                    })?
                    .games
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Default::default(),
            Err(e) => return Err(format!("No se pudo leer la biblioteca anterior: {e}")),
        };
        self.call(move |db| {
            let tx = db.transaction().map_err(|e| e.to_string())?;
            if tx.query_row("SELECT EXISTS(SELECT 1 FROM records WHERE namespace='migration' AND key='legacy-imported')", [], |row| row.get::<_, bool>(0)).map_err(|e| e.to_string())? {
                return Ok(());
            }
            let existing: i64 = tx.query_row("SELECT COUNT(*) FROM games", [], |row| row.get(0)).map_err(|e| e.to_string())?;
            if existing != 0 {
                return Err("La biblioteca ya contiene datos sin una marca de importación. Se conservan ambas fuentes para revisión.".into());
            }
            let mut ids = std::collections::HashSet::new();
            let mut paths = std::collections::HashSet::new();
            let mut expected = Vec::with_capacity(games.len());
            for mut game in games.into_values() {
                game.ensure_id();
                if !ids.insert(game.id.clone()) || !paths.insert(game.path.clone()) {
                    return Err("La biblioteca anterior contiene identidades o rutas duplicadas. No se importó ningún juego.".into());
                }
                expected.push((game.id.clone(), serde_json::to_value(&game).map_err(|e| e.to_string())?));
                write_game(&tx, &game)?;
            }
            let count: i64 = tx.query_row("SELECT COUNT(*) FROM games WHERE active=1", [], |row| row.get(0)).map_err(|e| e.to_string())?;
            if usize::try_from(count).ok() != Some(expected.len()) {
                return Err("El conteo de juegos importados no coincide; se conserva el original.".into());
            }
            for (id, source) in expected {
                let body: String = tx.query_row("SELECT payload FROM games WHERE id=?1 AND active=1", [&id], |row| row.get(0)).map_err(|e| e.to_string())?;
                let stored: crate::library::Game = serde_json::from_str(&body).map_err(|e| e.to_string())?;
                if serde_json::to_value(&stored).map_err(|e| e.to_string())? != source {
                    return Err("Los datos importados no coinciden; se conserva el original.".into());
                }
            }
            tx.execute("INSERT INTO records(namespace,key,payload) VALUES('migration','legacy-imported','true')", []).map_err(|e| e.to_string())?;
            tx.commit().map_err(|e| e.to_string())
        })
    }

    pub fn backup(&self, destination: &Path) -> Result<()> {
        let destination = destination.to_path_buf();
        self.call(move |db| {
            let mut copy = Connection::open(&destination).map_err(|e| e.to_string())?;
            {
                let backup =
                    rusqlite::backup::Backup::new(db, &mut copy).map_err(|e| e.to_string())?;
                backup
                    .run_to_completion(64, Duration::from_millis(10), None)
                    .map_err(|e| e.to_string())?;
            }
            let check: String = copy
                .query_row("PRAGMA quick_check", [], |r| r.get(0))
                .map_err(|e| e.to_string())?;
            if check != "ok" {
                return Err(format!("La copia no pasó la verificación: {check}"));
            }
            Ok(())
        })
    }

    pub fn record_session(
        &self,
        session: &str,
        game: &crate::library::Game,
        seconds: u64,
    ) -> Result<bool> {
        let (session, game) = (session.to_owned(), game.clone());
        self.call(move |db| {
            let tx = db.transaction().map_err(|e| e.to_string())?;
            let inserted = tx
                .execute(
                    "INSERT OR IGNORE INTO sessions(id,game_id,seconds) VALUES(?1,?2,?3)",
                    params![session, game.id, seconds.min(i64::MAX as u64) as i64],
                )
                .map_err(|e| e.to_string())?;
            if inserted != 0 {
                write_game(&tx, &game)?;
            }
            tx.commit().map_err(|e| e.to_string())?;
            Ok(inserted != 0)
        })
    }
}

fn writer_lock(path: &Path) -> Result<Option<std::fs::File>> {
    validate_local_path(path)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let file = match std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path.with_extension("writer-lock"))
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let deadline = std::time::Instant::now() + Duration::from_millis(100);
    loop {
        if file.try_lock().is_ok() {
            break;
        }
        if std::time::Instant::now() >= deadline {
            return Err("La biblioteca está en uso por otra instancia de Play CE. Volvé a esa ventana o cerrala antes de continuar.".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Ok(Some(file))
}

fn write_game(db: &Connection, game: &crate::library::Game) -> Result<()> {
    let body = serde_json::to_string(game).map_err(|e| e.to_string())?;
    // Keep historical user state when another ROM replaces the same pathname.
    db.execute(
        "UPDATE games SET active=0 WHERE path=?1 AND id<>?2 AND active=1",
        params![game.path, game.id],
    )
    .map_err(|e| e.to_string())?;
    let changed = db.execute("INSERT INTO games(id,path,sort_title,platform,favorite,last_played,active,payload) VALUES(?1,?2,?3,?4,?5,?6,1,?7) ON CONFLICT(id) DO UPDATE SET path=excluded.path,sort_title=excluded.sort_title,platform=excluded.platform,favorite=excluded.favorite,last_played=excluded.last_played,active=1,payload=excluded.payload WHERE payload!=excluded.payload OR active!=1", params![game.id, game.path, game.title().to_lowercase(), game.platform, game.favorite, game.last_played, body]).map_err(|e| e.to_string())?;
    crate::cache_metrics::DB_ROWS_CHANGED
        .fetch_add(changed as u64, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

fn validate_local_path(path: &Path) -> Result<()> {
    let text = path.to_string_lossy();
    if text.starts_with(r"\\") && (!text.starts_with(r"\\?\") || text.starts_with(r"\\?\UNC\")) {
        return Err("La DB debe estar en almacenamiento local".into());
    }
    Ok(())
}

fn open_read_only(path: &Path) -> Result<Connection> {
    validate_local_path(path)?;
    let db = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| format!("No se pudo leer la biblioteca: {e}"))?;
    let version: i64 = db
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if version != SCHEMA {
        return Err("La biblioteca requiere migración y no tiene permiso de escritura".into());
    }
    verify_database(&db)?;
    Ok(db)
}

fn verify_database(db: &Connection) -> Result<()> {
    let result: String = db
        .query_row("PRAGMA quick_check", [], |r| r.get(0))
        .map_err(|e| format!("No se pudo verificar la biblioteca; se conserva el original: {e}"))?;
    if result != "ok" {
        return Err(format!(
            "Biblioteca dañada; se conserva el original: {result}"
        ));
    }
    Ok(())
}

fn open_database(path: &Path) -> Result<Connection> {
    validate_local_path(path)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let db = match Connection::open(path) {
        Ok(db) => db,
        Err(_) if path.is_file() => return open_read_only(path),
        Err(error) => return Err(format!("No se pudo abrir la biblioteca: {error}")),
    };
    db.busy_timeout(Duration::from_secs(2))
        .map_err(|e| e.to_string())?;
    let version: i64 = db
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if version > SCHEMA {
        return Err("Esta biblioteca necesita una versión más reciente de Play CE".into());
    }
    verify_database(&db)?;
    if db.is_readonly("main").map_err(|e| e.to_string())? {
        if version != SCHEMA {
            return Err("La biblioteca requiere migración y no tiene permiso de escritura".into());
        }
        return Ok(db);
    }
    db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;")
        .map_err(|e| e.to_string())?;
    if version == 0 {
        db.execute_batch("BEGIN IMMEDIATE;
            CREATE TABLE IF NOT EXISTS games(id TEXT PRIMARY KEY,path TEXT NOT NULL,sort_title TEXT NOT NULL,platform TEXT NOT NULL,favorite INTEGER NOT NULL,last_played INTEGER NOT NULL,active INTEGER NOT NULL,payload TEXT NOT NULL);
            CREATE UNIQUE INDEX IF NOT EXISTS active_path ON games(path) WHERE active=1;
            CREATE INDEX IF NOT EXISTS games_title ON games(active,sort_title,id);
            CREATE INDEX IF NOT EXISTS games_recent ON games(active,last_played DESC,id);
            CREATE TABLE IF NOT EXISTS records(namespace TEXT NOT NULL,key TEXT NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(namespace,key));
            CREATE TABLE IF NOT EXISTS sessions(id TEXT PRIMARY KEY,game_id TEXT NOT NULL,seconds INTEGER NOT NULL);
            PRAGMA user_version=1; COMMIT;").map_err(|e| e.to_string())?;
    }
    Ok(db)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_verifies_user_state_and_rejects_duplicate_identities_atomically() {
        let d = tempfile::tempdir().unwrap();
        let rom = d.path().join("game.md");
        std::fs::write(&rom, b"rom fixture").unwrap();
        let mut game = crate::library::scan_one(&rom, &Default::default()).unwrap();
        game.ensure_id();
        game.favorite = true;
        game.hidden = true;
        game.user_title = Some("Mi juego".into());
        game.user_art = Some("portada.png".into());
        game.pack_path = "selected.ay".into();
        game.available_packs = vec!["selected.ay".into(), "other.ay".into()];
        game.last_played = 1234567;
        game.seconds_played = 9876;
        let legacy = d.path().join("library.json");
        let body = serde_json::to_vec(&serde_json::json!({"games": {"one": &game}})).unwrap();
        std::fs::write(&legacy, &body).unwrap();
        let db = Store::open(&d.path().join("catalog.sqlite3")).unwrap();
        db.import_legacy(&legacy).unwrap();
        db.import_legacy(&legacy).unwrap();
        assert_eq!(
            serde_json::to_value(db.load_games().unwrap()).unwrap(),
            serde_json::json!([game])
        );
        assert_eq!(std::fs::read(&legacy).unwrap(), body);

        let duplicate = Store::open(&d.path().join("duplicate.sqlite3")).unwrap();
        std::fs::write(
            &legacy,
            serde_json::to_vec(&serde_json::json!({"games": {"one": &game, "two": &game}}))
                .unwrap(),
        )
        .unwrap();
        assert!(
            duplicate
                .import_legacy(&legacy)
                .unwrap_err()
                .contains("duplicadas")
        );
        assert!(duplicate.load_games().unwrap().is_empty());
        assert_eq!(
            duplicate
                .get::<bool>("migration", "legacy-imported")
                .unwrap(),
            None
        );
    }
    #[test]
    fn a_second_writer_cannot_enter_and_corruption_is_preserved() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("catalog.sqlite3");
        let first = Store::open(&path).unwrap();
        first.put("user", "favorite", &true).unwrap();
        assert!(Store::open(&path).err().unwrap().contains("otra instancia"));
        assert_eq!(first.get::<bool>("user", "favorite").unwrap(), Some(true));
        let corrupt = d.path().join("corrupt.sqlite3");
        let original = b"this is not a database";
        std::fs::write(&corrupt, original).unwrap();
        assert!(Store::open(&corrupt).is_err());
        assert_eq!(std::fs::read(&corrupt).unwrap(), original);
    }

    #[test]
    fn backups_rotate_only_verified_snapshots_and_read_only_cannot_write() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("catalog.sqlite3");
        let store = Store::open(&path).unwrap();
        for version in 1..=4 {
            store.put("user", "version", &version).unwrap();
            store.checkpoint_and_backup().unwrap();
        }
        for index in 1..=3 {
            let db =
                open_read_only(&path.with_extension(format!("backup-{index}.sqlite3"))).unwrap();
            let version: String = db
                .query_row(
                    "SELECT payload FROM records WHERE namespace='user' AND key='version'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(version, (5 - index).to_string());
            assert!(db.execute("DELETE FROM records", []).is_err());
        }
        assert!(!path.with_extension("backup-new.sqlite3").exists());
    }

    #[test]
    fn malformed_import_preserves_original_and_is_retryable() {
        let d = tempfile::tempdir().unwrap();
        let legacy = d.path().join("library.json");
        std::fs::write(&legacy, b"{broken").unwrap();
        let db = Store::open(&d.path().join("catalog.sqlite3")).unwrap();
        assert!(db.import_legacy(&legacy).is_err());
        assert_eq!(std::fs::read(&legacy).unwrap(), b"{broken");
        std::fs::write(&legacy, br#"{"games":{}}"#).unwrap();
        db.import_legacy(&legacy).unwrap();
        db.import_legacy(&legacy).unwrap();
    }
    #[test]
    fn newer_schema_is_rejected_without_downgrade() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("catalog.sqlite3");
        Connection::open(&path)
            .unwrap()
            .execute_batch("PRAGMA user_version=99")
            .unwrap();
        assert!(Store::open(&path).err().unwrap().contains("más reciente"));
        assert_eq!(
            Connection::open(&path)
                .unwrap()
                .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            99
        );
    }
    #[test]
    fn identical_records_do_not_write_again_and_backup_is_consistent() {
        let d = tempfile::tempdir().unwrap();
        let db = Store::open(&d.path().join("catalog.sqlite3")).unwrap();
        db.put("packs", "one", &vec![1, 2, 3]).unwrap();
        let before = db.call(|c| Ok(c.total_changes())).unwrap();
        db.put("packs", "one", &vec![1, 2, 3]).unwrap();
        assert_eq!(before, db.call(|c| Ok(c.total_changes())).unwrap());
        let copy = d.path().join("copy.sqlite3");
        db.backup(&copy).unwrap();
        assert_eq!(
            Store::open(&copy)
                .unwrap()
                .get::<Vec<i32>>("packs", "one")
                .unwrap(),
            Some(vec![1, 2, 3])
        );
    }
}
