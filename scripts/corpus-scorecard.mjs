import { spawnSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { inflateRawSync } from 'node:zlib'

const root = resolve(new URL('../', import.meta.url).pathname)

export async function main(arguments_ = process.argv.slice(2)) {
  const manifestBytes = await readFile(resolve(root, 'fixtures/corpus.json'))
  const manifest = JSON.parse(manifestBytes)
  const all = arguments_.includes('--all')
  const outputArgument = arguments_.find((value) => value.startsWith('--output='))
  const output = resolve(
    root,
    outputArgument?.slice('--output='.length) ?? 'target/corpus-scorecard.json',
  )
  const binary = resolve(
    root,
    process.platform === 'win32' ? 'target/debug/wasmppt.exe' : 'target/debug/wasmppt',
  )
  const fixtures = manifest.fixtures.filter((fixture) =>
    fixture.path?.endsWith('.pptx') && (all
      ? fixture.tier === 'pull-request' || fixture.tier === 'scheduled'
      : fixture.tier === 'pull-request'))
  const temporary = await mkdtemp(resolve(tmpdir(), 'wasmppt-scorecard-'))
  try {
    const results = []
    for (const fixture of fixtures) {
      results.push(await scoreFixture(fixture, { binary, temporary }))
    }
    const features = aggregateFeatures(results)
    const version = run(binary, ['--version'])
    const report = {
      schema: 2,
      tier: all ? 'scheduled' : 'pull-request',
      generatedAt: new Date().toISOString(),
      invocation: ['node', 'scripts/corpus-scorecard.mjs', ...arguments_],
      tools: {
        node: process.version,
        wasmppt: version.stdout.trim(),
        platform: `${process.platform}-${process.arch}`,
      },
      manifestSha256: sha256(manifestBytes),
      presentations: results,
      features,
    }
    await mkdir(dirname(output), { recursive: true })
    await writeFile(output, `${JSON.stringify(report, null, 2)}\n`)
    if (results.some((result) => !result.matchesExpected)) process.exitCode = 1
    return report
  } finally {
    await rm(temporary, { recursive: true, force: true })
  }
}

export async function scoreFixture(fixture, {
  binary,
  temporary,
  execute = run,
  comparePreservationEvidence = comparePreservation,
  compareEditEvidence = compareEdit,
}) {
  const input = resolve(root, fixture.path)
  const source = await readFile(input)
  const actualSha256 = sha256(source)
  const declaration = fixture.scorecard
  const declarationFailures = validateDeclaration(fixture)

  const openOperation = execute(binary, ['validate', input])
  const open = stage([openOperation], [
    ...declarationFailures,
    ...(actualSha256 === fixture.sha256
      ? []
      : [`fixture hash is ${actualSha256}; expected ${fixture.sha256}`]),
  ])

  const preserveOutput = resolve(temporary, `${fixture.id}-preserve.pptx`)
  const preserveOperations = [execute(binary, [
    'inject-text',
    input,
    preserveOutput,
    declaration.edit.binding,
    `Preservation probe ${fixture.id}`,
  ])]
  let preservation
  const preserveChecks = []
  if (preserveOperations[0].exitCode === 0) {
    preserveOperations.push(execute(binary, ['validate', preserveOutput]))
    try {
      preservation = await comparePreservationEvidence(
        input,
        preserveOutput,
        declaration.preserve,
        [declaration.edit.part],
      )
      preserveChecks.push(...preservation.failures)
    } catch (error) {
      preserveChecks.push(`preservation comparison failed: ${errorMessage(error)}`)
    }
  }
  const preserve = stage(preserveOperations, preserveChecks)

  const editOutput = resolve(temporary, `${fixture.id}-edit.pptx`)
  const editOperations = [execute(binary, [
    'inject-text',
    input,
    editOutput,
    declaration.edit.binding,
    declaration.edit.value,
  ])]
  let editEvidence
  const editChecks = []
  if (editOperations[0].exitCode === 0) {
    editOperations.push(execute(binary, ['validate', editOutput]))
    editOperations.push(execute(binary, ['resolve', editOutput, String(declaration.edit.slide)]))
    try {
      editEvidence = await compareEditEvidence(input, editOutput, declaration.edit)
      editChecks.push(...editEvidence.failures)
      if (!editOperations[2].stdout.includes(declaration.edit.value)) {
        editChecks.push('resolved edited slide does not contain the declared value')
      }
    } catch (error) {
      editChecks.push(`edit comparison failed: ${errorMessage(error)}`)
    }
  }
  const edit = stage(editOperations, editChecks)

  const renderOperations = declaration.slides.map((slide) =>
    execute(binary, ['resolve', input, String(slide)]))
  const actualDiagnostics = diagnosticCodes(renderOperations)
  const expectedDiagnostics = [...fixture.expected.diagnostics].toSorted()
  const diagnosticsMatch = arraysEqual(actualDiagnostics, expectedDiagnostics)
  const renderChecks = []
  if (!diagnosticsMatch) {
    renderChecks.push(
      `diagnostics are ${actualDiagnostics.join(', ') || '(none)'}; expected ${expectedDiagnostics.join(', ') || '(none)'}`,
    )
  }
  for (const region of declaration.featureRegions) {
    if (!declaration.slides.includes(region.slide)) {
      renderChecks.push(`feature region slide ${region.slide} is not declared for structural resolve`)
    }
  }
  const render = stage(renderOperations, renderChecks)
  const structuralSlides = declaration.slides.map((slide, index) => ({
    slide,
    status: renderOperations[index].exitCode === 0 ? 'pass' : 'fail',
    command: renderOperations[index].command,
  }))
  const structuralRegions = declaration.featureRegions.map((region) => ({
    ...region,
    status: structuralSlides.find((slide) => slide.slide === region.slide)?.status ?? 'fail',
  }))
  const stages = { open, preserve, edit, render }
  const outcomes = outcomesFromStages(stages)
  const expected = Object.fromEntries(
    ['open', 'preserve', 'edit', 'render'].map((dimension) => [
      dimension,
      fixture.expected[dimension],
    ]),
  )
  const matchesExpected = resultMatchesExpected(outcomes, expected, diagnosticsMatch)

  return {
    id: fixture.id,
    path: fixture.path,
    sha256: fixture.sha256,
    actualSha256,
    featureTags: fixture.featureTags ?? [],
    declaredSlides: declaration.slides,
    featureRegions: declaration.featureRegions,
    expected,
    ...outcomes,
    matchesExpected,
    diagnostics: {
      expected: expectedDiagnostics,
      actual: actualDiagnostics,
      match: diagnosticsMatch,
    },
    fidelity: {
      structuralResolve: {
        status: outcomes.render,
        slides: structuralSlides,
        featureRegions: structuralRegions,
      },
      pixel: {
        status: 'not-run',
        evidence: 'target/visual-report/report.json from the browser visual gate',
      },
      desktopConsumers: {
        status: 'not-run',
        evidence: 'office-ground-truth workflow artifacts',
      },
    },
    evidence: { preservation, edit: editEvidence },
    stages,
  }
}

export function outcomesFromStages(stages) {
  return {
    open: stages.open.status,
    preserve: stages.preserve.status,
    edit: stages.edit.status,
    render: stages.render.status,
  }
}

export function resultMatchesExpected(outcomes, expected, diagnosticsMatch) {
  return Object.entries(outcomes).every(
    ([dimension, status]) => status === expected[dimension],
  ) && diagnosticsMatch
}

export async function comparePreservation(
  sourcePath,
  outputPath,
  declaration,
  allowedChangedParts = [],
) {
  const comparison = await compareArchives(sourcePath, outputPath)
  const output = parseZip(await readFile(outputPath))
  const allowed = new Set(allowedChangedParts)
  const unexpectedDifferences = comparison.differences.filter(
    (difference) => !allowed.has(difference.name),
  )
  const declared = {
    unknownXmlParts: declaredParts(output, declaration.unknownXmlParts, comparison.differences),
    relationshipParts: declaredParts(output, declaration.relationshipParts, comparison.differences),
    opaqueParts: declaredParts(output, declaration.opaqueParts, comparison.differences),
  }
  const missingDeclared = Object.values(declared)
    .flat()
    .filter((part) => !part.present)
    .map((part) => `declared preservation part is missing: ${part.name}`)
  const changedDeclared = Object.values(declared)
    .flat()
    .filter((part) => !part.unchanged)
    .map((part) => `declared preservation part changed: ${part.name}`)
  return {
    sourceSha256: comparison.sourceSha256,
    outputSha256: comparison.outputSha256,
    entries: comparison.entries,
    differences: comparison.differences,
    allowedChangedParts,
    unexpectedDifferences,
    declared,
    failures: [
      ...unexpectedDifferences.map((difference) => formatDifference(difference)),
      ...missingDeclared,
      ...changedDeclared,
    ],
  }
}

export async function compareEdit(sourcePath, outputPath, declaration) {
  const comparison = await compareArchives(sourcePath, outputPath)
  const unrelated = comparison.differences.filter(
    (difference) => difference.name !== declaration.part,
  )
  const changed = comparison.differences.some(
    (difference) => difference.name === declaration.part && difference.kind === 'changed',
  )
  const output = parseZip(await readFile(outputPath))
  const edited = output.get(declaration.part)
  const decoded = edited === undefined ? '' : new TextDecoder().decode(inflateEntry(edited))
  const escapedValue = escapeXml(declaration.value)
  const failures = unrelated.map((difference) => formatDifference(difference))
  if (!changed) failures.push(`declared edited part did not change: ${declaration.part}`)
  if (!decoded.includes(escapedValue)) {
    failures.push(`declared edited part does not contain escaped value: ${declaration.part}`)
  }
  return {
    sourceSha256: comparison.sourceSha256,
    outputSha256: comparison.outputSha256,
    changedPart: declaration.part,
    changed,
    escapedValuePresent: decoded.includes(escapedValue),
    unrelatedDifferences: unrelated,
    failures,
  }
}

async function compareArchives(sourcePath, outputPath) {
  const sourceBytes = await readFile(sourcePath)
  const outputBytes = await readFile(outputPath)
  const source = parseZip(sourceBytes)
  const output = parseZip(outputBytes)
  const names = new Set([...source.keys(), ...output.keys()])
  const differences = []
  let unchanged = 0
  for (const name of [...names].toSorted()) {
    const before = source.get(name)
    const after = output.get(name)
    if (before === undefined) {
      differences.push({ name, kind: 'added' })
    } else if (after === undefined) {
      differences.push({ name, kind: 'removed' })
    } else if (
      before.method !== after.method ||
      before.crc32 !== after.crc32 ||
      before.uncompressedSize !== after.uncompressedSize ||
      !before.compressed.equals(after.compressed)
    ) {
      differences.push({ name, kind: 'changed' })
    } else {
      unchanged += 1
    }
  }
  return {
    sourceSha256: sha256(sourceBytes),
    outputSha256: sha256(outputBytes),
    entries: { source: source.size, output: output.size, unchanged },
    differences,
  }
}

function parseZip(bytes) {
  const eocd = findEndOfCentralDirectory(bytes)
  const count = bytes.readUInt16LE(eocd + 10)
  let offset = bytes.readUInt32LE(eocd + 16)
  const entries = new Map()
  for (let index = 0; index < count; index += 1) {
    if (bytes.readUInt32LE(offset) !== 0x02014b50) throw new Error('invalid ZIP central directory')
    const method = bytes.readUInt16LE(offset + 10)
    const crc32 = bytes.readUInt32LE(offset + 16)
    const compressedSize = bytes.readUInt32LE(offset + 20)
    const uncompressedSize = bytes.readUInt32LE(offset + 24)
    const nameLength = bytes.readUInt16LE(offset + 28)
    const extraLength = bytes.readUInt16LE(offset + 30)
    const commentLength = bytes.readUInt16LE(offset + 32)
    const localOffset = bytes.readUInt32LE(offset + 42)
    const name = bytes.subarray(offset + 46, offset + 46 + nameLength).toString('utf8')
    if (bytes.readUInt32LE(localOffset) !== 0x04034b50) throw new Error(`invalid local header: ${name}`)
    const localNameLength = bytes.readUInt16LE(localOffset + 26)
    const localExtraLength = bytes.readUInt16LE(localOffset + 28)
    const dataOffset = localOffset + 30 + localNameLength + localExtraLength
    if (entries.has(name)) throw new Error(`duplicate ZIP entry: ${name}`)
    entries.set(name, {
      name,
      method,
      crc32,
      uncompressedSize,
      compressed: bytes.subarray(dataOffset, dataOffset + compressedSize),
    })
    offset += 46 + nameLength + extraLength + commentLength
  }
  return entries
}

function findEndOfCentralDirectory(bytes) {
  const minimum = Math.max(0, bytes.length - 65_557)
  for (let offset = bytes.length - 22; offset >= minimum; offset -= 1) {
    if (bytes.readUInt32LE(offset) === 0x06054b50) return offset
  }
  throw new Error('ZIP end-of-central-directory record is missing')
}

function inflateEntry(entry) {
  if (entry.method === 0) return entry.compressed
  if (entry.method === 8) return inflateRawSync(entry.compressed)
  throw new Error(`unsupported ZIP compression method ${entry.method} in ${entry.name}`)
}

function declaredParts(entries, names, differences) {
  return names.map((name) => ({
    name,
    present: entries.has(name),
    unchanged: !differences.some((difference) => difference.name === name),
  }))
}

function validateDeclaration(fixture) {
  const declaration = fixture.scorecard
  if (declaration === undefined) return ['fixture has no scorecard declaration']
  const failures = []
  if (!Array.isArray(declaration.slides) || declaration.slides.length === 0) {
    failures.push('fixture declares no slides to resolve')
  }
  if (!Array.isArray(declaration.featureRegions) || declaration.featureRegions.length === 0) {
    failures.push('fixture declares no feature regions')
  }
  if (declaration.edit?.binding === undefined || declaration.edit?.part === undefined) {
    failures.push('fixture declares no executable text edit')
  }
  return failures
}

function stage(operations, checkFailures = []) {
  const failures = [
    ...operations
      .filter((operation) => operation.exitCode !== 0)
      .map((operation) => operation.failure),
    ...checkFailures,
  ]
  return { status: failures.length === 0 ? 'pass' : 'fail', operations, failures }
}

function run(binary, arguments_) {
  const command = [binary, ...arguments_]
  const result = spawnSync(binary, arguments_, { encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 })
  const exitCode = result.status ?? 1
  const stderr = result.stderr?.trim() ?? ''
  return {
    command,
    exitCode,
    stdout: result.stdout?.trim() ?? '',
    stderr,
    failure: exitCode === 0
      ? null
      : stderr || result.error?.message || `command exited with status ${exitCode}`,
  }
}

function diagnosticCodes(operations) {
  const codes = new Set()
  for (const operation of operations) {
    for (const line of operation.stderr.split('\n')) {
      const match = /^render ([A-Za-z0-9-]+) /.exec(line)
      if (match !== null) codes.add(kebabCase(match[1]))
    }
  }
  return [...codes].toSorted()
}

function aggregateFeatures(results) {
  const features = new Map()
  for (const result of results) {
    for (const feature of result.featureTags) {
      const current = features.get(feature) ?? {
        cases: 0,
        passed: 0,
        open: 0,
        preserve: 0,
        edit: 0,
        render: 0,
      }
      current.cases += 1
      for (const dimension of ['open', 'preserve', 'edit', 'render']) {
        if (result[dimension] === 'pass') current[dimension] += 1
      }
      if (['open', 'preserve', 'edit', 'render'].every(
        (dimension) => result[dimension] === 'pass',
      )) current.passed += 1
      features.set(feature, current)
    }
  }
  return Object.fromEntries([...features].toSorted(([left], [right]) => left.localeCompare(right)))
}

function formatDifference(difference) {
  return `${difference.kind} ZIP entry: ${difference.name}`
}

function escapeXml(value) {
  return value.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;')
}

function kebabCase(value) {
  return value.replaceAll(/([a-z0-9])([A-Z])/g, '$1-$2').toLowerCase()
}

function arraysEqual(left, right) {
  return left.length === right.length && left.every((value, index) => value === right[index])
}

function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex')
}

function errorMessage(error) {
  return error instanceof Error ? error.message : String(error)
}

const isMain = process.argv[1] !== undefined &&
  resolve(process.argv[1]) === resolve(fileURLToPath(import.meta.url))
if (isMain) await main()
