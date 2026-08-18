# Tables, charts, and advanced content

Status: implemented baseline with explicit capability boundaries

Office identifies a graphic frame by its DrawingML `graphicData` URI. The project follows the
finite table, chart, diagram, picture, compatibility, locked-canvas, and OLE categories documented
by [Microsoft's OOXML implementation notes](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oe376/f58e82a5-5590-4e36-b178-e12989960415).

## Tables

The lazy resolver reads grid-column widths, row heights, rich cell text, horizontal/vertical merge topology,
banding flags, solid cell fills, per-side borders, and the cell-owned text margins and vertical
anchor from `a:tcPr`. Cell properties override `a:bodyPr` for those values, matching DrawingML's
table ownership boundary. It lowers each visible cell to the same
fill, stroke, and text primitives used
by ordinary shapes, so Canvas and SVG do not own table layout logic. Template generation retains
the compiled repeated-row mechanism: it clones the original `a:tr`, patches bound cell text, and
preserves unsupported cell and row extension markup. Generation exposes transactional `fail`,
bounded `clip`, height-preserving `shrink`, and capacity-driven `continue` overflow policies.
Continuation partitions rows without browser font measurement and clones the complete authored
source slide through the same deterministic topology path as explicit slide copies. It rejects a
second continuation table or an explicit copy request on the same source slide before emission.

Deck composition creates native tables directly from planned row slices. It repeats declared
header rows only on derived slices and derives a restrained, contrast-safe table treatment from
the compiled theme: dark text, a lightly tinted and bold header, subtle banding and borders, padded
cells, and vertical centering. Equal deterministic row and column allocation is the explicit
fallback because the semantic deck contract carries no author-invented row or column measurements.

## Charts

Column, bar, line, pie, doughnut, area, scatter, bubble, and two-dimensional combination charts read series/category/numeric
caches, grouping, title, legend, and the relationship to an embedded workbook. The supported 2D
families lower to shared display primitives; 3D families remain explicitly unsupported.

`InjectionData.set_chart` accepts complete categories and series. Generation fails before output
when lengths differ or values are non-finite. For a supported chart part, one generation updates:

- series text, category, and numeric caches plus their formula ranges; and
- `xl/worksheets/sheet1.xml` inside the related embedded workbook.

Both parts are produced in the same package generation. Tests reopen the generated PPTX and the
nested XLSX, assert identical Korean/XML-sensitive labels and numeric values, and prove the old
cache and workbook values are gone. Open XML identifies a numeric series cache as `c:numCache`;
see Microsoft's [NumberingCache API mapping](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.drawing.charts.numberreference.numberingcache?view=openxml-3.0.1).
Chart updates may use stable `wasmppt:chart:<id>` Alt Text metadata, so host applications do not
depend on producer-assigned chart part names.

The shared editable-chart projector also creates new bar, column, line, area, pie, doughnut, and
scatter chart parts for deck composition. It starts from a bounded host-neutral skeleton and then
uses the same cache/formula and nested-workbook mutation functions as template injection. Scatter
categories are parsed as finite numeric X values. Composer output therefore cannot expose a chart
revision whose cache and workbook disagree.

## Advanced content policy

Custom geometry, gradient and pattern fills, shadows and effects, SmartArt, animation,
transitions, 3D, OLE, and VBA are never silently classified as rendered. SmartArt can render an
authored picture fallback only when one `mc:AlternateContent` container proves the association
between one diagram choice and one fallback picture. The fallback follows the ordinary bounded,
lazy image path, including crop, external-resource policy, and visible decode failure behavior.
The engine does not search unrelated slide or diagram images and does not claim native SmartArt
layout or editing. If that association is absent or ambiguous, source bytes and relationships
survive unrelated edits and a stable diagnostic labels the placeholder instead of leaving blank
space. OLE and VBA are never activated; default POTM conversion strips prohibited active content
as documented in [high-speed template injection](injection.md).

EMF and WMF pictures use bounded host-agnostic parsing and SVG playback. The browser adapter
loads the optional converter Wasm on first metafile access, then caches the decoded image through
the normal renderer cache. Common GDI records render in Canvas and DOM/SVG; malformed, oversized,
or currently unsupported record streams fail the image resolver and retain the ordinary visible
image-unavailable fallback. The source package bytes are never rewritten by preview conversion.

SVG and GIF pictures use the same lazy image resolver. SVG preview rejects active or external
content before decode; GIF preview deterministically uses its first frame. Audio/video bytes are
never activated: an existing poster image may render and otherwise the semantic region remains a
non-playing placeholder. Strict namespace/relationship variants use the same bounded OPC and PML
paths, and template/macro/slideshow formats retain explicit conversion or preservation policy.

The machine-readable [PresentationML capability matrix](../capabilities/presentationml.json)
classifies read, preserve, edit, and render support independently. Tests require every feature to
declare all four dimensions. No chart or advanced-content dependency is added to the primary Wasm
bundle: parsing, lowering, and cache updates use the existing ZIP, XML, OPC, layout, display, and
template crates. The metafile parser and SVG player live in their own lazy Wasm artifact.

## Verification

The generated fixture includes a styled table, a column-chart cache linked to a nested workbook,
SmartArt data/drawing/extensions plus an associated PNG fallback, an EMF relationship, a
transition, animation timing, and 3D properties. Rust verifies lazy reachable-part reads, exact
SmartArt payload preservation, fallback bounds/crop/z-order, missing and malformed fallback
behavior, table layout, chart values, workbook linkage, atomic edits, and stable diagnostics. An
Apache POI SmartArt fixture independently verifies that a deck without an associated fallback
stays diagnostic-only even when diagram content contains other images. The real browser Wasm gate
resolves slide two and verifies that Canvas and DOM/SVG use the same image command while rendering
the fallback, table, chart, and converted EMF primitives.
