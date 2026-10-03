//! Work counters distinguish metadata checks from content reads.
use std::sync::atomic::{AtomicU64, Ordering};

pub static METADATA_CHECKS: AtomicU64 = AtomicU64::new(0);
pub static PACK_CONTENT_OPENS: AtomicU64 = AtomicU64::new(0);
pub static PACK_METADATA_READS: AtomicU64 = AtomicU64::new(0);
pub static PACK_AUDIT_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
pub static PACK_AUDIT_SUCCESSES: AtomicU64 = AtomicU64::new(0);
pub static PACK_AUDIT_BYTES: AtomicU64 = AtomicU64::new(0);
pub static ROM_HASH_BYTES: AtomicU64 = AtomicU64::new(0);
pub static DIRECTORIES_ENUMERATED: AtomicU64 = AtomicU64::new(0);
pub static DB_ROWS_CHANGED: AtomicU64 = AtomicU64::new(0);

pub fn snapshot() -> serde_json::Value {
    serde_json::json!({
        "metadata_checks": METADATA_CHECKS.load(Ordering::Relaxed),
        "pack_content_opens": PACK_CONTENT_OPENS.load(Ordering::Relaxed),
        "pack_metadata_reads": PACK_METADATA_READS.load(Ordering::Relaxed),
        "pack_audit_attempts": PACK_AUDIT_ATTEMPTS.load(Ordering::Relaxed),
        "pack_audit_successes": PACK_AUDIT_SUCCESSES.load(Ordering::Relaxed),
        "pack_audit_bytes": PACK_AUDIT_BYTES.load(Ordering::Relaxed),
        "rom_hash_bytes": ROM_HASH_BYTES.load(Ordering::Relaxed),
        "directories_enumerated": DIRECTORIES_ENUMERATED.load(Ordering::Relaxed),
        "db_rows_changed": DB_ROWS_CHANGED.load(Ordering::Relaxed),
    })
}
