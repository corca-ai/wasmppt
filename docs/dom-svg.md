# Accessible DOM and SVG backend

Status: implemented baseline

The secondary browser backend consumes the same decoded `DisplayScene` as Canvas. It does not
parse PresentationML, resolve themes, inherit placeholders, or calculate geometry independently.
Inline SVG projects the resolved graphic commands, while positioned HTML projects semantic text
and interaction metadata.

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
live-edited recomputation. The decoder retains v1-v8 compatibility.

The resolver reads `cNvPr` description/title attributes and hyperlink relationships. External
links are retained in the scene. The browser exposes clickable `http`, `https`, `mailto`, and
`tel` links; unsafe schemes remain available as selection metadata but are not activated.

## DOM contract

`DomSvgRenderer.render` creates one slide group with `aria-roledescription="slide"`. Within it:

- SVG paths and image elements carry source shape IDs and accessible names;
- HTML text is positioned over the graphic layer, remains selectable, and follows resolved
  z-order as DOM reading order;
- hyperlinks use anchors and preserve the resolved alternative description;
- `data-selection-id`, source shape ID, reading order, and command range support editor and
  selection integrations.

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

## Incremental updates and virtualization

Each host retains its slide root and keyed semantic elements. A newer revision updates existing
SVG groups and HTML nodes in place, an equal revision is a no-op, and an older revision is
ignored. This preserves selection and integration identity across ordinary updates.

`VirtualizedDomViewer` uses the same Worker resolver and bounded scene cache as the Canvas viewer.
Only visible slide hosts remain in the DOM; neighbor prefetch does not mount nodes. A viewport
change aborts stale work, and `dispose()` removes every mounted slide and cached scene.

## Deliberate limits

Optional exact font-byte shaping is supplied by the separately loaded Rustybuzz Wasm module and
retained on the same positioned run plan used by Canvas. General effect DAGs and native SmartArt
rendering remain unsupported.
Both rendering backends preserve the package
source and surface the same explicit diagnostic and visible placeholder for unsupported graphics
rather than silently dropping content.
