# Compatibility, security, and visual gates

Status: release gates implemented

Correctness evidence is part of the repository rather than an informal manual checklist. Pull
requests run the portable gates; a release additionally runs licensed desktop consumers on
controlled self-hosted machines.

## Corpus and provenance

`fixtures/corpus.json` records a stable ID, SHA-256, provenance, SPDX license, and redistribution
policy for every committed or downloaded deck. Generated fixtures include their exact generator
command. Third-party Apache POI fixtures are fetch-only and pinned by commit plus hash. Run
`node scripts/fetch-corpus.mjs target/corpus` to fetch and verify them from the manifest. A Node test
fails when a committed byte changes without updating its provenance record, or when a fixture
omits any policy field.

The pinned real-template set currently includes POTX conversion, a general sample show, a
master/layout deck, a chart deck, and a LibreOffice-produced deck. The portable scorecard validates
each declared source, performs independent preservation and edit round trips, and resolves every
declared slide and feature region without silent fallback. The checked-in dogfood POTX is generated in-repository
and covers metadata and visible-token text, image replacement, repeated table rows, and slide-copy
control through one browser workflow.

The current generated render fixture covers text, raster image
relationships and crops, groups and transforms, mixed text runs, gradients, bounded custom paths,
shadows and line ends, tables, charts and an embedded
workbook, SmartArt and EMF detection, animation, transition, and 3D diagnostics. Synthetic POTM
tests cover VBA and macro Action removal; unknown extension parts and markup are checked for
verbatim survival after unrelated edits. The reviewed UTF-8 text corpus separately pins Korean/CJK,
Arabic and Hebrew RTL, emoji sequences, and a deliberately missing font so script selection and
documented fallback behavior cannot disappear silently.

## Portable compatibility and security

Every CI run performs these independent gates:

- bounded ZIP/XML/OPC parsing and stable `LimitExceeded` errors;
- all ZIP, XML, relationship-graph, geometry-resolution, and binding-compilation fuzz binaries
  compile against the current API;
- default conversion passes `audit-macro-free`, which rejects VBA/data/signature parts,
  prohibited content types and relationships, and macro Action references;
- a pinned real POTX is converted through a forward-only sink and validated with Microsoft's
  `DocumentFormat.OpenXml` validator;
- a pinned real PPTX and generated output resolve without silent fallback claims;
- native, browser module-Worker, and workerd execute the same generated deck and require the same
  WPDL structural signature, while a separate exact-payload gate requires byte-identical PPTX
  output from the native sink and both bounded pull-stream hosts;
- the machine-readable PresentationML capability matrix declares read, preserve, edit, and render
  behavior for every listed feature.

The corpus scorecard's `render` outcome proves structural resolution only. Chromium pixel scoring
and controlled PowerPoint, LibreOffice, and Keynote results remain separate fidelity fields and
artifacts; none is inferred from ZIP validation or structural success. Each scorecard stage retains
its commands, versions, hashes, stdout/stderr, and failure list, including on a failing CI run.

The fuzz targets live under `crates/wasmppt-opc/fuzz`. They cover arbitrary ZIP opening and
inflation, relationship graphs and extension markup, raw XML tokenization, lazy slide/geometry
resolution, and template binding compilation. Browser hosts inspect PNG/JPEG dimensions and EXIF
orientation before decode and enforce both compressed-byte and decoded-pixel limits.

## Visual reports

The real Chromium integration writes `target/visual-report/report.json` plus one actual PNG per
slide. Schema 2 also records scored regions for text, shapes, raster images, charts, metafiles,
AutoFit modes, Unicode wrapping, paragraph metrics, columns, and text effects;
each feature has an explicit metric, tolerance, actual value, and pass/fail state. Each slide records
its pixel fingerprint. Slide one has zero tolerance for stable sampled background, group-fill, and
cropped-image colors. Slide two requires a declared minimum amount of non-background output for its
table and chart commands. CI uploads this directory as a revision-addressed artifact and fails when
the report or screenshots are absent.

On the controlled PowerPoint runner, both Canvas PNGs and 640-by-360 PowerPoint exports are compared
with ImageMagick. The JSON report publishes different-pixel count, total pixels, ratio, the 5%
per-channel fuzz rule, and the current 35% whole-slide tolerance. A checked-in manifest pins the
fixture hash, export size, metric, redistribution statement, and baseline owner. PowerPoint emits
its version, platform, fixture hash, and font inventory; missing or stale provenance fails closed.
The controlled-runner self-test changes exactly one pixel at zero tolerance and requires the
comparison script to emit a failed JSON row and difference image. The Chromium gate likewise
mutates a structural line-count fact and proves that the feature score fails closed.
This deliberately generous
baseline reflects the project's explicit advanced-feature gaps; tightening it is a versioned
compatibility change, and exceeding it blocks a release.

## Desktop consumers

`.github/workflows/office-ground-truth.yml` runs for rendering-affecting pull requests, release
publication, or manual dispatch:

- PowerPoint opens the deck read-only with automation security forced to disable active content,
  exports slides and PDF, and must complete inside a 15-minute timeout without a repair/error modal;
- LibreOffice Impress and Keynote each open and export the same deck on labeled controlled runners;
- all outputs are retained as immutable workflow artifacts.

The runner labels are part of the contract: `PowerPoint` and `ImageMagick` on Windows,
`LibreOffice` on Linux, and `Keynote` on macOS. A release stays queued or fails when an explicitly
required licensed runner is unavailable; the workflow does not silently skip that consumer.

Baseline updates are reviewed changes: regenerate the fixture deterministically, update
`fixtures/render/powerpoint-baseline.json` with its SHA-256 and policy metadata, dispatch the Office
workflow, and attach the resulting actual/reference/difference images plus `provenance.json` to the
pull request. A maintainer other than the author reviews any tolerance increase. Ordinary developer
machines inspect the uploaded report but never synthesize a PowerPoint reference.

## Diagnostics and policy evolution

WPDL v2 through v10 transport resolver diagnostics unchanged to Canvas and DOM/SVG. New diagnostic variants
append stable numeric wire codes. Unknown future codes decode as `unknown`, so older frontends fail
honestly without corrupting the scene. Security-limit regressions and unknown-markup loss are test
failures, never benchmark tradeoffs.
