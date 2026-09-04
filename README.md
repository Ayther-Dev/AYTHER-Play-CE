<p align="center">
  <img src="resources/AYTHER-Play.png" alt="AYTHER Play CE logo" width="160">
</p>

<h1 align="center">AYTHER Play CE</h1>

<p align="center">
  An offline-first, community-edition game launcher designed for local libraries,
  controller-friendly navigation, local saves, and locally supplied enhancement packs.
</p>

> [!WARNING]
> **Early-stage repository.** As of September 4, 2026, this repository contains design
> material, branding, vendored fonts, and Windows resource metadata only. It does not yet
> contain the Rust/Slint application, a Cargo workspace, the referenced generators, a test
> suite, or release artifacts. The project cannot currently be built or run from this
> repository. Behaviors and commands described as *planned* are design contracts, not
> shipped functionality.

## Overview

AYTHER Play CE is planned as the open-source player and launcher in the AYTHER ecosystem.
It is intentionally independent of AYTHER-operated services: users provide their own games
and optional `.ay` packs from local storage, and the application operates without an
account, cloud storage, telemetry, or a service-managed updater.

CE is not the proprietary AYTHER Play application with features disabled. The two editions
have different product boundaries:

| Area | AYTHER Play CE | AYTHER Play |
| --- | --- | --- |
| Distribution model | Open-source code under MPL-2.0 | Separate proprietary product |
| Primary navigation | Home, Library, Settings | Home, Library, Hub, Downloads, Profile, Settings |
| Pack acquisition | Local file or designated folder | AYTHER Hub catalog and downloads |
| Pack assurance | Local integrity checks only | Service-backed publisher trust may be available |
| Online services | None required or planned in CE | Account, cloud saves, telemetry, and updates |
| Backup model | Explicit local export | May include service-backed synchronization |

The proprietary AYTHER Play code and service integrations are not part of this repository
and are not licensed by this project.

## Trust model for packs

AYTHER Play CE deliberately distinguishes **integrity** from **authenticity**:

- Integrity answers whether a local pack is complete and unchanged according to its own
  `integrity.toml` manifest. The planned installer verifies the complete archive before
  changing local state or presenting the pack as installed.
- Authenticity answers whether a pack was created by the publisher it claims. That requires
  an independent trust source. CE has no service-backed key registry and must not claim to
  authenticate a pack.

A pack that passes integrity validation can still be untrusted or malicious. CE must not
display a signature, publisher-trust, or lock indicator when it has only validated hashes.
Users remain responsible for obtaining packs from sources they trust.

## Planned product behavior

The current design intent includes:

- a ten-foot interface usable without a mouse;
- local library discovery, compatibility information, game launching, and local saves;
- installation of `.ay` packs selected from disk;
- discovery of packs in one or more user-designated folders, without copying or deleting
  the source files;
- precedence for explicitly installed packs over matching packs discovered in a folder;
- preservation of the previous installed version when a real rollback target exists; and
- a visible local export workflow, because CE has no cloud backup.

The planned command-line contract is shown below for design and review purposes. These
commands are **not available in the current repository snapshot**.

```text
ayther_play_ce --install <pack.ay> [--id <game-id>]
ayther_play_ce --rollback <game-id>
ayther_play_ce --packs
```

### Local pack-handling contract

The implementation should keep every local installation entry point on one code path. A pack
selected from Settings, a missing-pack game detail, or the CLI should reach the same service
(historically named `pack_install::install_local()`), rather than duplicate validation and
filesystem behavior.

That service is expected to:

1. audit the complete archive before modifying installed state;
2. reject an invalid archive without leaving a partial installation;
3. preserve the previous installed version before replacement;
4. offer rollback only when a usable previous version actually exists; and
5. report file integrity without implying publisher trust.

Folder discovery is intentionally non-destructive. The `pack_dirs` TOML setting is a list even
if the initial settings UI adds one folder at a time. Removing a folder from configuration must
not delete its `.ay` files; they simply stop appearing in the library. When the same pack is
both installed and discovered, the explicitly installed copy takes precedence.

### Expected architectural guardrails

Once implementation begins, automated checks should enforce the edition boundary at more than
one layer:

- CE dependencies, modules, and symbols must not include a Hub client or another
  AYTHER-service integration.
- The complete localization catalog should be checked for user-visible references to Hub-only
  features. Source comments may explain the boundary and should not be rejected by that check.
- Removing the three proprietary-only navigation sections must leave focus traversal
  contiguous in both the Slint view and the controller router.
- Generated icons should validate the design name, vendored release codepoint, and TTF character
  map together so an invalid mapping cannot silently render the wrong glyph.

## Repository layout

| Path | Purpose | Current state |
| --- | --- | --- |
| `docs/design/play.pen` | JSON-based UI design source | Still reflects the six-section proprietary Play flow |
| `fonts/` | Vendored text and icon fonts | Present; licensing and provenance are documented locally |
| `resources/` | Application icon, cover art, and Win32 resource metadata | Present |
| Application source | Rust and Slint implementation | Not yet present |
| Build and test configuration | Cargo manifests, CI, packaging, and test harnesses | Not yet present |

## Known gaps

The repository is not release-ready. The following work is required before implementation
claims, build instructions, or compatibility guarantees can be published:

1. Add the standalone Rust/Slint application source and Cargo manifests.
2. Update `docs/design/play.pen` for CE: retain only Home, Library, and Settings; remove Hub,
   Downloads, Profile, sign-in, telemetry, and Hub-news flows; and add the local pack-folder
   and install-from-disk screens.
3. Add the exact third-party license texts required for redistribution, then add and document
   the token, icon, and third-party-notice generators referenced by the existing design notes.
4. Add automated checks for pack integrity, controller focus routing, forbidden Hub
   dependencies and UI strings, generated assets, and platform-specific resources.
5. Define supported platforms, reproducible packaging, CI, release signing, and a release
   support policy.

The historical monorepo command `cargo test -p ayther_play_ce` cannot be verified or run in
this standalone snapshot. Once a Cargo workspace exists, this README will provide exact,
tested setup, build, test, and packaging commands.

## Contributing

Contributions are welcome while the project is taking shape, especially documentation,
design review, test planning, portability analysis, and narrowly scoped implementation work.
Read [CONTRIBUTING.md](CONTRIBUTING.md) before opening an issue or pull request.

For security-sensitive reports, follow [SECURITY.md](SECURITY.md) and do not disclose the
details in a public issue.

## License and legal boundaries

Copyright © 2026 David Lazarte. Individual files may identify additional copyright holders.

Except for identified third-party material and files that state otherwise, original source
code and documentation in this repository are made available under the
[Mozilla Public License 2.0](LICENSE). MPL-2.0 is a file-level copyleft license; consult the
license text for the controlling terms.

Additional boundaries apply:

- MPL-2.0 does not grant rights in contributor trademarks, service marks, or logos. The
  AYTHER names and visual identity are not automatically licensed for branding or
  endorsement by the source-code license.
- Vendored fonts and icon assets remain under their respective third-party licenses. See
  [fonts/README.md](fonts/README.md) for provenance and license details.
- AYTHER Play, the AYTHER Hub, and their proprietary source and service integrations are not
  included in or licensed through this repository.
- Games, ROMs, firmware, BIOS files, and third-party `.ay` packs are not distributed with
  this project. Users and distributors are responsible for ensuring they have the rights
  required for any content they use.

The software, once available, will be provided without warranty as described in MPL-2.0.
