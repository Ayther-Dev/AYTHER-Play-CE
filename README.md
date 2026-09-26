<p align="center">
  <img src="resources/AYTHER-Play.png" alt="AYTHER Play CE logo" width="160">
</p>

<h1 align="center">AYTHER Play CE</h1>

<p align="center">
  An offline-first, community-edition game launcher for local libraries,
  controller-friendly navigation, local saves, and locally supplied enhancement packs.
</p>

> [!WARNING]
> **Pre-release software.** The repository contains an active Rust/Slint implementation and
> automated tests, but it has no published release, stable public API, compatibility promise,
> or production support policy. The Cargo workspace is self-contained; AYTHER Engine and
> AYTHER Runtime are versioned external dependencies obtained from their own repositories.

## Overview

AYTHER Play CE is the open-source, local-first launcher in the AYTHER ecosystem. It discovers
games on the user's machine, starts the AYTHER runtime, manages local session data and saves,
and can inspect and install `.ay` enhancement packs supplied through local storage.

CE is intentionally independent of AYTHER-operated services. It requires no account, cloud
storage, telemetry endpoint, managed catalog, or service-controlled updater. The proprietary
AYTHER Play product and its service integrations are separate products and are not included or
licensed through this repository.

## Current implementation

The current source tree includes:

- a ten-foot Slint interface with keyboard and controller focus routing;
- local library discovery, filtering, metadata, and No-Intro data support;
- custom game covers: choose **Asignar carátula / Assign cover art** in a game's menu
  and select a local PNG, JPG, or WebP image. Keep the image at its selected location;
  the choice survives restarts and rescans. **Restaurar valores automáticos / Restore
  automatic values** resets both the custom title and cover;
- automatic covers beside the ROM: `Game.md` uses `Game.png`, `Game.jpg`,
  `Game.jpeg`, or `Game.webp`, in that order (extensions are case-insensitive).
  These take priority over images in the covers directory; a manually selected cover
  takes priority over both. Restart or rescan with **F5** to discover new images;
- runtime launch configuration, process supervision, and structured runtime-event handling;
- local save, session, crash-recovery, diagnostics, import, and export workflows;
- local `.ay` pack discovery, inspection, integrity auditing, installation, and rollback;
- bundled English localization alongside the Spanish source catalog;
- Windows executable resources and vendored UI fonts; and
- unit and source-contract tests for domain behavior, accessibility, localization, focus,
  design-token drift, and the Community Edition boundary.

These capabilities are under active development. Their presence in source does not imply a
supported release or a stable on-disk format.

## Trust model for packs

AYTHER Play CE distinguishes **integrity** from **authenticity**:

- Integrity checks whether a local pack is complete and matches the hashes declared in its own
  `integrity.toml` manifest. Installation audits the staged archive before replacing installed
  state.
- Authenticity establishes who produced a pack. That requires an independent trust source,
  which CE does not provide.

A pack that passes its integrity audit can still be untrusted or malicious. CE must not present
a local hash result as publisher identity, signature trust, provenance, or safety. Obtain packs
only from sources you trust.

## Technology

| Area | Technology |
| --- | --- |
| Application and domain logic | Rust 2024 |
| Desktop UI | Slint 1.16 |
| Configuration and local metadata | TOML, JSON, and gettext PO catalogs |
| Parallel library scanning | Rayon and WalkDir |
| Pack handling | ZIP, SHA-256, and `ayther_core` |
| Controller input | gilrs |
| Windows resources | `embed-resource` and Win32 `.rc` metadata |

## AYTHER dependencies

Play uses [AYTHER Engine v0.1.0-rc.9](https://github.com/Ayther-Dev/AYTHER-Engine/releases/tag/v0.1.0-rc.9),
with the documented offline-reader patch in [`vendor/ayther_core/CE-PATCH.md`](vendor/ayther_core/CE-PATCH.md).
No sibling Engine checkout is required.

Play targets [Runtime v0.1.0-beta.5](https://github.com/Ayther-Dev/AYTHER-Runtime/releases/tag/v0.1.0-beta.5),
published on 2026-09-15. The Windows AMD64 distribution and source commit
`428249701c7c9404fbb4a1308ee1cc3d46a3d32d` are pinned with the archive SHA-256 in
[`tools/runtime-lock.json`](tools/runtime-lock.json). Real probe and invalid-map
integration checks passed against that distributed binary.

Use Rust **1.95.0** (selected by `rust-toolchain.toml`). On Windows install Visual
Studio 2022 C++ Build Tools and Windows SDK, plus Git and PowerShell 7. From this
standalone Cargo workspace:

```powershell
./tools/prepare-runtime.ps1
cargo fmt --all -- --check
cargo check -p ayther_play_ce --all-targets --all-features --locked
cargo clippy -p ayther_play_ce --all-targets --all-features --locked -- -D warnings
cargo test -p ayther_play_ce --all-features --locked
```

The preparation script creates a clean detached source checkout in
`validation-deps/runtime-source` and sets `AYTHER_RUNTIME_SOURCE`. Developers may
instead set that variable to an absolute path or a path relative to Play; without
an override the tests use `../AYTHER-Runtime`. Release validation uses the pinned
source and hash-verified distribution, not a developer checkout.

Custom keyboard and controller controls are configured in Play and passed as a
TOML map. See [`docs/RUNTIME_INPUT_MAP.md`](docs/RUNTIME_INPUT_MAP.md) for defaults,
allowed values and failures. Beta.1 predates this contract. Default/custom controls
inside an actual game still require the manual checks in the validation record.

CI runs formatting, compilation, Clippy and tests. Real-process tests are marked
ignored in ordinary development and explicitly required by the separate release
integration workflow. Missing dependencies fail that workflow. See
[`docs/RELEASE.md`](docs/RELEASE.md) for clean builds, packaging and manual checks,
[`docs/PERSISTENCE.md`](docs/PERSISTENCE.md) for save and pack guarantees, and
[`docs/BETA_VALIDATION.md`](docs/BETA_VALIDATION.md) for actual validation results.

## Runtime behavior

The launcher expects a compatible `ayther_runtime` executable and the cores it will load.
Runtime paths are configuration-dependent and are probed before launch. Games, ROMs, firmware,
BIOS files, cores, and third-party packs are not distributed by this project.

The command-line entry points currently include:

```text
ayther_play_ce --install <pack.ay> [--id <game-id>]
ayther_play_ce --rollback <game-id>
ayther_play_ce --packs
```

Run the binary with `--help` from a valid workspace build for the authoritative command-line
contract. Interfaces may change before the first release.

## Local pack-handling contract

All local installation entry points must converge on the same implementation. A pack selected
from Settings, a missing-pack game detail, and the CLI must share validation and filesystem
semantics. That path must:

1. audit the complete staged archive before modifying installed state;
2. reject invalid archives without leaving partial installations;
3. preserve the previous installed version before replacement;
4. offer rollback only when a usable previous version exists; and
5. report local integrity without implying publisher trust.

Folder discovery is non-destructive. Removing a configured pack folder does not delete its
files; its packs simply stop appearing in the library. When both an explicitly installed pack
and a discovered pack match the same game, the installed copy takes precedence.

## Public certificates for local packs

If your Runtime requires signed packs, explicitly associate a pack with its author's
public trust registry in `play_config.toml`. Play passes that registry to Runtime only
when launching that pack; Runtime performs the signature verification. No private
signing key is needed or read by Play.

```toml
[pack_trust_registries]
'C:\Packs\example.ay' = 'C:\Certificates\example-trust.toml'
```

Use the installed pack's full path. Other packs and launches without a pack keep their
existing behavior. A startup rejection is explained in the dialog; the redacted
diagnostic preserves the Runtime reason and error details.

## Startup responsiveness

Play starts maximized on Home. Cover images load on a background worker, with
progress visible across tabs. Home artwork is queued first. Thumbnails (up to
1024 pixels per side) are cached under the Play configuration directory at
`cache/covers`; replacing an image invalidates its thumbnail using file size and
modification time. Removing this generated cache is safe; originals are preserved.
Library scanning reports its current stage and coalesces repeated scan requests.

To measure local startup work without launching games or modifying the library:

```sh
cargo test --bin ayther_play_ce profile_local_startup -- --ignored --nocapture
```

This opt-in measurement reads the configured library, compares source image loading
with cold and cached thumbnails in a temporary directory, and times pack indexing
and compatibility checks. Normal logs also record cover preparation and scan times.

On Windows, `./tools/verify-window-startup.ps1` opens the compiled application,
checks its native maximized state and monitor work-area coverage during loading,
then verifies Restore and closes only that test instance. The default executable
is `target/release/ayther_play_ce.exe`; pass `-Executable` to test another build.
Results and startup logs are saved in `artifacts/window-startup`.

## Repository layout

| Path | Purpose |
| --- | --- |
| `src/` | Rust application host and domain modules |
| `ui/` | Slint UI, design tokens, and icon mappings |
| `tests/` | Cross-module and source-contract integration tests |
| `docs/design/play-ce.pen` | JSON-based Community Edition UI design source |
| `lang/` | Translation template and bundled English catalog |
| `resources/` | Application artwork and Windows resource metadata |
| `fonts/` | Vendored text and icon fonts with provenance notes |

## Known limitations

- No versioned release, installer, package repository, or binary artifact is published.
- Source-contract tests require the AYTHER Runtime source checkout described above.
- CI, supported platforms, minimum Rust version, release signing, and long-term support are not
  yet defined.
- Generated Slint token and icon files are present, but their generator workflow is not included
  in this snapshot.
- Compatibility and persistence formats may change before the first stable release. Existing
  serialized field names are preserved through explicit aliases where identifiers have changed.

## Contributing and security

Read [CONTRIBUTING.md](CONTRIBUTING.md) before proposing a change. It documents the project
boundaries, Rust/Slint naming rules, validation expectations, and contribution checklist.

Report security-sensitive findings privately according to [SECURITY.md](SECURITY.md). Do not
include vulnerability details, personal data, copyrighted game content, or destructive payloads
in a public report.

## License and legal boundaries

Copyright © 2026 David Lazarte. Individual files may identify additional copyright holders.

Except for identified third-party material and files that state otherwise, original source and
documentation are available under the [Mozilla Public License 2.0](LICENSE). MPL-2.0 is a
file-level copyleft license; the license text controls in case of conflict with this summary.

- MPL-2.0 does not grant trademark, service-mark, logo, endorsement, or branding rights.
- Vendored fonts and icons retain their respective third-party licenses. See
  [fonts/README.md](fonts/README.md).
- The proprietary AYTHER Play product, its source, and its managed services are not licensed by
  this repository.
- Games, ROMs, firmware, BIOS files, cores, and third-party `.ay` packs are not distributed here.
  Users and distributors are responsible for obtaining all necessary rights.
- The software is provided without warranty under the terms of MPL-2.0.
