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

The content profiler records minimum, preferred, and maximum width demand alongside measured
height. Text demand uses shaped advances when exact bytes exist. List demand includes complete
nested item subtrees. Table demand derives per-column weights from cell text and start, center, or
end alignment instead of assigning every column the same width. Candidate scoring penalizes a
frame below preferred demand, while a frame below minimum demand is not a legal fit.

Raster PNG, JPEG, and GIF dimensions and SVG width/height or viewBox dimensions are decoded from
bounded resource bytes. A matching host hint is accepted; a stale hint is replaced by the
byte-derived value. Only an undecodable resource falls back to a positive validated hint. This
includes the display-axis swap for JPEG EXIF orientations five through eight and makes portrait,
square, and landscape demand identical on native and Wasm hosts.
Contain-fit media may scale down to its candidate slot, but only while both rendered dimensions
remain above `PlannerPolicy::readable_media_floor`. A figure in an indivisible figure/caption group
reserves one quarter of its slot for the following caption before measurement. When that stacked
reservation would collapse an extreme portrait below the floor, a bounded side-by-side fit keeps
the portrait and its caption or short related copy in one topology slot. If neither orientation
fits at the media and type floors, the candidate remains unavailable. The selected visible frame
is centered inside the template margins and has the canonical resource aspect within integer EMU
tolerance.

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

Adjacent prose ranges, list items, table rows, and code lines assigned to the same page lane are
coalesced back into one contiguous source slice. The planner keeps legal break opportunities
between these ranges during search, but the composer receives one editable text box, list, table,
or code block for each final contiguous run.

On a title layout only the title is a fixed header. Every following subtitle, prose, or credit
block flows through the subtitle region in source order. The same bounded candidate search places
those cover details without overlapping independently positioned header and body fragments.

## Candidate search and cost

For each source position the planner evaluates explicit stack, two- and three-column flow,
mirrored weighted split, mirrored media/text, related media/text cards, two/four/six-peer grid,
lead/supporting, two/four/six-item gallery, table-wide, and comparison topologies. Each topology
owns a finite slot set. Continuous prose, list, code, and weighted splits enumerate bounded
contiguous partitions; peer and gallery groups occupy distinct slots; and media/text candidates
assign by semantic role rather than assuming source order is visual order. Adjacent weak
media/text relations may additionally form one, two, or three source-ordered cards for one candidate;
they remain separable in every other candidate, so unrelated prose is not pulled into a card.
Candidates never infer layout from example slides or visible shape names. The selected topology
and slot count are encoded on each physical page, while fixed template content and topology-slot
regions remain distinguishable through composition.

Media/text candidates derive a bounded set of mirrored column and top/bottom split positions from
the canonical media aspect, readable media floor, measured text minimum/preferred width, measured
text height, and template margins. Fixed narrow/equal/wide ratios are only a safe fallback when a
resource cannot be profiled. Measured fit selects among the derived breakpoints without depending
on Markdown source order. Same-paragraph, adjacent-block, and blank-separated source relations
become decreasing affinity costs; source side penalizes a contrary visual order, while an explicit
caption remains an indivisible hard relation. Contain whitespace, relation distance, visual
imbalance, and complexity are scored after readability. Gallery candidates enumerate bounded
source-ordered justified rows, their transposed column equivalents, and lead/supporting variants.
Track weights come from canonical intrinsic aspects; caption-bearing items reserve part of their
track before those weights are derived. Cover-crop loss is measured from the byte-derived source
aspect and candidate frame. A raster uses cover only while that loss is at or below
`PlannerPolicy::max_cover_crop_per_mille`; otherwise the same candidate deterministically falls
back to contain. Portrait, square, wide, and mixed collections can therefore choose non-uniform
geometry without unsafe distortion or implicit crop. Seven to ten items are balanced across
continuation pages by the same global page-load score.

Gallery ancestry remains explicit on internal flow units after the semantic `Gallery` container is
expanded. Only raster images authored in that gallery context may select `ContentFit::Cover`.
Standalone raster figures and SVG use `Contain` in every topology; data-bearing media is therefore
never cropped by a heuristic. Charts remain atomic planned graphic frames rather than picture
placements. A selected picture fit is serialized together with the allocation slot, canonical
source dimensions, exact visible frame, and optional centered source crop. Canvas/WPDL and editable
PPTX composition consume that placement unchanged and never repeat the aspect or crop calculation.

Standard prose and list flow uses at most eight semantic units per column. It remains a readable
stack through eight units, balances 9--16 units across two columns, then creates balanced
continuation pages; 11 and 16 equal-demand items therefore resolve to 6/5 and 8/8, while 17 and 18
resolve to 9/8 and 9/9 across pages. Three-column flow is reserved for code and admits at most 12
logical lines per lane. These thresholds are candidate bounds, not a shortcut around measurement:
every admitted partition must still fit measured demand at or above the readable floor.

Dynamic programming compares the worst readability band before physical page count. A comfortable
continuation therefore wins over a one-page result compressed toward the readable floor. Remaining
ties compare balanced semantic-unit load across continuation pages, normalized measured content
area, width/crop loss, measured slot-demand imbalance, continuation orphaning, squared whitespace,
topology complexity, and a stable topology ordinal. An equally balanced solution keeps the earlier
page or column at least as full as the later one. Peer, gallery, table, and mixed media/text
collections reject stack or generic flow assignments that would discard their semantic topology;
table rows use only the table-wide topology. Source order, contiguous flow partitions, and authored
relation groups are hard constraints rather than soft cost terms.

Text may shrink only to `PlannerPolicy::readable_floor`. An atomic group that cannot fit any legal
candidate at that floor fails with `PLAN_ATOMIC_OVERFLOW`; it is not clipped or silently split.

## Continuations

Derived physical pages retain the logical slide's hidden state and selected template layout. They
carry the source H2/title identity for repeated heading chrome and the minimal `n/total` marker.
The first continued fragment of a table on each page records its header-row repeat count. The
planner coalesces every contiguous row range into exactly one native-table fragment per page, and
the composer renders repeated-header metadata without creating additional source fragments.

## Bounds and failure policy

`PlannerLimits` independently bounds exact font faces and bytes, flow units, evaluated
topology/slot assignments, accepted candidates per source position, font measurements, and
dynamic-programming states. Contiguous partition enumeration has a fixed per-topology cap and every
evaluated assignment consumes the global candidate-work counter. Measurement
results are cached only within one immutable spec/font planning pass and keyed by node, slice,
exact template region, region role, frame width and height, font size, and repeated table-header
state. Exceeding a work bound fails with `PLAN_WORK_LIMIT`.

These are denial-of-service and resource contracts, not a UX slide cap. The caller's `DeckLimits`
still bounds decoded collections and final physical pages for its host environment. Tests cover
determinism, exact coverage, relation preservation, readable type, atomic overflow, repeated table
headers, byte-derived media dimensions, alignment-aware table demand, nested-list preservation,
contiguous editable slices, peer-slot assignment, mirrored media/text assignment, balanced
contiguous columns, readability-first ordering, aspect-aware 2--10 item galleries, explicit
contain/cover policy, incremental slide reuse, fallback-font diagnostics, and property-generated
bounded termination.

## Canonical quality corpus

`fixtures/deck-gates/corpus.json` pins the end-to-end `autolayout-v3` corpus beside its generated
Starter POTX and WDSF input. The corpus includes variable title details, long prose, a long list
with an intentionally empty in-progress item, multi-page tables and code, mixed aspect-ratio media,
ten-item galleries with captions, quotes, sections, display math, definitions, statements, and a
hidden page. It also crosses 4:1, 16:9, 1:1, 3:4, and 1:4 resources with image-only, caption,
short-copy, long-prose, and 2/3/5/9 related media/text contexts, plus JPEG EXIF orientations six
and eight. Those cases produce 117 independently identified quality images. The fixture generator
is the source of truth; CI regenerates all four files and fails on byte drift.

The native gate rejects lost or duplicate source coverage, overlapping geometry, type below the
readable floor, empty or badly imbalanced selected columns, singleton final-page orphans,
undersized media, fragmented table row slices, and invalidation beyond one edited logical slide.
It emits `native-quality.json` with topology counts, contract results, and raw per-image source
axes, visible frame, aspect error, and crop loss. Chrome then decodes and renders every resulting
WPDL scene through the public Canvas renderer, checks every source-backed bound, rejects blank
slides, and records the same raw evidence from the actual `draw-image` commands and decoded image
dimensions in `browser-quality.json`. The final gate requires the two evidence sets plus exact
template-plan, deck-plan, WPDL, PPTX, and topology parity across native, browser, and workerd hosts.

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
