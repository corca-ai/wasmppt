# Development Guide

This document defines the bootstrap toolchain, package entry points, feature policy, and
verification commands. For the durable subsystem design, see [System architecture](architecture.md).

## Toolchain

- Pinned development Rust: 1.96.0
- Minimum supported Rust version (MSRV): 1.85.1
- Rust edition: 2024
- Wasm target: `wasm32-unknown-unknown`
- Node.js: 24 or newer
- Documentation linter: `awiki`

`rust-toolchain.toml` installs the development toolchain, `rustfmt`, Clippy, and the Wasm
target. CI separately checks the workspace with the MSRV so using a newer local compiler
does not silently raise the compatibility floor.

## Rust entry points

| Package | Kind | Host dependency | Initial responsibility |
| --- | --- | --- | --- |
| `wasmppt-opc` | library | none | bounded ZIP and OPC substrate |
| `wasmppt-xml` | library | none | loss-aware namespace and XML tokens |
| `wasmppt-pml` | library | none | PresentationML typed views |
| `wasmppt-template` | library | none | binding plans and injection |
| `wasmppt-layout` | library | none | theme, layout, and slide resolution |
| `wasmppt-display` | library | none | backend-neutral display lists |
| `wasmppt-wasm` | `cdylib` and library | `wasm-bindgen` | narrow Wasm ABI |
| `wasmppt-cli` | binary | native standard library | inspection and verification CLI |

Core crates have empty default feature sets and MUST remain host-agnostic. Run
`npm run check:core-boundary` to traverse the resolved Cargo dependency graph and reject
browser, JavaScript, Wasm binding, or Cloudflare runtime packages reachable from core.

All crates are `publish = false` during the pre-alpha architecture phase. Publishing is
enabled only after public API, semver, compatibility, and release artifact policies are
accepted.

## JavaScript entry points

| Import | Purpose |
| --- | --- |
| `@corca-ai/wasmppt` | browser package and future Web Worker adapter |
| `@corca-ai/wasmppt-worker` | Cloudflare Workers adapter |

Both packages are private scaffolds until their runtime APIs are implemented. They are
separate so browser UI dependencies cannot inflate or constrain the Worker integration.

## Build profiles

- `release`: speed-oriented native and Wasm baseline with thin LTO.
- `wasm-release`: speed-oriented Wasm release with fat LTO.
- `wasm-small`: explicitly size-oriented Wasm comparison build.

The default production path uses measured `wasm-release` results. `wasm-small` is not
selected merely because it is smaller; it must meet the same latency contract.

## Verification

Run the complete local bootstrap suite from the repository root:

```sh
cargo fmt --all --check
cargo check --workspace --all-targets --all-features --locked
cargo test --workspace --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo check --workspace --all-features --locked --target wasm32-unknown-unknown
cargo +1.85.1 check --workspace --all-targets --all-features --locked
cargo deny check
npm ci
npm run check
npm run build
awiki lint -root docs
```

Build and report the raw Wasm artifact size with:

```sh
cargo build --profile wasm-release --locked --target wasm32-unknown-unknown -p wasmppt-wasm
node scripts/report-wasm-size.mjs \
  target/wasm32-unknown-unknown/wasm-release/wasmppt_wasm.wasm
```

CI runs the same gates. Runtime compatibility, security, visual, and performance gates
will be added as the corresponding architecture slices become executable.

The compatibility job converts a pinned real POTX and validates its PPTX output with the
Microsoft Open XML SDK wrapper under `tools/openxml-validator`.

Run the package parser fuzz target separately with `cargo-fuzz`:

```sh
cargo fuzz run --fuzz-dir crates/wasmppt-opc/fuzz open_package
cargo fuzz run --fuzz-dir crates/wasmppt-opc/fuzz package_graph
```

## Related documents

- Return to the [documentation index](index.md) for the complete project map.
- Read the [OPC and ZIP substrate](opc.md) contract before changing package I/O.
- Read the [loss-aware OOXML graph](ooxml.md) contract before changing XML or
  relationship handling.
- Follow the [documentation guide](metadoc.md) when changing development documentation.
