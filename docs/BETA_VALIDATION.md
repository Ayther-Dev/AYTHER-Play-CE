# Beta readiness validation — 2026-09-05

This is a partial validation record, not a release approval.

## Revisions and environment

- Play baseline: `9ea2e39c692ba97405ec93a1de45b0d39308db65`. Its implementation was
  initially untracked. Historical **375 passed / 2 failed** is a baseline, not
  evidence of comprehensive coverage.
- Implementation commit: `b89cd75c2caa4c46aca648a055e14f66d3565b52`, branch
  `feat/beta-readiness`. Subsequent commits pin the definitive distribution and
  record validation; no behavioral source changes after that implementation.
- Rust: `rustc 1.95.0 (59807616e 2026-04-14)`, Windows x86-64 MSVC.
- Engine: `v0.1.0-rc.6`, locked commit
  `091ee4ca088dca66837ee285d8a02dd5a1d90e86`.
- Runtime: published `v0.1.0-beta.2`, commit
  `401cc3e4c0fdebe90e9eb5192ea21c48ec02e518`.
- Runtime Windows asset: `ayther-runtime-0.1.0-beta.2-Windows-AMD64-Runtime.zip`.
  SHA-256: `01fd646b295422ed5b97462f7970711901689b6ffe822c1730b2cd7adf106e23`.
  Downloaded bytes match the release digest; packaged metadata reports beta.2,
  protocol 1 and Engine rc.6. Exact URL is in `tools/runtime-lock.json`.
- Initial unauthenticated release lookup returned 404. A subsequent GitHub CLI
  lookup confirmed the published release and both repositories' public visibility;
  the earlier lookup was not evidence that beta.2 was unavailable.

## Automated evidence

- Formatting and strict all-target/all-feature Clippy: passed, no global lint
  suppressions added.
- Main suite against beta.2 source: **382 passed, 0 failed, 2 explicitly ignored**.
- Separate required real integration: **2 passed, 0 failed, 0 ignored** against
  the distributed Runtime. This covers the Play core probe and Play's launcher
  with missing, malformed and contract-rejected input maps in a path with spaces.
  Every map failure returns 78, preserves `input.map_invalid`, writes no save and
  emits no Ready/NowPlaying event.
- Negative integration gate: missing dependency variables cause failure, not a
  skipped/pass result. Incomplete release metadata also fails preparation.
- Clean local clone of the committed Play implementation, with a new `target`
  directory: format, locked offline check, Clippy and all tests passed. Check took
  3m 37s. Rust crate/Engine source caches were reused; compilation outputs and
  sibling checkouts were not. Runtime was separately cloned and pinned. This is
  not a fresh Windows installation or a network bootstrap test without caches.
- Original MPL-2.0 metadata-only probe fixture compiled successfully. It contains
  no ROM, firmware, commercial game or third-party emulator code.
- Windows release build and versioned packaging succeeded. The extracted binary
  ran `--packs` outside the development tree and exited 0. This checks the loader
  and CLI startup, not visual UI behavior or in-game interaction.
- Remote GitHub Actions execution has not been observed; the equivalent local
  checks above passed. CI is configured for pushes/PRs and release integration
  for version tags/manual dispatch.

## Task closure

| Tasks | Result |
| --- | --- |
| SAVE-01–04 | Complete: atomic publication without deleting the destination, error propagation, cleanup policy, injected failures and real Windows sharing-denial regression. Power-loss durability explicitly not claimed. |
| PACK-01–04 | Complete: centralized portable IDs and checks for all lifecycle destinations before mutation, including CLI entry points. Traversal/absolute/separator/reserved-name regressions preserve outside witnesses; ordinary install/update/rollback pass. |
| RT-01–03 | Complete: updated help oracle and missing-flag regression, final tag/asset/checksum pinned, integration required rather than silently skipped. |
| RT-04–05 | Pending: default/custom keyboard and physical controller mappings must still be verified inside an authorized playable game/core. Source and TOML tests do not close these checks. |
| RT-06 | Automated process behavior passed for all requested invalid-map categories. Localized Play diagnosis implemented; visual confirmation remains manual. |
| BUILD-01–04 | Implementation tracked, toolchain pinned, Clippy clean, basic CI configured. |
| BUILD-05 | Real probe and invalid-map integration passed with versioned Runtime and redistributable probe fixture; workflow configured. |
| BUILD-06 | Clean-clone commands passed with cached dependency sources and fresh build output; clean-OS verification remains outside this run. |
| DOC-01–03 | README, input-map contract and standalone contributor instructions updated. |
| REL-01 | Windows package generation and extracted CLI startup passed; graphical startup/in-game checks on a clean Windows host remain pending. |
| REL-02 | This record distinguishes actual automated evidence, historical baseline and remaining manual checks. |

## Remaining limits

The beta remains unapproved until default/custom controls are verified in-game,
Play's error UI is checked visually and the package is exercised on a clean host.
Pack checks assume no concurrent hostile replacement of directory entries. Save
publication does not establish power-loss durability or guarantees for Runtime's
independent save implementation. The probe-only fixture cannot emulate a game.
