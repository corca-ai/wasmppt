# Browser Dogfood Playground

The public [wasmppt playground](https://corca-ai.github.io/wasmppt/) is a parallel template garden
for Generation API v2. It is a static GitHub Pages application: no presentation, content, or image
is uploaded to a server.

## Workflow

The page loads one scalar Wasm module in a module Worker and prepares two bundled POTX templates in
parallel. It discovers their common text bindings and presents one shared editor. There is no
template picker, upload drop zone, compile button, or generate button.

An input event is coalesced on the next animation frame and the same partial delta is applied to
both independent `LiveSession` handles. This focused four-slide comparison keeps every Canvas
mounted, rerenders only invalidated slides within its own deck, and exports each exact current
revision in the background.
The two generated PPTX files have separate download links. This deliberately resembles a small
CSS Zen Garden: identical content is shown through two genuinely different PowerPoint packages,
not two CSS skins over one rendered scene.

The bundled templates are:

- `fixtures/dogfood/report.potx`, an editorial report with a title slide and metrics table; and
- `fixtures/dogfood/garden.potx`, a high-contrast graphic treatment with the same shared bindings.

Both fixtures are generated in the repository, recorded in `fixtures/corpus.json`, and covered by
the corpus hash gate. They share `title`, `subtitle`, `metrics.label`, `metrics.value`, and `hero`
semantics while retaining separate part graphs, overlays, invalidation, caches, and downloadable
outputs. The generated fixtures include the minimum PresentationML master, layout, theme, and
presentation-properties graph required by desktop PowerPoint.

## Build and deployment

Build and test the exact static artifact locally:

```sh
npm run build:wasm-hosts
npm run build:pages
npm run test:pages
```

The CI `pages-dogfood` job downloads the same revision-bound Wasm artifact used by browser,
workerd, and performance gates. It builds the site and runs the Chrome smoke test. That test proves
there are two distinct initial Canvas results, one edit changes both previews, a burst is coalesced
into one revision per session, no upload control exists, and both downloads pass Microsoft Open XML
SDK validation. Successful `main` pushes upload `target/pages`; the dependent `pages-deploy` job
publishes that artifact through GitHub's official Pages workflow.

## Scope

This is a focused dogfood and comparison surface, not a general template uploader or hosted
document service. It intentionally has no analytics, persistence, remote template fetch, or
server-side generation. The public browser API still accepts caller-provided templates; only this
playground removes the upload UI to make the live-editing path immediately understandable.

Browser memory limits still apply. Applications processing large decks should consume
`generateStream` incrementally instead of collecting every chunk into one Blob as this
download-focused example does.

## Related documents

- See [template bindings and TemplatePlan](bindings.md) for authoring and preparation metadata.
- See [high-speed template injection](injection.md) for structured generation and pull output.
- See [live editing and incremental preview](live-editing.md) for scheduling and revision parity.
- See [runtime host adapters](hosts.md) for browser and Cloudflare memory contracts.
- Return to the [documentation index](index.md) for the project map.
