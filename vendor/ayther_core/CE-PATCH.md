# Offline pack policy for Play CE

Source: AYTHER-Engine v0.1.0-rc.9, commit
`af2623d536c9b3f9a08f9c61757f0b3c1e6bbfa7`, `core/` (MPL-2.0).

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
