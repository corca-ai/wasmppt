# Compatibility corpus and fidelity scorecard

`fixtures/corpus.json` is the single fixture registry. It records a stable ID, source or local
path, SHA-256, producer/provenance, license, redistribution policy, execution tier, feature tags,
expected diagnostics, executable scorecard declarations, and independent open/preserve/edit/render
outcomes. Each scored presentation declares its slide indices, feature regions, preservation parts,
and one text binding edit. A fixture may enter the
pull-request tier only when it is small and deterministic; larger or slower cases remain scheduled.

The repository contains 50 independently generated multilingual presentations. They cross common
preset geometry, theme/gradient fill, text decoration, RTL, vertical text, rotation, and spacing.
Pinned Apache POI files add independently produced POTX/PPTX cases. Controlled PowerPoint,
LibreOffice, and Keynote runners publish desktop-consumer evidence; Google Slides, python-pptx,
and additional producer exports can be added only with affirmative redistribution provenance.

Regenerate the generated tier and refresh its hashes with:

```sh
cargo run -p wasmppt-native --example write_compat_corpus -- fixtures/compat
node scripts/update-generated-corpus.mjs
node --test scripts/corpus.test.mjs
```

The fast scorecard executes the ten pull-request fixtures. The scheduled workflow executes all
local PPTX fixtures and publishes raw JSON. `open` validates the source package; `preserve` performs
an unrelated binding edit and compares every other raw compressed entry plus the declared unknown
XML, relationship, and opaque parts; `edit` performs the declared edit, validates the result,
reopens its declared slide, verifies the decoded value, and proves unrelated entries unchanged;
and `render` structurally resolves every declared slide and feature region. Expected diagnostic
codes are compared with the actual stable codes. A valid ZIP is never treated as visual equivalence.

Scorecard schema 2 retains the exact commands, tool versions, declared and actual fixture hashes,
stdout/stderr, exit codes, and per-stage failures. Structural resolve, Chromium pixel evidence, and
desktop-consumer evidence are distinct fields: the portable scorecard marks the latter two
`not-run` and links to their authoritative workflow artifacts. Artifact upload runs even when a
stage fails, so failure evidence is not lost.

```sh
cargo build -p wasmppt-cli
node scripts/corpus-scorecard.mjs --output=target/corpus-scorecard-pr.json
node scripts/corpus-scorecard.mjs --all --output=target/corpus-scorecard-all.json
```

To add a fixture, document its producer version, license and redistribution permission, pin its
hash, choose a tier, identify feature regions/tags, and declare expected diagnostics. Quarantine a
regressing external fixture by keeping its registry record and changing its expected outcome with
an issue link; do not silently remove evidence. Promote it after the raw scorecard and relevant
desktop-consumer artifact pass.

The browser shaping gate obtains `KaTeX_Main-Regular.ttf` from the pinned `katex` npm dependency
(MIT license) during `npm ci`; the font is not copied into the repository or published package.
It verifies exact font-byte glyph clusters, OpenType feature-sensitive cache identity, UAX #14
break offsets, and warm cache reuse. Presentation fixtures and derived PowerPoint images continue
to follow the registry redistribution policy above.

The live-editing performance corpus is generated separately from compatibility claims. Its public
contract in `benchmarks/fixtures.json` defines text, image, and mixed POTX cases at 10, 50, and 200
slides, including multilingual text and deterministic media payloads. The benchmark report records
each generated hash. Fixed dogfood and advanced-content fixtures cover table, chart/workbook, and
slide-topology deltas. See [performance contract](performance.md) for commands and budgets.

See [compatibility and validation](compatibility.md) for consumer gates and
[rendering](rendering.md) for WPDL structural/visual evidence.
