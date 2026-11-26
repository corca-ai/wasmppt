# Deck compatibility gate

Status: portable cross-host correctness and performance gate implemented; licensed desktop
evidence remains a controlled release gate

The deck gate makes the complete Starter POTX plus WDSF path a tested contract before a host
integrates it. It executes the same checked-in inputs through native Rust, the browser module
Worker, and Cloudflare `workerd`. Passing requires exact WDTP, WDPL, WPDL, page topology, and PPTX
bytes across all three hosts. A ZIP that merely opens, or three hosts that merely agree on a hash
without executing the planner, do not pass.

## Fixtures and regeneration

`fixtures/deck-gates/starter.potx` is a deterministic Cortex Theme Starter v3 with title,
statement, and one flexible content-envelope layout. The content layout opts in to academic purpose typography with generation metadata and supplies a
measured-title body frame and optional media-only bleed frame; the planner constructs every body topology inside
those bounds. Content body regions accept nested section headings as flow content while the leading
title/section still maps to the title placeholder.
`deck-spec.wdsf` covers every renderable `SemanticRole` from title through
statement, plus the typed table row, cell, and column contracts. It includes rich text and safe
links, Korean/CJK and RTL text, nested ordered/unordered lists, PNG, GIF first-frame conversion,
SVG, tables, charts, code, diagrams, display math, definitions, hidden slides, missing-font
diagnostics, and enough content for automatic continuation pages. Its media-quality matrix covers
4:1, 16:9, 1:1, 3:4, and 1:4 sources as image-only, caption, short-copy, long-prose, and 2/3/5/9
related pairs, with two JPEG EXIF display-axis cases. Unmodified independent research figures with
axes and legends add landscape and portrait multipanel cases and a long bilingual heading; their
[provenance](../fixtures/deck-gates/research/provenance.json) records exact upstream bytes and CC BY attribution. `atomic-overflow.wdsf` is a valid contract
that cannot fit at the readable floor.

The continuation table must retain comfortable type, one editable table per physical page, and
repeated header rows on continued pages. Exact input and topology expectations live in the
[corpus manifest](../fixtures/deck-gates/corpus.json) and executable gate. Intrinsic contain
placement keeps extreme portrait-only cases on the legal media bleed when they fit.

Regenerate and compare the fixtures with:

```sh
fixture_dir=$(mktemp -d)
cargo run --locked -p wasmppt-deck --example write_gate_fixtures -- "$fixture_dir"
cmp fixtures/deck-gates/starter.potx "$fixture_dir/starter.potx"
cmp fixtures/deck-gates/starter-conference.potx "$fixture_dir/starter-conference.potx"
cmp fixtures/deck-gates/starter-handout.potx "$fixture_dir/starter-handout.potx"
cmp fixtures/deck-gates/custom.potx "$fixture_dir/custom.potx"
cmp fixtures/deck-gates/deck-spec.wdsf "$fixture_dir/deck-spec.wdsf"
cmp fixtures/deck-gates/atomic-overflow.wdsf "$fixture_dir/atomic-overflow.wdsf"
cmp fixtures/deck-gates/corpus.json "$fixture_dir/corpus.json"
```

The generator validates the WDSF and asserts the complete supported-role set. Table row/cell/column
coverage is asserted separately because those are typed table members rather than standalone
semantic nodes. Existing `starter.potx` defaults to seminar; conference and handout variants retain
the same three master/layout contracts. `custom.potx` retains the preceding fixed geometry and
explicit styles without academic opt-in, so custom-template preservation is independently tested.

The aggregate [academic evidence runner](../scripts/academic-layout-evidence.mjs) uses the public
Node SDK on one shared semantic corpus: research question, method, results, empty in-progress list,
formula, table, long bilingual headings, five aspect ratios with caption/prose/list contexts, and
licensed research multipanels. It records role fonts, physical pages, exact image geometry, body
area, whitespace, elapsed time, retained admission bytes and sampled process memory. Whitespace
alone is not a passing condition. Source presence, overlap, caption association, crop and aspect
fidelity are checked separately. The public SDK does not expose candidate counts; the report marks
that per-case field unavailable rather than estimating it. The native work-counter
[companion](../benchmarks/results/academic-layout/planner-metrics.json) reports actual aggregate
candidates and measurements. Internal work limits remain unchanged. With `--academic=true`, the
runner also checks painted source-owned runs against purpose role ranges. The short-title
research-question probe uses its resolved first full-width prose source shape as the actual Stack
lane top, with the authored safe bottom and width bounding its available body envelope. It must
reclaim space above the preferred body top, begin below the measured title, and retain at least
72% of page area. Other pages report this metric unavailable because centered media or split lanes
do not reveal that boundary. Empty inherited end runs and preserved template furniture do not define painted body
typography; fixed `custom` profiles remain an explicit opt-out.

```sh
node scripts/academic-layout-evidence.mjs --names=starter,custom --academic=true --strict
```

Use `--engine=PATH`, `--themes=DIRECTORY`, `--names=paper,editorial,dark,warm,cortex-theme-starter`
and `--output=PATH` to compare a saved pre-change engine with a new engine on the exact same
corpus and saved template bytes. Retain both engine artifacts and input hashes before editing
behavior; changed template profiles are a separate comparison. `--baseline=true` selects each
existing seminar template for every purpose, enabling old-engine comparisons before purpose
variants exist. Reports distinguish complete accepted pages, recoverable failed slides and
rejected contracts.

## Portable evidence

The `Hosts / byte and visual parity` job creates `target/deck-gates/report.json` and retains the
input hashes, page topology, per-host sizes and SHA-256 values, and raw timings for every host. Each
host records plan, all-page resolution, and export samples; the first plan sample is cold and
subsequent samples produce the enforced warm p50/p95 summary. The gate:

- executes planning and composition independently in native, Chromium Worker, and workerd;
- compares exact compiled-template plans, physical plans, every resolved display list, and PPTX;
- compares slide count, presentable indices, physical/logical ownership, hidden state, and
  continuation metadata returned by the host APIs;
- renders every physical page through real Chrome Canvas and compares every declared image's decoded
  display axes, visible frames, aspect error, and crop loss against the host-neutral native plan;
- mutates every renderable semantic role independently and requires a changed native plan,
  display list, or package, then flips one plan byte and proves the cross-host comparator notices;
- rejects a truncated WDSF with the stable `payload/invalid-deck-spec` envelope; and
- rejects atomic overflow with `layout/plan-atomic-overflow` before exposing a session, plan,
  preview, or partial package.

To reproduce the parity comparison locally, first prepare Chromium and comparator inputs using
[performance reproduction](performance.md#reproduce). Then generate each host's evidence:

```sh
cargo run --locked -p wasmppt-wasm --example deck_gate_native -- fixtures/deck-gates target/deck-gates
mkdir -p target/host-parity
set -o pipefail
npm test --workspace @corca-ai/wasmppt-worker 2>&1 | tee target/host-parity/workerd.log
npm run test:browser --workspace @corca-ai/wasmppt
node scripts/deck-gates.mjs --workerd-log=target/host-parity/workerd.log
```

Run the pipeline commands in Bash or Zsh so a workerd test failure is not hidden by `tee`.
The [CI host job](../.github/workflows/ci.yml) additionally compares regenerated fixture bytes
before running this sequence.

The Open XML compatibility job validates the generated PPTX with Microsoft's Open XML SDK. The
controlled PowerPoint workflow opens the same browser-generated deck without a repair dialog and
exports slides and PDF. Canvas, DOM/SVG, standalone HTML, and browser PDF continue to consume the
same exact WPDL page geometry and presentable page set; their pixel, structural, resource, and print
geometry tolerances remain owned by the browser visual and offline-output gates rather than being
reimplemented here.

## Performance and resource contracts

`benchmarks/budgets.json` owns separate browser and workerd ceilings for cold Starter compilation
plus planning, warm plan p95, all-page resolution p95, and current-revision PPTX export p95. Native,
browser, and workerd raw samples and p50/p95 summaries are stored in the compatibility report. The
existing performance job additionally owns scalar Wasm
size, first-visible latency, incremental edit-to-pixels, background export, cache residency, peak
memory, and the bounded visibility stress test. A deck result is eligible only after the exact
correctness comparisons pass.

Package/XML count, inflation, compression-ratio, payload, string, collection, node, nesting,
resource, physical-page, fragment, planner-work, overlay, and browser-resource limits remain the
single host-neutral bounds documented by their owning layers. Existing malicious ZIP/XML/template
tests exercise those bounds. This gate adds WDSF truncation and atomic-layout failures at both Wasm
hosts and native planning, ensuring host adapters preserve stable failure semantics and never
publish partial artifacts.

## Capability status

Implemented:

- deterministic Starter, comprehensive WDSF, and atomic-overflow fixture generation;
- native/browser/workerd exact WDTP, WDPL, WPDL, topology, and PPTX parity;
- portable Open XML SDK validation, browser surface gates, raw deck timings, and stable failure
  envelopes;
- controlled PowerPoint open-without-repair, slide export, and PDF export evidence.

Deferred:

- a software rasterizer for non-browser hosts;
- tighter PowerPoint pixel tolerances after the existing advanced-feature gaps close.

Unsupported:

- treating workerd as a Canvas, DOM, SVG, or PDF renderer;
- accepting a repaired Office file, a structurally valid ZIP, or cross-host hash agreement without
  executing each host as visual or semantic correctness;
- emitting partial plans, presentations, HTML, or PDF when a template, resource, limit, or atomic
  layout fails.

## Related documents

- [Semantic deck contracts](deck-engine.md) define WDSF, WDTP, and WDPL.
- [Semantic layout and pagination](deck-layout.md) define bounded planning and continuations.
- [Editable deck composition](deck-compose.md) defines deterministic PPTX projection.
- [Runtime host adapters](hosts.md) define the three execution environments.
- [Compatibility gates](compatibility.md) own visual and desktop-consumer evidence.
- [Performance contract](performance.md) owns the shared budgets and raw benchmark policy.

The expanded academic corpus adds both canonical list/image source orders, short two/eight-item
lists, sparse and near-full columns, and two-page continuations. Use `--expanded=true` with the SDK
evidence runner to include these cases; omission preserves the historical same-input corpus.
Reports retain actual resolved fonts, text, bounds and per-page continuation totals. The native
shared fixture exercises the same policy through existing cross-host and export gates.
