# Compatibility corpus and fidelity scorecard

`fixtures/corpus.json` is the single fixture registry. It records a stable ID, source or local
path, SHA-256, producer/provenance, license, redistribution policy, execution tier, feature tags,
expected diagnostics, and independent open/preserve/edit/render outcomes. A fixture may enter the
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
local PPTX fixtures and publishes raw JSON. Results keep open, preserve, edit, and render separate;
a valid ZIP is never treated as visual equivalence.

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

See [compatibility and validation](compatibility.md) for consumer gates and
[rendering](rendering.md) for WPDL structural/visual evidence.
