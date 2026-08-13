# High-Speed Template Injection

This document describes the implemented `PreparedTemplate` generation path for POTX and
POTM input and macro-free PPTX output.

## Prepared and warm phases

`TemplateCompiler` resolves bindings once. `PreparedTemplate::new` then verifies the plan
source hash and completeness flags, opens the ZIP index, caches only parts that can become
dirty, resolves image relationship and crop ranges, compiles repeated table-row ranges,
indexes slide IDs and relationships, and computes the static macro-removal patches.

`generate`, `generate_to`, and `generate_cursor` do not rescan the package. A warm generation applies data to
the cached ranges, recompresses only dirty or new entries, and raw-copies every unchanged
compressed payload. `generate_to` accepts the forward-only `OutputSink` capability and
does not require seeking. Its statistics distinguish raw copies, rewrites, removals,
total dirty bytes, the largest dirty entry, and the largest emitted host chunk;
unchanged entries always record zero inflation and zero recompression.

`generate_cursor` is the bounded pull path used by Wasm hosts. Each `pull(maximum_bytes)` call
returns at most the requested bytes. Unchanged compressed payloads are copied directly from the
source package into the returned chunk; no complete output archive is retained. Dirty entry bytes
are prepared as a working set and each entry's compressed form is retained only while that entry
is drained. The final central directory is emitted last.

## Structured Generation API v2

Browser callers pass one `GenerationData` object to `generateStream` or `generate`:

```ts
{
  text: { title: 'Q3 report' },
  images: {
    hero: { bytes: pngBytes, extension: 'png', contentType: 'image/png' }
  },
  tables: { metrics: [{ label: 'Latency', value: '12 ms' }] },
  tablePolicies: { metrics: { maximumRows: 12, overflow: 'shrink' } },
  slides: { 'ppt/slides/slide2.xml': 2 },
  charts: {
    sales: {
      categories: ['Q1', 'Q2'],
      series: [{ name: 'Revenue', values: [10, 14] }]
    }
  },
  semanticShapes: {
    callout: {
      visible: true,
      copies: 2,
      richText: [{ text: 'Priority', bold: true, color: 'FF0000' }],
      hyperlink: 'https://example.com',
      fillColor: 'FFF2CC'
    }
  },
  notes: { 'ppt/slides/slide1.xml': 'Speaker-only context' }
}
```

The adapter deterministically encodes this object into the little-endian `WPPD` binary payload
schema. Image bytes cross the Wasm boundary directly, without JSON or base64. Rust and TypeScript
share a golden WPPD v2 payload test; Rust continues to decode WPPD v1. The decoder rejects unknown schema versions, excessive counts,
non-finite chart values, truncation, and trailing data. A flat string record remains a temporary
text-only compatibility shorthand.

Cloudflare clients send these same bytes as the body of an R2-template generation request with
media type `application/vnd.corca.wasmppt.injection-v2`; no host-specific payload translation is
required.

## Text and table data

Text replacement uses the explicit `PreserveFirstRun` policy. The replacement inherits
the first participating DrawingML run's formatting; subsequent participating runs are
emptied. XML-sensitive characters are escaped and Unicode remains UTF-8.

Bindings named `table_id.field` inside the same `<a:tr>` define a repeated table row.
Call `set_table_rows("table_id", rows)` with one map per output row. The complete original
row markup is cloned for each record, so cell properties and unsupported row extensions
survive while bound text ranges change.

`tablePolicies` makes overflow intentional. `fail` rejects the request transactionally,
`clip` emits only the declared capacity, and `shrink` keeps all rows while scaling cloned row
heights to the declared capacity. A zero capacity is invalid. Continuation-slide pagination is
not implicit; authors use the existing deterministic slide repetition API when a table must span
slides.

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
reference. `fit` selects preserve, cover, or contain behavior; an explicit crop remains
authoritative. External hyperlinks and unrelated relationships remain unchanged.

## Semantic shapes and notes

`semanticShapes` addresses an existing Alt Text/manifest/visible-token binding ID rather than
an OOXML part or shape ID. Generation validates every operation before output, can exclude or
repeat the complete shape, allocates deterministic slide-local shape IDs, writes mixed rich-text
runs, updates an existing safe hyperlink relationship, and changes an existing solid fill.
Unknown markup inside repeated shapes remains intact. `notes` addresses a source slide part and
updates its related notes-slide text without cloning or sharing notes accidentally.

## Chart data

Set a chart graphic frame's Alt Text Description to `wasmppt:chart:sales`, then call
`set_chart("sales", data)`. The compiler resolves that stable authoring name to the chart
relationship and part once; raw part names remain accepted as a low-level compatibility path.
Generation validates dimensions and finite numeric values, then updates both
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
`macroPolicy: 'reject'` instead rejects a package containing those artifacts or a macro action
during preparation with the stable `WasmpptMacroPresentError` name. Macro-preserving PPTM output
is not exposed until its package semantics can be implemented and validated end to end.

Unknown parts and unsupported XML are not normalized. If an unrelated entry is not dirty,
its compressed bytes survive verbatim.

Generated slides always have a deterministic opaque background for compatibility with consumers
that render an unspecified PresentationML background as transparent. The generator preserves an
existing slide, layout, or master background. Only when the complete inheritance chain has no
`p:bg` does it add an explicit white background at the available master or layout level, falling
back to the generated slide when no inheritance part exists.

## Validation

Rust tests cover WPPD v1/v2, POTX, Strict POTX targeted editing, synthetic POTM stripping,
explicit and inherited background preservation, missing-background defaulting, split-run text, escaping, Unicode, image
media/relationships/crops/content types, repeated table rows, deterministic slide clones,
slide exclusion, hyperlinks, notes, opaque parts, malformed bindings, and a non-seekable
sink, semantic conditions/repetition/rich text, named chart bindings, explicit table overflow,
notes edits, and buffered/streaming parity.

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

- See [live editing and incremental preview](live-editing.md) for delta merge, overlay preview,
  invalidation, and current-revision export.
- See [template bindings and TemplatePlan](bindings.md) for authoring and cache identity.
- See the [OPC and ZIP substrate](opc.md) for raw-copy and forward-only writer contracts.
- Return to the [documentation index](index.md) for the project map.
