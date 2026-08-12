# Live editing and incremental preview

Status: implemented browser and core baseline

The live path merges editor deltas into one prepared POTX or POTM session, resolves the affected
slides from a logical package view, and exports the exact same revision as PPTX. Preview never
serializes and reopens an intermediate ZIP.

## Revision contract

`PreparedTemplate::start_live_session` accepts complete initial Generation API v2 data and returns
a `LiveSession` at revision zero. `apply_delta(expected, next, delta)` accepts every v2 operation
kind and has four invariants:

- `expected` must equal the current revision and `next` must be exactly `expected + 1`;
- only keys present in the delta replace current values;
- payload, injection, and logical OPC graph validation finish before the revision commits; and
- rejection leaves the prior data, overlay, and revision unchanged.

The update reports changed binding IDs, exact changed part names, graph-topology change, affected
slide indices, a topology/dependency/none invalidation reason, and overlay reuse counters. Wasm and
JavaScript expose opaque numeric handles;
callers cannot retain Rust references across a memory growth.

Browser live sessions belong to one module Worker and must be released explicitly. Cloudflare's
HTTP adapter deliberately keeps only immutable prepared templates in isolate-global cache. It does
not retain mutable live sessions between requests; a server integration that performs several
edits in one request must create and release its session inside that request.

```js
const session = await client.createLiveSession(prepared.handle, initialData)
const update = await client.applyLiveDelta(session.handle, session.revision, {
  text: { title: 'Current title' },
})
const slide = await client.resolveLiveSlide(update.handle, update.revision, 0)
const pptx = await client.generateLive(update.handle, update.revision)
await client.releaseLiveSession(update.handle)
```

## Virtual package and parity

`PreparedOverlay` implements the OPC `PackagePartSource` capability. It presents a single logical
package made from source ZIP entries, rewritten XML, new media, removed parts, and cloned slides.
Unchanged compressed entries remain in the source archive. Rewritten bytes use shared immutable
ownership across revisions.

Layout and relationship resolution read that source directly. Export walks the same overlay order
through `GenerationCursor`; it does not re-run injection. Integration tests resolve a slide from
the overlay, export it, reopen the PPTX, and require identical display-list bytes, diagnostics, and
dependency fingerprints. Existing macro stripping, unknown-markup preservation, relationship
allocation, chart/workbook atomicity, and deterministic ZIP rules therefore apply to both paths.

## Invalidation and caches

The prepared plan maps binding operations to potentially affected parts. A text edit normally
materializes one slide XML part and shares every unrelated rewritten part from the prior revision.
Image edits additionally invalidate their slide, relationship part, media part, and content types.
Tables, charts and workbooks, semantic shapes, hyperlinks, and notes use their corresponding plan
dependencies. Slide inclusion or cloning changes topology and takes the conservative full rebuild
path.

Every resolved slide fingerprint hashes the slide's complete reachable dependency branch and exact
part bytes. Reverse OPC relationships map changed parts to affected slides. Missing proof selects a
broader invalidation; it never permits a false cache hit.

The bounded cache layers are:

- compiled templates, keyed by template and compiler identity;
- shared patched overlay parts, retained only by current and in-flight immutable revisions;
- 16 MiB Wasm display-list cache, keyed by slide index and dependency fingerprint;
- 32 MiB browser resource cache, keyed by exact content fingerprint and conversion kind;
- 32 MiB decoded-image LRU, keyed by the same content identity;
- 4 MiB text-measurement LRU, keyed by resolved CSS font and text; and
- 8 MiB rich-text layout LRU, keyed by resolver identity and the complete run tree, bounds,
  wrapping, margins, and flow encoded in the display command.

Returning from content A to B and then A reuses A's resource and decoded-image entries while they
remain within budget. Cache telemetry exposes residency, peak bytes, hits, misses, and evictions.
Releasing a session prevents an in-flight resource from repopulating its cache mapping.

## Scheduling and rendering

The dogfood editor accumulates input in a pending delta, schedules at most one update per animation
frame, and permits only one coordinated mutation batch at a time. It applies that shared delta to
two independent template sessions in parallel. Edits that arrive during the batch are coalesced
into the next revision. Each session retains its own revision checks, invalidation set, caches, and
PPTX export; stale results cannot cross from one template into the other.

The general viewer virtualizes offscreen slides as described below. The four-slide parallel garden
keeps both two-slide previews mounted so the comparison remains visually stable during scrolling;
it still resolves only invalidated slides after each edit.

Visible slides render before offscreen work. An `IntersectionObserver` maintains the visible set;
offscreen canvases are unmounted, and a slide is resolved again only when its dependency
fingerprint changed or it becomes visible without a mounted canvas. Existing canvases survive
unrelated edits. Resource reads and display lists cross the Worker boundary as transferables.

The current implementation redraws an invalidated slide as one Canvas unit. Shape-level dirty
rectangles are intentionally gated: the 10/50/200-slide browser benchmark shows that exact
slide-level invalidation is within the release budget, while partial clearing would need new proof
for overlap, effects, group transforms, text reflow, and z-order. It should be added only if a
profile demonstrates a material remaining bottleneck and visual tests can prove equivalence.

## Current-revision download

Preview and download share a session revision. After each accepted edit, the app renders visible
invalidated slides immediately and schedules `generateLive` after a short idle interval. Export
streams the immutable current overlay in the Worker. A newer edit aborts or supersedes older export
work; an older Blob never replaces the current download.

If the user clicks while the Blob is stale, the click is held, the exact current revision is
generated, and a temporary link downloads that result. Blob URLs and generation handles are
released, and only one completed Blob plus one active stream are retained.

## Performance evidence

The native benchmark runs text-heavy, image-heavy, and mixed templates at 10, 50, and 200 slides.
Its live samples separate delta application, dependency invalidation, invalidated-slide resolution,
input-to-render-ready latency, and background export. It records copy counts, maximum invalidated
slides, shared materialized parts, output bytes, and peak resident estimates.

Chromium repeats the mixed 10/50/200 matrix through the real module Worker and scalar Wasm. Each
sample applies editor input, resolves slide zero, paints Canvas pixels, and records render/cache
telemetry; the final revision is streamed to PPTX. CI enforces the live latency, invalidation,
binary-size, and memory budgets in `benchmarks/budgets.json` and uploads raw reports.

Workerd separately measures a bounded `WPLC` request that creates a session, applies a delta,
streams the final PPTX, and releases mutable state before returning. Its raw report records
request-local ownership, copies, output bytes, and p50/p95 under a distinct budget.

## Related documents

- See [high-speed template injection](injection.md) for Generation API v2 semantics.
- See [lazy slide resolution](rendering.md) for dependency traversal and fingerprints.
- See [browser Canvas renderer](canvas.md) for drawing and resource ownership.
- See [runtime host adapters](hosts.md) for Worker and Cloudflare lifecycle rules.
- See [performance contract](performance.md) for reproduction and claim policy.
