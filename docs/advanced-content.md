# Tables, charts, and advanced content

Status: implemented baseline with explicit capability boundaries

Office identifies a graphic frame by its DrawingML `graphicData` URI. The project follows the
finite table, chart, diagram, picture, compatibility, locked-canvas, and OLE categories documented
by [Microsoft's OOXML implementation notes](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oe376/f58e82a5-5590-4e36-b178-e12989960415).

## Tables

The lazy resolver reads grid-column widths, row heights, cell text, row and column spans, and
solid cell fills. It lowers each visible cell to the same fill, stroke, and text primitives used
by ordinary shapes, so Canvas and SVG do not own table layout logic. Template generation retains
the compiled repeated-row mechanism: it clones the original `a:tr`, patches bound cell text, and
preserves unsupported cell and row extension markup.

## Charts

Column, bar, and line charts read series names, category caches, number caches, chart direction,
and the relationship to an embedded workbook. They lower to basic shared display primitives.
Pie, area, scatter, and unknown chart caches remain readable and editable but rendering emits
`UnsupportedChartKind`.

`InjectionData.set_chart` accepts complete categories and series. Generation fails before output
when lengths differ or values are non-finite. For a supported chart part, one generation updates:

- series text, category, and numeric caches plus their formula ranges; and
- `xl/worksheets/sheet1.xml` inside the related embedded workbook.

Both parts are produced in the same package generation. Tests reopen the generated PPTX and the
nested XLSX, assert identical Korean/XML-sensitive labels and numeric values, and prove the old
cache and workbook values are gone. Open XML identifies a numeric series cache as `c:numCache`;
see Microsoft's [NumberingCache API mapping](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.drawing.charts.numberreference.numberingcache?view=openxml-3.0.1).

## Advanced content policy

Custom geometry, gradient and pattern fills, shadows and effects, SmartArt, EMF/WMF, animation,
transitions, 3D, OLE, and VBA are never silently classified as rendered. Their source bytes and
relationships survive unrelated edits. Stable diagnostic codes describe the missing rendering
capability. OLE and VBA are never activated; default POTM conversion strips prohibited active
content as documented in [high-speed template injection](injection.md).

The machine-readable [PresentationML capability matrix](../capabilities/presentationml.json)
classifies read, preserve, edit, and render support independently. Tests require every feature to
declare all four dimensions. No chart or advanced-content dependency is added to the default Wasm
bundle: parsing, lowering, and cache updates use the existing ZIP, XML, OPC, layout, display, and
template crates.

## Verification

The generated two-slide fixture includes a styled table, a column-chart cache linked to a nested
workbook, SmartArt relationships, an EMF relationship, a transition, animation timing, and 3D
properties. Rust verifies lazy reachable-part reads, table layout, chart values, workbook linkage,
atomic edits, and stable diagnostics. The real browser Wasm gate resolves slide two and verifies
that Canvas and DOM/SVG receive the same semantic kinds and diagnostic codes while rendering table
and chart primitives.
