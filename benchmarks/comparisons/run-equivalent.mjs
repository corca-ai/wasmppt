import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { execFileSync, spawnSync } from 'node:child_process'
import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { arch, cpus, platform, release, totalmem } from 'node:os'
import { join, relative, resolve } from 'node:path'
import { comparisonEvaluation, timingSummary } from './equivalent-report.mjs'
import { textSlideFailures, textWorkload } from './text-workload.mjs'

const root = resolve(import.meta.dirname, '../..')
const args = process.argv.slice(2)
assert(args.every((arg) => arg === '--ci' || /^--(?:iterations|process-runs)=\d+$/.test(arg)), 'unknown comparison option')
const ci = args.includes('--ci')
const configuration = {
  iterations: ci ? 10 : Number(args.find((arg) => arg.startsWith('--iterations='))?.split('=')[1] ?? 30),
  processRuns: ci ? 3 : Number(args.find((arg) => arg.startsWith('--process-runs='))?.split('=')[1] ?? 1),
  measured: 'Node scalar-Wasm template preparation and generation versus Node PptxGenJS authoring plus declared adapter correction; includes serialization and output buffer assembly.',
  excluded: 'Common text input construction, library/Wasm initialization, preauthored template construction, file I/O, validation, and rendering.',
}
assert(Number.isSafeInteger(configuration.iterations) && configuration.iterations >= 3 && configuration.iterations <= 1000)
assert(Number.isSafeInteger(configuration.processRuns) && configuration.processRuns >= 1 && configuration.processRuns <= 10)
const outputDirectory = join(root, 'target/benchmarks/equivalent')
const reportPath = join(root, 'target/benchmarks/equivalent.json')
await mkdir(outputDirectory, { recursive: true })
const dotnet = process.env.WASMPPT_DOTNET ?? 'dotnet'
const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex')
const report = {
  schema: 1,
  generatedAt: new Date().toISOString(),
  source: {
    revision: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim(),
    dirty: execFileSync('git', ['status', '--porcelain'], { cwd: root, encoding: 'utf8' }).trim().length > 0,
  },
  workload: textWorkload,
  configuration,
  environment: {
    hardware: { cpu: cpus()[0]?.model ?? 'unknown', logicalCpus: cpus().length, totalMemoryBytes: totalmem(), architecture: arch() },
    os: { platform: platform(), release: release() },
    runtimes: { node: process.version },
  },
  libraries: {},
  adapterPolicy: {
    template: 'PptxGenJS authors a token-bound POTX off the clock; template creation is not a wasmppt capability or timing.',
    pptxgenjs: 'Move notesMasterIdLst before sldIdLst in ppt/presentation.xml to satisfy CT_Presentation ordering; JSZip load, edit, and DEFLATE reserialization are included in author timings.',
    rawComparator: 'Unmodified PptxGenJS output and timings remain visible; failed Open XML validation excludes those samples from ratios.',
    semanticDifferences: ['Preauthored template injection versus new-deck authoring.', 'Different ZIP metadata and serialized package bytes.', 'No pixel-fidelity, arbitrary-template preservation, or fastest claim.'],
  },
  processes: [],
  failures: [],
  comparison: { eligible: false, ratios: null, claim: null },
  passed: false,
}

function command(program, commandArgs, timeout = 120_000) {
  const child = spawnSync(program, commandArgs, { cwd: root, encoding: 'utf8', timeout, maxBuffer: 16 * 1024 * 1024 })
  return {
    command: [program, ...commandArgs], exitCode: child.status,
    stdout: child.stdout ?? '', stderr: child.stderr ?? '', error: child.error?.message,
  }
}

try {
  report.environment.runtimes.dotnet = execFileSync(dotnet, ['--version'], { encoding: 'utf8' }).trim()
  const comparatorPackage = JSON.parse(await readFile(join(root, 'benchmarks/comparisons/pptxgenjs/node_modules/pptxgenjs/package.json'), 'utf8'))
  const zipPackage = JSON.parse(await readFile(join(root, 'benchmarks/comparisons/pptxgenjs/node_modules/jszip/package.json'), 'utf8'))
  report.libraries.pptxgenjs = { name: comparatorPackage.name, version: comparatorPackage.version }
  report.libraries.jszip = { name: zipPackage.name, version: zipPackage.version }
  const wasmPath = join(root, 'packages/wasmppt-worker/src/generated/wasmppt_wasm_bg.wasm')
  const wasmBytes = await readFile(wasmPath)
  report.artifacts = { scalarWasmBytes: wasmBytes.length, scalarWasmSha256: sha256(wasmBytes), contractSha256: sha256(await readFile(join(root, 'benchmarks/comparisons/text-workload.mjs'))) }
  const validatorDirectory = join(outputDirectory, 'validator')
  report.validator = command(dotnet, [
    'build', 'tools/openxml-validator/OpenXmlValidator.csproj', '--configuration', 'Release',
    '--output', validatorDirectory, `-p:BaseIntermediateOutputPath=${join(outputDirectory, 'validator-obj')}/`, '--nologo',
  ])
  report.validator.sdkPackageVersion = (await readFile(join(root, 'tools/openxml-validator/OpenXmlValidator.csproj'), 'utf8'))
    .match(/Include="DocumentFormat.OpenXml" Version="([^"]+)"/)[1]
  assert.equal(report.validator.exitCode, 0, `Open XML validator build failed: ${report.validator.stderr || report.validator.stdout}`)
  const { createTextTemplate } = await import('./pptxgenjs/equivalent.mjs')
  const template = await createTextTemplate()
  const templatePath = join(outputDirectory, 'template.potx')
  await writeFile(templatePath, template)
  report.template = { path: relative(root, templatePath), bytes: template.length, sha256: sha256(template) }
  for (let processIndex = 0; processIndex < configuration.processRuns; processIndex += 1) {
    const run = command(process.execPath, [join(import.meta.dirname, 'sample-equivalent.mjs'), templatePath, join(outputDirectory, `process-${processIndex}`), String(configuration.iterations)])
    assert.equal(run.exitCode, 0, `comparison sampler failed: ${run.stderr}`)
    const measured = JSON.parse(run.stdout.trim())
    report.libraries.wasmppt = { name: 'wasmppt', version: measured.engineVersion, host: 'node-scalar-wasm', profile: 'wasm-release' }
    report.processes.push(Object.assign({ processIndex, sampler: run }, measured))
  }
  const wasm = await import('../../packages/wasmppt-worker/src/generated/wasmppt_wasm.js')
  await wasm.default({ module_or_path: wasmBytes })
  const { decodeDisplayList } = await import('../../packages/wasmppt/dist/canvas.js')
  const engine = new wasm.WasmpptEngine()
  try {
    for (const processRun of report.processes) {
      for (const sample of processRun.samples) {
        const bytes = await readFile(sample.output)
        sample.outputSha256 = sha256(bytes)
        sample.outputBytes = bytes.length
        const openXml = command(dotnet, [join(validatorDirectory, 'OpenXmlValidator.dll'), sample.output], 30_000)
        const semantic = { passed: false, slideCount: null, slides: [], failures: [] }
        let presentation
        try {
          presentation = engine.open_presentation(bytes)
          semantic.slideCount = engine.presentation_slide_count(presentation)
          if (semantic.slideCount !== textWorkload.slides) semantic.failures.push('requested slide count changed')
          for (let slideIndex = 0; slideIndex < textWorkload.slides; slideIndex += 1) {
            const displayList = engine.resolve_presentation_slide(presentation, slideIndex)
            const scene = decodeDisplayList(displayList)
            const failures = textSlideFailures(scene, slideIndex)
            semantic.slides.push({ slideIndex, commandCount: scene.commands.length, diagnostics: scene.diagnostics, displayListSha256: sha256(displayList), failures, passed: failures.length === 0 })
          }
          semantic.passed = semantic.failures.length === 0 && semantic.slides.every((slide) => slide.passed)
        } catch (error) {
          semantic.failures.push(error.message ?? String(error))
        } finally {
          if (presentation !== undefined) engine.release_presentation(presentation)
        }
        sample.validation = { openXml, semantic }
        sample.output = relative(root, sample.output)
      }
    }
  } finally {
    engine.free()
  }
  Object.assign(report, comparisonEvaluation(report.processes, configuration))
  const preparationSamples = report.processes.flatMap((run) => run.samples.filter((sample) => sample.phase === 'cold').map((sample) => sample.preparationMs))
  report.preparation = { samplesMs: preparationSamples, summary: timingSummary(preparationSamples) }
  assert(report.passed, 'equivalent-output benchmark failed; inspect eligibility and validation evidence')
  console.log(`Equivalent-output comparison: ${reportPath}`)
  for (const participant of report.participants) console.log(`${participant.phase}: eligible=${participant.eligible}, p50=${participant.summary.p50Ms.toFixed(3)}ms, p95=${participant.summary.p95Ms.toFixed(3)}ms`)
} catch (error) {
  report.failures.push(error.message)
  throw error
} finally {
  await writeFile(reportPath, `${JSON.stringify(report, null, 2)}\n`)
}
