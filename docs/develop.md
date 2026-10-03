# Development guide

New contributors should read the [architecture](architecture.md), bootstrap below, then follow the
[subsystem map](index.md#subsystem-contracts) for the area they will change. For application
integration rather than repository work, use [getting started](getting-started.md).

## Toolchain

- Pinned development Rust: 1.96.0
- Primary workspace minimum supported Rust version (MSRV): 1.85.1
- Optional EMF/WMF converter MSRV: 1.88.0
- Rust edition: 2024
- Wasm target: `wasm32-unknown-unknown`
- Node.js: 24 or newer
- Documentation linters: `awiki` and `markdownlint-cli2`
- Source linters: Clippy, `oxlint`, and ShellCheck
- Rust quality tools: use versions pinned in [CI](../.github/workflows/ci.yml) for local gates
  and the [scheduled workflow](../.github/workflows/rust-deep-quality.yml) for fuzzing/Miri.

`rust-toolchain.toml` installs the development toolchain, `rustfmt`, Clippy, and the Wasm
target. CI separately checks the workspace with the MSRV so using a newer local compiler
does not silently raise the compatibility floor.
The root `[workspace.package].rust-version` is the primary MSRV source of truth. The two
metafile crate manifests independently declare their higher MSRV, while `rust-toolchain.toml`
is the development-toolchain source. Contract-sync tests compare every workflow and document
consumer against those declarations.

`npm ci` installs the pinned JavaScript and Markdown linters. Install ShellCheck through your
system package manager and `awiki` with the pinned install command in CI's `documentation` job.
Install Rust quality tools at the workflow-pinned versions; hooks perform no installation.
CI also pins Actionlint and Typos
to validate workflow semantics and spelling without adding those slower tools to every local
commit. See [quality gates](quality.md) for tier ownership and quarantine policy.

## Bootstrap

From the repository root:

```sh
rustup show
cargo fetch --locked
npm ci
npm run hooks:install
npm run build
npm run precommit
```

Fetching Cargo dependencies first is necessary because local hooks use `--offline`. The tracked
hooks perform no installations or network fetches. Enable them once per clone; the installer sets
`core.hooksPath` to `.githooks` and checks executable bits.

The checked-in Wasm artifacts suffice for JavaScript-only work. For Rust changes affecting the
engine or other inputs covered by the provenance manifest, install `wasm-bindgen-cli` matching
`workspace.dependencies.wasm-bindgen` in [Cargo.toml](../Cargo.toml), then run:

```sh
npm run build:wasm-hosts
node scripts/check-wasm-artifact-manifest.mjs
```

Commit the regenerated host bindings, Wasm bytes, and manifest together. The build invokes the CLI;
its version must match the Rust dependency. CI builds its own artifact and compares bindings and
provenance, rather than requiring macOS and Linux LLVM output to have identical bytes.

## Rust entry points

| Package | Kind | Host dependency | Responsibility |
| --- | --- | --- | --- |
| `wasmppt-deck` | library | none | semantic deck and physical-plan contracts |
| `wasmppt-deck-template` | library | none | explicit Cortex Theme Starter POTX profiles |
| `wasmppt-deck-layout` | library | none | bounded semantic layout and pagination |
| `wasmppt-deck-compose` | library | none | editable PresentationML and live package overlays |
| `wasmppt-opc` | library | none | bounded ZIP and OPC substrate |
| `wasmppt-xml` | library | none | loss-aware namespace and XML tokens |
| `wasmppt-pml` | library | none | PresentationML typed views |
| `wasmppt-template` | library | none | binding plans and injection |
| `wasmppt-layout` | library | none | theme, layout, and slide resolution |
| `wasmppt-metafile` | library | none | bounded EMF/WMF-to-SVG conversion |
| `wasmppt-shaper` | library | none | bounded exact font-byte shaping and line breaks |
| `wasmppt-display` | library | none | backend-neutral display lists |
| `wasmppt-native` | library | native standard library | file source and sink capabilities |
| `wasmppt-wasm` | `cdylib` and library | `wasm-bindgen` | narrow Wasm ABI |
| `wasmppt-metafile-wasm` | `cdylib` and library | `wasm-bindgen` | optional lazy metafile ABI |
| `wasmppt-shaper-wasm` | `cdylib` and library | `wasm-bindgen` | optional browser font-shaping ABI |
| `wasmppt-cli` | binary | native standard library | inspection and verification CLI |

Core crates have empty default feature sets and MUST remain host-agnostic. Run
`npm run check:core-boundary` to traverse the resolved Cargo dependency graph and reject
browser, JavaScript, Wasm binding, or Cloudflare runtime packages reachable from core.

The [core boundary check](../scripts/core-boundary.mjs) owns the enforced crate set. Preserve the
existing separation between package orchestration and pure XML, color, chart, table, measurement,
and geometry helpers. Public re-exports remain at each crate's entry point; see the relevant
[subsystem contract](index.md#subsystem-contracts) before changing their behavior.

All crates are unpublished during pre-alpha. [Release readiness](release.md) defines the
conditions for enabling publication.

## JavaScript entry points

| Import | Purpose |
| --- | --- |
| `@corca-ai/wasmppt/deck` | Host-neutral semantic types and source-offset helpers |
| `@corca-ai/wasmppt/browser` | Semantic engine, Canvas view and HTML export |
| `@corca-ai/wasmppt/node` | In-process semantic engine for Node |
| `@corca-ai/wasmppt` | Lower-level browser API and versioned module-Worker adapter |
| `@corca-ai/wasmppt/browser-worker` | Self-initializing browser Worker with a startup handshake |
| `@corca-ai/wasmppt-worker` | Cloudflare Workers adapter |

The root [package manifest](../package.json) exports TypeScript source for exact-commit Git
consumers; `/node` exports checked-in JavaScript rebuilt by `npm run build:node` (also in
`npm run build`). `npm run check:node-artifact` rejects source/bundle drift.
Package-local manifests export built `dist` files for workspace builds and tests.
[Browser integration](browser.md) explains asset emission and startup; [host adapters](hosts.md)
define transport, ownership, limits, and errors.

## Build profiles and artifacts

[Cargo.toml](../Cargo.toml) defines speed-oriented `release` with thin LTO, `wasm-release` with fat
LTO, and the size-oriented comparison profile `wasm-small`. Production uses `wasm-release`;
a smaller build must still meet the [latency contract](performance.md).

The host build emits scalar, optional metafile, and optional shaper modules. To report their raw
sizes after `npm run build:wasm-hosts`:

```sh
node scripts/report-wasm-size.mjs \
  target/wasm32-unknown-unknown/wasm-release/wasmppt_wasm.wasm
node scripts/report-wasm-size.mjs \
  target/wasm32-unknown-unknown/wasm-release/wasmppt_metafile_wasm.wasm
node scripts/report-wasm-size.mjs \
  target/wasm32-unknown-unknown/wasm-release/wasmppt_shaper_wasm.wasm
```

## Verify a change

The [root scripts](../package.json) are the executable definitions of the local gates:

| When | Command | Scope |
| --- | --- | --- |
| Before every handoff or commit | `npm run precommit` | Staged whitespace, format/lint, types, contracts, docs graph, offline Rust library and host-free package tests |
| Before pushing | `npm run prepush` | Workspace check/Clippy/tests/doctests/Wasm, package tests including workerd, dependency policy |
| Host/render/performance changes | `npm run prepush:full` | Push gate plus Wasm build, Chromium, Pages, and native performance budgets |
| Core coverage changes | `npm run coverage:core` | Coverage ratchet; install cargo-llvm-cov and `llvm-tools-preview` first |
| Documentation changes | `awiki lint -root docs` | Flat documentation graph; included in precommit |

`prepush` requires cargo-deny and cargo-machete already installed. `prepush:full` additionally
requires the matching `wasm-bindgen` CLI, Chromium, and comparison inputs. Install Chromium with
`npx playwright install chromium`; prepare benchmark inputs using [performance reproduction](performance.md#reproduce).
The [quality guide](quality.md) explains CI ownership and release-only checks.

For focused Rust work, run the relevant crate tests and Clippy first. CI also checks the primary
and optional-module MSRVs independently using the commands in its `msrv` job. Native doctests run
separately from nextest. For fuzzing, use the pinned nightly and cargo-fuzz from the
[scheduled checks](quality.md#scheduled-deep-checks), then select a target:

```sh
cargo fuzz run --fuzz-dir crates/wasmppt-opc/fuzz open_package
cargo fuzz run --fuzz-dir crates/wasmppt-opc/fuzz package_graph
```

## Evidence and troubleshooting

- Missing offline Cargo dependency: run `cargo fetch --locked` before retrying the hook.
- Generated-artifact drift: run the host build and review its manifest and binding changes.
- Documentation example failure: `npm run check:doc-examples` type-checks the actual TypeScript
  blocks in the [semantic SDK](semantic-sdk.md), [browser integration](browser.md) and the [R2 example](hosts.md#r2-request-example).
- Browser/Pages failure: inspect `target/visual-report` and `target/pages-downloads`; the
  [playground guide](playground.md) describes static assembly and deployment.
- Cross-host or semantic-layout failure: follow the [deck gate](deck-gates.md) and
  [compatibility guide](compatibility.md) to regenerate and compare revision-bound evidence.
- Benchmark regression: retain raw reports and inspect [budget margins](performance.md#release-budgets)
  before changing a threshold.

CI reuses one revision-bound Wasm artifact across host and performance jobs. It never substitutes
an artifact from another revision. The Pages job deploys the exact tested static output on `main`.
Office consumers and full corpus checks have separate [quality tiers](quality.md).

A broken local environment is not permission to weaken a check. If an exceptional local hook
bypass is necessary, disclose it in the PR and reproduce the gate after repair; CI remains
required. Documentation changes follow the [documentation guide](metadoc.md).
