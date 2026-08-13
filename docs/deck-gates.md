# Deck compatibility gate

Status: portable cross-host correctness and performance gate implemented; licensed desktop
evidence remains a controlled release gate

The deck gate makes the complete Starter POTX plus WDSF path a tested contract before a host
integrates it. It executes the same checked-in inputs through native Rust, the browser module
Worker, and Cloudflare `workerd`. Passing requires exact WDTP, WDPL, WPDL, page topology, and PPTX
bytes across all three hosts. A ZIP that merely opens, or three hosts that merely agree on a hash
without executing the planner, do not pass.

## Fixtures and regeneration

`fixtures/deck-gates/starter.potx` is a deterministic Cortex Theme Starter with title, content, and
statement layouts. Content body regions accept nested section headings as flow
content while the leading title/section still maps to the title placeholder.
`deck-spec.wdsf` covers every renderable `SemanticRole` from title through
statement, plus the typed table row, cell, and column contracts. It includes rich text and safe
links, Korean/CJK and RTL text, nested ordered/unordered lists, PNG, GIF first-frame conversion,
SVG, tables, charts, code, diagrams, display math, definitions, hidden slides, missing-font
diagnostics, and enough content for automatic continuation pages. `atomic-overflow.wdsf` is a
valid contract that cannot fit at the readable floor.

Regenerate and compare the fixtures with:

```sh
fixture_dir=$(mktemp -d)
cargo run --locked -p wasmppt-deck --example write_gate_fixtures -- "$fixture_dir"
cmp fixtures/deck-gates/starter.potx "$fixture_dir/starter.potx"
cmp fixtures/deck-gates/deck-spec.wdsf "$fixture_dir/deck-spec.wdsf"
cmp fixtures/deck-gates/atomic-overflow.wdsf "$fixture_dir/atomic-overflow.wdsf"
```

The generator validates the WDSF and asserts the complete supported-role set. Table row/cell/column
coverage is asserted separately because those are typed table members rather than standalone
semantic nodes.

## Portable evidence

The `Hosts / byte and visual parity` job creates `target/deck-gates/report.json` and retains the
input hashes, page topology, per-host sizes and SHA-256 values, and raw timings for every host. Each
host records seven plan, all-page resolution, and export samples; the first plan sample is cold and
the remaining six produce the enforced warm p50/p95 summary. The gate:

- executes planning and composition independently in native, Chromium Worker, and workerd;
- compares exact compiled-template plans, physical plans, every resolved display list, and PPTX;
- compares slide count, presentable indices, physical/logical ownership, hidden state, and
  continuation metadata returned by the host APIs;
- mutates every renderable semantic role independently and requires a changed native plan,
  display list, or package, then flips one plan byte and proves the cross-host comparator notices;
- rejects a truncated WDSF with the stable `payload/invalid-deck-spec` envelope; and
- rejects atomic overflow with `layout/deck-planning-failed` before exposing a session, plan,
  preview, or partial package.

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
memory, and the 1,000-page visibility stress test. A deck result is eligible only after the exact
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
- additional independently produced semantic-deck fixtures with affirmative redistribution
  provenance;
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
