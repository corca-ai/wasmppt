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
- tint, shade, luminance modifier/offset, and alpha color transforms;
- master and layout background and non-placeholder shapes;
- slide placeholder inheritance from layout and master by type/index;
- nested group transform chains without converting coordinates to pixels;
- source-layer z-order, transforms, flips and 1/60000-degree rotation;
- solid/no fills, line color and width, image relationships and source crops;
- rectangle, rounded rectangle, ellipse, line, triangle, right triangle, diamond,
  parallelogram, and hexagon preset geometry;
- paragraph-preserving text collection plus effective first-run font size, family,
  color, bold/italic emphasis, horizontal and vertical alignment, and text-frame
  margins. Run-level mixed formatting remains an explicit next text-layout slice.

Unsupported graphic frames, custom geometry, gradient/pattern fills, and effects produce
explicit `ResolveDiagnostic` values. Source OOXML remains untouched, so a later backend
or fallback can recover it. The resolver never silently claims those features were drawn.

## Dependency invalidation

The OPC graph is inverted once. `invalidated_slides(part_name)` walks reverse internal
relationships and returns only slides that can reach the changed theme, master, layout,
media, font, or other part. A cache miss or conservative additional relationship may do
more work, but no cache entry is reused across an untracked dependency.

The generated two-branch fixture proves that changing theme 1 invalidates slide 1 but not
slide 2, and vice versa. Media invalidation reaches only its referencing slide.

## Binary display list

`DisplayList` contains typed command and side tables:

- clear, group push/pop, preset fill/stroke, image, and text commands;
- group transforms in EMU;
- UTF-8 string and image-resource tables;
- a fixed `WPDL` header, schema version, slide size, and table counts;
- semantic command ranges for source shape IDs, reading order, accessible names, and links;
- resolver diagnostics shared without reinterpretation by every rendering backend.

WPDL version 3 extends `DrawText` with the effective text-frame style and adds an
explicit preserved-graphic placeholder command. Version 1 and 2 scenes still decode
with documented defaults. Unsupported SmartArt, metafiles, OLE, and graphic frames no
longer become invisible regions: backends draw a labeled placeholder while retaining
the diagnostic and untouched source package.

`encode()` emits a stable little-endian format. `structural_signature()` hashes the exact
wire bytes. The same fixture has the same signature in native Rust and in a real Chrome
Wasm module Worker; the browser integration test treats a mismatch as a failure.

## Fixtures and verification

`fixtures/render/basic.pptx` exercises two independent theme/master/layout branches,
placeholder inheritance, theme transforms, group transforms, image crops, z-order,
initial geometry, and explicit unsupported diagnostics. The pinned Apache POI
`SampleShow.pptx` fixture supplies an independent real-world resolution check in CI.

```sh
cargo test -p wasmppt-layout -p wasmppt-display
cargo run -p wasmppt-cli -- resolve fixtures/render/basic.pptx 0
npm run build:wasm-hosts
npm run test:browser --workspace @corca-ai/wasmppt
```

The CLI prints command, diagnostic, parsed-part, and structural-signature counts so corpus
runs can detect both crashes and suspiciously empty output.
