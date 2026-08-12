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

export function contractErrors(inputs) {
  const errors = []
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

  const benchmarkConsumers = `${inputs.browserIntegration}\n${inputs.nativeBenchmark}`
  for (const budget of Object.keys(inputs.budgets.browserScalarWasm)) {
    if (!benchmarkConsumers.includes(`browserScalarWasm.${budget}`)) {
      errors.push(`browser performance budget ${budget} is not enforced by benchmark code`)
    }
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
  }
  const jsonPaths = {
    capabilities: 'capabilities/presentationml.json',
    renderCorpus: 'fixtures/render/corpus.json',
    corpus: 'fixtures/corpus.json',
    budgets: 'benchmarks/budgets.json',
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
