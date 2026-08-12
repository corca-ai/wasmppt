# High-Speed Template Injection

This document describes the implemented `PreparedTemplate` generation path for POTX and
POTM input and macro-free PPTX output.

## Prepared and warm phases

`TemplateCompiler` resolves bindings once. `PreparedTemplate::new` then verifies the plan
source hash and completeness flags, opens the ZIP index, caches only parts that can become
dirty, resolves image relationship and crop ranges, compiles repeated table-row ranges,
indexes slide IDs and relationships, and computes the static macro-removal patches.

`generate` and `generate_to` do not rescan the package. A warm generation applies data to
the cached ranges, recompresses only dirty or new entries, and raw-copies every unchanged
compressed payload. `generate_to` accepts the forward-only `OutputSink` capability and
does not require seeking. Its statistics distinguish raw copies, rewrites, and removals;
unchanged entries always record zero inflation and zero recompression.

## Text and table data

Text replacement uses the explicit `PreserveFirstRun` policy. The replacement inherits
the first participating DrawingML run's formatting; subsequent participating runs are
emptied. XML-sensitive characters are escaped and Unicode remains UTF-8.

Bindings named `table_id.field` inside the same `<a:tr>` define a repeated table row.
Call `set_table_rows("table_id", rows)` with one map per output row. The complete original
row markup is cloned for each record, so cell properties and unsupported row extensions
survive while bound text ranges change.

## Images

Set a picture shape's Alt Text Description to:

```text
wasmppt:image:hero
```

`insert_image` data supplies bytes, a safe extension, an `image/*` content type, and optional
crop values in DrawingML's 1/1000-percent units. The compiler resolves `a:blip/@r:embed`
to its slide relationship. Generation writes a deterministic media part, rewrites that
relationship target, updates or inserts `a:srcRect`, and adds or corrects the content-type
default. The old media part is removed only when the source graph proves it has one
reference. External hyperlinks and unrelated relationships remain unchanged.

## Chart data

`set_chart("ppt/charts/chart1.xml", data)` replaces complete categories and series for a
supported chart. Generation validates dimensions and finite numeric values, then updates both
the chart's text/category/number caches and the related embedded workbook worksheet in the same
output. Cache formulas and workbook cell ranges are resized consistently. See
[tables, charts, and advanced content](advanced-content.md) for read and render coverage.

## Slide inclusion and cloning

`set_slide_copies(part_name, count)` controls a source slide:

- `0` removes its presentation list item, presentation relationship, slide part,
  slide-relationship part, and content-type override;
- `1` retains it; and
- values above `1` add deterministic clones.

New slide part numbers, presentation slide IDs, and relationship IDs are selected from
the unused source sets in stable source order. Shape IDs are scoped to a slide and remain
unchanged in its clone. Layout, media, and external hyperlink relationships are shared.
A cloned slide intentionally drops the notes-slide relationship so multiple slides never
claim the same notes part; the original slide and notes remain intact.

## Macro-free conversion

The default conversion removes VBA project/data parts, VBA and signature relationship
parts, content-type declarations for removed parts, package digital signatures, and
macro Action attributes. It converts POTX, POTM, and macro-enabled presentation main
content types to the PPTX presentation main content type. The library never executes VBA.

Unknown parts and unsupported XML are not normalized. If an unrelated entry is not dirty,
its compressed bytes survive verbatim.

## Validation

Rust tests cover POTX, synthetic POTM stripping, split-run text, escaping, Unicode, image
media/relationships/crops/content types, repeated table rows, deterministic slide clones,
slide exclusion, hyperlinks, notes, opaque parts, malformed bindings, and a non-seekable
sink.

CI also downloads [Apache POI's](https://github.com/apache/poi) Apache-licensed
`bug59273.potx` at a pinned SHA-256,
converts it through the forward-only CLI path, validates ZIP and relationship structure,
and runs Microsoft `DocumentFormat.OpenXml` 3.5.1 validation over the resulting PPTX. The
validator wrapper is in `tools/openxml-validator`.

Local commands are:

```sh
cargo run -p wasmppt-cli -- validate template.potx
cargo run -p wasmppt-cli -- convert template.potx result.pptx
cargo run -p wasmppt-cli -- validate result.pptx
dotnet run --project tools/openxml-validator/OpenXmlValidator.csproj -- result.pptx
```

## Related documents

- See [template bindings and TemplatePlan](bindings.md) for authoring and cache identity.
- See the [OPC and ZIP substrate](opc.md) for raw-copy and forward-only writer contracts.
- Return to the [documentation index](index.md) for the project map.
