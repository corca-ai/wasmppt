# Cortex Theme Starter compiler

Status: explicit Starter v3 envelope discovery, validation, and deterministic cache identity
implemented; automatic topology selection and PresentationML composition are implemented by the
downstream deck-layout and deck-compose crates

`wasmppt-deck-template` converts a bounded POTX package into the host-neutral
`DeckTemplatePlan` consumed by semantic layout. It is separate from `wasmppt-template`: the latter
compiles author-inserted binding tokens for repeated data injection, while this crate compiles safe
layout envelopes for generated semantic decks.

## Discovery contract

A Cortex Theme Starter v3 contains exactly one preserved slide layout for each required
`p:sldLayout/@matchingName` value:

- `wasmppt:title-v3`
- `wasmppt:statement-v3`
- `wasmppt:content-envelope-v3`

There are no capability-specific split, media, gallery, table, or comparison layouts. The template
owns visual identity and safe outer geometry; the planner constructs columns, media/text splits,
galleries, tables, comparisons, and continuation pages procedurally inside the content envelope.
Removing the optional media bleed therefore degrades to the same safe envelope instead of selecting
a different fallback layout.

Regions use these exact standard `p:ph/@type` and `p:ph/@idx` identities:

| Layout | Identity | Meaning |
| --- | --- | --- |
| `wasmppt:title-v3` | `title:1` | cover title |
| `wasmppt:title-v3` | `subTitle:2` | ordered cover-details flow |
| `wasmppt:statement-v3` | `ctrTitle:5` | centered statement |
| `wasmppt:content-envelope-v3` | `title:3` | content heading |
| `wasmppt:content-envelope-v3` | `body:4` | safe text and mixed-content envelope |
| `wasmppt:content-envelope-v3` | `pic:5` | optional media-only bleed envelope |

The optional `pic:5` frame must contain `body:4`. A pure-media composition may use the larger
frame; text and mixed media/text composition always stays within `body:4`. The bleed placeholder is
geometry metadata and does not become an insertion region in the compiled plan.

The compiler never uses example slides, slide order, visible `p:cSld/@name` values,
`p:cNvPr/@name` values, Alt Text, fixed slide numbers, or an out-of-package manifest to infer a
role. An arbitrary POTX therefore gets stable missing-layout diagnostics instead of a guessed
profile. PowerPoint preserves `matchingName` and placeholder identities when a Starter is edited
and saved normally.

The title subtitle region accepts subtitle, prose, and credit text without guessing whether a block
is an author, date, or description. The content body accepts all supported semantic body roles.
Footer, date, and slide-number placeholders are preserved page-furniture assets rather than
semantic regions.

## Resolved profile

The compiler retains the exact positive `p:sldSz` EMU values without replacing unusual but valid
page sizes. It resolves layout placeholders against matching master placeholder type/index pairs
and then resolves:

- safe and optional bleed frame geometry plus DrawingML text insets;
- nine-level layout, master-placeholder, and master title/body/other text styles;
- font size, Latin/East Asian/complex-script typeface, emphasis, indentation, and color;
- theme major/minor fonts and the resolved sRGB color scheme;
- effective layout or master background;
- non-placeholder master/layout shapes, pictures, logos, footers, and their relationship parts.

Backgrounds and assets point to exact XML byte ranges in the original hash-matched POTX. This
keeps extension markup and unsupported OOXML available to the later composition layer. The
compiler does not normalize or rewrite the package.

## Validation and safety

Opening uses `wasmppt-opc::PackageLimits`, so entry counts, sizes, names, compression ratios, and
overlap checks fail before profile allocation. Compilation additionally validates:

- exactly one non-macro POTX main content type and its package relationship;
- bounded, namespace-correct PresentationML XML;
- resolvable master, layout, theme, and internal relationship targets;
- unique required matching names and exact placeholder identities;
- positive inherited geometry, safe frames and bleed contained by the slide, bleed containing its
  safe body frame, and non-overlapping effective generated-content envelopes;
- preserved layout or master assets not overlapping any effective generated-content envelope;
- absence of VBA, ActiveX, OLE/package embeddings, custom UI, digital signatures, and macro or
  program actions.

Ordinary package metadata relationships such as core properties remain valid; only the exact
embedded-package relationship is active content.

Contract diagnostics accumulate in one deterministic result. ZIP indexing or graph construction
failures return `ThemeCompileError`; inspectable profile failures return a plan with error
diagnostics and `cacheable = false`. Active-content-bearing input is never cacheable.

## Cache and host boundary

The cache key hashes the POTX bytes, Starter validator version, WDTP schema version, compiler and
OPC engine versions, and policy identifier. Plan and child identities derive from that key or the
template hash plus semantic identifiers, never collection positions or visible names. The same
Rust implementation and WDTP v4 encoder compile for native and `wasm32-unknown-unknown`, giving
both hosts one deterministic cache boundary. Starter v2 is intentionally not decoded as v3.

## Verification

```sh
cargo test -p wasmppt-deck-template --all-features
cargo clippy -p wasmppt-deck-template --all-targets --all-features -- -D warnings
cargo check -p wasmppt-deck-template --target wasm32-unknown-unknown
npm run check:core-boundary
```

Integration tests build real OPC ZIP fixtures and cover the exact v3 contract, optional bleed
presence and absence, planner bleed selection, inherited geometry and styles, preserved-asset
exclusion, exact page size, accumulated errors, active-content rejection, deterministic cache
identity, and deterministic WDTP bytes.

## Related documents

- [Semantic deck contracts](deck-engine.md) defines the output plan and wire format.
- [Automatic slide layout](deck-layout.md) defines procedural topology selection within the
  template envelope.
- [OPC and ZIP substrate](opc.md) defines package limits and graph safety.
- [Loss-aware OOXML graph](ooxml.md) defines relationship and exact-source-range behavior.
- [Template bindings](bindings.md) defines the separate repeated binding-injection workload.
