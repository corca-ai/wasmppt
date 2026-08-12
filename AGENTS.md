# wasmppt Agent Guide

High-performance Rust/WebAssembly tooling for PowerPoint Open XML packages.

Before changing architecture, public APIs, package semantics, rendering, or runtime
boundaries, read the relevant documents below completely.

## Core mission

Build a loss-aware, embeddable PowerPoint engine whose compiled-template generation
path and browser rendering path are both demonstrably fast.

## Documentation index

- [Documentation index](docs/index.md): canonical map of project documentation.
- [System architecture](docs/architecture.md): goals, boundaries, data model,
  execution model, performance contract, and delivery slices.
- [Development guide](docs/develop.md): toolchain, entry points, feature policy,
  build profiles, and required verification commands.
- [Documentation guide](docs/metadoc.md): documentation structure, writing rules,
  and required lint command.

## Working rules

- Keep the Rust core host-agnostic; browser and Cloudflare APIs belong in adapters.
- Preserve unknown OOXML parts and markup unless an explicit conversion policy removes them.
- A fast path must prove its invalidation boundary or fall back to a safe path.
- Treat compatibility, peak memory, binary size, and latency as tested contracts.
- Update the relevant documentation in the same change as an architectural or API change.
