# Accessible DOM and SVG backend

Status: output-only standalone document serialization implemented

The secondary browser backend consumes the same decoded `DisplayScene` as Canvas. It does not
parse PresentationML, resolve themes, inherit placeholders, or calculate geometry independently.
Inline SVG projects the resolved graphic commands, while positioned HTML projects semantic text
and interaction metadata. Cortex interactive preview, presentation, and storyboard surfaces remain
Canvas-only; DOM/SVG exists for offline HTML and browser PDF output.

## Semantic WPDL boundary

WPDL version 2 added two side tables without changing Canvas drawing commands:

- semantic elements map a source shape ID and reading order to an exact command range, resolved
  bounds, name, alternative description, and hyperlink;
- diagnostics carry the same stable code, source part, optional shape ID, and message emitted by
  the Rust resolver.

WPDL version 3 added effective text-frame styling and preserved-graphic placeholders. WPDL
version 4 adds paragraph/run-preserving rich text, linear gradients, bounded move/line/close
custom paths, outer shadows, and connector line ends. WPDL version 7 adds typed spacing, authored
normal-AutoFit hints, shape-resize bounds, columns, lazy embedded fonts, and editable 2D text
outlines, outer shadows, glow, blur, soft edges, and reflection. WPDL version 8 separates inner
shadow paint from outer shadow paint. WPDL version 9 distinguishes source-faithful AutoFit from
live-edited recomputation. WPDL version 10 adds explicit reading order and optional source-backed
semantic identity and ranges. The decoder retains v1-v9 compatibility.

The resolver reads `cNvPr` description/title attributes and hyperlink relationships. External
links are retained in the scene. The browser exposes clickable `http`, `https`, `mailto`, and
`tel` links; unsafe schemes remain available as selection metadata but are not activated.

## DOM contract

`DomSvgRenderer.render` creates one slide group with `aria-roledescription="slide"`. Within it:

- SVG paths and image elements carry source shape IDs and accessible names;
- HTML text is positioned over the graphic layer, remains selectable, and follows resolved
  z-order for paint while the selectable overlay follows explicit semantic reading order;
- hyperlinks use anchors and preserve the resolved alternative description;
- `data-selection-id`, source shape ID, explicit reading order, source range, and command range
  support editor and selection integrations.

Preset and bounded curved custom geometry, solid/linear/radial/pattern fills, strokes, outer shadows,
connector line ends, group transforms, rotations, flips, and source-cropped images are projected
from the display list. DOM text and Canvas use the same rich-text layout planner for mixed runs,
font resolution, wrapping, margins, alignment, and vertical anchoring. The Canvas and SVG
integration fixture asserts the same resolved title fill, stroke width, image bounds, and
transform command range. Features that are not lowered by the shared core have identical
diagnostic codes in both backends.

DOM/SVG and Canvas also consume the same `scene/geometry` preset path plan and transform math.
SVG serialization and DOM accessibility remain adapter policy, but adding or correcting a preset
or group transform has one backend-neutral owner. Focused tests project every supported preset to
both Canvas operations and SVG path data before the browser visual fingerprint gate runs.

## Standalone HTML and PDF output

`serializeDeckSessionToHtml` resolves the engine-owned presentable page indices from one exact
deck-session revision. Each page carries its physical page ID, logical-slide ID, authoring index,
hidden state, continuation ordinal/total, and optional continuation label. Hidden pages remain in
the exact PPTX overlay with `show="0"` but do not enter this presentation/PDF page set.

The selected POTX dimensions travel through `DeckPlan` and WPDL in English Metric Units (EMU).
The serializer requires every page to have that same size and derives both CSS boxes and `@page`
print geometry from it. There is no aspect ratio or output page-size option. Page order, fragment
ownership, placement, semantic IDs, selectable text, safe links, accessible names, and continuation
metadata therefore project the same revision used by Canvas and PPTX.

The Cortex host supplies one authorized resolver from a package part name to bytes. The serializer:

- loads only WPDL image and embedded-font part names;
- rejects an unresolved, empty, oversized, unsupported, or active resource instead of emitting a
  partial document;
- accepts bounded PNG, JPEG, and inactive SVG, and freezes GIF to the browser-decoded first frame
  as an inline PNG;
- deobfuscates OOXML fonts, enforces OpenType preview/print permission, and emits inline
  `@font-face` rules;
- escapes host title/language metadata and semantic text through DOM text APIs; and
- emits no script and a Content Security Policy with `default-src 'none'`, data-only images/fonts,
  no connect/media/object/frame/form/base source, and inline style as the sole exception.

Safe `http`, `https`, `mailto`, and `tel` anchors remain usable when a reader chooses them. They are
navigation metadata, not automatically fetched document resources. Project URLs, arbitrary network
image/font loaders, host HTML, and output-specific CSS overrides are outside this API.

`serializeOfflineHtmlDocument` exposes the lower-level resolved-page adapter for browser hosts that
already own exact WPDL and `DeckPageMetadata`. Identical inputs and resource bytes serialize to
identical UTF-8 bytes in the supported browser. `OfflineDocumentError.code` distinguishes invalid
document topology, unresolved resources, unsafe resources, and resource-limit failures.
The standalone deck is an accessible list whose slide list items carry their valid set position
and size, while retaining the slide roledescription from the shared DOM projection.

## Incremental DOM primitive

Each host retains its slide root and keyed semantic elements. A newer revision updates existing
SVG groups and HTML nodes in place, an equal revision is a no-op, and an older revision is
ignored. This preserves selection and integration identity across ordinary updates.

`VirtualizedDomViewer` remains a low-level test and integration primitive, not a Cortex preview
surface. It uses the same Worker resolver and bounded scene cache as the Canvas viewer. Only visible
slide hosts remain in the DOM; neighbor prefetch does not mount nodes. A viewport change aborts
stale work, and `dispose()` removes every mounted slide and cached scene.

## Deliberate limits

Optional exact font-byte shaping is supplied by the separately loaded Rustybuzz Wasm module and
retained on the same positioned run plan used by Canvas. An unambiguously associated SmartArt
picture fallback is emitted through the same image command as Canvas. General effect DAGs and
native SmartArt layout and rendering remain unsupported.
Both rendering backends preserve the package
source and surface the same explicit diagnostic and visible placeholder for unsupported graphics
rather than silently dropping content.
