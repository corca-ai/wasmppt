# wasmppt

`wasmppt` is a high-performance Rust/WebAssembly engine for reading, transforming, writing,
and rendering PowerPoint Open XML packages in browsers, Cloudflare Workers, and native runtimes.
Its primary fast path compiles a POTX/POTM template once, generates PPTX files repeatedly, and
preserves unknown OOXML content that it does not edit.

Status: pre-alpha. Generation API v2, WPDL v10 rendering, Canvas 2D, offline DOM/SVG documents,
native adapters, and Cloudflare R2 generation are implemented and continuously tested. Crates and npm packages are
deliberately unpublished (`publish = false` and `private: true`); there is no stable semver API yet.
The future stability boundary will cover the documented Rust facades, package-root TypeScript
exports, WPPD/WPDL wire versions, error-envelope fields, and released host artifacts.

Try the browser-only [wasmppt playground](https://corca-ai.github.io/wasmppt/). Uploaded templates
are compiled and rendered locally and never leave the page.

## Prerequisites and build

- Rust and Wasm target versions pinned by `rust-toolchain.toml`
- Node.js 24 or newer and npm
- `wasm-bindgen-cli` matching the workspace dependency (the build script checks this)

Until packages are published, clone the repository and build from the workspace:

```sh
npm ci
npm run build:wasm-hosts
npm run build
```

`npm run build:wasm-hosts` emits the scalar engine plus optional metafile and font-shaper Wasm
assets into the browser and Cloudflare packages. `npm run build:pages && npm run test:pages`
assembles and tests a complete static example under `target/pages`. Rust applications currently
depend on the workspace crates by path; the executable examples can be run with:

```sh
cargo run -p wasmppt-opc --example open_rewrite -- input.pptx output.pptx
cargo run -p wasmppt-template --example compile_generate -- template.potx output.pptx title "Quarterly report"
cargo run -p wasmppt-layout --example resolve_slide -- output.pptx 0
```

## Browser generation quickstart

The example assumes the built module Worker from `target/pages/worker.js`. `prepare` transfers the
template buffer, so its `byteLength` becomes zero. The returned handle stays Worker-owned until
`release` completes.

```js
import { WasmpptWorkerClient } from './packages/wasmppt/dist/index.js'

const worker = new Worker('/worker.js', { type: 'module' })
const client = new WasmpptWorkerClient(worker)
const controller = new AbortController()
const template = await document.querySelector('input[type=file]').files[0].arrayBuffer()
const prepared = await client.prepare(template, { macroPolicy: 'strip' })

try {
  const pptx = await client.generate(
    prepared.handle,
    { text: { title: 'Quarterly report' } },
    { signal: controller.signal },
  )
  const url = URL.createObjectURL(new Blob([pptx], {
    type: 'application/vnd.openxmlformats-officedocument.presentationml.presentation',
  }))
  const link = Object.assign(document.createElement('a'), { href: url, download: 'report.pptx' })
  link.click()
  URL.revokeObjectURL(url)
} finally {
  await client.release(prepared.handle)
  client.terminate()
}
```

Use `generateStream` instead of `generate` when the host can consume a
`ReadableStream<Uint8Array>` directly. Cancelling that stream or aborting its signal releases its
generation cursor; it does not release the prepared template handle.

## Browser rendering quickstart

Opening a presentation also transfers its `ArrayBuffer`. Resolve only visible slides, decode the
backend-neutral WPDL scene, and explicitly release the presentation handle.

```js
import {
  CanvasDisplayListRenderer,
  WasmpptWorkerClient,
  decodeDisplayList,
} from './packages/wasmppt/dist/index.js'

const client = new WasmpptWorkerClient(new Worker('/worker.js', { type: 'module' }))
const bytes = await fetch('/report.pptx').then((response) => response.arrayBuffer())
const presentation = await client.openPresentation(bytes)
const canvas = document.querySelector('canvas')
const renderer = new CanvasDisplayListRenderer()

try {
  const scene = decodeDisplayList(await client.resolveSlide(presentation.handle, 0))
  canvas.width = Math.ceil(scene.width / 9_525)
  canvas.height = Math.ceil(scene.height / 9_525)
  await renderer.render(scene, canvas.getContext('2d'))
} finally {
  renderer.clear()
  await client.releasePresentation(presentation.handle)
  client.terminate()
}
```

Canvas owns interactive projection. `serializeDeckSessionToHtml` owns selectable, accessible,
network-closed HTML and browser PDF input from the exact presentable deck-session revision:

```js
import { WasmpptWorkerClient, serializeDeckSessionToHtml } from './packages/wasmppt/dist/index.js'

const client = new WasmpptWorkerClient(new Worker('/worker.js', { type: 'module' }))
const potx = await fetch('/theme.potx').then((response) => response.arrayBuffer())
const wdsf = await fetch('/deck.wdsf').then((response) => response.arrayBuffer())
const template = await client.prepareDeckTemplate(potx)
const session = await client.createDeckSession(template.handle, wdsf)

try {
  const offline = await serializeDeckSessionToHtml(client, session, { title: 'Quarterly report' })
  const url = URL.createObjectURL(new Blob([offline.bytes], { type: 'text/html' }))
  const link = Object.assign(document.createElement('a'), { href: url, download: 'report.html' })
  link.click()
  URL.revokeObjectURL(url)
} finally {
  await client.releaseDeckSession(session.handle)
  await client.releaseDeckTemplate(template.handle)
  client.terminate()
}
```

The serializer reads only package parts named by WPDL, inlines every image/font under a closed
Content Security Policy, freezes GIF to its first frame, and rejects unresolved or unsafe required
resources. Its `@page` geometry comes only from the selected POTX through the deck plan.

## Cloudflare R2 generation and errors

Bind an R2 bucket as `TEMPLATES`, upload `templates/report.potx`, and send the same WPPD v2 payload
used by the browser. The successful response body is a streamed PPTX.

```js
import { encodeInjectionData } from './packages/wasmppt/dist/index.js'

const response = await fetch('/v1/generate?r2=templates%2Freport.potx', {
  method: 'POST',
  headers: { 'content-type': 'application/vnd.corca.wasmppt.injection-v2' },
  body: encodeInjectionData({ text: { title: 'Quarterly report' } }),
})

if (!response.ok) {
  const { error } = await response.json()
  console.error(`${error.domain}/${error.code}`, error.partName, error.causeCode)
  throw new Error(error.message)
}
const pptx = await response.arrayBuffer()
```

Machine logic MUST branch on `domain`, `code`, and optional context fields, never on `message`.
Cloudflare responses use `{ "error": <error-envelope-v1> }`; browser calls reject with
`WasmpptError`, which exposes the same envelope. See [runtime host adapters](docs/hosts.md) for HTTP
status mapping, request limits, direct-template requests, and request-local live generation.

## Ownership and lifecycle rules

| Resource | Ownership rule |
| --- | --- |
| Input `ArrayBuffer` | `prepare` and `openPresentation` transfer it to the Worker; do not reuse it. |
| Opaque handle | Worker-owned; valid only on its originating client and until explicit release. |
| Generated `ArrayBuffer` | Caller-owned after the Promise resolves. |
| Output stream | Caller reads or cancels it; completion/cancellation releases only its cursor. |
| `AbortSignal` | Cooperative cancellation between bounded phases; termination is the hard stop. |
| Client/renderer | Call release methods, `clear()`/`dispose()`, then `terminate()` during teardown. |

Revisions and handles are not interchangeable across Workers. `terminate()` rejects all pending
operations but cannot replace normal per-handle release in a long-lived Worker.

## Project documentation

- [Documentation index](docs/index.md)
- [System architecture](docs/architecture.md)
- [Development and verification guide](docs/develop.md)
- [Release readiness](docs/release.md)
- [Contributor and agent guide](AGENTS.md)

Implementation work is tracked in [GitHub Issues](https://github.com/corca-ai/wasmppt/issues).

## Guiding goals

- Run the same deterministic Rust core across supported hosts.
- Preserve unsupported OOXML content instead of silently discarding it.
- Compile templates once and make repeated injection proportional to changed parts.
- Stream large inputs and outputs with bounded memory.
- Publish reproducible compatibility and performance evidence before making speed claims.

## License

[MIT](LICENSE)
