# Offline pack policy for Play CE

Source: AYTHER-Engine v0.1.0-rc.13, commit
`b00d3e02b51d4d3a013426e236b3e04983c3d920`, `core/` (MPL-2.0).

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

The vendored crate is a workspace member so its tests can be run with
`cargo test -p ayther_core --lib` and with `--features local-packs`. Two tests
specifically asserting implicit signature verification run without `local-packs`;
the explicit trust-store policy and CE integrity regression suites still run.
