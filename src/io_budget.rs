//! At most two heavy readers, with one reader per native volume.
use std::{
    collections::HashSet,
    sync::{
        Condvar, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
static BUDGET: OnceLock<(Mutex<HashSet<String>>, Condvar)> = OnceLock::new();
pub struct Permit(String);
pub fn acquire(identity: &str, cancel: &AtomicBool) -> Result<Permit, String> {
    let volume = identity.split(':').next().unwrap_or("unknown").to_owned();
    let (mutex, changed) = BUDGET.get_or_init(Default::default);
    let mut active = mutex.lock().map_err(|e| e.to_string())?;
    while active.len() >= 2 || active.contains(&volume) {
        if cancel.load(Ordering::Acquire) {
            return Err("Operación cancelada".into());
        }
        active = changed
            .wait_timeout(active, Duration::from_millis(50))
            .map_err(|e| e.to_string())?
            .0;
    }
    if cancel.load(Ordering::Acquire) {
        return Err("Operación cancelada".into());
    }
    active.insert(volume.clone());
    Ok(Permit(volume))
}
impl Drop for Permit {
    fn drop(&mut self) {
        let (mutex, changed) = BUDGET.get_or_init(Default::default);
        if let Ok(mut active) = mutex.lock() {
            active.remove(&self.0);
        }
        changed.notify_all();
    }
}
