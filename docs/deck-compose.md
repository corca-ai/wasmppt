# Editable deck composition

Status: editable text, lists, raster images, SVG, deterministic GIF stills, immutable live
overlays, and pull-based PPTX export implemented; tables and charts are the next composition slice

`wasmppt-deck-compose` projects an exact validated `DeckSpec`, `DeckTemplatePlan`, and `DeckPlan`
tuple into PresentationML. It is a host-neutral Rust core: it neither scrapes a DOM nor invokes a
JavaScript presentation generator. A template SHA-256 mismatch, invalid plan, unsupported content,
unsafe media, or resource overrun fails with a stable `ComposeErrorCode` before an output revision
is exposed.

## Output ownership

Composition replaces the presentation's slide topology in one revision. It materializes only:

- `[Content_Types].xml`, `ppt/presentation.xml`, and its relationship part;
- generated slide and slide-relationship parts; and
- media referenced by the generated slides.

Old slide parts are removed. Layouts, masters, themes, decorations, unrelated media, extension
markup, and unknown parts remain in the template package. `PackageOverlay` serves those untouched
parts directly from the original archive and raw-copies their compressed payloads during export.
The live overlay itself implements `PackagePartSource`, so rendering and dependency fingerprinting
operate on the same logical revision that export later serializes.

## Editable semantics

Text is emitted as DrawingML paragraphs and runs rather than flattened pictures. Bold, italic,
strikethrough, inline-code typeface, explicit template-derived font size/typeface/color, and safe
external web, mail, and telephone hyperlinks remain editable. Nested lists preserve source order,
hierarchy level, ordered start value, and deterministic indentation. Source-anchor links stay
non-active until an explicit internal-slide target contract exists.

Each shape and relationship receives a deterministic source-order identifier. Hidden state is
written on the physical slide. Derived continuation pages add only the planned repeated heading
and minimal `n/total` marker; neither becomes another source-owned fragment.

## Media policy

PNG and JPEG payloads pass through unchanged. `cover` computes a centered DrawingML source crop;
`contain` retains the complete resource in its planned frame. Alt text is written to the picture's
non-visual properties.

SVG is retained as vector media and referenced through the Office SVG extension. XML parsing
rejects scripts, foreign objects, event handlers, JavaScript URLs, imports, and external references.
GIF input is decoded under byte and pixel bounds, composited onto its logical first-frame canvas,
and encoded as a deterministic PNG still in the core. These rules are identical for native and
Wasm builds.

## Streaming and bounds

`PresentationOverlay::generation_cursor` accepts a positive maximum output chunk size and emits
the exact PPTX revision without constructing a complete PPTX buffer or base64 media graph. Peak
materialized memory is bounded by `ComposeLimits`; unchanged compressed source bytes and output
bytes are not retained by the composer. The revision digest covers template identity, spec
identity, the encoded plan, and every materialized logical part.

## Verification

```sh
cargo test -p wasmppt-deck-compose --all-features
cargo clippy -p wasmppt-deck-compose --all-targets --all-features -- -D warnings
cargo check -p wasmppt-deck-compose --all-features --target wasm32-unknown-unknown
```

Tests verify editable run properties and hyperlinks, nested numbering, SVG retention, deterministic
GIF stills, unknown-part preservation, raw compressed reuse, one-byte overlay pulls, configured
bounds, and structural equality between direct live-overlay resolution and a streamed/reopened PPTX.

## Related documents

- [Semantic deck contracts](deck-engine.md) defines the validated input tuple.
- [Cortex Theme Starter compiler](deck-template.md) defines layout discovery and template policy.
- [Semantic layout and pagination](deck-layout.md) defines physical page and fragment planning.
- [OPC and ZIP substrate](opc.md) defines immutable overlays and exact streaming export.
