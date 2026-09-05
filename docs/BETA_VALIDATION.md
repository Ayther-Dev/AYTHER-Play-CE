# Beta readiness validation — 2026-09-05

This is a partial validation record, not a release approval.

## Revisions and environment

- Play baseline: `9ea2e39c692ba97405ec93a1de45b0d39308db65`. The implementation was
  initially untracked; the historical 375 passed / 2 failed result is a baseline,
  not coverage evidence. Changes are recorded by the commit containing this file.
- Rust: `rustc 1.95.0 (59807616e 2026-04-14)`, Windows x86-64 MSVC.
- Engine: `v0.1.0-rc.6`, locked commit
  `091ee4ca088dca66837ee285d8a02dd5a1d90e86`.
- Runtime source: clean detached checkout of
  `218aab6970f1472d0da9e5c21015972a3bfd4974`, prepared separately from the sibling.
- Distributed Runtime: **unavailable for validation**. The public GitHub release
  API for `v0.1.0-beta.2` returned HTTP 404. Final asset/platform checksum remains
  unset in `tools/runtime-lock.json` rather than guessed.

## Evidence and task status

| Tasks | Status and evidence |
| --- | --- |
| SAVE-01–04 | Implemented. Same-directory unique temporary, sync, replacement without destination deletion; injected write/replacement failures preserve prior bytes and clean temporaries. Windows sharing-denial regression additionally exercises real replacement failure. Power-loss durability is explicitly not claimed. |
| PACK-01–04 | Implemented. Shared ID rule and lifecycle destination checks before mutations, including CLI calls. Temp-directory regressions cover traversal, absolute paths, Unix/Windows separators, reserved names and nonregular destinations; ordinary install/update/rollback tests remain. |
| RT-01 | Help contract reads runtime_application.cpp. Both prior failures fixed; missing-flag regression preserves the oracle. |
| RT-02 | Blocked on final public beta.2 artifact. Source is pinned; release ZIP and checksum are not available. |
| RT-03 | Real integration is explicitly ignored in normal runs and required with --ignored for release. Missing Runtime/core fails, never returns success. The release preparation command was checked to fail on pending artifact metadata. |
| RT-04–05 | Not executed: distributed Runtime and an authorized playable core/game/controller setup are still required. TOML/unit/source tests do not close in-game checks. |
| RT-06 | Implemented failure-reason propagation and localized Play guidance. Real-process test covers missing/malformed/rejected maps and spaces in path, but remains unexecuted without beta.2. UI behavior also needs manual confirmation. |
| BUILD-01 | Sources, UI, tests, lockfile and resources included in version control; caches, build outputs and dependency checkouts excluded. |
| BUILD-02–04 | Rust 1.95.0 pinned; strict Clippy fixed without global suppressions. Windows PR/push CI configured for format/check/Clippy/tests. Remote Actions execution is not claimed. |
| BUILD-05 | Release workflow and original redistributable probe-only core prepared. Real integration remains blocked on the distribution. The fixture does not emulate a game. |
| BUILD-06 | Clean-checkout verification result recorded below when complete. Dependency downloads are explicit; no sibling checkout is required. |
| DOC-01–03 | README, input-map contract and standalone contributor instructions updated. |
| REL-01 | Versioned Windows package script and external dependencies documented. Package startup outside the development tree and manual game validation are not yet certified. |
| REL-02 | This record distinguishes completed automated checks, baseline, blocked integration and manual work. |

## Automated run

Initial pinned-source run: **380 passed, 0 failed, 2 explicitly ignored**. Strict
Clippy passed. Later regression additions and clean-checkout results are recorded
below after their execution. The two ignored cases are the real Runtime/core probe
and the invalid-map launcher integration; they are not counted as passed.

## Open limitations

The beta remains unapproved until the final distribution can be hash-pinned and
the release workflow plus in-game default/custom controls and packaged startup
checks have run. Pack checks assume no concurrent hostile modification of the
configured directory. Save publication does not establish power-loss durability
or guarantees for Runtime's own independent save implementation.
