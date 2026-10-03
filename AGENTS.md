# wasmppt Agent Guide

Build a loss-aware, embeddable PowerPoint engine with measured generation and rendering speed.

Before changing architecture, public APIs, package semantics, rendering, or runtime boundaries,
read the relevant documents completely. Start with the [documentation map](docs/index.md),
[architecture](docs/architecture.md), [development workflow](docs/develop.md), and
[documentation rules](docs/metadoc.md).

- Keep the Rust core host-agnostic; browser and Cloudflare APIs belong in adapters.
- Preserve unknown OOXML parts and markup unless an explicit conversion policy removes them.
- Prove fast-path invalidation boundaries or fall back to a safe path.
- Treat compatibility, peak memory, binary size, and latency as tested contracts.
- Update relevant documentation in the same change as architecture or API changes.
- Run `npm run precommit` before handoff; run `npm run hooks:install` once per clone.
