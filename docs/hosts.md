# Runtime Host Adapters

The native, browser, and Cloudflare Workers adapters expose the same Rust engine without
allowing host APIs into the six core crates. The executable contract is the shared
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
- querying a conservative prepared-template byte weight;
- generating from parallel binding ID/value arrays;
- draining output by bounded `Uint8Array` chunks;
- explicitly releasing template and output handles.

The scalar artifact is always correct. SIMD and threads are reported as optional runtime
capabilities; neither changes document semantics or enables a code path without a scalar
fallback.

Generated `wasm-bindgen` glue is checked in so browser and Worker packages are usable
without a Rust toolchain. Regenerate it with `npm run build:wasm-hosts`; the generator
version must equal the Rust `wasm-bindgen` dependency.

## Browser Worker protocol

Protocol version 1 uses monotonically allocated request IDs and discriminated messages
for prepare, generate, release, cancel, progress, chunk, success, and error events. The
main thread transfers the input `ArrayBuffer`, so ownership moves to the module Worker
instead of paying a structured-clone copy. Generated chunks are also transferred.

`WasmpptWorkerClient` owns every pending Promise and stream controller. Explicit
termination, Worker `error`, and `messageerror` reject all pending operations. Cancellation
is cooperative: a pre-aborted request fails before dispatch, and an in-flight request is
observed when the Worker yields between output chunks. Synchronous Rust preparation or
generation cannot be interrupted in the middle; applications requiring a hard CPU cancel
should terminate that Worker, which deterministically rejects every pending request.

## Cloudflare Workers

The ES-module Worker accepts `POST /v1/generate`. A template comes from the bounded
request stream or from `?r2=KEY` through the `TEMPLATES` R2 binding. `R2TemplateSource`
uses ranged binding reads, not Cloudflare's REST API. The response is a
`ReadableStream<Uint8Array>` drained from the Wasm output handle in bounded chunks.

The default explicitly accounted budget is 80 MiB:

| Component | Limit |
| --- | ---: |
| Request or R2 input | 16 MiB |
| Generated output | 32 MiB |
| Immutable prepared-plan cache | 32 MiB |
| Output chunk | 256 KiB |

Configuration rejects an input + output + cache total at or above 128 MiB. Cloudflare
documents 128 MB per isolate including JavaScript and WebAssembly memory, so the accounted
budget deliberately leaves headroom for runtime and transient allocations. The response
exposes the accounted ceiling in `x-wasmppt-accounted-memory-bytes` for integration tests.
See [Workers limits](https://developers.cloudflare.com/workers/platform/limits/).

The byte-budgeted LRU stores only immutable prepared handles. It is an optimization:
eviction or a miss recompiles the template and cannot change correctness. All input bytes,
bindings, output handles, offsets, readers, and stream controllers are request-local, in
line with [Cloudflare's global-state guidance](https://developers.cloudflare.com/workers/best-practices/workers-best-practices/).
R2 ranged reads follow the official
[Workers R2 API](https://developers.cloudflare.com/r2/api/workers/workers-api-reference/).

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
`ArrayBuffer.byteLength` becomes zero immediately after transfer. The workerd integration
tests request bodies, R2, streaming PPTX output, cache accounting, and oversized input.
