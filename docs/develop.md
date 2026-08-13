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
- Documentation linters: `awiki` and `markdownlint-cli2`
- Source linters: Clippy, `oxlint`, and ShellCheck
- Rust quality tools: cargo-nextest 0.9.143, cargo-llvm-cov 0.8.7, cargo-machete 0.9.2,
  cargo-deny 0.19.8, and cargo-fuzz 0.13.2

`rust-toolchain.toml` installs the development toolchain, `rustfmt`, Clippy, and the Wasm
target. CI separately checks the workspace with the MSRV so using a newer local compiler
does not silently raise the compatibility floor.
The root `[workspace.package].rust-version` is the primary MSRV source of truth. The two
metafile crate manifests independently declare their higher MSRV, while `rust-toolchain.toml`
is the development-toolchain source. Contract-sync tests compare every workflow and document
consumer against those declarations.

`npm ci` installs the pinned JavaScript and Markdown linters. Install `awiki` and ShellCheck
separately before using the local pre-commit gate. Install the Rust quality tools at the exact
versions above; the hooks deliberately perform no installation. CI also pins Actionlint and Typos
to validate workflow semantics and spelling without adding those slower tools to every local
commit. See [quality gates](quality.md) for tier ownership and quarantine policy.

## Rust entry points

| Package | Kind | Host dependency | Initial responsibility |
| --- | --- | --- | --- |
| `wasmppt-deck` | library | none | semantic deck and physical-plan contracts |
| `wasmppt-deck-template` | library | none | explicit Cortex Theme Starter POTX profiles |
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

### Core implementation ownership

The large template and layout entry points keep orchestration separate from deterministic
planning and parsing:

- `wasmppt-deck-template` owns the strict Starter policy and package orchestration. Its
  private XML projection retains element source ranges and exposes no package I/O. The
  compiler reads packages only through bounded `wasmppt-opc` APIs and emits only
  host-neutral `wasmppt-deck` values.
- `wasmppt-template::inject` owns package reads, generation state, caching, and output
  orchestration. Its `patch` module owns bounded XML replacements, escaping, and relationship
  target normalization; its `table` module owns row overflow and height-scaling policy. Neither
  helper module may read or write a package.
- `wasmppt-layout::resolve` owns dependency traversal, inheritance order, diagnostics, and slide
  assembly. Its `color` module owns theme color maps, font-scheme extraction, DrawingML color
  parsing, and ordered color transforms. It consumes only XML tokens and resolved value types.

Dependencies point from each orchestrator into these focused modules, never back into package I/O
or across sibling modules. Preserve original byte ranges for template patches, apply color
transforms in document order, and keep the public crate re-exports at their existing entry points
when extending these areas.

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
cargo nextest run --workspace --all-features --locked
cargo test --workspace --all-features --locked --doc
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo check --workspace --all-features --locked --target wasm32-unknown-unknown
cargo +1.85.1 check --workspace --all-targets --all-features --locked \
  --exclude wasmppt-metafile --exclude wasmppt-metafile-wasm
cargo +1.88.0 check -p wasmppt-metafile -p wasmppt-metafile-wasm --all-targets --locked
cargo deny check
cargo machete --skip-target-dir
npm ci
npm run lint
npm run check
npm run check:contracts
npm run build
npm run build:wasm-hosts
npm test --workspace @corca-ai/wasmppt-worker
npm run test:browser --workspace @corca-ai/wasmppt
npm run build:pages
npm run test:pages
node benchmarks/run.mjs --ci
awiki lint -root docs
```

Generate and enforce the current core coverage baseline separately:

```sh
rustup component add llvm-tools-preview
npm run coverage:core
```

Enable the repository-owned hooks once per clone:

```sh
npm run hooks:install
```

The pre-commit hook runs `npm run precommit`, composed from two reproducible layers:

- `npm run check:fast` checks Rust formatting, JavaScript/TypeScript/Markdown/shell lint,
  package types, architectural and cross-file contracts, and the documentation graph.
- `npm run test:fast` runs offline Rust library tests, repository tool tests, browser-package Node
  tests, and Worker adapter tests that do not launch workerd.

Staged whitespace is checked before both layers. The gate performs no installs or network fetches;
run `npm ci` and install the separately listed system tools during bootstrap. Its warm-run target is
under 30 seconds on a contributor machine (3.2 seconds measured on the reference macOS checkout on
2026-08-13). Browser integration, workerd integration, and performance suites remain deliberate
omissions. CI is authoritative. In an exceptional situation, bypass only this local guard with
`git commit --no-verify`, disclose the bypass in the pull request, and run `npm run precommit` as
soon as the environment is repaired.

The hooks are version-controlled under `.githooks/`; the installer verifies their execute bits and
sets the clone-local `core.hooksPath`, so no implementation is copied into `.git`. Repository tests
exercise installation in a temporary Git repository and lock the hook-to-command mapping. A failed
hook prints the exact manual reproduction command.

The pre-push hook stores Git's ref update stream in the temporary file named by
`WASMPPT_PRE_PUSH_REFS`, then runs `npm run prepush`. The default medium-cost, offline gate runs
workspace Cargo check and Clippy, native library/integration tests, separate doctests, a Wasm target
check, package tests (including workerd), core boundary and contract checks, cargo-deny's
license/source/duplicate policy, and cargo-machete. It requires pinned quality tools to be installed
during bootstrap but never installs them itself. Warm runs should take under two minutes and clean
builds roughly ten minutes, depending on the machine.

Use `npm run prepush:full` only when the local checkout already has `wasm-bindgen`, Chromium, and the
benchmark inputs. It additionally builds release Wasm hosts, runs browser and Pages integration,
and enforces the native benchmark matrix; it still performs no dependency installation. PowerPoint
consumers, corpus downloads, and full cross-host CI matrices remain remote gates. An exceptional
push can bypass local hooks with `git push --no-verify`; disclose it and rely on required CI before
merge.

Build and report the raw Wasm artifact size with:

```sh
cargo build --profile wasm-release --locked --target wasm32-unknown-unknown -p wasmppt-wasm
node scripts/report-wasm-size.mjs \
  target/wasm32-unknown-unknown/wasm-release/wasmppt_wasm.wasm
node scripts/report-wasm-size.mjs \
  target/wasm32-unknown-unknown/wasm-release/wasmppt_metafile_wasm.wasm
node scripts/report-wasm-size.mjs \
  target/wasm32-unknown-unknown/wasm-release/wasmppt_shaper_wasm.wasm
```

CI runs the same gates, including Actionlint workflow validation, Typos spell checking, runtime
compatibility, security, visual, and performance contracts on their real host adapters. The
contract synchronization check prevents WPDL version, decoder compatibility, fixture signature,
visual corpus, and browser budget declarations from drifting independently. One dedicated job
builds the scalar `wasm-release`
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
checked-in Wasm host bindings, browser package, and two dogfood POTX templates. `npm run test:pages`
serves that directory and uses real Chrome to apply one editor delta to both templates, render both
previews, and save both generated PPTX files under `target/pages-downloads`. CI validates those
exact browser downloads with the Microsoft Open XML SDK before publishing the Pages artifact.
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
