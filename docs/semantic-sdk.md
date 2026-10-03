# Semantic deck SDK

Use `@corca-ai/wasmppt/deck` for host-neutral authoring types, `/browser` for a browser engine,
Canvas and HTML, and `/node` for an in-process Node engine. Both engines expose the same semantic
API over the checked-in Rust/Wasm implementation. The [Starter POTX contract](deck-template.md)
is required. Arbitrary authored templates use the separate [binding API](browser.md).

## Authoring boundary

`DeckSpec` contains a document `key` and ordered `slides`. Slides, nodes, list items, table rows,
columns, and cells have nonempty string keys unique within that document. Keep a key when the
same content moves or changes; allocate a new one for a new semantic object. The SDK owns binary
IDs and WDSF encoding. Hosts must not duplicate either format or derive identity from an offset
that changes on an unrelated edit.

Optional `source` spans describe the current source location, independently of identity. Their
`start` and `end` are UTF-8 byte offsets; `version` is optional host metadata. `DeckSourceText`
converts UTF-16 editor offsets with `toUtf8`, `toUtf16`, and `range`, rejecting positions inside a
Unicode scalar. With no provenance, authoring and rendering still work, but source navigation is
unavailable. The [Rust contracts](deck-engine.md) define roles, rich text, lists, tables, charts,
media relations, and split policy. Product parsing, authorization and UI remain host-owned.

## Browser example

The host bundler creates a module Worker from `@corca-ai/wasmppt/browser-worker`; see
[Worker asset emission](browser.md#connect-a-worker). This example borrows caller buffers and
returns PPTX and standalone HTML from the same accepted snapshot:

```ts
import { createBrowserEngine, CanvasView, exportHtml } from '@corca-ai/wasmppt/browser'
import type { DeckSpec } from '@corca-ai/wasmppt/deck'

export async function renderAndExport(
  worker: Worker, potx: Uint8Array, canvas: HTMLCanvasElement,
): Promise<{ pptx: ArrayBuffer; html: Uint8Array }> {
  const engine = await createBrowserEngine({ worker })
  const view = new CanvasView(canvas)
  const deck = engine.createDeck()
  try {
    const prepared = await engine.prepareDeckTemplate(potx)
    if (!prepared.ok) throw new Error(prepared.diagnostics.map(item => item.message).join('\n'))
    const template = prepared.value
    try {
      const spec: DeckSpec = {
        key: 'quarterly-report',
        slides: [{ key: 'cover', kind: 'title', nodes: [{
          key: 'heading', role: 'title',
          content: { kind: 'text', text: { runs: [{ text: 'Quarterly report' }] } },
        }] }],
      }
      const accepted = await deck.replace({ template, spec })
      if (!accepted.ok) throw new Error(accepted.diagnostics.map(item => item.message).join('\n'))
      const snapshot = accepted.value
      try {
        const page = snapshot.pages[0]
        if (page) await view.render(snapshot, { pageId: page.id, width: 960 })
        const html = await exportHtml(snapshot, { title: 'Quarterly report' })
        const pptx = await new Response(snapshot.exportPptx()).arrayBuffer()
        return { pptx, html: html.bytes }
      } finally { await snapshot.dispose() }
    } finally { await template.dispose() }
  } finally {
    await view.dispose()
    await deck.dispose()
    engine.dispose()
  }
}
```

For an editor, keep the engine, template, deck, and mounted `CanvasView` objects. Each
`deck.replace({ template, spec, assets, fonts }, { signal })` replaces the complete input tuple
atomically. Calls on one deck are queued in invocation order. Rejected input and cancellation
before publication preserve the accepted revision; a late abort does not roll back a published
success. The first accepted revision is zero. Missing resources and invalid object lifetimes
throw; template/spec/planning rejection returns `{ ok: false, diagnostics }`. Runtime, transport,
and memory-budget failures also throw. Diagnostics have string codes, categories, severity,
optional semantic keys, and source ranges; never branch on a message or numeric code range.

## Inputs and ownership

`prepareDeckTemplate` copies POTX bytes and returns `DeckTemplate`. Its immutable `describe()`
result contains page size, layout capabilities, matching names, region frames and accepted roles,
required font families, and diagnostics. Coordinates are English Metric Units (EMU). No consumer
needs to decode a binary plan or search it for strings.

`registerAsset({ bytes, mediaType, intrinsicSize? })` copies authorized image bytes and returns a
caller-owned `DeckAsset` lease. Equal bytes and metadata share one registration and content `id`.
Pass `assets: new Map([['cover-photo', asset]])` and use `assetId: 'cover-photo'` in the spec, or use
the registration ID directly. SVG content can also name a `fallbackAssetId`; PPTX requires its PNG
fallback. Hosts own fetching and SVG production. Keep a registration between edits to avoid
retransmitting it. Dispose each lease when its owner no longer needs it; accepted snapshots retain
referenced media independently.

`registerFonts([{ family, bytes }])` creates a `DeckFonts` lease. Passing it to `replace` supplies
those exact font bytes to semantic measurement, Canvas and HTML. Font families must be unique;
this API selects face zero and does not expose variable-font axes. Missing faces use deterministic
layout fallback with a `font` diagnostic. Browser faces are scoped by content identity, so two
retained font revisions cannot silently use each other's bytes. HTML checks embedding permission;
PPTX does not embed these registered fonts and its viewer must have suitable fonts installed.

A successful `replace` returns a caller-owned immutable `DeckSnapshot`; the deck separately owns
its current snapshot. `retain()` returns another independently disposable lease. Every owner
must call `dispose()`. Replacing or disposing the deck does not invalidate retained snapshots.
Input buffers are never detached from callers. Engine disposal aborts work, terminates the runtime,
and invalidates all its objects.

## Rendering and export

`CanvasView.render` resolves one page and its resources lazily, paints a detached canvas, then
publishes a complete frame. A newer render cancels the older pending render. `hitTest({ x, y })`
accepts viewport client coordinates and returns semantic key, source, page, revision and bounds.
The displayed frame keeps small source metadata rather than retaining an entire old package.
The host owns mounting, visibility, scrolling, pixel width and navigation.

`resolvePage(pageId)` exposes the decoded display scene for advanced consumers. Resources remain
lazy references; use `CanvasView` for a complete Canvas rendering operation. Physical page IDs are
stable for a logical key and continuation ordinal. Source metadata participates in scene
invalidation, so source-only edits cannot return old navigation ranges. Such edits may still
replan; the SDK makes no source-only layout-reuse promise.

`exportHtml(snapshot, { pageIds? })` exports an explicit ordered selection. Omission selects the
non-hidden pages; duplicate or foreign IDs fail. It retains the snapshot until all images,
permitted fonts and print geometry are closed into the artifact. `snapshot.exportPptx()` exports
the complete package, including hidden pages marked hidden, through a pull-driven stream. Read or
cancel that stream: it owns a snapshot lease until completion, cancellation or engine shutdown.
Rendering and either export may read an older retained snapshot while a new revision is accepted.
No application-level revision lock or fabricated session metadata is needed.

## Node example

The `/node` entry is compiled JavaScript, including in an exact-commit Git installation. It needs
no DOM, Worker global, TypeScript loader or private generated-module import:

```ts
import { readFile } from 'node:fs/promises'
import { createNodeEngine } from '@corca-ai/wasmppt/node'

export async function inspectTemplate(path: string) {
  const engine = await createNodeEngine()
  try {
    const prepared = await engine.prepareDeckTemplate(await readFile(path))
    if (!prepared.ok) return prepared
    try { return prepared.value.describe() }
    finally { await prepared.value.dispose() }
  } finally { engine.dispose() }
}
```

Node uses the same in-process dispatch and Rust planner, without browser rendering. Synchronous
Wasm phases run on the calling thread; applications needing CPU isolation must put this adapter
in their own Node worker. Cloudflare's [HTTP adapter](hosts.md#cloudflare-workers) remains separate.

## Limits and verification

`maxRetainedBytes` can tighten the default 256 MiB retained-state account. Registrations, template
bytes and accepted snapshots are charged; each snapshot reserves its complete bounded scene cache
and conservatively charges shared input bytes again. `accountedBytes()` reports this admission
account, not process heap or peak memory. JS metadata, decoded browser images and transient
parser/composer allocations have separate lifetimes and limits. Budget failure publishes no new
snapshot. Release old snapshots to admit further revisions.

The SDK enforces bounded wire payloads, resources, nesting and collection sizes before sending
work to the core. Output pulls are bounded. Browser cancellation is cooperative between bounded
phases; engine disposal is the hard stop. Unknown OOXML preservation and fast-path invalidation
remain governed by [composition](deck-compose.md) and [architecture](architecture.md).

[SDK tests](../packages/wasmppt/test/deck-api.test.mjs) exercise real Wasm snapshots, cancellation,
registered resources, exact fonts, Unicode mapping and retained budgets. The
[browser contract test](../packages/wasmppt/test/deck-api.browser.mjs) runs with the normal browser
host gate and checks concurrent paint publication, hit mapping and selected offline output.
