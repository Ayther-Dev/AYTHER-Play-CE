# Contributing to AYTHER Play CE

Thank you for helping build AYTHER Play CE. The project is an active, pre-release Rust/Slint
application. Its public interfaces, persistence formats, workspace layout, and supported
platforms are not yet stable. Contributions must distinguish implemented behavior from release
support and must not present an unversioned development snapshot as production-ready.

## Before contributing

- Search [existing issues](https://github.com/Ayther-Dev/AYTHER-Play-CE/issues) before opening
  a new one.
- Use an issue to discuss substantial product-scope, architecture, data-format, trust-model,
  or licensing changes before investing in an implementation.
- Report vulnerabilities privately as described in [SECURITY.md](SECURITY.md).
- Confirm that you have the right to contribute every code, design, font, image, and data file
  included in the change.

## Product constraints

Changes must respect the project's core boundaries:

1. CE must remain usable without an AYTHER account or AYTHER-operated service.
2. CE may validate a pack's local integrity but must not represent that result as publisher
   identity, signature trust, or authenticity.
3. Managed catalog, download, account, cloud-save, telemetry, and service-managed update
   features belong to the separate proprietary AYTHER Play product.
4. The primary UI must remain practical for ten-foot, keyboard, and controller navigation.
5. User-provided games, firmware, BIOS files, and packs must not be committed to the repository
   or included in test fixtures without clear redistribution rights.

## Development workflow

1. Fork the repository and create a focused branch from `main`.

   ```text
   git switch main
   git pull --ff-only
   git switch -c feat/short-description
   ```

   Use an accurate prefix such as `feat/`, `fix/`, `docs/`, or `chore/`.

2. Keep commits small and use a short, imperative subject line. Explain *why* in the commit
   body when the reason is not obvious from the diff.
3. Update documentation, tests, and [CHANGELOG.md](CHANGELOG.md) when a change affects users,
   contributors, compatibility, security, or a documented contract.
4. Open a pull request that links the relevant issue and describes the behavior, risks,
   validation performed, and any follow-up work.

## Naming and API conventions

Identifiers must be English and follow the native conventions of each language:

- Rust types, traits, and enum variants use `UpperCamelCase`; modules, functions, methods,
  fields, parameters, and local variables use `snake_case`.
- Boolean predicates should read as questions, normally with `is_`, `has_`, `can_`, or
  `should_`. Use domain abbreviations such as ROM, CRC, and HD only when they are clearer than
  their expanded form.
- Slint components use `PascalCase`; properties, callbacks, element IDs, and loop variables use
  descriptive `kebab-case` names. Remember that public Slint names generate Rust accessors.
- Prefer names that describe a role or result over implementation details, single-letter names,
  or historical terminology. Short iterator variables are acceptable when their scope and
  meaning are immediately obvious.
- Renaming an internal identifier must not silently break stored user data or an external
  protocol. Preserve serialized keys with `serde` aliases or provide an explicit migration.
- User-visible source text may remain in the catalog's source language, but it must use the
  localization mechanism and have a complete English translation.

Run `rustfmt` on Rust changes. Do not mechanically reformat unrelated code in a focused pull
request.

## Validation

Play is a standalone Cargo workspace. `rust-toolchain.toml` selects Rust 1.95.0.
Cargo fetches Engine v0.1.0-rc.6 directly; no parent workspace or sibling Engine is required.
Run `./tools/prepare-runtime.ps1` to prepare pinned Runtime sources as described in
[docs/RELEASE.md](docs/RELEASE.md). Rust-changing pull requests should pass:

```text
cargo fmt --all -- --check
cargo check -p ayther_play_ce --all-targets --all-features --locked
cargo clippy -p ayther_play_ce --all-targets --all-features --locked -- -D warnings
cargo test -p ayther_play_ce --all-features --locked
```

Some source-contract tests read sibling runtime files directly. If the required repository is
not available, report that limitation separately; do not describe the affected tests as passed.

For documentation and asset changes:

- verify that relative links resolve from the file containing them;
- keep Markdown readable as plain text as well as on GitHub;
- preserve UTF-8 encoding and the line endings defined in `.gitattributes`;
- record the origin, version, license, and checksum of vendored third-party assets; and
- inspect design changes at the intended display size and with controller focus states.

Do not claim a command passed unless it was run against the exact revision in the pull
request.

## Pull request checklist

- [ ] The change is focused and its user-visible effect is explained.
- [ ] Planned and implemented behavior are clearly distinguished.
- [ ] Tests or other reproducible validation cover the change where possible.
- [ ] Documentation and the changelog are updated where relevant.
- [ ] No secrets, personal game data, ROMs, firmware, or unlicensed assets are included.
- [ ] New dependencies and assets have documented provenance and compatible licenses.
- [ ] Security and privacy implications have been considered.
- [ ] Generated files, if any, were produced from checked-in sources and are in sync.

## Licensing contributions

Unless a file clearly states otherwise, contributions to original project source files are
made available under the [Mozilla Public License 2.0](LICENSE). By submitting a contribution,
you represent that you have the right to do so under those terms. Third-party material keeps
its original license and must be identified explicitly.

MPL-2.0 does not grant trademark rights. Do not imply endorsement or reuse AYTHER branding
outside the permissions provided by applicable law or a separate authorization.
