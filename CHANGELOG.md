# Changelog

All notable changes to AYTHER Play CE will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project intends to follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html) once
versioned releases begin.

## [Unreleased]

## [0.1.0-beta.2] - 2026-11-20

### Fixed

- The Windows release build no longer opens a terminal window next to Play, and launching a
  game or probing a core no longer opens one for `ayther_runtime.exe`. Debug builds keep the
  console; a release's log is captured by redirecting stderr (`ayther_play_ce.exe 2> play.log`).

## [0.1.0-beta.1] - 2026-11-20

First published pre-release: a Windows installer (per user, no administrator rights) with
the locked AYTHER Runtime `v0.1.0-beta.21` and Tessera. It is not code-signed; verify it
with its `.sha256` file.

### Added

- Release workflow: a `v*` tag builds the Windows installer and publishes it as a GitHub
  pre-release with its SHA-256 sidecar.
- Pinned Rust toolchain, Windows checks and required release integration preparation.
- Save failure-injection regressions and pack-path isolation tests.
- Beta validation record, Runtime control-map documentation and versioned packaging script.

- Initial repository assets: product identity, UI design source, cover art, vendored fonts,
  icon codepoints, and Windows executable resource metadata.
- Rust application, domain modules, Slint integration, localization catalogs, and automated
  unit and source-contract tests.
- Project governance and community documentation.
- Repository-wide formatting, line-ending, and ignore rules for the planned Rust/Slint
  application and its supporting tooling.

### Changed

- Runtime lock bumped to `v0.1.0-beta.21`.
- Reframed product documentation around the repository's verifiable early-stage state.
- Clarified the architectural and licensing boundary between AYTHER Play CE and the separate
  proprietary AYTHER Play product.
- Documented that CE validates local pack integrity but does not establish publisher
  authenticity or trust.
- Removed obsolete hosted-service UI and stale repository paths from the CE design
  documentation.
- Normalized Rust and Slint type, function, method, field, property, callback, and local-variable
  names to descriptive English identifiers using each language's native naming conventions.
- Preserved compatibility with previously serialized session fields through explicit aliases.
- Updated source-contract tests and design references to the Community Edition design filename.

### Security

- Preserve existing saves when writing or replacing a temporary file fails.
- Validate pack identifiers and lifecycle paths before any filesystem mutation.

- Added a coordinated vulnerability-reporting policy and documented the local-pack trust
  boundary.

[Unreleased]: https://github.com/Ayther-Dev/AYTHER-Play-CE/compare/v0.1.0-beta.2...HEAD
[0.1.0-beta.2]: https://github.com/Ayther-Dev/AYTHER-Play-CE/compare/v0.1.0-beta.1...v0.1.0-beta.2
[0.1.0-beta.1]: https://github.com/Ayther-Dev/AYTHER-Play-CE/releases/tag/v0.1.0-beta.1
