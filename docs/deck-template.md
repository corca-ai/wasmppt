# Cortex Theme Starter compiler

Status: explicit Starter v2 discovery, capability compilation, validation, and deterministic
cache identity implemented; automatic layout and PresentationML composition are implemented by
the downstream deck-layout and deck-compose crates

`wasmppt-deck-template` converts a bounded POTX package into the host-neutral
`DeckTemplatePlan` consumed by semantic layout. It is separate from
`wasmppt-template`: the latter compiles author-inserted binding tokens for repeated data
injection, while this crate compiles layout affordances for generated semantic decks.

## Discovery contract

A minimal Cortex Theme Starter v2 contains exactly one preserved slide layout for each
required `p:sldLayout/@matchingName` value:

- `wasmppt:title-v2`
- `wasmppt:content-flow-v2`
- `wasmppt:statement-v2`

A Starter may also expose these optional capabilities:

- `wasmppt:content-split-v2`
- `wasmppt:media-start-v2`
- `wasmppt:media-end-v2`
- `wasmppt:gallery-v2`
- `wasmppt:table-v2`
- `wasmppt:comparison-v2`

Each optional name may occur at most once. If an optional capability is absent, the planner may
construct the topology procedurally inside `content-flow`; it never guesses a layout from its
visible name or an example slide.

Regions use standard `p:ph/@type` and `p:ph/@idx` identities. The compiler never uses
example slides, slide order, visible `p:cSld/@name` values, `p:cNvPr/@name` values, Alt Text,
fixed slide numbers, or an out-of-package manifest to infer a role. An arbitrary POTX therefore
gets stable missing-layout diagnostics instead of a guessed profile. PowerPoint preserves
`matchingName` and placeholder identities when a Starter is edited and saved normally.

Title layouts require title and subtitle regions. The subtitle region is the ordered cover-details
flow and accepts subtitle, prose, and credit text without guessing whether a block is an author,
date, or description. Content-flow requires title and body. Content-split and comparison require a
title and two independently identified body placeholders. Media-start and media-end require title,
body, and media; gallery requires title and at least two media placeholders; table requires title
and a table placeholder. Statement requires a centered-title or title region mapped to the
statement role. Optional footer, date, slide-number, and other supported placeholders remain
available without becoming role identifiers.
Footer, date, and slide-number placeholders are compiled as preserved page-furniture assets rather
than semantic regions.

## Resolved profile

The compiler retains the exact positive `p:sldSz` EMU values without replacing unusual but
valid page sizes. It resolves layout placeholders against matching master placeholder type/index
pairs and then resolves:

- frame geometry and DrawingML text insets;
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
- unique required matching names and placeholder identities;
- capability-specific placeholder counts, positive inherited geometry, non-overlapping regions
  contained by the slide, and a positive page size;
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
Rust implementation and WDTP v3 encoder compile for native and `wasm32-unknown-unknown`, giving
both hosts one deterministic cache boundary.

## Verification

```sh
cargo test -p wasmppt-deck-template --all-features
cargo clippy -p wasmppt-deck-template --all-targets --all-features -- -D warnings
cargo check -p wasmppt-deck-template --target wasm32-unknown-unknown
npm run check:core-boundary
```

Integration tests build real OPC ZIP fixtures and cover minimal-valid and capability-complete v2
Starters, capability discovery independence, inherited geometry and styles, asset source ranges,
exact page size, accumulated missing/duplicate errors, active-content rejection, deterministic
cache identity, and deterministic WDTP bytes.

## Related documents

- [Semantic deck contracts](deck-engine.md) defines the output plan and wire format.
- [OPC and ZIP substrate](opc.md) defines package limits and graph safety.
- [Loss-aware OOXML graph](ooxml.md) defines relationship and exact-source-range behavior.
- [Template bindings](bindings.md) defines the separate repeated binding-injection workload.
