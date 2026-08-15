# Editable deck composition

Status: editable text, lists, raster images, SVG, deterministic GIF stills, tables, supported 2D
charts, immutable live overlays, and pull-based PPTX export implemented

`wasmppt-deck-compose` projects an exact validated `DeckSpec`, `DeckTemplatePlan`, and `DeckPlan`
tuple into PresentationML. It is a host-neutral Rust core: it neither scrapes a DOM nor invokes a
JavaScript presentation generator. A template SHA-256 mismatch, invalid plan, unsupported content,
unsafe media, or resource overrun fails with a stable `ComposeErrorCode` before an output revision
is exposed.

## Output ownership

Composition replaces the presentation's slide topology in one revision. It materializes only:

- `[Content_Types].xml`, `ppt/presentation.xml`, and its relationship part;
- generated slide and slide-relationship parts;
- media referenced by the generated slides; and
- chart parts, their relationships, and coordinated embedded XLSX workbooks.

Old slide parts are removed. Layouts, masters, themes, decorations, unrelated media, extension
markup, and unknown parts remain in the template package. `PackageOverlay` serves those untouched
parts directly from the original archive and raw-copies their compressed payloads during export.
The live overlay itself implements `PackagePartSource`, so rendering and dependency fingerprinting
operate on the same logical revision that export later serializes.

## Editable semantics

Text is emitted as DrawingML paragraphs and runs rather than flattened pictures. Bold, italic,
strikethrough, inline-code typeface, explicit template-derived font size/typeface/color, and safe
external web, mail, and telephone hyperlinks remain editable. Nested lists preserve source order,
hierarchy level, ordered start value, and deterministic indentation. An empty list item remains one
editable bullet or numbered paragraph, so an in-progress authoring line does not invalidate the
deck or disappear from export. Source-anchor links stay non-active until an explicit internal-slide
target contract exists.

Each shape and relationship receives a deterministic source-order identifier. Hidden state is
written on the physical slide. Derived continuation pages add only the planned repeated heading;
ordinal, total, and label remain plan metadata for navigation and accessibility. The composer does
not compete with template-owned slide-number fields by materializing another visible counter.

Container nodes never become output shapes. The validated plan names only source-owning leaf
nodes, and each coalesced fragment becomes exactly one editable text box, list, code box, table,
picture, or chart. Composition does not re-split a planned slice or perform output-specific
column fitting.

For inline formula flow, surrounding text leaves remain editable text boxes and formula leaves
remain SVG pictures. All leaves inherit the `Prose` or `Subtitle` container role, including in a
template region that accepts only subtitles. The composer suppresses per-placeholder body margins
on those planned text spans because the planner has already applied the parent region margins
once; it consumes the planned horizontal frames without repeating baseline, wrapping, or
formula-size decisions. Formula SVG paint that uses the CSS `currentColor` keyword resolves to the
effective template-region text color before the media part is written. Standalone display math
uses the same rule, while diagrams and other authored SVG retain their original paint.

Tables remain native DrawingML tables. Planned row slices retain source order, continuation pages
prepend exactly the planned source header rows, and every cell uses the same editable rich-run and
hyperlink writer as ordinary text. The template plan's region text style and theme color slots
drive cell text, header fill, banding, and borders. Column widths are deterministic content-demand
weights derived from visible cell text and declared start/center/end alignment; a wide table keeps
extra width for its leading key column. Row heights likewise follow the maximum wrapped-cell demand
for each visible row. The weighted geometry consumes the exact planned frame, retains declared cell
alignment, and never turns a continued page into multiple independently editable tables.

Bar, column, line, area, pie, doughnut, and scatter nodes become native chart graphic frames.
Each chart cache and its embedded `Sheet1` workbook are built by the same projection primitives
used by compiled-template chart mutation, then exposed as one immutable overlay revision. Scatter
categories are numeric X values and reject non-numeric strings. Empty inputs, series/category
length mismatches, and non-finite values fail before an overlay is returned. The closed
`DeckSpec::ChartKind` set is the editable contract; other OOXML chart families remain preserved or
diagnosed by the resolver and are never mislabeled as editable.

## Media policy

PNG and JPEG payloads pass through unchanged. The accepted plan already contains the allocation
slot, visible shape frame, contain/cover mode, canonical source size, and optional centered
DrawingML crop. The composer verifies prepared media dimensions against that source size, then
writes the visible frame and crop verbatim; it performs no aspect-fit calculation. Alt text is
written to the picture's non-visual properties. The same planned bounds and crop survive reopened
PPTX resolution and WPDL lowering.

Plan validation rejects overlapping source-owned fragment frames, non-canonical media placement,
and font/media choices that do not match the semantic content. After the overlay is resolved, the
browser adapter also requires every
source-owned display-list element to have the exact planned bounds before it attaches source
semantics; geometry drift is a hard error rather than a missing hit-test annotation.

SVG is retained as vector media and referenced through the Office SVG extension. XML parsing
rejects scripts, foreign objects, event handlers, JavaScript URLs, imports, and external references.
GIF input is decoded under byte and pixel bounds, composited onto its logical first-frame canvas,
and encoded as a deterministic PNG still in the core. These rules are identical for native and
Wasm builds.

## Streaming and bounds

`PresentationOverlay::generation_cursor` accepts a positive maximum output chunk size and emits
the exact PPTX revision without constructing a complete PPTX buffer or base64 media graph. Peak
materialized memory is bounded by `ComposeLimits`; unchanged compressed source bytes and output
bytes are not retained by the composer. The revision digest covers template identity, spec
identity, the encoded plan, and every materialized logical part.

## Verification

```sh
cargo test -p wasmppt-deck-compose --all-features
cargo clippy -p wasmppt-deck-compose --all-targets --all-features -- -D warnings
cargo check -p wasmppt-deck-compose --all-features --target wasm32-unknown-unknown
```

Tests verify editable run properties and hyperlinks, nested numbering, trailing empty items,
single-table continuation slices, split-table header repetition, content-weighted table geometry,
cell alignment, theme-derived table styling, coordinated chart caches/workbooks, SVG retention,
deterministic GIF stills, unknown-part preservation, raw compressed reuse, one-byte overlay pulls,
configured bounds, and structural equality between direct live-overlay resolution and a
streamed/reopened PPTX. The Open XML SDK fixture includes editable text, a table, and a chart with
its nested workbook.

## Related documents

- [Semantic deck contracts](deck-engine.md) defines the validated input tuple.
- [Cortex Theme Starter compiler](deck-template.md) defines layout discovery and template policy.
- [Semantic layout and pagination](deck-layout.md) defines physical page and fragment planning.
- [OPC and ZIP substrate](opc.md) defines immutable overlays and exact streaming export.
