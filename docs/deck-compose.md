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
written on the physical slide. Derived continuation pages add only the planned repeated heading
and minimal `n/total` marker; neither becomes another source-owned fragment.

Tables remain native DrawingML tables. Planned row slices retain source order, continuation pages
prepend exactly the planned source header rows, and every cell uses the same editable rich-run and
hyperlink writer as ordinary text. The template plan's region text style and theme color slots
drive cell text, header fill, banding, and borders. Because `DeckSpec` does not invent column or
row measurements, the planned frame is divided deterministically across its declared columns and
visible rows.

Bar, column, line, area, pie, doughnut, and scatter nodes become native chart graphic frames.
Each chart cache and its embedded `Sheet1` workbook are built by the same projection primitives
used by compiled-template chart mutation, then exposed as one immutable overlay revision. Scatter
categories are numeric X values and reject non-numeric strings. Empty inputs, series/category
length mismatches, and non-finite values fail before an overlay is returned. The closed
`DeckSpec::ChartKind` set is the editable contract; other OOXML chart families remain preserved or
diagnosed by the resolver and are never mislabeled as editable.

## Media policy

PNG and JPEG payloads pass through unchanged. `cover` computes a centered DrawingML source crop;
`contain` retains the complete resource in its planned frame. Alt text is written to the picture's
non-visual properties.

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

Tests verify editable run properties and hyperlinks, nested numbering, split-table header
repetition, theme-derived table styling, coordinated chart caches/workbooks, SVG retention,
deterministic GIF stills, unknown-part preservation, raw compressed reuse, one-byte overlay pulls,
configured bounds, and structural equality between direct live-overlay resolution and a
streamed/reopened PPTX. The Open XML SDK fixture includes editable text, a table, and a chart with
its nested workbook.

## Related documents

- [Semantic deck contracts](deck-engine.md) defines the validated input tuple.
- [Cortex Theme Starter compiler](deck-template.md) defines layout discovery and template policy.
- [Semantic layout and pagination](deck-layout.md) defines physical page and fragment planning.
- [OPC and ZIP substrate](opc.md) defines immutable overlays and exact streaming export.
