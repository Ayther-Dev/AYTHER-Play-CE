# Building and validating Play CE

Initial validation target: Windows x86-64 MSVC. Install Git, Rustup, PowerShell 7
and Visual Studio 2022 Build Tools with Desktop development with C++ and a Windows
SDK (linker and resource compiler). Release integration also uses GitHub CLI (`gh`)
with repository read access; GitHub Actions supplies its standard `GH_TOKEN`.
`rust-toolchain.toml` selects Rust 1.95.0,
rustfmt and Clippy. Allow Cargo network access on first use for locked crates and
Engine `v0.1.0-rc.6`; no sibling Engine checkout is required. UI, translations and
fonts are embedded by the build. Other platforms are not release-validated here.

From a fresh Play clone in PowerShell:

```powershell
git clone https://github.com/Ayther-Dev/AYTHER-Play-CE.git
cd AYTHER-Play-CE
./tools/prepare-runtime.ps1
cargo fmt --all -- --check
cargo check -p ayther_play_ce --all-targets --all-features --locked
cargo clippy -p ayther_play_ce --all-targets --all-features --locked -- -D warnings
cargo test -p ayther_play_ce --all-features --locked
```

The preparation script sets `AYTHER_RUNTIME_SOURCE` in the invoking PowerShell
process to a clean, detached, pinned checkout under `validation-deps`. For normal
development an explicit source override or the sibling checkout still works, but
only the prepared source is used by CI. Ignored real-process tests are not passed
integration checks.

Release validation additionally requires:

```powershell
./tools/prepare-runtime.ps1 -Integration
cargo test -p ayther_play_ce --all-features --locked --bin ayther_play_ce -- --ignored
```

The release workflow runs on version tags or manual dispatch. Beta.2's published
Windows AMD64 artifact, source commit, URL, SHA-256 and executable path are fixed
in `tools/runtime-lock.json`. For a future upgrade, review and update all fields;
pending or incomplete metadata must fail release validation. Do not substitute a
locally built Runtime or mark a missing dependency as passed. The script checks the archive hash
and builds the original MPL-2.0 `tools/probe_fixture.rs` for the real metadata
probe. That fixture is freely redistributable but does not emulate a game.

The automated release checks cover the actual probe and invalid maps through
Play's process launcher. They do not close the manual game checks below:

1. Supply a core and game whose use is authorized; record their versions and
   SHA-256 without committing personal game data.
2. Start Play against the distributed beta.2 Runtime, restore default controls,
   launch a game, verify keyboard and controller input and a clean exit.
3. Swap keyboard and controller assignments in Play, put the generated map in a
   path containing spaces, and verify each changed action inside the game. Save
   the TOML and evidence in the validation record.
4. Verify missing, malformed and contract-invalid maps show the useful failure
   message in Play, never a successfully started session.

After a clean committed build, `./tools/package-play.ps1 -Version 0.1.0` builds
release mode and produces `dist/ayther-play-ce-0.1.0-windows-x86_64.zip`, a SHA-256
sidecar and embedded build metadata. The version must match Cargo. The package
contains Play, README, MPL license and font notices/assets. Check that all font
license notices are present before distributing; font provenance alone is not a
substitute for license text. Runtime, its DLLs, core, ROM, BIOS and packs remain
external. Install the applicable MSVC runtime if the target machine requires it.

Extract into a directory outside the development tree on a clean Windows host.
Open Play, complete setup, configure the external Runtime/core and perform the
game checks above. Check DLL loading and font rendering; the packaging script
does not prove startup or close those checks automatically. Record the archive
hash, commit, toolchain, OS/GPU and results in the beta validation record. A package
is not approved for release until those checks and license review are complete.
