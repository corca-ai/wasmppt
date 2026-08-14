# Semantic layout and automatic pagination

Status: host-neutral bounded planner implemented

`wasmppt-deck-layout` turns a validated `DeckSpec` and `DeckTemplatePlan` into a deterministic
`DeckPlan`. It uses real template frames and rich-text measurement. It has no DOM, browser font
API, Markdown parser, package I/O, or arbitrary product slide-count policy.

## Inputs and identity

The planner consumes source-backed semantic nodes, compiled layout regions, exact page geometry,
resolved text styles, and an optional `FontCatalog`. A catalog contains immutable font bytes,
face indices, and a host-computed identity over the complete set. The plan identity hashes the
encoded source contract, template and cache identity, font-catalog identity, and complete planner
policy. Equal inputs therefore produce byte-for-byte equal plans.

`DeckPlanner::replan` is the revision fast path. It reuses exact prior physical pages only when
the template plan, font catalog, planner policy, limits, logical slide value, and every resource
referenced by that slide are unchanged. A mismatched prior plan identity falls back to a complete
plan. The result names invalidated logical slides, old and new physical page identities, and the
reused-page count so a host can invalidate only proven dependencies.

If the requested template font bytes are available, measurement uses `wasmppt-shaper`. Otherwise
the planner uses deterministic conservative metrics and emits `PLAN_FONT_RISK` for each affected
source node. Fallback is observable; it is never presented as exact font fidelity.

## Semantic flow

Splittable content becomes source-owned units before candidate search:

- prose prefers paragraph and sentence boundaries, then bounded Unicode line-break boundaries;
- lists split only between list items;
- tables split only between rows;
- code splits only between logical lines;
- galleries keep each figure and caption group together;
- a figure and following caption, quote and credit, and section heading and first child are
  indivisible authored relations;
- raster images, SVG, charts, diagrams, and display math remain atomic.

Every fragment retains its exact source slice. Repeated table headers and repeated continuation
headings are page chrome metadata rather than duplicate source fragments, so exact-ownership
validation still proves complete, ordered, single coverage.

On a title layout only the title is a fixed header. Every following subtitle, prose, or credit
block flows through the subtitle region in source order. The same bounded candidate search places
those cover details without overlapping independently positioned header and body fragments.

## Candidate search and cost

For each source position the planner evaluates a fixed generic family over the selected template
body or statement frame: stack, balanced columns, weighted split, peer grid, lead/supporting, and
dominant-content split. Candidates never infer layout from example slides or visible shape names.

Dynamic programming first minimizes physical page count, then a deterministic cost combining font
reduction, squared unused-frame area, narrow text measure, single-text orphaning, and candidate
complexity. Squared whitespace makes an equally sized pagination prefer balanced pages and avoids
a needlessly sparse final page. Source order and authored relation groups are hard constraints,
not soft cost terms.

Text may shrink only to `PlannerPolicy::readable_floor`. An atomic group that cannot fit any legal
candidate at that floor fails with `PLAN_ATOMIC_OVERFLOW`; it is not clipped or silently split.

## Continuations

Derived physical pages retain the logical slide's hidden state and selected template layout. They
carry the source H2/title identity for repeated heading chrome and the minimal `n/total` marker.
The first continued fragment of a table on each page records its header-row repeat count. The
composer renders this metadata without creating additional source fragments.

## Bounds and failure policy

`PlannerLimits` independently bounds exact font faces and bytes, flow units, candidate pages,
candidates per source position, font measurements, and dynamic-programming states. Measurement
results are cached by node, slice, region role, frame width, and font size. Exceeding a work bound
fails with `PLAN_WORK_LIMIT`.

These are denial-of-service and resource contracts, not a UX slide cap. The caller's `DeckLimits`
still bounds decoded collections and final physical pages for its host environment. Tests cover
determinism, exact coverage, relation preservation, readable type, atomic overflow, repeated table
headers, incremental slide reuse, fallback-font diagnostics, and property-generated bounded
termination.

## Verification

```sh
cargo test -p wasmppt-deck-layout --all-features
cargo clippy -p wasmppt-deck-layout --all-targets --all-features -- -D warnings
cargo check -p wasmppt-deck-layout --all-features --target wasm32-unknown-unknown
npm run check:core-boundary
```

## Related documents

- [Semantic deck contracts](deck-engine.md) defines source ownership and binary plan contracts.
- [Cortex Theme Starter compiler](deck-template.md) defines the POTX layout and region profile.
- [System architecture](architecture.md) defines the host boundary and composition pipeline.
- [Performance contract](performance.md) defines the repository-wide latency and memory gates.
