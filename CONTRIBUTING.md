# Contributing to AYTHER Play CE

Thank you for helping build AYTHER Play CE. The project is currently in repository-bootstrap
and design-alignment work: there is no buildable application or stable public API yet.
Contributions should preserve that distinction and avoid presenting planned behavior as
shipped behavior.

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
3. Hub catalog, download, account, cloud-save, telemetry, and service-managed update features
   belong to the separate proprietary AYTHER Play product.
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

## Current validation

There is no Cargo manifest, runnable test suite, formatter configuration, or CI workflow in
the current snapshot. For documentation and asset changes:

- verify that relative links resolve from the file containing them;
- keep Markdown readable as plain text as well as on GitHub;
- preserve UTF-8 encoding and the line endings defined in `.gitattributes`;
- record the origin, version, license, and checksum of vendored third-party assets; and
- inspect design changes at the intended display size and with controller focus states.

When the Rust workspace is added, Rust-changing pull requests are expected to pass commands
equivalent to the following, adjusted only when the checked-in workspace documents a more
specific workflow:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

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

