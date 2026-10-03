//! Attribute-only revisions and read reservations. No ROM/pack bytes are read here.
use serde::{Deserialize, Serialize};
use std::{fs::File, io, path::Path, sync::atomic::Ordering, time::UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Revision {
    pub bytes: u64,
    pub modified_ns: u64,
    pub identity: String,
    pub change_token: Option<i64>,
}

impl Revision {
    pub fn observe(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        Self::from_file(&file)
    }

    pub fn from_file(file: &File) -> io::Result<Self> {
        crate::cache_metrics::METADATA_CHECKS.fetch_add(1, Ordering::Relaxed);
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "no es un archivo regular",
            ));
        }
        let modified_ns = metadata
            .modified()?
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_nanos()
            .try_into()
            .map_err(io::Error::other)?;
        let (identity, change_token) = native_identity(file, &metadata)?;
        Ok(Self {
            bytes: metadata.len(),
            modified_ns,
            identity,
            change_token,
        })
    }

    /// Cross-process reuse needs a native content-change token, not just mtime.
    pub fn proves_unchanged(&self, other: &Self) -> bool {
        self == other && self.change_token.is_some()
    }

    pub fn runtime_token(&self) -> String {
        format!(
            "{}|{}|{}|{}",
            self.identity,
            self.bytes,
            self.modified_ns,
            self.change_token.unwrap_or(0)
        )
    }
}

/// Windows rejects this reservation if a writer already exists and prevents
/// replacement/deletion until every owner releases it.
pub fn reserve(path: &Path) -> io::Result<File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ);
    }
    options.open(path)
}

/// The installer keeps content immutable while renaming its inspected staging
/// file. Identity is checked after publication; deletion sharing permits rename.
pub fn reserve_for_promotion(path: &Path) -> io::Result<File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_DELETE, FILE_SHARE_READ};
        options.share_mode(FILE_SHARE_READ | FILE_SHARE_DELETE);
    }
    options.open(path)
}

pub fn reserve_for_mutation(path: &Path) -> io::Result<File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_DELETE, FILE_SHARE_READ};
        options
            .access_mode(0x8000_0000 | 0x0001_0000)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_DELETE);
    }
    options.open(path)
}

pub fn directory_identity(path: &Path) -> io::Result<String> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS);
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_dir() {
        return Err(io::Error::other("La raíz no es un directorio"));
    }
    native_identity(&file, &metadata).map(|(identity, _)| identity)
}

#[cfg(windows)]
fn native_identity(file: &File, _: &std::fs::Metadata) -> io::Result<(String, Option<i64>)> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::{
        Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle},
        System::IO::DeviceIoControl,
    };
    let handle = file.as_raw_handle();
    // SAFETY: both APIs receive a live file handle and correctly sized output buffers.
    unsafe {
        let mut info: BY_HANDLE_FILE_INFORMATION = std::mem::zeroed();
        if GetFileInformationByHandle(handle, &mut info) == 0 {
            return Err(io::Error::last_os_error());
        }
        let identity = format!(
            "{:08x}:{:08x}{:08x}:{:08x}{:08x}",
            info.dwVolumeSerialNumber,
            info.nFileIndexHigh,
            info.nFileIndexLow,
            info.ftCreationTime.dwHighDateTime,
            info.ftCreationTime.dwLowDateTime
        );
        let mut buffer = [0u8; 1024];
        let mut bytes = 0u32;
        // Request V2 explicitly; unsupported providers remain unconfirmed.
        let versions = [2u16, 2u16];
        let ok = DeviceIoControl(
            handle,
            0x0009_00eb,
            versions.as_ptr().cast(),
            4,
            buffer.as_mut_ptr().cast(),
            buffer.len() as u32,
            &mut bytes,
            std::ptr::null_mut(),
        );
        let token = if ok != 0 && bytes >= 32 && u16::from_le_bytes([buffer[4], buffer[5]]) == 2 {
            let usn = i64::from_le_bytes(buffer[24..32].try_into().map_err(io::Error::other)?);
            (usn > 0).then_some(usn)
        } else {
            None
        };
        Ok((identity, token))
    }
}

#[cfg(unix)]
fn native_identity(_: &File, md: &std::fs::Metadata) -> io::Result<(String, Option<i64>)> {
    use std::os::unix::fs::MetadataExt;
    Ok((
        format!("{}:{}", md.dev(), md.ino()),
        Some(
            md.ctime()
                .saturating_mul(1_000_000_000)
                .saturating_add(md.ctime_nsec()),
        ),
    ))
}

#[cfg(not(any(windows, unix)))]
fn native_identity(_: &File, _: &std::fs::Metadata) -> io::Result<(String, Option<i64>)> {
    Ok((String::new(), None))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn weak_attributes_never_prove_cross_session_content() {
        let r = Revision {
            bytes: 4,
            modified_ns: 1,
            identity: "file".into(),
            change_token: None,
        };
        assert!(!r.proves_unchanged(&r));
    }
    #[test]
    fn token_or_identity_change_invalidates_revision() {
        let r = Revision {
            bytes: 4,
            modified_ns: 1,
            identity: "file".into(),
            change_token: Some(4),
        };
        let mut changed = r.clone();
        changed.change_token = Some(5);
        assert!(!r.proves_unchanged(&changed));
    }
    #[cfg(windows)]
    #[test]
    fn reservation_blocks_mutation_until_released() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("pack.ay");
        std::fs::write(&p, b"original").unwrap();
        let guard = reserve(&p).unwrap();
        assert!(std::fs::write(&p, b"changed").is_err());
        assert!(std::fs::remove_file(&p).is_err());
        drop(guard);
        std::fs::write(&p, b"changed").unwrap();
    }

    #[test]
    fn equal_size_and_restored_mtime_do_not_reuse_a_native_revision() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("rom.md");
        std::fs::write(&path, b"AAAA").unwrap();
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        let before = Revision::observe(&path).unwrap();
        std::fs::write(&path, b"BBBB").unwrap();
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(modified))
            .unwrap();
        let after = Revision::observe(&path).unwrap();
        assert_eq!(before.bytes, after.bytes);
        assert_eq!(before.modified_ns, after.modified_ns);
        assert!(!before.proves_unchanged(&after));
        eprintln!(
            "native change-token available: {}",
            after.change_token.is_some()
        );
    }
}
