# Lazy Slide Resolution and Display Lists

`wasmppt-layout` resolves one requested slide through its reachable PresentationML
dependency branch. `wasmppt-display` lowers that result to a compact backend-neutral
binary display list. Canvas and HTML/SVG backends consume this shared representation in
later layers; they do not interpret OOXML independently.

## Lazy document model

`PresentationDocument::open` indexes the ZIP and OPC relationship graph, parses the main
presentation part for slide order and dimensions, and stops. It does not parse slide,
layout, master, or theme XML and it never decodes media. `open_trace()` makes this contract
observable.

Resolving slide `n` follows only:

```text
slide n -> slide layout -> slide master -> theme
       \-> referenced media metadata (not decoded)
```

`ResolutionTrace` lists visited parts, parsed XML parts, and decoded media parts. This is
both a diagnostic API and a regression-test surface for lazy behavior.

## Resolution semantics

The first resolver implements:

- slide size in integer English Metric Units (EMU);
- theme color schemes, master/override color maps, `srgbClr`, `sysClr`, and
  `schemeClr`;
- tint, shade, luminance/saturation/hue, RGB, inversion, grayscale, and alpha transforms;
- master/layout backgrounds, background references, non-placeholder shapes, and show-master flags;
- slide placeholder inheritance, nine text-style levels, and header/footer placeholders;
- nested group transform chains without converting coordinates to pixels;
- source-layer z-order, transforms, flips and 1/60000-degree rotation;
- solid/no, linear/radial gradient and pattern fills, line color, width, dash and line ends, image
  relationships and source crops;
- nineteen common preset geometries including polygons, stars, arrows, plus and chevron;
- paragraph/run-preserving text with mixed font size, Latin/East-Asian/complex-script
  families, color and emphasis; bullets, indentation, spacing and alignment; and
  RTL, tabs, character spacing, baseline shifts, decoration, vertical flow, text-frame
  margins, vertical anchoring, wrapping and autofit mode;
- bounded move/line/quadratic/cubic/arc/close custom paths and outer shadows.

Unsupported graphic frames and effect DAGs produce
explicit `ResolveDiagnostic` values. Source OOXML remains untouched, so a later backend
or fallback can recover it. The resolver never silently claims those features were drawn.

## Dependency invalidation

The OPC graph is inverted once. `invalidated_slides(part_name)` walks reverse internal
relationships and returns only slides that can reach the changed theme, master, layout,
media, font, or other part. A cache miss or conservative additional relationship may do
more work, but no cache entry is reused across an untracked dependency.

The generated two-branch fixture proves that changing theme 1 invalidates slide 1 but not
slide 2, and vice versa. Media invalidation reaches only its referencing slide.

Live overlays expose the same part graph without a ZIP round trip. Each slide dependency
fingerprint hashes the exact bytes of the complete reachable branch, including relationship
parts. Display-list reuse therefore survives an unrelated edit but cannot survive a changed
theme, layout, master, media, chart, or other reachable dependency. See
[live editing and incremental preview](live-editing.md) for revision and fallback rules.

## Binary display list

`DisplayList` contains typed command and side tables:

- clear, group push/pop, preset fill/stroke, image, and text commands;
- group transforms in EMU;
- UTF-8 string and image-resource tables;
- a fixed `WPDL` header, schema version, slide size, and table counts;
- semantic command ranges for source shape IDs, reading order, accessible names, and links;
- resolver diagnostics shared without reinterpretation by every rendering backend.

WPDL version 7 adds typed paragraph spacing, authored normal-AutoFit hints, shape-resize AutoFit,
columns, embedded-font resources, and common editable-text effects including outlines, shadows,
glow, blur, soft edges, and reflection. Character and common automatic
numbering markers remain semantic paragraph data; picture bullets carry their lazy image
relationship and use the same bounded media resolver as ordinary images. Positioned runs expose
paragraph-local UTF-16 source ranges to selection and accessibility consumers. It retains the RTL/tab/vertical
text metadata, decoration and spacing, curved custom paths, radial gradients, patterns, and the
expanded preset set introduced by version 5. WPDL version 4 adds
paragraph/run-preserving rich text, linear gradients, bounded custom
paths, outer shadows, connectors, and arrowheads. Version 3 extends `DrawText` with the
effective text-frame style and adds an
explicit preserved-graphic placeholder command. Version 1 and 2 scenes still decode
with documented defaults. Unsupported SmartArt, OLE, and graphic frames no longer become
invisible regions: backends draw a labeled placeholder while retaining the diagnostic and
untouched source package. EMF/WMF pictures remain ordinary image commands and retain their
package part name. A separate Wasm module converts those bytes to dimensioned SVG only when
an image resolver requests them; Canvas decodes the SVG through an HTML image and DOM/SVG
uses it as an image resource.

`encode()` emits a stable little-endian format. `structural_signature()` hashes the exact
wire bytes. The same fixture has the same signature in native Rust and in a real Chrome
Wasm module Worker; the browser integration test treats a mismatch as a failure.

## Fixtures and verification

`fixtures/render/basic.pptx` is deterministically generated and exercises two independent theme/master/layout branches,
placeholder inheritance, theme transforms, group transforms, image crops, z-order,
mixed text runs, advanced geometry, gradients, shadows, a real synthesized EMF record stream,
and explicit unsupported diagnostics. `fixtures/render/corpus.json` pins the feature regions used
by Chromium visual reports. The pinned Apache POI
`SampleShow.pptx` fixture supplies an independent real-world resolution check in CI.

```sh
cargo test -p wasmppt-layout -p wasmppt-display
cargo run -p wasmppt-cli -- resolve fixtures/render/basic.pptx 0
npm run build:wasm-hosts
npm run test:browser --workspace @corca-ai/wasmppt
```

The CLI prints command, diagnostic, parsed-part, and structural-signature counts so corpus
runs can detect both crashes and suspiciously empty output.
