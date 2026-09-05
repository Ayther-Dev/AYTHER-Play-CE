# Changelog

All notable changes to AYTHER Play CE will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project intends to follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html) once
versioned releases begin.

## [Unreleased]

### Added

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

No versioned release has been published.

[Unreleased]: https://github.com/Ayther-Dev/AYTHER-Play-CE/commits/main
