# Documentation

This is the canonical index for living `wasmppt` documentation.

## Start here

- [System architecture](architecture.md) — mission, core boundaries, package and
  rendering pipelines, runtime model, performance contract, and delivery slices.
- [Development guide](develop.md) — toolchain, crate and package entry points,
  feature policy, profiles, and verification commands.
- [Quality gates](quality.md) — local hooks, pinned Rust tools, CI tiers, coverage ratchet,
  scheduled deep checks, and quarantine policy.
- [OPC and ZIP substrate](opc.md) — implemented lazy ZIP indexing, raw-copy
  rewriting, security limits, determinism, and memory budgets.
- [Loss-aware OOXML graph](ooxml.md) — namespace-aware source ranges, content
  types, relationships, conformance detection, diagnostics, and typed views.
- [Template bindings and TemplatePlan](bindings.md) — PowerPoint authoring,
  split-run tokens, manifests, diagnostics, serialization, and cache identity.
- [Semantic deck contracts](deck-engine.md) — source-backed `DeckSpec`, template and
  physical plans, stable identities, exact-ownership validation, and binary boundaries.
- [Cortex Theme Starter compiler](deck-template.md) — explicit POTX layout contract,
  resolved template profiles, active-content policy, and deterministic cache identity.
- [Semantic layout and pagination](deck-layout.md) — bounded candidate search, exact font
  measurement, authored content relations, continuation policy, and deterministic plans.
- [Editable deck composition](deck-compose.md) — loss-aware plan projection, live package
  overlays, editable text and lists, image/SVG policy, and pull-based PPTX export.
- [High-speed template injection](injection.md) — prepared warm generation,
  text, images, tables, slides, macro stripping, streaming, and validation.
- [Live editing and incremental preview](live-editing.md) — revisioned deltas,
  virtual package overlays, invalidation, bounded caches, scheduling, and current-revision export.
- [Runtime host adapters](hosts.md) — native files, opaque Wasm handles, browser
  Worker protocol, Cloudflare/R2 streaming, cache and memory budgets.
- [Browser dogfood playground](playground.md) — local template compilation, binding
  discovery, structured injection, streamed downloads, and GitHub Pages deployment.
- [Lazy slide resolution](rendering.md) — theme/master/layout inheritance,
  geometry and diagnostics, dependency invalidation, and binary display lists.
- [Browser Canvas renderer](canvas.md) — Worker-owned lazy resolution, Canvas
  execution, fonts, virtualization, resource budgets, and stage telemetry.
- [Offline DOM and SVG output](dom-svg.md) — selectable text, accessibility, safe links,
  exact page metadata, closed resources, deterministic HTML, and PDF print geometry.
- [Tables, charts, and advanced content](advanced-content.md) — table layout,
  chart caches and workbook edits, explicit fallbacks, and the capability matrix.
- [Compatibility, security, and visual gates](compatibility.md) — corpus provenance,
  fuzz and limit gates, cross-host parity, visual reports, and desktop consumers.
- [Deck compatibility gate](deck-gates.md) — comprehensive semantic fixtures, exact
  native/browser/workerd plan and package parity, bounded failure cases, and deck budgets.
- [Compatibility corpus and fidelity scorecard](corpus.md) — 50 generated cases,
  producer metadata, PR/scheduled tiers, regeneration, and raw per-feature results.
- [Performance contract and reproducible benchmarks](performance.md) — public fixture matrix,
  raw samples, cross-host release budgets, comparison rules, and claim policy.
- [Documentation guide](metadoc.md) — how documentation is organized, written,
  linked, and linted.
- [Release readiness](release.md) — semver, artifact provenance, compatibility evidence,
  support policy, and publication checklist.

## Project planning

Implementation is tracked in [GitHub Issues](https://github.com/corca-ai/wasmppt/issues).
Architecture documents describe durable decisions; issues describe execution work and
acceptance criteria.

## Standards and platform references

- [ECMA-376: Office Open XML](https://ecma-international.org/publications-and-standards/standards/ecma-376/)
- [PresentationML document structure](https://learn.microsoft.com/en-us/office/open-xml/presentation/structure-of-a-presentationml-document)
- [Cloudflare Workers WebAssembly](https://developers.cloudflare.com/workers/runtime-apis/webassembly/)
- [Cloudflare Workers limits](https://developers.cloudflare.com/workers/platform/limits/)
