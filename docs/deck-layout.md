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
Display-math SVG dimensions represent their natural CSS-pixel extent at a 12-point authoring base.
The planner scales that extent to the template region's type size, centers a lone formula in a
statement region, and only scales down when the result exceeds its safe frame. It never enlarges a
formula to the generic media slot or applies the raster-image readability floor.
Contain-fit media may scale down to its candidate slot, but only while both rendered dimensions
remain above `PlannerPolicy::readable_media_floor`. A figure in an indivisible figure/caption group
reserves the actual measured following text height and gaps at the candidate width and role size. When that stacked
reservation would collapse an extreme portrait below the floor, a bounded side-by-side fit keeps
the portrait and its caption or short related copy in one topology slot. If neither orientation
fits at the media and type floors, the candidate remains unavailable. The selected visible frame
is centered inside the template margins and has the canonical resource aspect within integer EMU
tolerance. For an image with validated intrinsic dimensions, that resolved contain placement and
the media floor are the fit authority; a provisional text-line-height media estimate cannot reject
it first. Content without intrinsic resource dimensions, such as a chart, retains the conservative
measured-height check.
Pure-media bleed does not inherit body text insets. Text and mixed content retain their authored
safe frames and text margins.

## Presentation purpose

Purpose is document-wide and independent of theme color. The following sizes are points;
ordinary text, list content and table cells use body sizes. Only an explicit `Caption` role
uses caption type, including emphasized captions represented by the host with that role.

| Purpose | Content title | Body preferred / floor / cap | Caption | Cover |
| --- | ---: | ---: | ---: | ---: |
| Conference | 30 | 24 / 22 / 28 | 18 | 40 |
| Seminar (default) | 28 | 20 / 18 / 24 | 16 | 36 |
| Handout | 26 | 18 / 16 / 22 | 14 | 32 |

Purpose typography and dynamic header reclamation apply only to regions carrying explicit
[template generation bounds](deck-template.md#generation-bounds-and-typography). Custom POTX
regions without these opt-in bounds retain their fonts and frames, while still using improved
candidates and full-image containment. Exact font bytes and observable fallback diagnostics
are unchanged. After topology and source pagination, ordinary body text in full-height stack and column lanes is measured against their final
available width and height. A density-dependent preferred size adds at most four points to the
nominal body target, in half-point increments; bounded wrapping and fitting can reduce that size
within the unchanged floor. Two short bullets therefore receive larger type than eight short
bullets without shrinking the latter merely to establish a difference. Each physical continuation
uses its own measured headroom. Growth never changes accepted source partitions or page count.
Captions, tables, formulas, grid/card rows, vertical media splits, hard mixed-content lanes and custom unopted regions retain their role
policy. Sparse slides may retain whitespace; filling the page is not a hard constraint.

## Semantic flow

Splittable content becomes source-owned units before candidate search:

- prose prefers paragraph and sentence boundaries, then bounded Unicode line-break boundaries;
- lists split only between list items;
- tables split only between rows;
- code splits only between logical lines;
- galleries keep each figure and caption group together;
- a `Never` figure container and multipanel figures with a shared caption remain indivisible;
- a figure and following caption, quote and credit, and section heading and first child are
  indivisible authored relations;
- raster images, SVG, charts, diagrams, and display math remain atomic.

Every fragment retains its exact source slice. Repeated table headers and repeated continuation
headings are page chrome metadata rather than duplicate source fragments, so exact-ownership
validation still proves complete, ordered, single coverage.

Adjacent prose ranges, list items, table rows, and code lines assigned to the same page lane are
coalesced into one contiguous source slice **before fitting and measuring** a candidate lane.
Legal break opportunities remain available between these ranges during search. Body padding,
line rounding, and inter-block gaps apply once to the resulting editable shape, rather than once
per sentence, item, or code line. Prose load counts source text runs; list items and code lines
retain their authored load counts. Punctuation and the bounded text chunk size therefore do not
create additional visual blocks.

Text wraps at Unicode line-break opportunities, retaining complete words that fit a line and
using emergency character breaks only for an oversized token. Deterministic fallback advances
account for full-width Unicode text. Code and inline-code runs use the composed Courier New typeface, or observable deterministic
monospace fallback metrics when its bytes are missing. Paragraph indentation uses the same helper as editable
composition; nested list paragraphs are measured at their own hierarchy level. Visible bullet/number advances,
including continued ordered-list numbers and hanging indentation, participate in first-line wrapping.

On a title layout only the title is a fixed header. Every following subtitle, prose, or credit
block flows through the subtitle region in source order. The same bounded candidate search places
those cover details without overlapping independently positioned header and body fragments.
A title slide uses the title layout only when the layout has a region for every block after the
heading. A title slide that also carries a list, table, figure, code or other body content is
planned on the content envelope instead, with its title as the content heading, so that content
paginates like any other content slide. A template without a content envelope keeps the title
layout.

## Candidate search and cost

For each source position the planner evaluates explicit stack, two- and three-column flow,
mirrored weighted split, mirrored media/text, related media/text cards, two/four/six-peer grid,
lead/supporting, two/four/six-item gallery, table-wide, and comparison topologies. Each topology
owns a finite slot set. Continuous prose, list, code, and weighted splits enumerate bounded
contiguous partitions. Authored block boundaries remain candidate cuts. Within a continuous text run, a measured
advance frontier retains explicit hard-break boundaries, line-width crossings, demand-balanced
cuts and run endpoints rather than
performing a quadratic search over punctuation. Balanced cuts are evaluated before consuming
the finite partition capacity.
Per-slot load constraints prune illegal cuts before the partition bound
is applied, so invalid early cuts cannot hide a later legal partition. Candidate retention chooses
the best topology per source end before applying its bound and retains the furthest fitting ends;
duplicate topologies never consume the retained capacity or prevent other topologies from being
evaluated. The total assignment and measurement work limits still bound the complete search.
A whole-slide page in the normal readability band is optimal in the leading readability and
page-count score dimensions. The planner evaluates every whole-page topology first and returns the best such
comfortable page without exploring its unused continuation states. If no comfortable whole page
exists, bounded dynamic programming still compares continuations against compressed candidates.
Sampled column failures never trigger prefix pruning. Stack search also accounts for later
atomic blocks or non-peer content that can restore its eligibility.
Text advances are cached by source slice, region and type size. Text/table height measurements
reuse width-dependent results across remaining frame heights; media retains both dimensions in
its cache key because contain fitting depends on height. Both caches retain explicit bounds.
Same-role continuous content uses equal-width flow columns; weighted splits
require separate content groups with distinct semantic roles. Peer and gallery groups
occupy distinct slots; and media/text candidates
assign by semantic role rather than assuming source order is visual order. Display equations
remain in the source-ordered text flow; they are not media lanes that can be collected
separately from the surrounding explanation. Adjacent weak
media/text relations may additionally form one, two, or three source-ordered cards for one candidate;
they remain separable in every other candidate, so unrelated prose is not pulled into a card.
Candidates never infer layout from example slides or visible shape names. The selected topology
and slot count are encoded on each physical page, while fixed template content and topology-slot
regions remain distinguishable through composition.

Media/text candidates derive a bounded set of mirrored column and top/bottom split positions from
the canonical media aspect, readable media floor, measured text minimum/preferred width including composed paragraph indentation, measured
text height, and template margins. Fixed narrow/equal/wide ratios are only a safe fallback when a
resource cannot be profiled. Measured fit selects among the derived breakpoints without depending
on Markdown source order. Same-paragraph, adjacent-block, and blank-separated source relations
become decreasing affinity costs; source side penalizes a contrary visual order, while an explicit
caption remains an indivisible hard relation. One explicit canonical pair is the exception:
a logical body consisting of exactly one ordinary prose/list node and one figure image/SVG uses
text on the left and the complete image on the right, in either source order. This changes visual
slot ownership, never source sequence, source association or fragment reading order. Multi-image,
gallery, caption and card bodies retain their existing policies. Contain whitespace, relation distance, visual
imbalance, and complexity are scored after readability. Gallery candidates enumerate bounded
source-ordered justified rows, transposed columns, equal grids, and dimension-floor-aware tracks.
Track weights come from canonical intrinsic aspects. Each candidate measures its captions at
its actual width. Source-ordered rows and transposed columns use consistent track alignment;
the planner does not give an arbitrary gallery image a hero slot. Seven to ten items can
continue across pages when their measured figure/caption cards cannot fit.
Identical-aspect peers use uniform track candidates. Matching peers share a measured picture
height after caption fitting, including unequal captions and interleaved different-aspect panels.
Their original size disparity is a quantized alignment cost ahead of whitespace, so an oversized
track cannot win by shrinking its peers. Pictures align at the top of their allocated row;
caption measurements and source order remain intact. Captioned media outside a gallery also
exclude lead/supporting candidates, so source position alone cannot promote an image.
Heading and explicit-caption associations union whole existing flow groups, including across
leaf-node boundaries. A heading before a multipanel figure retains bounded card fitting.
Nested galleries retain inherited hard groups. Ordered image/caption pairs inside an indivisible
figure use bounded card arrangements and either fit as a whole or diagnose atomic overflow.

All raster images and SVG use `Contain`, including images in galleries. No semantic candidate
crops or stretches source figures. The picture's complete axes, legends and whitespace survive;
there is no OCR, trimming or AI interpretation. The legacy crop policy field remains part of
planner identity for native compatibility but cannot authorize semantic cropping.
Charts remain atomic planned graphic frames rather than picture
placements. A selected picture fit is serialized together with the allocation slot, canonical
source dimensions, exact visible frame, and optional centered source crop. Canvas/WPDL and editable
PPTX composition consume that placement unchanged and never repeat the aspect or crop calculation.

Standard prose and list flow evaluates stacks and equal-width two-column partitions against
actual width, Unicode wrapping and measured height. An item count does not force columns or
continuations. Three-column flow is reserved for code and retains its bounded line candidates.
Finite partition, candidate, measurement and dynamic-state limits remain unchanged.

Dynamic programming compares the worst readability band before physical page count. One font
step below the preferred size remains in the normal band; two or more steps enter a compressed
band. A comfortable continuation wins over a one-page result compressed toward the readable floor. Remaining
ties compare balanced semantic-unit load across continuation pages, normalized measured content
area, width/crop loss, measured slot-demand imbalance, continuation orphaning, squared whitespace,
topology complexity, and a stable topology ordinal. An equally balanced solution keeps the earlier
page or column at least as full as the later one. Peer, gallery, table, and mixed media/text
collections reject stack or generic flow assignments that would discard their semantic topology;
table rows use only the table-wide topology. Source order, contiguous flow partitions, and authored
relation groups are hard constraints rather than soft cost terms.

Text may shrink only to its template minimum, bounded below by the planner's body
readability policy. Header planning first measures the preferred type size and
fits the title vertically inside its template generation envelope. A short title returns unused
reservation to the body; custom fixed regions retain their authored frame. With host
measurement enabled and no exact font bytes, header wrapping uses the same
resolved family, bold/italic style and measured token advances as Canvas. The same
height delta moves the body and media bleed, increasing or reducing their remaining
height; ordinary body pagination then runs in that resolved region. If this
cannot fit, title type decreases by the bounded font step toward its own minimum.
There is no page-percentage title cap. Every accepted fragment retains explicit
geometry and typography. Continuation chrome uses the first page's exact title
frame and type size, including the measured synthetic continuation suffix.

An atomic group that cannot fit any legal candidate at its floor produces
`PLAN_ATOMIC_OVERFLOW`. Its logical slide enters `DeckPlan::failed_slides` with
source diagnostics; other slides continue. Validation proves exact ordered
coverage of ready slides and a unique, diagnosed, source-owned failed inventory
with no physical output. Failed prior revisions conservatively use full
replanning; unchanged complete revisions retain the dependency-proven fast path.
Work limits and invalid inputs remain whole-revision errors.

A `Prose` or `Subtitle` child sequence whose same-role leaves contain both text and SVG formulas is
an inline formula flow. The planner measures each text span at the selected template type size,
scales each formula from its natural CSS-pixel extent at that same size, packs the atoms in source
order, and wraps only between atoms. Atoms on one row are vertically centered against the row's
maximum extent. Inline text uses font advances when the requested font is available and a
character-class advance estimate otherwise; it does not inherit the block-flow minimum width.
Hosts may supply `DeckPlanner::plan_with_inline_measurer` with the same font resolver and advances
used for painting. Theme font tokens resolve to the composed run's concrete family before host
measurement. The measured advances, including template emphasis and inline-code faces, enter
the plan identity and determine formula placement and wrapping. Such plans conservatively use
full planning rather than reusing pages under an unverified host-font identity.
Leading soft whitespace after a formula becomes one measured word-space gap in the fragment
geometry instead of relying on a consumer to paint whitespace at the start of a separate text
box. The same whitespace collapses when its atom starts a wrapped row.
The direct-container and same-role requirements keep sections that contain
standalone display math, media-caption groups, and ordinary prose children in their block flows.

## Continuations

Derived physical pages retain the logical slide's hidden state and selected template layout. They
carry the source H2/title identity for repeated heading chrome and structured continuation
ordinal, total, and label metadata. For a multi-page logical slide, its actual visible title ends
with the accepted label in parentheses, including the first page: `(1/2)`, `(2/2)`. Template slide
number fields remain independent page furniture. The source title range is unchanged; the suffix
and repeated headings own no additional source fragments.

The planner measures every actual ordinal/total heading string. If the accepted topology needs a
larger heading frame, it reserves that measured height and replans the body conservatively.
Reservation increases monotonically at a fixed title font, and all attempts consume the original
cumulative assignment and measurement budgets. The final total is measured before acceptance;
there is no host counter or renderer-specific refit. Synthetic measurement IDs include the label,
preventing reuse of unsuffixed or stale-total widths. A work-limit failure remains explicit.
The first continued fragment of a table on each page records its header-row repeat count. The
planner coalesces every contiguous row range into exactly one native-table fragment per page, and
the composer renders repeated-header metadata without creating additional source fragments.
Table-wide pages may include explanations, lists, code, sections, media, and independent tables
in source order. Adjacent rows coalesce only within the same source table; unrelated blocks keep
their own fitted groups and type choices. Header repetition is tracked independently for each
source table on each page, including tables inside authored relation groups. Non-table fragments
always carry a zero repeat count.

`DeckPlanner::plan_with_metrics` returns the identical accepted plan plus actual candidate-assignment
and cache-miss measurement counts, including rejected candidates. Counts describe one complete
planning pass, not process memory. The existing per-slide-attempt assignment and per-pass
measurement limits still apply; metrics do not change candidate retention or scoring.

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

`fixtures/deck-gates/corpus.json` pins the end-to-end `academic-autolayout-v4` corpus beside its generated
Starter POTX and WDSF input. The corpus includes variable title details, long prose, a long list
with an intentionally empty in-progress item, multi-page tables and code, mixed aspect-ratio media,
ten-item galleries with captions, quotes, sections, display math, definitions, statements, and a
hidden page. It also crosses 4:1, 16:9, 1:1, 3:4, and 1:4 resources with image-only, caption,
short-copy, long-prose, and 2/3/5/9 related media/text contexts, plus JPEG EXIF orientations six
and eight. Each quality image has a stable identity. The
[fixture generator](../crates/wasmppt-deck/examples/write_gate_fixtures.rs) is the source of truth;
CI regenerates the inputs and manifest and fails on byte drift. The [deck gate](deck-gates.md)
defines reproduction and cross-host comparisons.

The native gate rejects lost or duplicate source coverage, overlapping geometry, type below the
readable floor, empty or badly imbalanced selected columns, singleton final-page orphans,
undersized media, fragmented table row slices, and invalidation beyond one edited logical slide.
Three-image cases also require equal visible sizes for matching peers, inside and outside a gallery.
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

Pagination first preserves authored media/text relations on the media page while
allowing remaining text to continue. If no cohesive plan exists, an atomic-overflow retry may
place ordinary related prose or list content on a following page, allowing an image-only bleed
page. Explicit captions remain hard groups; the retry uses the same cumulative candidate and
measurement bounds. Missing soft endpoints carry a deterministic affinity penalty. The first page includes its separately
placed headers when checking these relations. Missing relation endpoints make a
partial page ineligible before candidate retention; they do not justify stopping
prefix search.
