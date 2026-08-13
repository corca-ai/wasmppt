# Semantic deck contracts

Status: host-neutral contracts, validators, template-profile compiler, bounded planning,
editable composition, live overlays, and versioned binary codecs implemented

`wasmppt-deck` is the boundary between a host application's authoring model and the
wasmppt semantic layout and composition pipeline. It keeps Markdown, project storage,
browser APIs, and Cloudflare APIs outside the Rust core while retaining enough source
identity to return precise authoring diagnostics.

## Contract flow

```text
authoring adapter -> DeckSpec + resources
template compiler -> DeckTemplatePlan
                            |
                            v
                  semantic planner -> DeckPlan
                                          |
                                          v
                              PresentationML composer
```

The implemented contract types are independent of each future implementation stage.
An authoring adapter can create and validate `DeckSpec` before a planner exists, and a
planner can be tested with synthetic `DeckTemplatePlan` values without opening an OPC
package.

## DeckSpec

`DeckSpec` owns logical source intent:

- logical title and content slides, including persisted hidden state;
- stable UTF-8 byte `SourceRange` values and 128-bit `StableId` values;
- title, subtitle, prose, section, nested list, figure, caption, gallery, table,
  chart, code, diagram, display math, quote, credit, definition, and statement roles;
- bold, italic, strikethrough, inline-code, and typed safe-hyperlink rich-text runs;
- explicit `Never`, `Text`, `ListItems`, `TableRows`, `CodeLines`, and `Children` split policy;
- raster and SVG resources as binary bytes with media type and optional dimensions.

The model intentionally has no speaker-notes field. Markdown parsing, URL authorization,
project-file access and SVG production belong to the host adapter. The core validates that
the adapter's typed hyperlink target matches its safe scheme and that every referenced resource
is present. Composition deterministically decodes GIF first frames so native and Wasm hosts do
not produce different still images.

Because the semantic model has no speaker-notes field, deck composition treats notes attached
to POTX example slides as example content rather than reusable template furniture. Replacing the
example slides removes their notes-slide parts, relationships, and content-type overrides while
preserving the shared notes master and unrelated opaque template parts.

Call `StableId::from_source` with a stable document identity, exact source range, and
semantic role. Inserting an unrelated logical slide therefore does not renumber existing
content. Derived physical page IDs use the logical slide ID and its one-based continuation
ordinal; fragment IDs use the complete source node ID and fragment slice.

## Template and physical plans

`DeckTemplatePlan` contains the compiled template identity and SHA-256, exact page size,
deterministic cache key, theme fonts and colors, role-specific layouts, semantic regions,
and preserved assets. Each region has one EMU frame, standard placeholder type/index,
resolved text margins and hierarchy, a role, accepted semantic roles, and a required marker.
Assets retain their original package part and exact XML source range so a later composer can
copy unknown markup from the hash-matched POTX rather than reconstructing it. Visible layout
and shape names are not part of this contract.

`DeckPlan` contains physical pages grouped by logical slide. A page carries its stable ID,
selected template-layout ID, hidden state, one-based continuation ordinal and total, repeated
heading identity, minimal `n/total` label, and planned regions. Each
`PlannedFragment` owns:

- one source node;
- a whole, UTF-8 text, list-item, table-row, or code-line slice;
- an exact frame inside its planned and template regions;
- explicit font size, column count, and content-fit choice;
- repeated table-header row count for the first continued table fragment on a page.

The plan names its `DeckSpec` and `DeckTemplatePlan` identities and repeats the exact page
size. Composition MUST reject a mismatched input set rather than silently reflow it.

## Validation

`validate_deck_spec` checks unique non-nil identities, nested source containment,
role/content/split consistency, safe hyperlinks, resource ownership, table and chart
shape, finite chart values, and configured safety limits.

`validate_deck_plan` additionally checks:

- every renderable semantic source extent is covered exactly once;
- text slices are contiguous UTF-8 boundaries and list/table slices are contiguous;
- physical fragment order matches semantic source order;
- every page and fragment stays on its logical slide and an accepting template region;
- template, planned-region, and fragment geometry is positive and contained;
- physical page groups preserve logical-slide order;
- continuation ordinals, totals, hidden state, page IDs, and fragment IDs are stable.
- selected layouts, repeated headings, continuation labels, and repeated table headers agree
  exactly with their source and template ownership.

Failures use append-only numeric `DeckDiagnosticCode` values. The public code wrapper
retains unknown future numeric values, and `known_name` returns `None` instead of mapping
them to a misleading older meaning.

## Binary boundary and limits

The little-endian envelopes are:

| Magic | Version | Value |
| --- | ---: | --- |
| `WDSF` | 2 | `DeckSpec` and binary resources |
| `WDTP` | 2 | `DeckTemplatePlan` |
| `WDPL` | 2 | `DeckPlan` |

Vectors and strings are length-prefixed. Media remains raw bytes rather than JSON or
base64. Encoding order follows source and plan vector order, so equal values produce
equal bytes. Decoders reject unknown schema versions, invalid tags and UTF-8, truncation,
trailing bytes, and configured payload, string, collection, depth, node, resource, page,
and fragment limits before allocating the declared content.

`DeckLimitCode` values are append-only and identify each bounded dimension. Callers may
tighten `DeckLimits` for a host but MUST NOT turn a limit failure into partial content.
Checked-in hexadecimal fixtures pin WDSF v2, WDTP v2, and WDPL v2. Older semantic-plan
payloads are intentionally unsupported because the planner boundary is replaced atomically.

## Verification

```sh
cargo test -p wasmppt-deck --all-features
cargo clippy -p wasmppt-deck --all-targets --all-features -- -D warnings
cargo test -p wasmppt-deck-template --all-features
cargo clippy -p wasmppt-deck-template --all-targets --all-features -- -D warnings
cargo test -p wasmppt-deck-layout --all-features
cargo clippy -p wasmppt-deck-layout --all-targets --all-features -- -D warnings
cargo test -p wasmppt-deck-compose --all-features
cargo clippy -p wasmppt-deck-compose --all-targets --all-features -- -D warnings
npm run check:core-boundary
```

The contract tests cover stable source identities, deterministic round trips, unknown
diagnostic codes, configured limits, and independent failures for loss, duplication,
reordering, target drift, geometry, continuation metadata, and derived IDs.

## Related documents

- [System architecture](architecture.md) defines how this contract fits package
  generation and rendering.
- [Runtime host adapters](hosts.md) defines the existing native, browser, and workerd
  transport boundaries.
- [Template bindings](bindings.md) describes the separate compiled-template injection
  workload.
- [Cortex Theme Starter compiler](deck-template.md) describes the explicit POTX profile.
- [Semantic layout and pagination](deck-layout.md) defines planner measurement, search, and
  continuation policy.
- [Editable deck composition](deck-compose.md) defines PresentationML, media, overlay, and
  streamed-export behavior.
