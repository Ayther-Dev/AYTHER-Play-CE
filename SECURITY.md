# Security Policy

## Supported versions

AYTHER Play CE has not published a release and is not ready for production use. The `main`
branch is under active development and may receive security fixes on a best-effort basis, but
it has no compatibility, response-time, or long-term-support guarantee.

| Version | Security support |
| --- | --- |
| `main` (pre-release) | Best effort only |
| Published releases | None available |

This table will be replaced with an explicit supported-version matrix before the first stable
release.

## Reporting a vulnerability

Do not disclose a suspected vulnerability in a public issue, discussion, pull request, or
social-media post.

1. Prefer GitHub's
   [private vulnerability reporting](https://github.com/Ayther-Dev/AYTHER-Play-CE/security/advisories/new)
   for this repository when the feature is available.
2. Include the affected revision or version, platform, prerequisites, reproduction steps,
   observed impact, and any suggested mitigation. Use minimal proof-of-concept data and remove
   personal information, secrets, copyrighted game content, and destructive payloads.
3. If private reporting is unavailable, open a public issue containing only a request for a
   private maintainer contact. Do not include vulnerability details in that issue.

Maintainers will respond and remediate on a best-effort basis. Until a staffed security process
and supported release line exist, the project cannot promise acknowledgement or resolution
deadlines. Please allow maintainers a reasonable opportunity to investigate and coordinate a
fix before public disclosure.

## Security scope

Reports are particularly useful when they demonstrate an impact such as:

- arbitrary code execution or unsafe command construction;
- path traversal, unsafe archive extraction, or writes outside an intended data directory;
- bypass of pack integrity validation or acceptance of a malformed `integrity.toml`;
- unintended deletion, corruption, or disclosure of local games, saves, settings, or packs;
- loading executable content from an untrusted location without an explicit user action;
- exposure of credentials, signing material, or sensitive local paths;
- a dependency or build-pipeline compromise with a reproducible path to affected users; or
- a UI claim that materially misrepresents an untrusted pack as authenticated.

The following are important trust boundaries rather than vulnerabilities by themselves:

- A successful local hash check establishes integrity against the pack's own manifest; it does
  **not** establish publisher identity, provenance, safety, or authenticity.
- CE does not provide an AYTHER-operated trust registry, account system, cloud saves,
  telemetry, or service-managed updates.
- Vulnerabilities in the separate proprietary AYTHER Play product or AYTHER-operated services
  are outside this repository's scope and should be reported through their designated channel.

Reports about roadmap-only features or hypothetical hardening gaps are usually design feedback
rather than exploitable vulnerabilities. Early threat-model contributions remain welcome, but
they must not be presented as vulnerabilities in released software because no release exists.

## Disclosure and safe testing

Test only systems and data you own or are authorized to assess. Do not access other users'
data, disrupt services, distribute copyrighted game content, or retain sensitive information.
Coordinated disclosure credit can be arranged when desired, but is not guaranteed.
