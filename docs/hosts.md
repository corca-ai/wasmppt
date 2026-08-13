# Runtime Host Adapters

The native, browser, and Cloudflare Workers adapters expose the same Rust engine without
allowing host APIs into the host-agnostic core crates. The executable contract is the shared
`fixtures/host-adapters/minimal.potx` fixture: CI opens and generates it through native
file capabilities, a real Chrome module Worker, and the `workerd` runtime.

## Native

`wasmppt-native` owns filesystem capabilities. `FileSource` implements bounded,
random-access reads over an open file and `FileSink` implements forward-only buffered
output. `MemorySource`, `VecSink`, and the I/O traits remain in `wasmppt-opc` because they
contain no operating-system or JavaScript dependency.

Native code enters the same `TemplateCompiler` and `PreparedTemplate` used by Wasm. The
filesystem adapter does not duplicate ZIP or OOXML logic.

## Wasm boundary

`WasmpptEngine` is an instance-local opaque-handle table. Its narrow ABI supports:

- copying one typed-array template into Wasm and compiling it once;
- compiling with explicit strip/reject and visible-token policies or restoring a
  source-verified binary plan;
- querying discovered bindings, diagnostics, and the binary plan;
- querying a conservative prepared-template byte weight;
- generating from a versioned structured binary payload;
- creating, revising, resolving, exporting, and releasing a revisioned live session;
- pulling ZIP output by bounded `Uint8Array` chunks;
- reading one display-list resource part lazily for browser image decoding; and
- explicitly releasing template, presentation, and generation-cursor handles.

EMF/WMF preview conversion is a second Wasm artifact rather than part of `wasmppt-wasm`.
The browser Worker dynamically imports and instantiates it only for a
`presentation-metafile-svg` request. Its host-agnostic converter accepts at most 8 MiB of
metafile input and returns at most 32 MiB of SVG. This keeps ordinary presentation startup
and the generation-only Cloudflare adapter free of the parser and SVG player code.

Exact font-byte shaping follows the same optional-artifact rule. `wasmppt-shaper-wasm` contains
HarfRust and accepts a bounded font, face index, language, script, OpenType features, variation
coordinates, direction, and UTF-8 string. It also emits bounded UAX #14 line-break offsets. Shaping
returns a compact
versioned `WPSH` run containing font units, glyph IDs, advances, offsets, clusters, and safe-break
flags. The browser loads it only when an application configures `WasmFontShaper`; generation-only
Cloudflare requests never fetch or instantiate it.

The scalar artifact is always correct. SIMD and threads are reported as optional runtime
capabilities; neither changes document semantics or enables a code path without a scalar
fallback.

Generated `wasm-bindgen` glue is checked in so browser and Worker packages are usable
without a Rust toolchain. Regenerate it with `npm run build:wasm-hosts`; the generator
version must equal the Rust `wasm-bindgen` dependency.

## Browser Worker protocol

Protocol version 4 uses monotonically allocated request IDs and discriminated messages
for prepare, generate, release, cancel, progress, chunk, success, and error events. The
main thread transfers the input `ArrayBuffer`, so ownership moves to the module Worker
instead of paying a structured-clone copy. Generation data uses the versioned `WPPD` binary
payload so image bytes need no base64 conversion. Generated chunks are also transferred.
The protocol also carries revisioned delta, live-slide resolution, cache telemetry, lazy
content-fingerprinted resource reads, and EMF/WMF-to-SVG requests; the latter fails explicitly
when a host chooses not to install the optional converter.

`WasmpptWorkerClient` owns every pending Promise and stream controller. Explicit
termination, Worker `error`, and `messageerror` reject all pending operations. Cancellation
is cooperative: a pre-aborted request fails before dispatch, and an in-flight request is
observed when the Worker yields between output pulls. Preparation and dirty-entry preparation
are synchronous; applications requiring a hard CPU cancel during those phases
should terminate that Worker, which deterministically rejects every pending request.

`createLiveSession`, `applyLiveDelta`, `resolveLiveSlide`, and `generateLiveStream` operate on one
Worker-owned session. Exact revision checks make stale work observable. Changed binding IDs, parts,
and slide indices are returned to the host; resources use content fingerprints so an A-B-A edit can
reuse A without confusing relationship IDs or part names for content identity.

## Cloudflare Workers

The ES-module Worker accepts `POST /v1/generate`. A template comes from the bounded
request stream or from `?r2=KEY` through the `TEMPLATES` R2 binding. `R2TemplateSource`
uses ranged binding reads, not Cloudflare's REST API. The response is a
`ReadableStream<Uint8Array>` drained from the Wasm output handle in bounded chunks.

For an R2 template, clients may send the same structured Generation API v2 bytes used by the
browser as an `application/vnd.corca.wasmppt.injection-v2` request body. The v1 media type remains
accepted during migration. Direct-template request
bodies continue to accept the `x-wasmppt-bindings` text-only header as a compatibility path because
the body itself contains the template. New structured integrations SHOULD store templates in R2.

`POST /v1/live-generate?r2=KEY` accepts a bounded `WPLC` v1 bundle containing complete initial WPPD
data followed by zero or more partial WPPD deltas. It creates a live session, applies monotonic
revisions, starts final-revision streaming, and releases the session before `fetch` returns its
response. `x-wasmppt-live-revision` identifies the streamed revision. The endpoint is a request-local
batch primitive, not a cross-request remote editor session. `encodeLiveEditBundle` creates the
binary request body from already encoded WPPD payloads.

The default explicitly accounted ceiling is 96.25 MiB:

| Component | Limit |
| --- | ---: |
| Request or R2 input | 16 MiB |
| Structured injection payload | 16 MiB |
| Dirty-entry working set / output safety ceiling | 32 MiB |
| Immutable prepared-plan cache | 32 MiB |
| Output chunk | 256 KiB |

The completed archive is never retained by the adapter. The 32 MiB output limit is nevertheless
counted conservatively because dirty entry bytes may coexist when a generation cursor starts.
Configuration rejects an input + payload + dirty output + chunk + cache total at or above 128 MiB. Cloudflare
documents 128 MB per isolate including JavaScript and WebAssembly memory, so the accounted
budget deliberately leaves headroom for runtime and transient allocations. The response
exposes the accounted ceiling in `x-wasmppt-accounted-memory-bytes` for integration tests.
See [Workers limits](https://developers.cloudflare.com/workers/platform/limits/).

The byte-budgeted LRU stores only immutable prepared handles. It is an optimization:
eviction or a miss recompiles the template and cannot change correctness. All input bytes,
bindings, output handles, offsets, readers, and stream controllers are request-local, in
line with [Cloudflare's global-state guidance](https://developers.cloudflare.com/workers/best-practices/workers-best-practices/).
Each cache lookup returns a request-scoped lease. Eviction removes the entry from future lookups
immediately, while an active lease defers releasing its Wasm handle until request payload parsing
and generation startup finish. Health telemetry separates lookup-resident bytes from bytes pinned
only by evicted leases.
R2 ranged reads follow the official
[Workers R2 API](https://developers.cloudflare.com/r2/api/workers/workers-api-reference/).
Mutable `LiveSession` handles are also request-local by contract and are never inserted into this
cache. The current HTTP adapter exposes one-shot streaming generation; a future multi-edit request
adapter must create and release its live handle inside `fetch` rather than retain it globally.

`wrangler.jsonc` is the configuration source of truth. `wrangler types` generates `Env`
from the R2 binding, and CI checks it for drift. Runtime tests use Cloudflare's
[Vitest integration](https://developers.cloudflare.com/workers/testing/vitest-integration/),
which executes inside `workerd`.

## Verification

```sh
npm run build:wasm-hosts
WASMPPT_HOST_FIXTURE=fixtures/host-adapters/minimal.potx \
  cargo test -p wasmppt-native --test file_adapters
npm test --workspace @corca-ai/wasmppt-worker
npm run test:browser --workspace @corca-ai/wasmppt
npm run check:core-boundary
```

The browser integration uses a real module Worker and asserts that the caller's
`ArrayBuffer.byteLength` becomes zero immediately after transfer. The browser and Pages tests
also compile the bundled POTX and consume a real pull stream. The workerd integration tests
request bodies, R2, streaming PPTX output, cache accounting, and oversized input.
