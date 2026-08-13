import { pathToFileURL } from 'node:url'
import { resolve } from 'node:path'
import { readFile } from 'node:fs/promises'

const root = new URL('../', import.meta.url)

function matchOne(source, pattern, label) {
  const match = source.match(pattern)
  if (match === null) throw new Error(`cannot read ${label}`)
  return match[1]
}

function sorted(values) {
  return [...values].toSorted((left, right) => left.localeCompare(right))
}

function sameValues(left, right) {
  return left.length === right.length && left.every((value, index) => value === right[index])
}

function artifactNames(source) {
  return sorted(new Set(source.match(/wasmppt(?:_metafile|_shaper)?_wasm\.wasm/g) ?? []))
}

export function toolchainErrors(inputs) {
  const errors = []
  const primary = matchOne(
    inputs.cargo,
    /\[workspace\.package\][\s\S]*?rust-version = "([^"]+)"/,
    'workspace MSRV',
  )
  const development = matchOne(
    inputs.rustToolchain,
    /channel = "([^"]+)"/,
    'development Rust toolchain',
  )
  const metafile = matchOne(
    inputs.metafileCargo,
    /rust-version = "([^"]+)"/,
    'metafile MSRV',
  )
  const metafileWasm = matchOne(
    inputs.metafileWasmCargo,
    /rust-version = "([^"]+)"/,
    'metafile Wasm MSRV',
  )
  const documentedDevelopment = matchOne(
    inputs.develop,
    /Pinned development Rust: ([0-9.]+)/,
    'documented development Rust',
  )
  const documentedPrimary = matchOne(
    inputs.develop,
    /Primary workspace minimum supported Rust version \(MSRV\): ([0-9.]+)/,
    'documented primary MSRV',
  )
  const documentedMetafile = matchOne(
    inputs.develop,
    /Optional EMF\/WMF converter MSRV: ([0-9.]+)/,
    'documented metafile MSRV',
  )
  if (documentedDevelopment !== development) {
    errors.push(`docs/develop.md development Rust ${documentedDevelopment} does not match rust-toolchain.toml ${development}`)
  }
  if (documentedPrimary !== primary) {
    errors.push(`docs/develop.md primary MSRV ${documentedPrimary} does not match Cargo.toml ${primary}`)
  }
  if (metafileWasm !== metafile) {
    errors.push(`metafile Wasm MSRV ${metafileWasm} does not match metafile MSRV ${metafile}`)
  }
  if (documentedMetafile !== metafile) {
    errors.push(`docs/develop.md metafile MSRV ${documentedMetafile} does not match crate MSRV ${metafile}`)
  }
  if (!inputs.ci.includes(`rustup toolchain install ${primary}`) ||
      !inputs.ci.includes(`cargo +${primary} check`)) {
    errors.push(`CI does not install and check primary MSRV ${primary}`)
  }
  if (!inputs.ci.includes(`dtolnay/rust-toolchain@${primary}`)) {
    errors.push(`CI performance job does not use primary MSRV ${primary}`)
  }
  if (!inputs.corpusWorkflow.includes(`dtolnay/rust-toolchain@${primary}`)) {
    errors.push(`scheduled corpus workflow does not use primary MSRV ${primary}`)
  }
  if (!inputs.ci.includes(`rustup toolchain install ${metafile}`) ||
      !inputs.ci.includes(`cargo +${metafile} check -p wasmppt-metafile`)) {
    errors.push(`CI does not install and check optional metafile MSRV ${metafile}`)
  }
  if (!inputs.ci.includes(`dtolnay/rust-toolchain@${metafile}`)) {
    errors.push(`CI Wasm build does not use optional metafile MSRV ${metafile}`)
  }
  if (!inputs.performance.includes(`Rust ${metafile}`)) {
    errors.push(`docs/performance.md does not use Wasm build Rust ${metafile}`)
  }

  const embeddedFonts = inputs.capabilities.features.find((feature) => feature.id === 'embedded-fonts')
  const render = embeddedFonts?.render ?? ''
  if (!render.includes('harfrust') || render.includes('rustybuzz')) {
    errors.push('embedded-font capability does not name HarfRust consistently')
  }

  const expectedArtifacts = [
    'wasmppt_metafile_wasm.wasm',
    'wasmppt_shaper_wasm.wasm',
    'wasmppt_wasm.wasm',
  ]
  for (const [consumer, source] of [
    ['scripts/build-wasm-hosts.sh', inputs.wasmBuild],
    ['docs/develop.md', inputs.develop],
    ['CI', inputs.ci],
  ]) {
    const actual = artifactNames(source)
    if (!sameValues(actual, expectedArtifacts)) {
      errors.push(`${consumer} Wasm artifacts (${actual.join(', ')}) do not match scalar, metafile, and shaper artifacts`)
    }
  }
  for (const field of ['scalarWasmBytes', 'metafileWasmBytes', 'shaperWasmBytes']) {
    if (!inputs.nativeBenchmark.includes(field)) {
      errors.push(`native benchmark does not report ${field}`)
    }
  }
  return errors
}

export function contractErrors(inputs) {
  const errors = toolchainErrors(inputs)
  const browserErrorVersion = Number(matchOne(
    inputs.browserError,
    /ERROR_ENVELOPE_VERSION = (\d+)/,
    'browser error envelope version',
  ))
  const workerErrorVersion = Number(matchOne(
    inputs.workerError,
    /ERROR_ENVELOPE_VERSION = (\d+)/,
    'workerd error envelope version',
  ))
  const wasmErrorVersion = Number(matchOne(
    inputs.wasm,
    /"version", &JsValue::from\((\d+)\)/,
    'Wasm error envelope version',
  ))
  const documentedErrorVersion = Number(matchOne(
    inputs.hosts,
    /Error envelope version (\d+)/,
    'documented error envelope version',
  ))
  if (!sameValues(
    [browserErrorVersion, workerErrorVersion, wasmErrorVersion],
    [documentedErrorVersion, documentedErrorVersion, documentedErrorVersion],
  )) {
    errors.push(
      `error envelope versions browser=${browserErrorVersion}, workerd=${workerErrorVersion}, Wasm=${wasmErrorVersion}; docs=${documentedErrorVersion}`,
    )
  }
  const rustVersion = Number(matchOne(
    inputs.rustDisplay,
    /DISPLAY_LIST_VERSION:\s*u16\s*=\s*(\d+)/,
    'Rust display-list version',
  ))
  if (inputs.capabilities.displayListVersion !== rustVersion) {
    errors.push(
      `capability matrix declares WPDL v${inputs.capabilities.displayListVersion}; Rust emits v${rustVersion}`,
    )
  }

  const decoderGuard = matchOne(
    inputs.canvas,
    /if \(((?:version !== \d+(?: && )?)+)\) \{/,
    'TypeScript display-list compatibility guard',
  )
  const decoderVersions = [...decoderGuard.matchAll(/version !== (\d+)/g)]
    .map((match) => Number(match[1]))
    .toSorted((left, right) => left - right)
  const requiredVersions = Array.from({ length: rustVersion }, (_, index) => index + 1)
  if (!sameValues(decoderVersions, requiredVersions)) {
    errors.push(
      `TypeScript decoder accepts WPDL versions ${decoderVersions.join(', ')}; expected ${requiredVersions.join(', ')}`,
    )
  }

  for (const [path, source] of Object.entries(inputs.docs)) {
    if (!source.includes(`v${rustVersion}`) && !source.includes(`version ${rustVersion}`)) {
      errors.push(`${path} does not identify WPDL v${rustVersion} as the current format`)
    }
  }

  const rustSignature = matchOne(
    inputs.displayTest,
    /structural_signature\(\),\s*0x([0-9a-f_]+)/,
    'Rust display-list fixture signature',
  ).replaceAll('_', '')
  const ciSignature = matchOne(
    inputs.ci,
    /grep 'signature ([0-9a-f]+)'/,
    'CI display-list fixture signature',
  )
  const workerSignature = matchOne(
    inputs.workerTest,
    /signature:\s*'([0-9a-f]+)'/,
    'Worker display-list fixture signature',
  )
  for (const [consumer, signature] of [['CI', ciSignature], ['Worker', workerSignature]]) {
    if (signature !== rustSignature) {
      errors.push(`${consumer} expects display signature ${signature}; Rust expects ${rustSignature}`)
    }
  }

  const reportFeatures = [...inputs.browserIntegration.matchAll(
    /\{ id: '([^']+)', slideIndex:/g,
  )].map((match) => match[1])
  const corpusFeatures = inputs.renderCorpus.presentations.flatMap(
    (presentation) => presentation.features.map((feature) => feature.id),
  )
  if (!sameValues(sorted(new Set(reportFeatures)), sorted(new Set(corpusFeatures)))) {
    errors.push(
      `visual report features (${sorted(new Set(reportFeatures)).join(', ')}) do not match render corpus (${sorted(new Set(corpusFeatures)).join(', ')})`,
    )
  }

  const registeredFixtures = new Set(
    inputs.corpus.fixtures.flatMap((fixture) => fixture.path === undefined ? [] : [fixture.path]),
  )
  for (const presentation of inputs.renderCorpus.presentations) {
    const path = `fixtures/render/${presentation.path}`
    if (!registeredFixtures.has(path)) errors.push(`render fixture ${path} is absent from fixtures/corpus.json`)
  }

  const benchmarkConsumers = [
    inputs.browserIntegration,
    inputs.nativeBenchmark,
    inputs.nativeBudgetEvaluator,
  ].join('\n')
  for (const budget of Object.keys(inputs.budgets.browserScalarWasm)) {
    if (!benchmarkConsumers.includes(`browserScalarWasm.${budget}`)) {
      errors.push(`browser performance budget ${budget} is not enforced by benchmark code`)
    }
  }

  const fixtureSlideCounts = inputs.benchmarkFixtures.slideCounts.toSorted((left, right) => left - right)
  const budgetSlideCounts = Object.keys(inputs.budgets.native.matrix)
    .map(Number)
    .toSorted((left, right) => left - right)
  if (!sameValues(fixtureSlideCounts, budgetSlideCounts)) {
    errors.push(
      `native performance matrix budgets (${budgetSlideCounts.join(', ')}) do not match fixtures (${fixtureSlideCounts.join(', ')})`,
    )
  }
  if (!inputs.nativeBenchmark.includes('budgetEvaluation') ||
      !inputs.nativeBudgetEvaluator.includes('marginPercent')) {
    errors.push('native benchmark does not publish per-metric budget margins')
  }

  return errors
}

export async function readRepositoryContracts() {
  const textPaths = {
    rustDisplay: 'crates/wasmppt-display/src/lib.rs',
    canvas: 'packages/wasmppt/src/canvas.ts',
    displayTest: 'crates/wasmppt-display/tests/display_list.rs',
    ci: '.github/workflows/ci.yml',
    workerTest: 'packages/wasmppt-worker/test/worker.spec.ts',
    browserIntegration: 'packages/wasmppt/test/browser-host.integration.mjs',
    nativeBenchmark: 'benchmarks/run.mjs',
    nativeBudgetEvaluator: 'benchmarks/budget-evaluation.mjs',
    browserError: 'packages/wasmppt/src/error.ts',
    workerError: 'packages/wasmppt-worker/src/error.ts',
    wasm: 'crates/wasmppt-wasm/src/lib.rs',
    hosts: 'docs/hosts.md',
    cargo: 'Cargo.toml',
    rustToolchain: 'rust-toolchain.toml',
    metafileCargo: 'crates/wasmppt-metafile/Cargo.toml',
    metafileWasmCargo: 'crates/wasmppt-metafile-wasm/Cargo.toml',
    develop: 'docs/develop.md',
    performance: 'docs/performance.md',
    corpusWorkflow: '.github/workflows/corpus-scorecard.yml',
    wasmBuild: 'scripts/build-wasm-hosts.sh',
  }
  const jsonPaths = {
    capabilities: 'capabilities/presentationml.json',
    renderCorpus: 'fixtures/render/corpus.json',
    corpus: 'fixtures/corpus.json',
    budgets: 'benchmarks/budgets.json',
    benchmarkFixtures: 'benchmarks/fixtures.json',
  }
  const docPaths = [
    'docs/rendering.md',
    'docs/canvas.md',
    'docs/dom-svg.md',
    'docs/compatibility.md',
  ]
  const textEntries = await Promise.all(Object.entries(textPaths).map(async ([key, path]) => [
    key,
    await readFile(new URL(path, root), 'utf8'),
  ]))
  const jsonEntries = await Promise.all(Object.entries(jsonPaths).map(async ([key, path]) => [
    key,
    JSON.parse(await readFile(new URL(path, root), 'utf8')),
  ]))
  const docs = Object.fromEntries(await Promise.all(docPaths.map(async (path) => [
    path,
    await readFile(new URL(path, root), 'utf8'),
  ])))
  return { ...Object.fromEntries(textEntries), ...Object.fromEntries(jsonEntries), docs }
}

export async function checkRepositoryContracts() {
  const errors = contractErrors(await readRepositoryContracts())
  if (errors.length > 0) throw new Error(`repository contracts are out of sync:\n- ${errors.join('\n- ')}`)
}

if (process.argv[1] !== undefined && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  await checkRepositoryContracts()
  console.log('repository contracts are synchronized')
}
