# Browser integration

Start with [getting started](getting-started.md) to install a pinned source dependency or build the
workspace. All examples run in a browser with a module Worker. Use the
[host contract](hosts.md#browser-worker-protocol) for transport details and error codes.

## Connect a Worker

For a Vite-style host, the bundler emits the exported Worker and its Wasm assets:

```js
import { connectWasmpptBrowserWorker } from '@corca-ai/wasmppt'
import WasmpptBrowserWorker from '@corca-ai/wasmppt/browser-worker?worker'

const client = await connectWasmpptBrowserWorker(new WasmpptBrowserWorker())
try {
  // Call the operations below with this client and your input buffers.
} finally {
  client.terminate()
}
```

The `?worker` import is bundler syntax. Other hosts must emit a module Worker URL themselves and
pass `new Worker(url, { type: 'module' })` to `connectWasmpptBrowserWorker`. Serve the generated
Wasm assets at their emitted URLs. Startup errors or timeouts terminate the Worker and reject.
A custom runtime must finish initialization before constructing `WasmpptWorkerClient`; the
playground's custom Worker has its own handshake and is not this exported entry point.

## Generate from a template

Author a `title` [binding](bindings.md#authoring-bindings) in your POTX/POTM. Read the file with
`await file.arrayBuffer()`, then pass that buffer to this function. `prepare` transfers it;
keep a copy beforehand if the application needs to reuse the bytes.

```ts
import type { WasmpptWorkerClient } from '@corca-ai/wasmppt'

export async function generateReport(
  client: WasmpptWorkerClient,
  template: ArrayBuffer,
  signal: AbortSignal = new AbortController().signal,
): Promise<ArrayBuffer> {
  const prepared = await client.prepare(template, { macroPolicy: 'strip' })
  try {
    return await client.generate(
      prepared.handle,
      { text: { title: 'Quarterly report' } },
      { signal },
    )
  } finally {
    await client.release(prepared.handle)
  }
}
```

For repeated generation, keep the prepared handle until all calls finish. Inspect the returned
binding descriptors and diagnostics rather than guessing authored names. Use `generateStream`
when the host can consume a `ReadableStream<Uint8Array>`; read or cancel it before releasing the
prepared handle. Completion/cancellation releases its cursor, not the template. The
[injection contract](injection.md) covers images, tables, charts, and other structured data.
For partial editor updates, use [live sessions](live-editing.md).

The returned buffer is a PPTX. To download it, wrap it in a Blob with MIME type
`application/vnd.openxmlformats-officedocument.presentationml.presentation`, create an object URL,
and attach it to an `<a download="report.pptx">`. Revoke the URL when the link is replaced or removed.

## Render a slide

Opening also transfers its buffer. Resolve only slides you need and provide a lazy resource
loader so images do not become unavailable-image placeholders. This example uses browser fonts;
configure [FontResolver and embedded fonts](canvas.md#fonts-and-line-wrapping) when exact fonts
are available.

```ts
import {
  CanvasDisplayListRenderer,
  decodeDisplayList,
  decodeRasterImage,
  decodeSvgImage,
  type WasmpptWorkerClient,
} from '@corca-ai/wasmppt'

export async function renderFirstSlide(
  client: WasmpptWorkerClient,
  bytes: ArrayBuffer,
  canvas: HTMLCanvasElement,
): Promise<void> {
  const presentation = await client.openPresentation(bytes)
  const renderer = new CanvasDisplayListRenderer()
  try {
    const scene = decodeDisplayList(await client.resolveSlide(presentation.handle, 0))
    canvas.width = Math.ceil(scene.width / 9_525)
    canvas.height = Math.ceil(scene.height / 9_525)
    const context = canvas.getContext('2d')
    if (context === null) throw new Error('Canvas 2D is unavailable')
    await renderer.render(scene, context, {
      imageResolver: async (image, signal) => {
        if (!image.partName) throw new Error('Image has no package part')
        if (/\.(emf|wmf)$/i.test(image.partName)) {
          const svg = await client.presentationMetafileSvg(
            presentation.handle, image.partName, { signal },
          )
          return decodeSvgImage(svg, signal)
        }
        const resource = await client.presentationResource(
          presentation.handle, image.partName, { signal },
        )
        return /\.svg$/i.test(image.partName)
          ? decodeSvgImage(resource, signal)
          : decodeRasterImage(resource, {}, signal)
      },
    })
  } finally {
    renderer.clear()
    await client.releasePresentation(presentation.handle)
  }
}
```

The EMU-to-pixel conversion uses 96 CSS pixels per inch. Inspect `scene.diagnostics` for preserved
features that are not fully rendered. For scrolling and repeated rendering, retain the renderer
and use the [virtualized viewer](canvas.md#lazy-viewer-and-resource-ownership) instead of reopening
and clearing each slide.

## Export a semantic deck to HTML

Use the [semantic SDK](semantic-sdk.md) to pass a typed `DeckSpec`, render immutable snapshots,
and export an explicit page selection to standalone HTML. The SDK owns binary encoding, resource
registration, cache invalidation and revision lifetimes. `CanvasView` replaces host-written image
loaders and frame publication for this path.

The lower-level `createDeckSession` and `serializeDeckSessionToHtml` operations remain available
for binary-protocol consumers. They require WDSF buffers and an exact-revision read transaction;
see the [host protocol](hosts.md#browser-worker-protocol). Save HTML bytes with an `.html`
extension. The [offline serializer](dom-svg.md) inlines permitted resources and derives print
geometry from the selected POTX. Hidden pages stay in PPTX and are omitted by default from HTML.
Use the browser's print facility for PDF output.

## Ownership and lifecycle

These rules apply to the lower-level client shown above. The [SDK lifecycle](semantic-sdk.md#inputs-and-ownership)
uses copied input buffers and disposable objects instead.

| Resource | Ownership rule |
| --- | --- |
| Input `ArrayBuffer` | Prepare/open/spec operations transfer it; do not reuse a detached buffer. |
| Opaque handle | Valid only on its originating client until explicit release. |
| Generated buffer | Caller-owned after the Promise resolves. |
| Output stream | Read or cancel it; settlement releases only its cursor. |
| `AbortSignal` | Cooperatively cancels work between bounded phases; terminate for a hard stop. |
| Renderer/viewer | Call `clear()` or `dispose()` during teardown. |
| Worker client | Release reusable handles, then call `terminate()` when its owner closes. |

The operation examples borrow the client; the connection owner terminates it in `finally`,
including after preparation or rendering failure. Termination rejects pending operations and
ends the Worker, but does not replace per-handle release in a long-lived application.
