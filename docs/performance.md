# Performance contract and reproducible benchmarks

Status: release budgets implemented; no world-fastest claim is published yet

Performance is a versioned, correctness-gated contract. A result is eligible for comparison only
when its generated package opens as ZIP/OPC, has the requested slide count, resolves to WPDL, and
retains the raw-copy invariant. Cold template compilation and warm injection are always separate.

## Reproduce

From a clean checkout with Rust 1.88, Node 24, `wasm-bindgen-cli` 0.2.127, and Chromium installed:

```sh
npm ci
npm run build:wasm-hosts
node benchmarks/run.mjs
npm ci --prefix benchmarks/comparisons/pptxgenjs --ignore-scripts
npm ci --prefix benchmarks/comparisons/pptx-browser --ignore-scripts
node benchmarks/prepare-browser-comparator.mjs
node benchmarks/comparisons/pptxgenjs/run.mjs 10 10 target/benchmarks/pptxgenjs-text-10.pptx
npm run test:browser --workspace @corca-ai/wasmppt
npm test --workspace @corca-ai/wasmppt-worker
```

`benchmarks/run.mjs` deterministically creates the public 3-by-3 matrix in
`target/benchmark-fixtures`: text-heavy, image-heavy, and mixed templates at 10, 50, and 200
slides. Set `WASMPPT_BENCH_ITERATIONS`; the default is 30. `--ci` uses ten iterations of the
declared release-budget fixture. Generated templates are source artifacts: their generator,
payload dimensions, compression mode, hashes, and redistribution license are recorded in the raw
report rather than hidden behind an unpublished corpus.

## Measurements

The native report contains every nanosecond sample plus p50/p95 and throughput for:

- `coldTemplateCompile`: ZIP index, package graph, binding plan, and immutable prepared cache;
- `warmInjection`: generation from one already-prepared template;
- `firstSlide`: presentation open plus resolution and WPDL encoding of slide zero;
- `visibleSlides`: presentation open plus the first three slides (or fewer);
- `allSlides`: presentation open plus every slide.

It also records input/output bytes, conservative prepared-plan resident bytes, OS-process peak RSS,
input/output copy counts, raw-copied bytes and entries, inflated and recompressed entries, scalar
Wasm binary size, revision and source dirty state, separately listed regenerated tracked build
artifacts, fixture hashes, CPU/RAM/OS/runtime, iteration count,
release profile, and compression configuration. Browser and workerd reports retain their own raw
warm samples because host scheduling cannot honestly be folded into a native headline.

The primary scalar Wasm size excludes the optional metafile converter. EMF/WMF presentations load
the separately reported converter artifact on first use; presentations without metafiles neither
fetch nor instantiate it. Both sizes remain visible so optional capability cost is not hidden.

## Release budgets

`benchmarks/budgets.json` is the only budget source. CI runs the actual native release binary,
Chromium module Worker with scalar Wasm, and Cloudflare workerd. It fails on p95 ceilings, native
peak RSS, scalar Wasm size, accounted Worker memory, loss of raw copies, any generation-time ZIP
inflation, or correctness failure. Absolute budgets are intentionally broad enough for shared CI;
tightening them is reviewed like an API change. Published artifacts contain the raw JSON and exact
generated budget fixture for each revision.

## Comparisons and claims

Competitor results belong under `benchmarks/comparisons/` and must name the exact package version,
runtime/browser, API settings, workload adapter, output validation, and known semantic differences.
PptxGenJS 4.0.1 is the initial named generation comparator; it authors a new deck and does not
perform POTX/POTM template injection, so its number must not be presented as an equivalent warm
injection result. Its dependencies are isolated from the product workspaces and the adapter uses
only generated text; its pinned dependency tree currently reports an `image-size` denial-of-service
advisory, which is another reason it must never process untrusted comparator inputs. Browser
renderers likewise require the same input deck, viewport, font/image
resources, visible-slide set, and pixel/semantic correctness thresholds.

The initial Canvas comparison pins `pptx-browser` 4.1.4. Version 4.1.5 was checked first but its
published npm tarball omits required modules including `src/zip.js` and `src/render.js`, so it is
reported as excluded rather than silently replaced or assigned a fabricated timing.
Version 4.1.4 loads the pinned deck but catches internal failures for the required text shapes.
Its raw timings remain visible with `eligible: false`; they cannot
beat a renderer that produced the required pixels.

No “world's fastest” or unqualified “fastest” claim is permitted until a committed raw comparison
for a bounded workload beats named current versions on declared hardware and passes all correctness
checks. A future claim must link that raw file and repeat its workload boundary in the same sentence.
