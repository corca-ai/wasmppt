# Browser Canvas renderer

Status: implemented baseline

The browser adapter keeps package indexing and lazy slide resolution inside an ES-module
Web Worker. The main thread receives only a transferable WPDL display list for the requested
slide. `WasmpptWorkerClient.openPresentation` returns an opaque handle and slide count;
`resolveSlide` resolves one slide from that retained document; `releasePresentation` releases
the compressed package and its relationship graph. `presentationResource` inflates one resource
part named by a resolved display list, allowing image decoding to stay lazy instead of copying all
media when the deck is opened. `presentationMetafileSvg` asks an independently loaded converter
Wasm module for browser-decodable SVG only when an EMF or WMF resource is visible.

## Rendering pipeline

`decodeDisplayList` validates the versioned little-endian WPDL stream, count bounds, resource
indices, and group-stack balance before execution. `CanvasDisplayListRenderer` then executes
the common display-list semantics on Canvas 2D:

- preset paths, fills, strokes, rotation, flips, and nested group transforms;
- linear/radial gradients, patterns, curved bounded custom paths, outer shadows, connectors, and line ends;
- relationship-addressed images with PowerPoint source cropping;
- shared WPDL v11 paragraph/run layout for mixed styles, run-owned hyperlinks, script-specific theme fonts,
  RTL, tabs, vertical flow, decoration, baseline/character spacing, bullets, indentation,
  wrapping, autofit, alignment and text-frame anchoring;
- deterministic cache eviction and disposal for decoded images.

Backend-neutral preset paths and shape/group transform matrices live in `scene/geometry`.
Canvas only projects that shared plan through the Canvas path and transform APIs; it does not
maintain a second preset-geometry table. The byte-budgeted cache is independently owned by
`cache/byte-budget-lru`, while the WPDL decoder remains a pure byte-to-scene operation that does
not read `document`, construct a canvas, or test browser capabilities. Existing symbols continue
to be re-exported from the package root and `canvas.js` compatibility entry point.

Canvas 2D and `OffscreenCanvasRenderingContext2D` expose the same core drawing and text
measurement operations. `renderOffscreenThumbnail` renders a bounded thumbnail in a Worker and
returns a transferable `ImageBitmap`; hosts without OffscreenCanvas receive an explicit fallback
signal and execute the identical scene on the main thread. See the
[Canvas 2D reference](https://developer.mozilla.org/en-US/docs/Web/API/CanvasRenderingContext2D)
and [Offscreen Canvas 2D reference](https://developer.mozilla.org/en-US/docs/Web/API/OffscreenCanvasRenderingContext2D).

## Fonts and line wrapping

`FontResolver` distinguishes Latin, East Asian, and complex-script runs. A caller may provide
theme font slots, exact `FontFace` sources, substitutions, and documented fallbacks. The
resolver reports whether the requested face is exact instead of disguising a fallback.
Supplied web fonts are loaded before canvas use through the
[CSS Font Loading API](https://developer.mozilla.org/en-US/docs/Web/API/CanvasRenderingContext2D/font).

`measureTextBatch` groups requests by the final CSS font and deduplicates identical
font/text pairs before measuring them on the target
canvas. Korean, Han, Hiragana, and Katakana text may wrap at character boundaries. Latin text
wraps at whitespace first and falls back to character boundaries when one token exceeds the
frame width; newlines remain deterministic. Tests cover an exact supplied-font host, documented
missing-font fallback, Korean and Latin wrapping, and mixed-order batched measurement.

For `a:normAutofit`, the shared rich-text planner reflows at each candidate font scale and uses a
bounded binary search for the largest scale whose measured width and height fit the text frame.
Base-font token measurements are reused across candidates, so AutoFit adds layout work without
repeating browser font measurements. Authored `fontScale` and `lnSpcReduction` values seed normal
AutoFit, while edited overflowing content is recomputed downward. `a:spAutoFit` keeps font sizes and
projects bounded effective geometry through both browser backends. Text columns honor `numCol` and
`spcCol` after margins and AutoFit are resolved.
PresentationML text frames do not define the Wordprocessing Shape `wps:linkedTxbx` chain used by
Word. If that foreign extension appears in a deck it remains byte-preserved and renders each shape
independently; cycles or cross-shape overflow are therefore never guessed from vendor markup.

## Lazy viewer and resource ownership

`VirtualizedCanvasViewer.setVisibleSlides` mounts canvases only for the visible indices. A new
viewport revision aborts stale resolution and drawing work. Visible slides complete before
neighbor prefetch begins. The scene LRU and decoded-image LRU both have explicit byte budgets and
hit/miss telemetry.
`setContentRevision` advances an immutable deck revision, removes only the slide scenes named by
the engine's invalidation result, and aborts older resolution work. Presentation canvases and
storyboard thumbnails can therefore request the same revisioned WPDL while preserving reusable
offscreen scenes. Deck-session creation and updates return the complete ordered page inventory for
that same revision before any WPDL is resolved. Hosts can therefore group continuation pages,
subdue hidden authoring pages, and mount the first visible canvas without eagerly materializing
offscreen scenes. `hitTestDisplayScene` and `hitTestDisplaySceneAtCanvasPoint` return only
source-backed semantic targets, ordered by z-order and stable reading order.
`WasmpptWorkerClient` also deduplicates in-flight raw and converted resources per presentation
and part name and exposes its bounded resident-byte count. Evicted image objects
are closed when their host resource supports `close()`. `dispose()` aborts work, removes every
canvas, clears both caches, and releases listeners owned by the mounted canvas elements.

Live rendering keys image resources by their exact part-content fingerprint rather than a
relationship ID. Text widths use a 4 MiB LRU keyed by resolved font and text; rich-text layout uses
an 8 MiB LRU keyed by font-resolver identity plus the full display command, including run tree,
bounds, wrapping, margins, and flow. `clear()` empties all renderer-owned caches. The dogfood viewer
keeps unrelated canvases mounted and redraws only visible invalidated slides; slide-level redraw is
the current measured correctness boundary.

The viewer intentionally accepts visible slide indices from the application. This avoids
installing a second scroll policy or an immortal global `IntersectionObserver`; React, vanilla
DOM, and custom viewers can feed the same bounded primitive.

## Telemetry and verification

Each render reports separate durations for slide resolution, font loading and measurement,
display execution, and media decode, plus command count, cache bytes, and cache hit rate. The browser
gate records first-visible-slide raw samples separately from injection and per-stage samples. It
runs the real Wasm module in a module Worker, transfers a two-slide PPTX, resolves only slide
zero, draws shapes, nested transforms, fills, strokes, text, an image crop, verifies cache
cleanup and bounded mounted canvases, runs a 1,000-slide scroll/disposal stress trace, renders and
closes an OffscreenCanvas thumbnail, and records a pixel fingerprint. Higher-fidelity visual
baselines and per-slide tolerance reports belong to the compatibility-gate slice.

## Current boundary

The renderer supports WPDL v11 while retaining v1-v10 decoding. Version 11 carries resolved
run-owned hyperlinks for the DOM accessibility layer. Version 10 carries stable semantic IDs,
source ranges, reading order, and hit-test bounds for Canvas authoring. Version 9 marks text from a
materialized live-edit overlay so normal AutoFit recomputes the largest fitting scale instead of
blindly retaining a stale authored hint. Embedded font relationships travel
as lazy resources; `registerEmbeddedFonts` applies size and OpenType embedding-permission checks
before registering exact `FontFace` bytes. Hosts may additionally load the independent
`wasmppt-shaper-wasm` artifact: its HarfRust pipeline returns deterministic font-unit advances,
offsets, glyph IDs, UTF-8 clusters, and safe-break flags which are retained in the shared layout
plan. Its request identity also covers language, script, OpenType features, and variation
coordinates. An authored SmartArt picture fallback uses this same image path only when its
`mc:AlternateContent` association is unambiguous; native SmartArt layout and drawing remain
unsupported.
Common editable 2D text paint covers solid/linear/radial/pattern fills, outlines, inner/outer
shadows, glow, blur, soft edges, a bounded reflection, and arch, wave, inflate, and deflate warp
presets. Unsupported effect-DAG nodes retain
readable unwarped text and source markup.
PNG/JPEG/GIF/SVG metadata is inspected before decode, byte and pixel limits are enforced, unsafe
SVG active/external content is rejected, GIF preview is the deterministic first frame, and browser
decode applies EXIF orientation. EMF/WMF supports common GDI records through the lazy
SVG converter; malformed or unsupported record streams fall back to an unavailable-image region.
Other unsupported preserved graphics render a labeled placeholder and retain their stable
diagnostic instead of disappearing. Canvas output is a bitmap and therefore is not the
accessibility surface; the secondary DOM/SVG backend owns selectable text, reading order, links,
and alternative text.
