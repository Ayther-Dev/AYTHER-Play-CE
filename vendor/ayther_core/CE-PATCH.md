# Offline pack policy for Play CE

Source: AYTHER-Engine v0.1.0-rc.17, commit
`4ed2310f13d5cfd16156fe14b185f3273fc5a5b0`, `core/` (MPL-2.0). The CE changes
were carried forward from the rc.13 snapshot with a three-way merge; upstream
rc.13→rc.17 does not touch `archive_vfs.rs` and only adds to `pack_validate.rs`.

The pinned upstream reader rejects all signatures in release builds when no
trust store is supplied. Play CE intentionally has no signer registry and audits
content through `verifier::audit`; consequently it could not install local packs
despite passing its debug tests.

This snapshot adds the opt-in `local-packs` feature. Without an explicit trust
store, it skips signature authentication in the legacy and indexed readers.
Archive policy checks, manifest parsing, integrity index checks and per-entry
hash verification are unchanged. It does not assign an accepted signer.
`open_with_trust_store` retains its strict policy, even with this feature enabled.
Without the feature, upstream behavior is unchanged.

The manifest has standalone package metadata instead of upstream workspace
inheritance. All other source files are unchanged. This local Cargo patch keeps
builds reproducible without depending on a sibling checkout or altering Cargo's
shared dependency cache. Remove it once a pinned upstream release supports CE's
offline integrity policy.

Regression checks: `cargo test --release --test offline_packs` and
`cargo test --release --bin ayther_play_ce pack_install::tests`.

The incremental catalog adds serializable `CatalogMetadata` and
`ValidationMetadata` projections. `read_metadata` reads only bounded metadata
entries and applies the existing archive policy. `validate_metadata` and
`grade_report` expose the same compatibility rules for cached projections;
`validate_path`/`compat_grade` delegate to them. Catalog display does not require
opening or decompressing asset payloads. Full byte auditing remains a separate
step in Play, and Runtime's own trust/integrity checks remain enabled.

rc.17 adds a validator rule (spec 002, R7) that reads the first bytes of every
asset `pose_substitutions.toml` assigns. To keep `validate_metadata` free of
archive access, `read_metadata` also captures `pose_substitutions.toml` and the
first 64 bytes of each entry its poses resolve to (`asset_heads`, serde default
empty); the rule then runs on that data with upstream's verdicts. Play's
snapshot cache bumps `INSPECTION_VERSION` so older snapshots are re-read.

The vendored crate is a workspace member so its tests can be run with
`cargo test -p ayther_core --lib` and with `--features local-packs`. Two tests
specifically asserting implicit signature verification run without `local-packs`;
the explicit trust-store policy and CE integrity regression suites still run.
