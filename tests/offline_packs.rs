//! CE's integrity-only policy must also work in optimized release builds.
use ayther_core::archive_vfs::AyArchive;
use ayther_core::pack_trust::{SignatureEnvelope, TrustStore};
use sha2::{Digest, Sha256};
use std::io::Write;

macro_rules! tr {
    ($($arg:tt)*) => { format!($($arg)*) };
}
#[path = "../src/verifier.rs"]
mod verifier;

fn pack(key: Option<&str>, indexed: bool, damaged: bool) -> tempfile::NamedTempFile {
    let file = tempfile::NamedTempFile::new().unwrap();
    let mut zip = zip::ZipWriter::new(file.reopen().unwrap());
    let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
    let manifest = b"[pack]\nname='Local pack'\nversion='1.0.0'\ngame_id='crc32:665d7df9'\nayther_min='0.8.0'\n[regions]\ndefault='NTSC'\nsupported=['NTSC']\n";
    let mut index = String::from("version = 1\n");
    for (name, data) in [
        ("manifest.toml", manifest.as_slice()),
        ("asset.bin", b"good"),
    ] {
        zip.start_file(name, options).unwrap();
        zip.write_all(if damaged && name == "asset.bin" {
            b"bad!"
        } else {
            data
        })
        .unwrap();
        index.push_str(&format!(
            "[[entry]]\npath='{name}'\nsize={}\nsha256='{:x}'\n",
            data.len(),
            Sha256::digest(data)
        ));
    }
    if indexed {
        zip.start_file("integrity.toml", options).unwrap();
        zip.write_all(index.as_bytes()).unwrap();
    }
    if let Some(key) = key {
        zip.start_file("signature.bin", options).unwrap();
        // Deliberately unverifiable: CE makes no claim about the signer.
        zip.write_all(&SignatureEnvelope::encode(key, [0; 64]).unwrap())
            .unwrap();
    }
    zip.finish().unwrap();
    file
}

#[test]
fn local_packs_do_not_require_a_trusted_signer() {
    for key in [
        None,
        Some("ayther-development-rfc8032"),
        Some("independent-author"),
    ] {
        let file = pack(key, true, false);
        assert_eq!(verifier::audit(file.path()), verifier::Audit::Complete);
        let archive = AyArchive::open_verbose(file.path().to_str().unwrap())
            .unwrap_or_else(|error| panic!("local signer {key:?}: {error:?}"));
        assert_eq!(archive.meta.game_id, "crc32:665d7df9");
        assert_eq!(archive.read("asset.bin").unwrap(), b"good");
    }
}

#[test]
fn legacy_local_packs_remain_readable_without_an_integrity_claim() {
    for key in [None, Some("independent-author")] {
        let file = pack(key, false, false);
        assert_eq!(verifier::audit(file.path()), verifier::Audit::MissingIndex);
        let archive = AyArchive::open_verbose(file.path().to_str().unwrap()).unwrap();
        assert_eq!(archive.read("asset.bin").unwrap(), b"good");
    }
}

#[test]
fn local_policy_still_rejects_modified_content() {
    let file = pack(None, true, true);
    assert!(matches!(
        verifier::audit(file.path()),
        verifier::Audit::Mismatch(_)
    ));
    let archive = AyArchive::open_verbose(file.path().to_str().unwrap()).unwrap();
    assert!(archive.read("asset.bin").is_none());
}

#[test]
fn explicit_trust_store_still_rejects_unsigned_and_unknown_signers() {
    let trust = TrustStore::from_toml("version = 1\n").unwrap();
    for key in [
        None,
        Some("ayther-development-rfc8032"),
        Some("independent-author"),
    ] {
        let file = pack(key, true, false);
        assert!(AyArchive::open_with_trust_store(file.path().to_str().unwrap(), &trust).is_err());
    }
}
