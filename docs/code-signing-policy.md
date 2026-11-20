# Code signing policy

Windows releases of AYTHER Play CE are code-signed so that Windows can show who
published them and that they were not modified after the build.

Free code signing provided by [SignPath.io](https://about.signpath.io),
certificate by [SignPath Foundation](https://signpath.org).

## What is signed

- `ayther_play_ce.exe`, built from this repository.
- The installer `ayther-play-ce-<version>-windows-x86_64-setup.exe`.

The installer also carries the AYTHER Runtime (`ayther_runtime.exe`), signed by
its own [release workflow](https://github.com/Ayther-Dev/AYTHER-Runtime/blob/main/docs/code-signing-policy.md),
and third-party libraries (SDL3, toml++, the Vulkan loader, zstd, the Visual
C++ runtime) as their upstream projects publish them. The uninstaller that Inno
Setup writes at install time is not signed.

Signing happens only in the GitHub Actions
[release workflow](../.github/workflows/release.yml), on a `v*` tag, from the
build of that tag: the staged `ayther_play_ce.exe` is signed before the
installer is compiled, then the installer is signed. Nothing built on a
developer machine is signed. The SignPath artifact configurations are kept in
[`.signpath/artifact-configurations`](../.signpath/artifact-configurations).

## Team roles

| Role | Members |
| --- | --- |
| Committers and reviewers | [Members of the Ayther-Dev organization](https://github.com/orgs/Ayther-Dev/people) with write access |
| Approvers | David Lazarte ([@davidlazarte](https://github.com/davidlazarte)) |

Every signing request is approved manually by an approver. All team members
use multi-factor authentication for GitHub and SignPath.

## Privacy policy

This program will not transfer any information to other networked systems
unless specifically requested by the user or the person installing or
operating it.

AYTHER Play CE manages a local library of games and content packs. It has no
telemetry, update check or network access of its own.

## Provider

SignPath Foundation is the signing provider while AYTHER Play CE is in beta.
Changing the provider replaces only the signing steps of the release workflow;
releases signed before the change keep their timestamped signatures, and
installers keep upgrading each other because the setup `AppId` never changes.
