# Development Guide

This document defines the bootstrap toolchain, package entry points, feature policy, and
verification commands. For the durable subsystem design, see [System architecture](architecture.md).

## Toolchain

- Pinned development Rust: 1.96.0
- Primary workspace minimum supported Rust version (MSRV): 1.85.1
- Optional EMF/WMF converter MSRV: 1.88.0
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
| `wasmppt-metafile` | library | none | bounded EMF/WMF-to-SVG conversion |
| `wasmppt-display` | library | none | backend-neutral display lists |
| `wasmppt-native` | library | native standard library | file source and sink capabilities |
| `wasmppt-wasm` | `cdylib` and library | `wasm-bindgen` | narrow Wasm ABI |
| `wasmppt-metafile-wasm` | `cdylib` and library | `wasm-bindgen` | optional lazy metafile ABI |
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
| `@corca-ai/wasmppt` | browser package and versioned Web Worker adapter |
| `@corca-ai/wasmppt-worker` | Cloudflare Workers adapter |

They are separate so browser UI dependencies cannot inflate or constrain the Worker
integration. See [Runtime host adapters](hosts.md) for their protocols and limits.

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
cargo +1.85.1 check --workspace --all-targets --all-features --locked \
  --exclude wasmppt-metafile --exclude wasmppt-metafile-wasm
cargo +1.88.0 check -p wasmppt-metafile -p wasmppt-metafile-wasm --all-targets --locked
cargo deny check
npm ci
npm run check
npm run build
npm run build:wasm-hosts
npm test --workspace @corca-ai/wasmppt-worker
npm run test:browser --workspace @corca-ai/wasmppt
npm run build:pages
npm run test:pages
node benchmarks/run.mjs --ci
awiki lint -root docs
```

Build and report the raw Wasm artifact size with:

```sh
cargo build --profile wasm-release --locked --target wasm32-unknown-unknown -p wasmppt-wasm
node scripts/report-wasm-size.mjs \
  target/wasm32-unknown-unknown/wasm-release/wasmppt_wasm.wasm
node scripts/report-wasm-size.mjs \
  target/wasm32-unknown-unknown/wasm-release/wasmppt_metafile_wasm.wasm
```

CI runs the same gates, including runtime compatibility, security, visual, and performance
contracts on their real host adapters. One dedicated job builds the scalar `wasm-release`
module and matching `wasm-bindgen` host files. The host-adapter and performance jobs depend
on that job and download its revision-bound artifact, so both exercise identical Wasm bytes
without compiling the release module twice. The artifact comes from the same workflow run;
CI never substitutes the latest successful artifact from another revision.

The compatibility job converts a pinned real POTX and validates its PPTX output with the
Microsoft Open XML SDK wrapper under `tools/openxml-validator`. It also resolves slides
from the generated output and the pinned real-world Apache POI `SampleShow.pptx` fixture.
The security-and-corpus job verifies fixture provenance, compiles all fuzz surfaces, and
exercises stable parser limits and preservation policies. Browser integration publishes a
per-slide report under `target/visual-report`; release ground truth uses the controlled
PowerPoint, LibreOffice, and Keynote workflow described in [compatibility gates](compatibility.md).
The performance-contract job publishes native, browser, and workerd raw samples and enforces the
budgets and correctness rules in the [performance contract](performance.md).

`npm run build:pages` assembles the static dogfood application under `target/pages` from the
checked-in Wasm host bindings, browser package, and dogfood POTX. `npm run test:pages` serves that
directory and uses real Chrome to compile the bundled template and download a generated PPTX.
CI reuses the single revision-bound Wasm artifact for this gate and deploys the exact tested static
directory to GitHub Pages on `main`. See the [browser dogfood playground](playground.md).

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
