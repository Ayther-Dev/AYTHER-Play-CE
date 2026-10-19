use std::{fs, path::Path};

const ENGINE_TAG: &str = "v0.1.0-rc.17";
const ENGINE_COMMIT: &str = "4ed2310f13d5cfd16156fe14b185f3273fc5a5b0";
const RUNTIME_TAG: &str = "v0.1.0-beta.11";
const RUNTIME_COMMIT: &str = "a2fbf061912b890a64e3c5b428ee8e61787a6a5a";
const RUNTIME_ASSET: &str = "ayther-runtime-0.1.0-beta.11-Windows-AMD64-Runtime.zip";
const RUNTIME_SHA256: &str = "6f82ef8d1da9485464fddc28329d3c9f60d8c96bb30cef518e601e575ad4e5d0";

fn read(root: &Path, path: &str) -> String {
    fs::read_to_string(root.join(path)).unwrap_or_else(|error| panic!("{path}: {error}"))
}

#[test]
fn published_engine_and_runtime_are_pinned_together() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let cargo = read(root, "Cargo.toml");
    let provenance = read(root, "vendor/ayther_core/CE-PATCH.md");
    let prepare_runtime = read(root, "tools/prepare-runtime.ps1");
    let release_workflow = read(root, ".github/workflows/integration.yml");
    let runtime: serde_json::Value =
        serde_json::from_str(&read(root, "tools/runtime-lock.json")).expect("runtime lock JSON");

    assert!(cargo.contains(&format!("tag = \"{ENGINE_TAG}\"")));
    assert!(provenance.contains(ENGINE_TAG));
    assert!(provenance.contains(ENGINE_COMMIT));
    assert_eq!(runtime["release_tag"], RUNTIME_TAG);
    assert_eq!(runtime["source_commit"], RUNTIME_COMMIT);
    assert_eq!(runtime["asset_name"], RUNTIME_ASSET);
    assert_eq!(runtime["sha256"], RUNTIME_SHA256);
    assert!(
        runtime["asset_url"]
            .as_str()
            .is_some_and(|url| url.ends_with(&format!("/{RUNTIME_TAG}/{RUNTIME_ASSET}")))
    );
    assert!(
        prepare_runtime.contains("runtime-$($lock.release_tag).zip"),
        "the Runtime cache must be versioned so an older release cannot poison validation"
    );
    assert!(
        release_workflow.contains("--skip profile_local_startup"),
        "release integration must not run the opt-in local profiling test"
    );
}
