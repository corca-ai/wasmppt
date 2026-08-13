import assert from 'node:assert/strict'
import test from 'node:test'
import {
  checkRepositoryContracts,
  contractErrors,
  qualityGateErrors,
  toolchainErrors,
  workflowPolicyErrors,
} from './check-contract-sync.mjs'

const validToolchains = {
  cargo: '[workspace.package]\nrust-version = "1.85.1"',
  rustToolchain: 'channel = "1.96.0"',
  metafileCargo: 'rust-version = "1.88.0"',
  metafileWasmCargo: 'rust-version = "1.88.0"',
  develop: [
    'Pinned development Rust: 1.96.0',
    'Primary workspace minimum supported Rust version (MSRV): 1.85.1',
    'Optional EMF/WMF converter MSRV: 1.88.0',
    'wasmppt_wasm.wasm wasmppt_metafile_wasm.wasm wasmppt_shaper_wasm.wasm',
  ].join('\n'),
  performance: 'From a clean checkout with Rust 1.88.0',
  corpusWorkflow: 'dtolnay/rust-toolchain@1.85.1',
  wasmBuild: 'wasmppt_wasm.wasm wasmppt_metafile_wasm.wasm wasmppt_shaper_wasm.wasm',
  ci: [
    'rustup toolchain install 1.85.1',
    'cargo +1.85.1 check',
    'dtolnay/rust-toolchain@1.85.1',
    'rustup toolchain install 1.88.0',
    'cargo +1.88.0 check -p wasmppt-metafile',
    'dtolnay/rust-toolchain@1.88.0',
    'wasmppt_wasm.wasm wasmppt_metafile_wasm.wasm wasmppt_shaper_wasm.wasm',
  ].join('\n'),
  capabilities: {
    features: [{ id: 'embedded-fonts', render: 'optional-harfrust-shaping' }],
  },
  nativeBenchmark: 'scalarWasmBytes metafileWasmBytes shaperWasmBytes',
}

test('repository contracts stay synchronized across code, docs, fixtures, and CI', async () => {
  await checkRepositoryContracts()
})

test('workflow policy rejects floating actions and missing timeouts', () => {
  assert.deepEqual(workflowPolicyErrors({
    good: 'jobs:\n  check:\n    timeout-minutes: 10\n    steps:\n      - uses: owner/action@0123456789abcdef0123456789abcdef01234567',
  }), [])
  assert.deepEqual(workflowPolicyErrors({ bad: 'steps:\n  - uses: owner/action@v2' }), [
    'bad action is not pinned to a full commit: owner/action@v2',
    'bad has no explicit job timeout',
  ])
})

test('local and CI quality layers stay mapped to their documented commands', () => {
  const rootPackage = JSON.stringify({ scripts: {
    precommit: 'npm run check:fast && npm run test:fast',
    prepush: 'npm run prepush:rust && npm run prepush:packages && npm run prepush:policy',
    'prepush:rust': 'cargo check && cargo clippy && cargo test --doc wasm32-unknown-unknown',
    'prepush:policy': 'cargo deny && cargo machete',
  } })
  const ci = 'cargo nextest run --workspace cargo test --workspace --all-features --locked --doc cargo-machete@0.9.2 cargo-llvm-cov@0.8.7'
  const quality = 'Quality / repository Rust / native correctness Packages / TypeScript and tests Security / dependency policy npm run precommit npm run prepush'
  const fuzzRunner = 'open_package package_graph slide_geometry template_bindings xml_tokens'
  assert.deepEqual(qualityGateErrors({ rootPackage, ci, quality, fuzzRunner }), [])
  assert.deepEqual(
    qualityGateErrors({ rootPackage, ci: ci.replace('cargo nextest run --workspace', ''), quality, fuzzRunner }),
    ['CI quality policy does not include cargo nextest run --workspace'],
  )
})

test('contract checker reports every independently stale consumer', () => {
  const errors = contractErrors({
    ...validToolchains,
    browserError: 'ERROR_ENVELOPE_VERSION = 1',
    workerError: 'ERROR_ENVELOPE_VERSION = 1',
    wasm: 'set_property(&envelope, "version", &JsValue::from(1))',
    hosts: 'Error envelope version 1',
    rustDisplay: 'pub const DISPLAY_LIST_VERSION: u16 = 2;',
    canvas: 'if (version !== 1) {',
    capabilities: { ...validToolchains.capabilities, displayListVersion: 1 },
    docs: { 'docs/rendering.md': 'WPDL v1' },
    displayTest: 'structural_signature(), 0xaaaa_bbbb',
    ci: `${validToolchains.ci}\ngrep 'signature cccccccc'`,
    workerTest: "signature: 'dddddddd'",
    browserIntegration: "const report = [{ id: 'text', slideIndex: 0 }]",
    nativeBenchmark: validToolchains.nativeBenchmark,
    nativeBudgetEvaluator: '',
    renderCorpus: {
      presentations: [{ path: 'basic.pptx', features: [{ id: 'image' }] }],
    },
    corpus: { fixtures: [] },
    benchmarkFixtures: { slideCounts: [10, 50] },
    budgets: {
      browserScalarWasm: { maximumFirstVisibleSlideMs: 500 },
      native: { matrix: { 10: { maximumP95Ns: {} } } },
    },
  })

  assert.deepEqual(errors, [
    'capability matrix declares WPDL v1; Rust emits v2',
    'TypeScript decoder accepts WPDL versions 1; expected 1, 2',
    'docs/rendering.md does not identify WPDL v2 as the current format',
    'CI expects display signature cccccccc; Rust expects aaaabbbb',
    'Worker expects display signature dddddddd; Rust expects aaaabbbb',
    'visual report features (text) do not match render corpus (image)',
    'render fixture fixtures/render/basic.pptx is absent from fixtures/corpus.json',
    'browser performance budget maximumFirstVisibleSlideMs is not enforced by benchmark code',
    'native performance matrix budgets (10) do not match fixtures (10, 50)',
    'native benchmark does not publish per-metric budget margins',
  ])
})

for (const [name, mutate, expected] of [
  [
    'development toolchain docs',
    (inputs) => ({ ...inputs, develop: inputs.develop.replace('1.96.0', '1.95.0') }),
    'docs/develop.md development Rust 1.95.0 does not match rust-toolchain.toml 1.96.0',
  ],
  [
    'primary MSRV docs',
    (inputs) => ({ ...inputs, develop: inputs.develop.replace('1.85.1', '1.85.0') }),
    'docs/develop.md primary MSRV 1.85.0 does not match Cargo.toml 1.85.1',
  ],
  [
    'CI primary MSRV check',
    (inputs) => ({ ...inputs, ci: inputs.ci.replace('rustup toolchain install 1.85.1', '') }),
    'CI does not install and check primary MSRV 1.85.1',
  ],
  [
    'CI performance MSRV',
    (inputs) => ({
      ...inputs,
      ci: inputs.ci.replace('dtolnay/rust-toolchain@1.85.1', 'dtolnay/rust-toolchain@1.85.0'),
    }),
    'CI performance job does not use primary MSRV 1.85.1',
  ],
  [
    'metafile Wasm crate MSRV',
    (inputs) => ({ ...inputs, metafileWasmCargo: 'rust-version = "1.88.1"' }),
    'metafile Wasm MSRV 1.88.1 does not match metafile MSRV 1.88.0',
  ],
  [
    'metafile MSRV docs',
    (inputs) => ({
      ...inputs,
      develop: inputs.develop.replace('converter MSRV: 1.88.0', 'converter MSRV: 1.88.1'),
    }),
    'docs/develop.md metafile MSRV 1.88.1 does not match crate MSRV 1.88.0',
  ],
  [
    'CI metafile MSRV check',
    (inputs) => ({ ...inputs, ci: inputs.ci.replace('rustup toolchain install 1.88.0', '') }),
    'CI does not install and check optional metafile MSRV 1.88.0',
  ],
  [
    'CI Wasm build Rust',
    (inputs) => ({
      ...inputs,
      ci: inputs.ci.replace('dtolnay/rust-toolchain@1.88.0', 'dtolnay/rust-toolchain@1.88.1'),
    }),
    'CI Wasm build does not use optional metafile MSRV 1.88.0',
  ],
  [
    'scheduled corpus MSRV',
    (inputs) => ({ ...inputs, corpusWorkflow: 'dtolnay/rust-toolchain@1.85.0' }),
    'scheduled corpus workflow does not use primary MSRV 1.85.1',
  ],
  [
    'performance reproduction Rust',
    (inputs) => ({ ...inputs, performance: 'From a clean checkout with Rust 1.88' }),
    'docs/performance.md does not use Wasm build Rust 1.88.0',
  ],
  [
    'capability shaping engine',
    (inputs) => ({
      ...inputs,
      capabilities: { features: [{ id: 'embedded-fonts', render: 'optional-rustybuzz-shaping' }] },
    }),
    'embedded-font capability does not name HarfRust consistently',
  ],
  [
    'developer artifact list',
    (inputs) => ({ ...inputs, develop: inputs.develop.replace(' wasmppt_shaper_wasm.wasm', '') }),
    'docs/develop.md Wasm artifacts (wasmppt_metafile_wasm.wasm, wasmppt_wasm.wasm) do not match scalar, metafile, and shaper artifacts',
  ],
  [
    'CI artifact list',
    (inputs) => ({ ...inputs, ci: inputs.ci.replace(' wasmppt_shaper_wasm.wasm', '') }),
    'CI Wasm artifacts (wasmppt_metafile_wasm.wasm, wasmppt_wasm.wasm) do not match scalar, metafile, and shaper artifacts',
  ],
  [
    'build artifact list',
    (inputs) => ({ ...inputs, wasmBuild: inputs.wasmBuild.replace(' wasmppt_shaper_wasm.wasm', '') }),
    'scripts/build-wasm-hosts.sh Wasm artifacts (wasmppt_metafile_wasm.wasm, wasmppt_wasm.wasm) do not match scalar, metafile, and shaper artifacts',
  ],
  [
    'benchmark artifact field',
    (inputs) => ({ ...inputs, nativeBenchmark: inputs.nativeBenchmark.replace(' shaperWasmBytes', '') }),
    'native benchmark does not report shaperWasmBytes',
  ],
]) {
  test(`toolchain contract rejects stale ${name}`, () => {
    assert.deepEqual(toolchainErrors(mutate(structuredClone(validToolchains))), [expected])
  })
}
