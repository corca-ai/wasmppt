# Browser Canvas renderer

Status: implemented baseline

The browser adapter keeps package indexing and lazy slide resolution inside an ES-module
Web Worker. The main thread receives only a transferable WPDL display list for the requested
slide. `WasmpptWorkerClient.openPresentation` returns an opaque handle and slide count;
`resolveSlide` resolves one slide from that retained document; `releasePresentation` releases
the compressed package and its relationship graph.

## Rendering pipeline

`decodeDisplayList` validates the versioned little-endian WPDL stream, count bounds, resource
indices, and group-stack balance before execution. `CanvasDisplayListRenderer` then executes
the common display-list semantics on Canvas 2D:

- preset paths, fills, strokes, rotation, flips, and nested group transforms;
- relationship-addressed images with PowerPoint source cropping;
- resolved text through an explicit font resolver and target-canvas measurement;
- deterministic cache eviction and disposal for decoded images.

Canvas 2D and `OffscreenCanvasRenderingContext2D` expose the same core drawing and text
measurement operations. The renderer currently targets an on-screen 2D context, while its
display executor is deliberately independent of Worker package parsing. See the
[Canvas 2D reference](https://developer.mozilla.org/en-US/docs/Web/API/CanvasRenderingContext2D)
and [Offscreen Canvas 2D reference](https://developer.mozilla.org/en-US/docs/Web/API/OffscreenCanvasRenderingContext2D).

## Fonts and line wrapping

`FontResolver` distinguishes Latin, East Asian, and complex-script runs. A caller may provide
theme font slots, exact `FontFace` sources, substitutions, and documented fallbacks. The
resolver reports whether the requested face is exact instead of disguising a fallback.
Supplied web fonts are loaded before canvas use through the
[CSS Font Loading API](https://developer.mozilla.org/en-US/docs/Web/API/CanvasRenderingContext2D/font).

`measureTextBatch` groups requests by the final CSS font and measures them on the target
canvas. Korean, Han, Hiragana, and Katakana text may wrap at character boundaries; newline
and Latin whitespace behavior remain deterministic. Tests cover an exact supplied-font host,
documented missing-font fallback, Korean wrapping, and mixed-order batched measurement.

## Lazy viewer and resource ownership

`VirtualizedCanvasViewer.setVisibleSlides` mounts canvases only for the visible indices. A new
viewport revision aborts stale resolution and drawing work. Neighbor scenes may be prefetched,
but the scene LRU and decoded-image LRU both have explicit byte budgets. Evicted image objects
are closed when their host resource supports `close()`. `dispose()` aborts work, removes every
canvas, clears both caches, and releases listeners owned by the mounted canvas elements.

The viewer intentionally accepts visible slide indices from the application. This avoids
installing a second scroll policy or an immortal global `IntersectionObserver`; React, vanilla
DOM, and custom viewers can feed the same bounded primitive.

## Telemetry and verification

Each render reports separate durations for slide resolution, font loading and measurement,
display execution, and media decode, plus the command count. The browser integration gate
runs the real Wasm module in a module Worker, transfers a two-slide PPTX, resolves only slide
zero, draws shapes, nested transforms, fills, strokes, text, an image crop, verifies cache
cleanup and bounded mounted canvases, and records a pixel fingerprint. Higher-fidelity visual
baselines and per-slide tolerance reports belong to the compatibility-gate slice.

## Current boundary

The baseline supports the WPDL v2 commands and semantic side tables produced by the lazy resolver. Advanced geometry,
effects, chart, table, and SmartArt coverage is tracked independently and remains explicitly
diagnosed. Canvas output is a bitmap and therefore is not the accessibility surface; the
secondary DOM/SVG backend owns selectable text, reading order, links, and alternative text.
