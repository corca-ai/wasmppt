# Performance contract and reproducible benchmarks

Status: release budgets implemented; no world-fastest claim is published yet

Performance is a versioned, correctness-gated contract. A result is eligible for comparison only
when its generated package opens as ZIP/OPC, has the requested slide count, resolves to WPDL, and
retains the raw-copy invariant. Cold template compilation and warm injection are always separate.

## Reproduce

Use the [development setup](develop.md), a matching `wasm-bindgen-cli`, Chromium, and .NET SDK
for the Open XML comparison gate. CI builds host artifacts with Rust 1.88.0; local reports record
the actual compiler and engine identity. Run from the repository root:

```sh
npm ci
npm run build:wasm-hosts
node benchmarks/run.mjs
npm ci --prefix benchmarks/comparisons/pptxgenjs --ignore-scripts
npm ci --prefix benchmarks/comparisons/pptx-browser --ignore-scripts
node benchmarks/prepare-browser-comparator.mjs
node benchmarks/comparisons/pptxgenjs/run.mjs 10 10 target/benchmarks/pptxgenjs-text-10.pptx
npm run build --workspace @corca-ai/wasmppt
node benchmarks/comparisons/run-equivalent.mjs --iterations=3
npm run test:browser --workspace @corca-ai/wasmppt
npm test --workspace @corca-ai/wasmppt-worker
```

[benchmarks/run.mjs](../benchmarks/run.mjs) creates the workload matrix from
[fixtures.json](../benchmarks/fixtures.json) in `target/benchmark-fixtures`. Set
`WASMPPT_BENCH_ITERATIONS` and `WASMPPT_BENCH_PROCESS_RUNS` for local calibration; the runner owns
defaults and `--ci` fixes the minimum sampling policy and mixed-workload subset. The separate
[deck gate](deck-gates.md#portable-evidence) needs native, browser, and captured workerd evidence.
Generated templates are source artifacts: their generator,
payload dimensions, compression mode, hashes, and redistribution license are recorded in the raw
report rather than hidden behind an unpublished corpus.

## Measurements

The native report contains every nanosecond sample plus p50/p95 and throughput for:

- `coldTemplateCompile`: ZIP index, package graph, binding plan, and immutable prepared cache;
- `warmInjection`: generation from one already-prepared template;
- `firstSlide`: presentation open plus resolution and WPDL encoding of slide zero;
- `visibleSlides`: presentation open plus the first three slides (or fewer);
- `allSlides`: presentation open plus every slide.

Each native matrix entry also retains live samples for `applyDelta`, dependency invalidation,
invalidated-slide resolution, `inputToRenderReady`, and `backgroundExport`. It records unchanged
media copy count, shared overlay parts, maximum invalidated slides, final output bytes, and peak
session residency. Chromium runs the mixed 10/50/200 matrix through the real module Worker and
records apply, resolve, Canvas render, input-to-pixels, current-revision export, and cache telemetry.
The native operation report separately exercises repeated table rows, chart plus embedded workbook,
and slide-topology changes against the dogfood and advanced-content fixtures.

It also records input/output bytes, conservative prepared-plan resident bytes, OS-process peak RSS,
input/output copy counts, raw-copied bytes and entries, inflated and recompressed entries, scalar
Wasm binary size, total generation-dirty bytes, largest simultaneously dirty entry, maximum pull
chunk, revision and source dirty state, separately listed regenerated tracked build artifacts,
fixture hashes, CPU/RAM/OS/runtime, process-run and per-process iteration counts,
release profile, and compression configuration. Browser and workerd reports retain their own raw
warm samples because host scheduling cannot honestly be folded into a native headline.

Report schema 3 separates `memory.logical` from `memory.process`. Logical residency records the
prepared template estimate, total generation-dirty bytes, largest dirty entry, live-session peak,
and completed output size. These phase-specific counters describe bytes owned by wasmppt; they are
not summed into a synthetic process-memory number. Process RSS is sampled once per fresh benchmark
child with `/usr/bin/time` and covers compile, generation, resolution, and live phases together.
Its maximum is an allocator/process high-water mark: repeated iterations may leave freed arenas in
the allocator, so it can exceed current logical residency without proving a leak. The raw report
retains every process RSS sample and labels this scope and allocator effect explicitly.

The primary scalar Wasm size excludes the optional metafile converter and exact-font shaper.
EMF/WMF presentations load the separately reported converter artifact on first use. Applications
load the HarfRust shaper only when exact font bytes are available and explicitly configured.
Presentations using neither capability do not fetch or instantiate either module. All artifact
sizes remain visible so optional capability cost is not hidden.

## Profiling template preparation

`node benchmarks/profile-prepare.mjs TEMPLATE REPORT [ENGINE_DIRECTORY]` measures scalar-Wasm
`prepare` in 200 fresh engines after 20 warmup calls. Module initialization and handle disposal
are outside those latency samples. A separate 200-call inspector pass includes disposal and
writes the raw V8 CPU profile to `REPORT.cpuprofile`; the JSON report retains every latency,
engine/template hash, runtime, CPU, revision, and aggregated self/inclusive function samples.
The optional engine directory allows a saved baseline or a Wasm build with function names to be
profiled without replacing release artifacts.

For named diagnostic profiles, build with `CARGO_PROFILE_WASM_RELEASE_STRIP=false` and run the
matching `wasm-bindgen` CLI into an isolated directory under `target`. Do not publish that
instrumented artifact as the size-budgeted release engine. Use the ordinary release artifact for
before/after latency and correctness comparisons. The revision-bound
[namespace-sharing measurement](../benchmarks/results/prepare-namespace-sharing.json) records
one preparation optimization and its generated POTX snapshot; it is historical evidence, not a
current performance baseline.
Reproduce it with `node benchmarks/profile-prepare.mjs benchmarks/results/prepare-namespace-sharing.potx target/benchmarks/prepare-current.json`.
This is bounded evidence for this preparation workload,
not a general speed claim or an Office fidelity result.

## Release budgets

`benchmarks/budgets.json` is the only budget source. CI runs the actual native release binary,
Chromium module Worker with scalar Wasm, and Cloudflare workerd. It fails on p95 ceilings, native
peak RSS, scalar Wasm size, accounted Worker memory, loss of raw copies, any generation-time ZIP
inflation, or correctness failure. Absolute budgets are intentionally broad enough for shared CI;
tightening them is reviewed like an API change. Native cold compile, warm generation,
first/visible/all-slide latency, logical memory, and RSS ceilings are enforced independently at 10,
50, and 200 slides. Adjacent sizes and the 10-to-200 span also enforce normalized growth
`(large metric / small metric) / (large slides / small slides)` so an absolute ceiling cannot hide
superlinear scaling. The checked-in ceilings include variance observed across repeated release
processes; CI refuses reports with fewer than three processes or ten timing samples per process.

The `wasm-release` profile uses size-oriented optimization for the bounded semantic
layout crate while retaining speed-oriented optimization for the remaining scalar
runtime. The same browser and workerd deck latency gates qualify this tradeoff;
size optimization does not relax any latency or correctness ceiling.

The scalar browser artifact has its own raw-Wasm ceiling in
[budgets.json](../benchmarks/budgets.json). Optional artifact costs are reported separately;
changes to the scalar feature boundary must still pass that ceiling.

The [deck compatibility gate](deck-gates.md) also enforces browser and workerd ceilings for
Starter compilation plus initial planning, all-page resolution, and exact-revision PPTX export.
Its report retains raw timing samples and cold/warm p50/p95 summaries for native, browser,
and workerd next to exact cross-host plan, display-list, topology, and package identities, so
latency evidence cannot outlive the correctness result it measured.

Every enforced check publishes its absolute and percentage margin in `budgetEvaluation`, including
passing checks, and the raw artifact is uploaded even when the gate fails. Published artifacts contain the raw JSON and exact
generated budget fixture for each revision. Browser reports additionally retain first-visible-slide
samples, resolution/font/display/media stage timings, and scene/resource/decoded-image cache bytes.
The visible set is awaited before neighbor prefetch can consume Worker or main-thread capacity.
The browser gate also executes a bounded rapid-scroll trace, enforcing the configured
strong-reference window, byte-budgeted cache residency, disposal, and average scheduling/render
budget. It exercises the OffscreenCanvas thumbnail path and closes the transferred ImageBitmap.
The same gate fails when a text edit invalidates more than one independent slide, loses overlay
sharing, exceeds live input-to-pixels or export latency, or exceeds bounded cache residency.

## Comparisons and claims

Competitor results belong under `benchmarks/comparisons/` and must name the exact package version,
runtime/browser, API settings, workload adapter, output validation, and known semantic differences.
PptxGenJS 4.0.1 is the initial named generation comparator; it authors a new deck and does not
perform POTX/POTM template injection, so its number must not be presented as an equivalent warm
injection result. Its dependencies are isolated from the product workspaces and the adapter uses
only generated text; its [isolated lockfile](../benchmarks/comparisons/pptxgenjs/package-lock.json) owns dependency
versions. Keep the workload generated and review dependency policy when updating it. Browser
renderers likewise require the same input deck, viewport, font/image
resources, visible-slide set, and pixel/semantic correctness thresholds.

The initial Canvas comparison pins `pptx-browser` 4.1.4. Version 4.1.5 was checked first but its
published npm tarball omits required modules including `src/zip.js` and `src/render.js`, so it is
reported as excluded rather than silently replaced or assigned a fabricated timing.
Version 4.1.4 loads the pinned deck but catches internal failures for the required text shapes.
Its raw timings remain visible with `eligible: false`; they cannot
beat a renderer that produced the required pixels.

### Equivalent editable-text output

`benchmarks/comparisons/text-workload.mjs` defines `generated-text-10-v1`: ten wide slides with
eight text boxes each, fixed positions, Arial 10-point black text, white backgrounds, and the same
generated Korean, Arabic, emoji, and XML-sensitive text. This contract is separate from the native
budget corpus. PptxGenJS authors a token-bound POTX once outside measurement; wasmppt fills that
template with the requested text. The comparator authors the same requested deck from scratch.

Run `node benchmarks/comparisons/run-equivalent.mjs --iterations=3` for a smoke comparison after
installing the isolated PptxGenJS dependencies and building the browser package. The additional
prerequisite is .NET SDK 8 for the pinned Microsoft Open XML validator. Set `WASMPPT_DOTNET` to an
explicit executable path when it is not on `PATH`. Local defaults are 30 iterations in one fresh
Node process; `--process-runs=N` adds independent processes. `--ci` fixes three fresh processes
with ten iterations each and cannot be weakened by local sample-count flags.

The report retains four phases under one schema: unprepared scalar-Wasm template generation
including preparation and handle cleanup (`cold`), prepared template reuse (`warm`), PptxGenJS
authoring plus the declared schema-order correction (`author`), and unmodified PptxGenJS authoring
(`raw-author`). Each measured sample includes ZIP serialization and complete output-buffer
assembly. Module/Wasm initialization and one-time warm preparation are recorded separately.
Common text input construction, template construction, disk I/O, validation, and rendering are
outside the timed interval. Both libraries consume the same preconstructed input values.
Libraries run under the same Node version, and sample execution order alternates each iteration.

PptxGenJS 4.0.1 places `notesMasterIdLst` after `sldIdLst`, which the Microsoft validator rejects.
The committed adapter moves that one list before `sldIdLst`; its ZIP loading and DEFLATE
reserialization costs are included in `author`. The same correction is applied to the off-clock
template. This is explicitly a PptxGenJS-plus-adapter pipeline, not an unmodified-library speed
result. Raw outputs, their timings, and the validator errors remain visible and ineligible.

Every timed PPTX is retained and validated with the pinned Microsoft `DocumentFormat.OpenXml` validator. The
scalar engine independently opens it, checks the exact slide count, resolves all ten slides,
and checks ordered painted text, text-box geometry, font family/size/color, page dimensions,
white background, and absence of resolver diagnostics. Empty trailing style runs are not painted
text and do not affect typography eligibility. A failed check, invalid timing, missing process,
or missing/duplicate iteration excludes that participant from comparison; successful structural
resolution never substitutes for the full Open XML gate.

`target/benchmarks/equivalent.json` retains versions, CPU/RAM/OS/runtime, source revision and dirty
state, contract/template/engine/output hashes, every timing, nearest-rank p50/p95, process startup
and preparation samples, validation commands/stdout/stderr, per-slide checks, and eligibility.
The report is written on failure. CI uploads it alongside the template and all timed output files
under `target/benchmarks/equivalent`. It publishes ratios only between eligible unprepared-template
generation and corrected new-deck authoring. Warm reuse has no equivalent comparator operation,
so its timings remain context with no ratio. Output equivalence here covers declared editable
text and geometry; it does not prove pixel fidelity or make the operations semantically identical.
Existing non-equivalent numbers remain available with explicit exclusion from comparative claims.

No “world's fastest” or unqualified “fastest” claim is permitted until a committed raw comparison
for a bounded workload beats named current versions on declared hardware and passes all correctness
checks. A future claim must link that raw file and repeat its workload boundary in the same sentence.

## Browser metadata microbenchmarks

After `npm run build`, run `node benchmarks/browser-metadata.mjs` to record raw samples,
median and p95 for an 8 MiB JPEG header scan and mixed Latin, Korean and emoji source-offset
indexing. Compare revisions on the same runtime and machine. These workloads isolate metadata
allocation costs; they do not measure complete slide rendering or set a release latency budget.

## Academic layout evidence

The [academic comparison provenance](../benchmarks/results/academic-layout/provenance.json) pins
pre-change engine/template revisions, exact input hashes and report scope. Its compressed raw
reports use the same semantic corpus and image bytes with saved original templates; a separate
matrix uses updated purpose profiles. The [same-input summary](../benchmarks/results/academic-layout/same-input-summary.json)
and [purpose-profile summary](../benchmarks/results/academic-layout/purpose-template-summary.json)
retain findings alongside the measurements. The [runner](../scripts/academic-layout-evidence.mjs)
records each public SDK replacement, role font ranges, physical pages, image frames, body area,
whitespace, retained admission bytes and sampled process RSS/heap. The
[comparison script](../scripts/compare-academic-layout-evidence.mjs) rejects differing semantic
corpora, assets or case inventories before calculating summaries.

These per-case measurements exclude engine startup, template preparation and page resolution;
they are bounded layout evidence rather than a release latency benchmark. Sampled RSS is not an
OS high-water measurement, and retained admission bytes are not total process memory. Native
cross-host corpus timings include independently licensed research figures after expansion and
must not be described as a same-workload speed comparison. The existing release latency, size,
memory and candidate-work budgets remain authoritative and unchanged.

The saved original templates distinguish media-only bleed from mixed-content safe bounds.
Media-only pictures use the full bleed allocation with zero text insets; an extreme portrait that
meets the unchanged media floor there must remain eligible. Explicit-caption portrait pages can still fail where their authored shared-page safe bounds
cannot satisfy that floor. Ordinary list content and weakly related prose may continue completely
after a media-only bleed page; explicit image captions retain shared-page cohesion. Both recovered
cases and remaining caption failures stay in the before/after reports; updated envelopes resolve
the remaining cases without lowering the floor. The new profile matrix
must pass source presence, non-overlap, zero cropping, canonical aspect and caption-association
checks. Shorter decks or less whitespace alone do not meet that contract.

The native academic work-counter companion runs with
`cargo run --release --locked -p wasmppt-wasm --example academic_planner_metrics -- fixtures/deck-gates target/academic-layout/planner-metrics.json`.
It records actual candidate assignments, cache-miss measurements, unchanged limits, source/template/plan
hashes and seven raw plan timings for each purpose on new Starter and fixed custom profiles.
Its scope excludes template compilation, composition, rendering and export. Process RSS must be
reported separately; these counters are not an estimate of peak resident memory.

The [native planner companion](../benchmarks/results/academic-layout/planner-metrics.json)
records actual bounded candidate assignments and cache-miss measurements for all three purposes
on opted-in Starter and fixed custom profiles. Its [OS memory report](../benchmarks/results/academic-layout/planner-metrics-memory.txt)
records the compiled release process maximum resident set across those repeated passes. The
[provenance](../benchmarks/results/academic-layout/provenance.json) retains the exact command,
compiler and binary identity. These native work counters complement the SDK matrix; they are not
fabricated per-case SDK counters and are not combined with SDK admission bytes.

The [same-input native counter report](../benchmarks/results/academic-layout/native-same-input-work.json)
compares actual work on the original native workload. Baseline counter instrumentation stays in
an isolated pinned source archive; every instrumented plan must match the frozen uninstrumented
plan bytes. The new typed defaults use a reversible wire-schema normalization with identical
content, assets, template, fonts and limits. Instrumented timings and memory are not performance
evidence. The report retains the counter patch, repeatability checks and replay instructions.

The short-title text probe separately records actual body top, reclaimed EMU and available body
area from resolved source geometry. Its first full-width prose block starts at the Stack lane top;
the authored safe bottom and width bound the remaining envelope. This observed area is distinct
from static preferred body area and painted content utilization. Other layouts report the actual
body envelope unavailable rather than inferring it from centered images or split lanes.

Separate `/usr/bin/time -l` runs of the complete Node matrices retain OS maximum resident set for
the frozen old engine with original templates, the final engine with those exact templates, and
the final engine with updated purpose profiles. Their raw memory reports and scope are linked from
the comparison provenance. This process high-water includes startup, assets, resolution, retained
report data and JSON output; it must not be relabelled as engine heap or combined with admission
bytes. The comparison script accepts optional before/after memory report paths after its three
required arguments to reproduce those separate memory fields in each summary.

The [expanded comparison](../benchmarks/results/academic-layout/expanded-summary.json) compares a
frozen preceding engine with the accepted engine on identical canonical-pair and text-density
inputs. Its [provenance](../benchmarks/results/academic-layout/expanded-provenance.json) retains
engine/input hashes, immutable baseline guards, native work counters, OS memory scope and gate
reports. Per-page resolved fonts and text distinguish upward recovery on sparse continuations
from a fixed maximum-font fit. A measured suffix that already fits the accepted title height
retains body geometry; only increased measured title height requires another bounded pagination
pass. Candidate and measurement limits remain unchanged.
