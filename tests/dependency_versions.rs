use std::{fs, path::Path};

const ENGINE_TAG: &str = "v0.1.0-rc.13";
const ENGINE_COMMIT: &str = "b00d3e02b51d4d3a013426e236b3e04983c3d920";
const RUNTIME_TAG: &str = "v0.1.0-beta.8";
const RUNTIME_COMMIT: &str = "94bd583503dcaf76cb553f31d240e879e0cc49e4";
const RUNTIME_ASSET: &str = "ayther-runtime-0.1.0-beta.8-Windows-AMD64-Runtime.zip";
const RUNTIME_SHA256: &str = "62cac82dd8faf9de75473ba17e699edcc0220d0c74c6de0bc13946fe49ba0d58";

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
