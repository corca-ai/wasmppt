# System Architecture

Status: implemented pre-alpha baseline. Public APIs remain unstable, and the deliberate
limits documented by each subsystem still apply.

This document defines the durable architecture for `wasmppt`. Execution work and
acceptance criteria live in [GitHub Issues](https://github.com/corca-ai/wasmppt/issues).

## Mission

`wasmppt` is a loss-aware Rust engine for reading, transforming, writing, and rendering
PowerPoint Open XML packages. It targets browsers, Cloudflare Workers, and native hosts
through a shared deterministic core.

The first optimized workload is:

> Compile a `.potm` or `.potx` template once, inject different data repeatedly, and
> stream valid `.pptx` files while touching the smallest provably sufficient set of
> package parts.

The first rendering workload is:

> Open a `.pptx`, resolve only the requested slides, build a compact display list, and
> render it through Canvas 2D or DOM/SVG without reparsing the presentation.

## Design principles

1. **One core, multiple hosts.** Browser, Cloudflare, and native integrations are host
   adapters around the same Rust implementation, not independent engines.
2. **Headless first.** The core has no DOM, Canvas, Cloudflare, or JavaScript dependency.
3. **Compile repeated work.** Package discovery, template binding discovery, and stable
   relationship analysis belong in a reusable `TemplatePlan`.
4. **Prove fast paths.** A partial rewrite or cache reuse must have a complete invalidation
   boundary. When that cannot be proven, the engine takes a safe broader path.
5. **Preserve what is not understood.** Unknown parts, relationships, extension markup,
   and markup-compatibility content pass through unchanged unless an explicit policy
   removes them.
6. **Bound memory.** Large package inputs and outputs are streamed. Peak memory is a
   release contract, not an incidental metric.
7. **Measure before claiming.** Speed claims require a public corpus, named competitors,
   reproducible environments, and cold/warm percentile results.

## System context

```text
                      host application
                             |
             +---------------+----------------+
             |                                |
       template generation                slide viewing
             |                                |
             v                                v
    +------------------+             +------------------+
    | TemplateCompiler |             | SlideResolver    |
    | TemplatePlan     |             | ResolvedSlide    |
    | Injector         |             | DisplayList      |
    +---------+--------+             +---------+--------+
              |                                |
              +---------------+----------------+
                              v
                  +-------------------------+
                  | PresentationML / OPC    |
                  | relationships, types,   |
                  | themes, masters, XML    |
                  +------------+------------+
                               v
                  +-------------------------+
                  | ZIP package substrate   |
                  | lazy inflate, raw copy, |
                  | streaming writer        |
                  +-------------------------+
```

Office Open XML presentations are ZIP packages containing a graph of parts rather than
one XML tree. Presentation parts, slides, layouts, masters, themes, media, content types,
and relationship parts must therefore be modeled as a graph. The normative vocabulary
and packaging basis is [ECMA-376](https://ecma-international.org/publications-and-standards/standards/ecma-376/);
Microsoft also provides a useful [PresentationML structure overview](https://learn.microsoft.com/en-us/office/open-xml/presentation/structure-of-a-presentationml-document).

## Repository and crate boundaries

The current workspace is:

```text
crates/
  wasmppt-deck/       semantic deck, template-plan, and physical-plan contracts
  wasmppt-deck-template/ explicit Cortex Theme Starter POTX profile compiler
  wasmppt-deck-layout/ bounded semantic layout and automatic pagination
  wasmppt-deck-compose/ editable PresentationML and immutable package overlays
  wasmppt-opc/        ZIP, content types, relationships, raw entry copying
  wasmppt-xml/        namespace-aware tokenization and range-based rewriting
  wasmppt-pml/        loss-aware PresentationML typed views
  wasmppt-template/   binding schema, TemplatePlan, injection, slide cloning
  wasmppt-layout/     theme/master/layout inheritance and text/geometry resolution
  wasmppt-metafile/   bounded host-agnostic EMF/WMF-to-SVG conversion
  wasmppt-display/    backend-neutral display list
  wasmppt-native/     native file ReadAt and forward-only output capabilities
  wasmppt-wasm/       narrow wasm-bindgen boundary
  wasmppt-metafile-wasm/ optional independently loaded metafile boundary
  wasmppt-cli/        inspection, validation, corpus, and benchmark commands

packages/
  wasmppt/            browser package and Web Worker adapter
  wasmppt-worker/     Cloudflare Workers adapter

examples/
  browser-dogfood/

benchmarks/             reproducible workloads, competitors, and release budgets
capabilities/           machine-readable PresentationML support matrix
fixtures/               generated and pinned compatibility/render inputs
tools/                  external validation adapters
```

Core crates MUST compile and test without `wasm-bindgen`, `web-sys`, `js-sys`, a DOM,
or a JavaScript runtime. CI will enforce this import and dependency boundary. See
[Runtime host adapters](hosts.md) for the implemented host APIs and their executable
shared-fixture contract.

## Semantic deck pipeline

Host authoring adapters convert their source language and authorized resources into a
source-backed `DeckSpec`. The core contract represents logical slides, semantic content,
rich-text runs, stable source ranges, split policies, table-column alignment, hidden slides, and
binary resources. Resource dimensions supplied by a host are hints that the core validates or
derives from bounded bytes; the contract does not parse Markdown or call a host resource API.

```text
host source adapter
        |
        v
    DeckSpec -----> wasmppt-deck-layout -----> DeckPlan
        |                                      |
        |                                physical pages,
        |                                topology slots,
        |                                regions, fragments,
        |                                type and fit choices
        v                                      v
source diagnostics             wasmppt-deck-compose
                                          |
                                          v
                              PresentationOverlay / PPTX
```

`DeckTemplatePlan` is the template-side input to the planner. It owns the exact page
geometry, stable template and cache identity, role-specific layouts, resolved placeholder
regions and text hierarchy, theme fonts and colors, preserved template assets, and template
diagnostics. `wasmppt-deck-template` compiles that value from the explicit Cortex Theme
Starter POTX profile without inspecting example slides or visible names. A `DeckPlan` names
both its source spec and template plan so a consumer
cannot accidentally compose against a different revision or POTX profile.

`wasmppt-deck` defines the contracts, bounded binary codecs, and validators;
`wasmppt-deck-layout` implements host-neutral semantic candidate generation, exact-font or
observable fallback measurement, and automatic pagination. The validators prove that renderable
source fragments appear once and in source order, remain on their logical slide and compatible
template region, use a valid topology/slot assignment and finite in-page geometry, and have stable
continuation metadata.
`wasmppt-deck-compose` projects the validated tuple into editable slide XML, native tables and
supported 2D charts with coordinated embedded workbooks, and an immutable
`PresentationOverlay`. Only changed package parts are materialized; untouched template entries
remain raw compressed bytes and exact export is drained through bounded pulls. See
[semantic deck contracts](deck-engine.md) for the public data and wire contract and
[Cortex Theme Starter compiler](deck-template.md) for the POTX profile boundary, and
[semantic layout and pagination](deck-layout.md) for planner policy, and
[editable deck composition](deck-compose.md) for output semantics.

## Package substrate

### Part graph

An opened package is an indexed graph keyed by compact `PartId` and `RelationshipId`
values. File names and namespace URIs are interned. Relationship cycles are valid and
MUST NOT be traversed recursively without cycle detection.

Each part advances lazily through states similar to:

```rust
enum PartState {
    RawCompressed,
    Inflated,
    Parsed,
    Dirty,
}
```

- `RawCompressed` retains the original compressed payload, CRC, compression method, and
  metadata needed to emit a new ZIP entry without inflation or recompression.
- `Inflated` exists only when a consumer needs uncompressed bytes.
- `Parsed` exposes a typed or token-indexed view while retaining unrecognized content.
- `Dirty` records the minimum output scope that must be serialized again.

The output writer rebuilds local headers and the central directory but reuses compressed
payloads for unchanged entries. Already-compressed media is stored or copied rather than
deflated again. Compression implementations remain behind an internal interface and are
selected by measured host-specific results.

### Host I/O ports

The core depends on capabilities, not a filesystem:

```rust
trait ReadAt {
    fn len(&self) -> u64;
    fn read_at(&self, offset: u64, buffer: &mut [u8]) -> Result<()>;
}

trait OutputSink {
    fn position(&self) -> u64;
    fn write_all(&mut self, bytes: &[u8]) -> Result<()>;
}
```

Memory buffers, native files, HTTP range sources, R2 objects, browser `Blob` objects,
and streaming responses are adapter concerns. An asynchronous host adapter obtains or
windows bytes before entering this synchronous core capability. The package layer MUST
NOT assume seekable output or buffer the complete result.

### Loss-aware XML

The hot path uses a namespace-aware pull tokenizer and byte ranges rather than a complete
schema-generated object tree. Typed views are handwritten for supported PresentationML
and DrawingML features. Unknown attributes, elements, namespaces, `mc:AlternateContent`,
and extension lists remain attached to their source ranges so an unrelated edit does not
delete future-Office content.

Transitional and Strict PresentationML use the same namespace-aware bounded mutation path.
Tests prove that a bound-text edit retains Strict conformance and unknown compressed bytes;
this does not claim every Strict-only authoring feature.

## Compiled-template generation

### Binding model

Stable bindings use shape metadata or a project-owned manifest. Visible `{{name}}`
tokens are a convenience fallback because PowerPoint may split visible text across
multiple `a:r` runs.

The compiler resolves bindings to concrete part IDs, token ranges, style sources, and
relationship actions. The implemented generation inputs are:

- text replacement with explicit run-style policy;
- image replacement with crop and relationship policy;
- repeated rows identified by `table_id.field` text bindings, with fail/clip/shrink/continue
  overflow and explicit continuation capacity;
- deterministic slide exclusion and cloning by slide part name;
- complete category and series replacement for a supported named chart or chart part, including its cache
  and related embedded workbook;
- conditional/repeated semantic shapes with deterministic IDs and rich text/basic style; and
- writable safe hyperlinks, image-fit policy, and notes addressed by slide part.

SmartArt editing remains unsupported because it requires coordinated diagram and fallback-image updates.

### TemplatePlan

`TemplateCompiler` produces an immutable, serializable `TemplatePlan`. A plan contains:

- the source template hash and plan schema version;
- macro and output conversion policy;
- the OPC part index and relationship dependency graph;
- binding selectors resolved to token or structural locations;
- ID allocation and slide-cloning rules;
- raw-copy, conditional-copy, and rewrite entry sets;
- explicit completeness flags for every optimization assumption.

The cache identity includes the plan schema, engine version, template hash, binding schema,
and macro policy. A mismatch
is a cache miss, never a warning.

### POTM/POTX to PPTX policy

Creating a presentation from a template is a semantic conversion, not an extension
rename. Under the default `MacroPolicy::Strip`, conversion MUST:

- remove VBA project, VBA data, and related signature parts;
- remove relationships and content type overrides for removed parts;
- remove macro action settings that cannot exist in `.pptx`;
- change the macro-enabled template main content type to the presentation content type;
- detect orphaned relationships and parts;
- invalidate or remove package signatures affected by the transformation.

This matches Microsoft's statement that `.pptx` cannot contain a VBA project or Action
settings and that presentations created from `.potm` do not inherit them. See the
[Office XML extension reference](https://learn.microsoft.com/en-us/office/compatibility/xml-file-name-extension-reference-for-office).

A future preserve-macro policy may emit `.pptm`; it MUST NOT label a macro-bearing package
as `.pptx`. The engine never executes macros.

### Fast-path proof

Every render operation selects one of these conceptual strategies:

```rust
enum WriteStrategy {
    BytePatch { touched_parts: Vec<PartId> },
    PartRewrite { touched_parts: Vec<PartId> },
    FullRewrite { reason: FullRewriteReason },
}
```

Byte patching is allowed only when XML boundaries, encoding, escaping, CRC updates, and
downstream references are proven safe. Otherwise the affected part is rewritten. If the
part dependency boundary is incomplete, the package takes the conservative full path.

## Live editing pipeline

`LiveSession` retains complete Generation API v2 data, an exact monotonically increasing revision,
and an immutable `PreparedOverlay`. The overlay is a virtual OPC source: layout reads rewritten
parts and unchanged source-ZIP parts through one capability, while export streams that same view.
No preview revision is serialized and reopened.

A delta commits only after injection and logical graph validation. The compiled plan maps its
changed binding IDs to potentially affected parts; unrelated materialized parts share immutable
bytes with the preceding revision. Reverse OPC dependencies produce affected slide indices, and a
hash of every reachable dependency forms the slide-scene cache key. Topology-changing slide
operations or any incomplete proof choose the broad fallback.

The browser Worker owns session mutation and display-list caches. The main thread coalesces input
once per animation frame, renders visible invalidated slides first, retains unrelated canvases, and
exports an immutable current revision during idle time. Content-addressed, byte-budgeted caches
cover resources, decoded images, text measurements, and rich-text layouts. Mutable sessions never
enter the Cloudflare isolate-global prepared-plan cache. See
[live editing and incremental preview](live-editing.md) for the executable contract.

A host MAY fan one editor delta out to several independent live sessions. Each template keeps its
own revision, overlay, invalidation set, scene cache, and exact-revision export; the host owns the
coordination policy and must not combine their mutable package state.

## Rendering pipeline

### Resolution

Rendering never consumes raw slide XML directly. `SlideResolver` combines:

```text
theme -> slide master -> slide layout -> slide -> local overrides
```

It resolves placeholder inheritance, color maps and transforms, text styles, geometry,
group transforms, fills, strokes, image crops, and z-order into `ResolvedSlide`.
Coordinates remain integer English Metric Units (EMU) until the backend conversion step
to avoid accumulated layout error.

Slides and media are resolved lazily. Changing a theme invalidates all slides that depend
on it; changing a layout invalidates only slides reachable from that layout; changing a
slide-local image invalidates only its consumers. The dependency graph is the source of
truth for this invalidation.

### Display list

`ResolvedSlide` is lowered to a compact backend-neutral display list containing commands
such as transforms, clips, path fills/strokes, images, and shaped text. Rust transfers a
binary command buffer and typed side tables across the Wasm boundary rather than a large
JSON object graph.

### Backends

- **Canvas 2D** is the primary interactive and thumbnail backend. Parsing and resolution
  run in a Web Worker; drawing and target-font measurement run in the browser context that
  owns the canvas.
- **DOM/SVG** is an output-only browser serializer. It uses positioned HTML for selectable
  text and inline SVG for PowerPoint geometry, then emits network-closed standalone HTML for
  HTML delivery and browser PDF printing. Interactive preview remains Canvas.
- **Software rasterization** is optional and deferred. It is the only possible raster
  route in hosts without Canvas, but it adds font, image-decoding, binary-size, and memory
  costs.

Cloudflare Workers do not provide a browser DOM or Canvas. The current Worker adapter exposes
streaming package generation only. Display-list resolution and Canvas or DOM/SVG projection
belong to the browser package; the Worker adapter does not pretend to offer browser rendering.

### Fonts and text

Pixel fidelity requires the intended fonts. `FontResolver` handles Latin, East Asian, and
complex-script theme slots, exact supplied web fonts, substitution policy, and an observable
exact-versus-fallback result. Browser-font measurement is batched to reduce Wasm/JavaScript
crossings. A deterministic font-bytes shaping path may be added later, but is not required for
the first renderer.

## Runtime adapters

### Browser

- Transfer the input `ArrayBuffer` to a parsing Worker instead of cloning it.
- Keep immutable plans and scenes keyed by document revision.
- Coalesce superseded render requests and discard stale revision results.
- Render only visible slides and prefetch a bounded neighbor window.
- Persist versioned `TemplatePlan` artifacts in IndexedDB when requested by the host.

### Cloudflare Workers

- Use the same Wasm core with a Worker-specific I/O and stream adapter.
- Keep immutable hot-template plans in a byte-budgeted cache; never store request scratch
  state globally.
- Stream response chunks and enforce lower internal memory budgets than the platform cap.
- Do not depend on threads. SIMD-specific builds require a measured scalar fallback and
  are introduced only when benchmarks prove a material gain.

Cloudflare currently documents a 128 MB per-isolate memory limit, support for Wasm SIMD,
and no Wasm threading. These details are platform facts and must be rechecked before a
release changes its runtime profile. See [Workers limits](https://developers.cloudflare.com/workers/platform/limits/)
and [Workers WebAssembly](https://developers.cloudflare.com/workers/runtime-apis/webassembly/).

### Native

The native facade is the reference environment for fuzzing, profiling, compatibility
inspection, and deterministic benchmarks. It uses the same core and produces artifacts
comparable with browser and Cloudflare results.

## Public API direction

The conceptual JavaScript API keeps expensive work explicit:

```ts
const plan = await WasmPpt.compileTemplate(templateBytes, {
  output: 'pptx',
  macroPolicy: 'strip',
})

await plan.renderTo(data, writableStream)

const deck = await WasmPpt.open(pptxBytes)
const scene = await deck.resolveSlide(0)
canvasRenderer.draw(scene, canvas)

const offline = await serializeDeckSessionToHtml(client, deckSession)
download(new Blob([offline.bytes], { type: 'text/html' }))
```

The actual API is not stable. It will prefer opaque handles, typed arrays, transferables,
and streaming sinks over per-shape Wasm calls or JSON serialization.

## Determinism and caching

Deterministic mode fixes ZIP entry ordering and timestamps, ID allocation, namespace and
attribute emission, compression implementation and level, and serialization policy.
Given identical engine bytes, template, data, and options, it aims to produce byte-identical
PPTX output across native and supported Wasm hosts.

Content-addressed artifacts include their producer identity and schema version. Browser
and Cloudflare caches may share an artifact only after cross-host parity has been verified. CI
feeds one checked-in WPPD payload to the native buffered sink and the browser/workerd bounded pull
paths, then requires complete PPTX byte equality. Its raw report retains each length and SHA-256;
the mismatch classifier reports the first divergent ZIP entry and byte category.
An incomplete dependency manifest is never sufficient for reuse.

## Performance contract

The library reports phase timings and work counters rather than only a total duration:

```text
zip.centralDirectoryMs     zip.inflateMs
xml.scanMs                 template.compileMs
template.injectMs          opc.graphUpdateMs
zip.compressMs             zip.writeMs
render.resolveMs           render.displayListMs
wasm.inputCopyBytes        wasm.outputCopyBytes
memory.peakBytes           parts.rawCopied
parts.inflated             parts.recompressed
```

CI budgets cover Wasm binary size, cold template compilation, warm injection p50/p95,
first-visible-slide rendering, representative all-slide rendering, peak memory, and output size.
Browser reports retain render stages and bounded cache residency separately from generation timings.
The central warm-path invariant is that unchanged entries are not inflated or recompressed.

A public performance claim names the workload and competitors and publishes hardware,
browser/runtime versions, corpus hashes, compression options, sample count, and raw
results. “Fastest” without a bounded workload is not an accepted claim.

## Correctness and security

The package reader treats every document as untrusted input and applies configurable,
safe defaults for:

- compressed and inflated bytes per part and package;
- compression ratio and overlapping-entry checks;
- part count, relationship count, XML depth, attributes, and token count;
- path normalization and traversal prevention;
- image byte limits, PNG/JPEG dimensions, EXIF orientation, and decoded pixel budgets;
- relationship cycles and external targets;
- DTD, entity, and external-resource behavior;
- macro and signature handling.

Parsing, validation, and conversion return stable machine-readable diagnostic codes.
Unsupported rendering features are reported explicitly and may use a recorded fallback;
they are not silently approximated as fully supported.

## Text-fidelity architecture

WPDL v10 adds source-backed semantic IDs, source ranges, explicit reading order, and hit-test
bounds. WPDL v9 keeps text layout backend-neutral and marks live-edited slide parts for AutoFit
recomputation; v8 introduced distinct inner-shadow paint. Rust resolves paragraph/run inheritance, typed point or
percentage spacing, authored normal-AutoFit hints, shape-resize mode, columns, numbering markers,
and common 2D text paint: outlines, inner/outer shadows, glow, blur, soft edges, and reflection.
Canvas and DOM/SVG consume one positioned run plan, so line
membership, effective shape bounds, source ordering, and column transitions cannot diverge by
backend. Layout is bounded to 16 columns, 12 AutoFit iterations, finite geometry, and byte-budgeted
host caches.

The same contract carries solid/gradient/pattern text paint plus eight bounded 2D arch, wave,
inflate, and deflate warp presets. These remain semantic text in DOM mode; effects use bounded
browser-native approximations while unsupported 2D effect-DAG nodes fall back to readable unwarped
text. All 3D camera, material, lighting, bevel, and extrusion
data remains preserved but outside the renderer.

Presentation-level embedded-font entries are carried as lazy package-part names rather than font
bytes in the display list. The browser host reads a part only on first use, enforces a 32 MiB default
limit, inspects OpenType `OS/2.fsType`, and refuses restricted or structurally unknown faces before
`FontFace` registration. Generation-only Wasm therefore does not instantiate font machinery.
The independently emitted `wasmppt-shaper-wasm` module uses HarfRust over the same accepted bytes.
It returns backend-neutral glyph advances, offsets, IDs, UTF-8 clusters, and safe-break flags;
bounded host caches key the result by font-byte fingerprint, face, language, script, OpenType
features, variation coordinates, direction, and text. Canvas and DOM still
paint semantic text with the registered face while consuming those exact advances for shared layout.

The optional text module supplies UAX #14 opportunities and caches plans by rule version, language
hint, and exact text under the same byte budget as shaped runs. The built-in fallback never splits the covered extended grapheme
forms (combining sequences,
variation selectors, emoji modifiers, ZWJ sequences, and regional-indicator pairs), honors NBSP,
WJ, ZWSP, soft hyphen, preserved newlines, and common Japanese prohibited-start/end punctuation,
and uses a documented dictionary-less fallback for Thai, Lao, and Khmer. Advanced bidi and vertical
writing remain outside this slice.

## Verification strategy

The verification environment is part of the architecture:

- unit and property tests for ZIP, relationships, IDs, XML escaping, and style resolution;
- fuzzing for ZIP, XML, relationship graphs, geometry, and image metadata;
- round-trip tests proving unknown markup and raw parts survive unrelated edits;
- macro-removal tests proving `.pptx` contains no prohibited VBA or Action content;
- Open XML validation and PowerPoint “opens without repair” compatibility checks;
- PowerPoint-ground-truth visual regression with Korean/CJK, RTL, emoji, and missing-font
  cases, with LibreOffice and Keynote as secondary consumers;
- native/browser/Cloudflare structural parity and deterministic byte parity;
- release-blocking size, latency, throughput, and memory budgets.

Fixtures must have recorded provenance and redistribution terms. Generated fixtures are
preferred; third-party decks and fonts are not committed without an affirmative license.

## Non-goals for the first release

- Executing VBA or other active content.
- Full OOXML schema object generation.
- Pixel-perfect rendering without the source fonts.
- Broader optional EMF/WMF playback and existing SmartArt fallback selection. Animation, 3D,
  and native SmartArt layout/rendering remain explicit non-goals.
- A full presentation editor UI.
- Thread-dependent correctness or performance.

## Implemented delivery slices

The pre-alpha baseline was delivered vertically so every stage produced a usable artifact:

1. package inspection, raw-copy writer, limits, and deterministic ZIP output;
2. compiled `.potm`/`.potx` bindings and macro-free `.pptx` generation;
3. lazy slide resolution and basic text/image/shape display lists;
4. Canvas 2D, followed by DOM/SVG;
5. tables, charts, advanced geometry, and higher-fidelity text;
6. published compatibility and performance evidence for release claims.

Further task breakdown and progress live in GitHub Issues, not this document.

## Documentation

- Return to the [documentation index](index.md).
- Follow the [documentation guide](metadoc.md) when changing this design.
