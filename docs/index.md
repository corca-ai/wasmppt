# Documentation

This is the canonical index for living `wasmppt` documentation.

## Start here

- [System architecture](architecture.md) — mission, core boundaries, package and
  rendering pipelines, runtime model, performance contract, and delivery slices.
- [Development guide](develop.md) — toolchain, crate and package entry points,
  feature policy, profiles, and verification commands.
- [OPC and ZIP substrate](opc.md) — implemented lazy ZIP indexing, raw-copy
  rewriting, security limits, determinism, and memory budgets.
- [Loss-aware OOXML graph](ooxml.md) — namespace-aware source ranges, content
  types, relationships, conformance detection, diagnostics, and typed views.
- [Template bindings and TemplatePlan](bindings.md) — PowerPoint authoring,
  split-run tokens, manifests, diagnostics, serialization, and cache identity.
- [High-speed template injection](injection.md) — prepared warm generation,
  text, images, tables, slides, macro stripping, streaming, and validation.
- [Documentation guide](metadoc.md) — how documentation is organized, written,
  linked, and linted.

## Project planning

Implementation is tracked in [GitHub Issues](https://github.com/corca-ai/wasmppt/issues).
Architecture documents describe durable decisions; issues describe execution work and
acceptance criteria.

## Standards and platform references

- [ECMA-376: Office Open XML](https://ecma-international.org/publications-and-standards/standards/ecma-376/)
- [PresentationML document structure](https://learn.microsoft.com/en-us/office/open-xml/presentation/structure-of-a-presentationml-document)
- [Cloudflare Workers WebAssembly](https://developers.cloudflare.com/workers/runtime-apis/webassembly/)
- [Cloudflare Workers limits](https://developers.cloudflare.com/workers/platform/limits/)
