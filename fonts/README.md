# Fonts used by AYTHER Play CE

These fonts are intentionally vendored so the interface can render consistently across
machines. Resolving a font only by its system family name would make appearance and metrics
depend on what happens to be installed on each device.

The three text families come from the product design system in `docs/design/play-ce.pen` through
the `--font-display`, `--font`, and `--font-mono` tokens. The icon family is defined per icon
in the design data, for example `{"icon": "check-circle", "library": "phosphor"}`. The planned
asset generators are not present in this early repository snapshot.

| File | Family | Intended role | License |
| --- | --- | --- | --- |
| `Khand-SemiBold.ttf` | Khand 600 | Display headings, calls to action, and product lockup | SIL OFL 1.1 |
| `Khand-Medium.ttf` | Khand 500 | Secondary display text, tabs, and section labels | SIL OFL 1.1 |
| `IBMPlexSans-Regular.ttf` | IBM Plex Sans 400 | Default interface and body text | SIL OFL 1.1 |
| `IBMPlexSans-Medium.ttf` | IBM Plex Sans 500 | Button labels and focused names | SIL OFL 1.1 |
| `IBMPlexSans-SemiBold.ttf` | IBM Plex Sans 600 | Row and card headings | SIL OFL 1.1 |
| `IBMPlexMono-Regular.ttf` | IBM Plex Mono 400 | Hashes, paths, versions, checksums, and clocks | SIL OFL 1.1 |
| `Phosphor.ttf` | Phosphor 2.1.2 Regular | Interface iconography | MIT |

The display system renders Khand 600 in uppercase with `0.03em` tracking.

Khand has no true italic in the vendored family, which provides upright weights 300–700.
The design calls for oblique display text; the planned Slint implementation uses synthetic
oblique rendering through `font-italic: true`. The design file does not render that distinction
and records it as a typography note.

## Phosphor icon font

`Phosphor.ttf` is intended to replace approximate Unicode symbols with the exact icons named
by the design. A visually similar glyph is not an acceptable substitute: it can communicate a
different action or silently render as a missing-glyph box.

The vendored font is the official file from `@phosphor-icons/web@2.1.2`, path
`src/regular/Phosphor.ttf`, with this SHA-256 digest:

```text
06b91e022b7ee899a63efced879392a74f0bacbda54e4467e9f663220d173a10
```

`Phosphor.codepoints` is the `name -> codepoint` table derived from `style.css` in the same
release. Its purpose is to make icon-name validation possible without network access. The
planned generator will use the table to produce the Slint icon catalog and will verify the
design name, release codepoint, and font character map together.

Only the Regular weight is vendored. Phosphor packages each weight as a separate font family;
heavier visual emphasis should be achieved through size and color unless another licensed font
file is intentionally added and documented.

## Licenses and provenance

- **Khand** — Indian Type Foundry, distributed through Google Fonts (`ofl/khand`), under the
  SIL Open Font License 1.1.
- **IBM Plex Sans and IBM Plex Mono** — IBM Corp., distributed through the IBM Plex project,
  under the SIL Open Font License 1.1.
- **Phosphor** — Phosphor Icons, release 2.1.2, under the MIT License.

These third-party assets are not relicensed by the repository's MPL-2.0 license. Distributors
must preserve the applicable font and icon license notices. A generated repository-wide
third-party notice is planned but its generator is not present in this snapshot.

The font license texts are included under `licenses/`: Khand and IBM Plex use OFL;
Phosphor uses MIT. Retrieved on 2026-09-05 from the Google Fonts `ofl/khand/OFL.txt`
and `ofl/ibmplexsans/OFL.txt` sources, and Phosphor web tag `v2.1.2/LICENSE`.

## Planned loading model

The planned Slint token module imports the font files by repository-relative path and exposes
the display, UI, monospaced, and icon families as design tokens. The application window sets
the UI family as its default. No absolute path or system-installed font is required.

The icon font should be referenced from one shared icon component only; all callers should use
generated icon names instead of embedding remembered Unicode codepoints.
