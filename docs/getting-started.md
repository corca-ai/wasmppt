# Getting started

`wasmppt` generates and renders PowerPoint Open XML packages through a shared Rust engine.
It is pre-alpha: crates have `publish = false`, npm packages are private, and APIs may change.
Use a pinned source revision until the [release requirements](release.md) are met.

## Choose a path

| Your task | Read next | Input and output |
| --- | --- | --- |
| Fill an authored PowerPoint template | [Browser integration](browser.md), then [bindings](bindings.md) and [injection](injection.md) | POTX/POTM plus binding data → PPTX |
| Generate pages from semantic content | [Deck contracts](deck-engine.md), then [Starter templates](deck-template.md), [layout](deck-layout.md), and [composition](deck-compose.md) | Starter POTX plus `DeckSpec` → planned editable PPTX |
| View an existing presentation | [Browser integration](browser.md), then [Canvas](canvas.md) | PPTX → lazy slide display lists → Canvas |
| Export selectable HTML or print to PDF | [Offline DOM/SVG](dom-svg.md) | Exact semantic deck-session revision → standalone HTML |
| Generate on a server | [Host adapters](hosts.md) | Native file capabilities or Cloudflare streaming HTTP |

The binding and semantic-deck pipelines use different template contracts. An arbitrary POTX can
carry authored bindings; semantic layout requires the explicit Starter profile. Neither pipeline
is a general Markdown parser. Check the [capability matrix](../capabilities/presentationml.json)
for independent read, preserve, edit, and render support. Preserved content is not necessarily
rendered, and exact font fidelity requires the intended font bytes.

To try the workflow first, open the [playground](playground.md). It edits shared content in bundled
templates locally; it is not an uploader for your own presentations.

## Browser source dependency

A bundler that accepts TypeScript source and module Workers can use an exact repository commit:

```json
{
  "dependencies": {
    "@corca-ai/wasmppt": "github:corca-ai/wasmppt#<full-commit-sha>"
  }
}
```

Replace the placeholder with the full commit you reviewed. The repository-root exports include
TypeScript and checked-in scalar Wasm bindings, so installation does not build Rust. The bundler
must emit Worker and Wasm asset URLs; [browser integration](browser.md) shows the startup handshake.
Local workspace package exports instead use built `dist` files.

## Build from a checkout

For JavaScript integration or the static playground, use Node as required by [`package.json`](../package.json):

```sh
npm ci
npm run build
npm run build:pages
```

This uses the checked-in Wasm artifacts. Serve `target/pages` over HTTP to run the playground.
`npm run test:pages` runs the browser smoke test when Chromium is installed. Rebuild Wasm only
when changing Rust or deliberately regenerating artifacts; the [development guide](develop.md)
explains the pinned toolchain, matching `wasm-bindgen` CLI, and provenance checks.

## Native examples

Install the toolchain from [`rust-toolchain.toml`](../rust-toolchain.toml). These runnable examples
show package rewriting, template generation, and lazy slide resolution:

```sh
cargo run -p wasmppt-opc --example open_rewrite -- input.pptx output.pptx
cargo run -p wasmppt-template --example compile_generate -- template.potx output.pptx title "Quarterly report"
cargo run -p wasmppt-layout --example resolve_slide -- output.pptx 0
```

The generation example requires a `title` binding in the input template. Author it using
[shape metadata or a visible token](bindings.md#authoring-bindings). Rust applications can use
workspace crates by path; start with the executable
[compile/generate example](../crates/wasmppt-template/examples/compile_generate.rs) for API usage
and [native host capabilities](hosts.md#native) for file-backed I/O.

## Errors and troubleshooting

Browser failures reject with `WasmpptError`; Cloudflare returns an `error` JSON envelope.
Branch on `domain`, `code`, and context fields, never on the human-readable `message`. The
[host contract](hosts.md#browser-worker-protocol) defines cancellation, limits, and lifecycle.

- Worker startup failure: check that the bundler serves the emitted module and Wasm URLs, then
  use `connectWasmpptBrowserWorker` to catch initialization errors before sending requests.
- Missing binding: inspect `prepare` diagnostics and descriptors, then check the
  [binding precedence rules](bindings.md). Incomplete plans cannot generate through the fast path.
- Missing image or substituted font: configure the [Canvas resource and font resolvers](canvas.md)
  and inspect scene diagnostics; opening a package alone does not decode its resources.
- Stale revision or unknown handle: keep operations on their originating client and use the
  [revision transaction](hosts.md#browser-worker-protocol) for semantic-deck resource reads.
- Layout overflow: inspect the source diagnostic and [layout policy](deck-layout.md); an atomic
  group that cannot fit fails without exposing partial output.

For contributing rather than integration, continue with the [contributor reading path](index.md#contribute)
and [development workflow](develop.md).
