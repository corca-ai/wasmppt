# Accessible DOM and SVG backend

Status: implemented baseline

The secondary browser backend consumes the same decoded `DisplayScene` as Canvas. It does not
parse PresentationML, resolve themes, inherit placeholders, or calculate geometry independently.
Inline SVG projects the resolved graphic commands, while positioned HTML projects semantic text
and interaction metadata.

## Semantic WPDL boundary

WPDL version 2 adds two side tables without changing Canvas drawing commands:

- semantic elements map a source shape ID and reading order to an exact command range, resolved
  bounds, name, alternative description, and hyperlink;
- diagnostics carry the same stable code, source part, optional shape ID, and message emitted by
  the Rust resolver.

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

Preset geometry, fills, strokes, group transforms, rotations, flips, and source-cropped images
are projected from the display list. The Canvas and SVG integration fixture asserts the same
resolved title fill, stroke width, image bounds, and transform command range. Features that are
not lowered by the shared core have identical diagnostic codes in both backends.

## Incremental updates and virtualization

Each host retains its slide root and keyed semantic elements. A newer revision updates existing
SVG groups and HTML nodes in place, an equal revision is a no-op, and an older revision is
ignored. This preserves selection and integration identity across ordinary updates.

`VirtualizedDomViewer` uses the same Worker resolver and bounded scene cache as the Canvas viewer.
Only visible slide hosts remain in the DOM; neighbor prefetch does not mount nodes. A viewport
change aborts stale work, and `dispose()` removes every mounted slide and cached scene.

## Deliberate limits

The backend exposes semantics that exist in WPDL v2. Rich paragraph/run styling, tables, charts,
custom geometry, gradients, effects, and SmartArt are added only when the common resolver and
display model can describe them. Until then, both rendering backends preserve the package source
and surface the same explicit diagnostic rather than producing backend-specific silent fallback.
