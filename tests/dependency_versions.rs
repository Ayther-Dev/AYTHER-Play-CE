use std::{fs, path::Path};

const ENGINE_TAG: &str = "v0.1.0-rc.18";
const ENGINE_COMMIT: &str = "79c47fedfff8be4f0b94c3a3ee121bb2ed0081b6";
const RUNTIME_TAG: &str = "v0.1.0-beta.12";
const RUNTIME_COMMIT: &str = "9f5ebf36b59b864bee65440345ffa8175a62fae4";
const RUNTIME_ASSET: &str = "ayther-runtime-0.1.0-beta.12-Windows-AMD64-Runtime.zip";
const RUNTIME_SHA256: &str = "3b502e04dd05ecd1767640508de169377c34c56e97d5ab91061fdb3c92b13768";

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
