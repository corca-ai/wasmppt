import assert from 'node:assert/strict'
import test from 'node:test'
import { comparisonEvaluation, timingSummary } from '../benchmarks/comparisons/equivalent-report.mjs'
import { textSlide, textSlideFailures, textWorkload } from '../benchmarks/comparisons/text-workload.mjs'

function sceneForSlide(index) {
  return {
    width: textWorkload.sizeEmu[0], height: textWorkload.sizeEmu[1], diagnostics: [], strings: [],
    commands: [
      { kind: 'clear', color: { red: 255, green: 255, blue: 255, alpha: 255 } },
      ...textSlide(index).map((field) => ({
        kind: 'draw-rich-text',
        bounds: { x: Math.round(field.options.x * 914400), y: Math.round(field.options.y * 914400), width: Math.round(field.options.w * 914400), height: Math.round(field.options.h * 914400) },
        frame: { paragraphs: [{ runs: [
          { text: field.text, style: { fontSize: 1000, fontFamily: 'Arial', color: { red: 0, green: 0, blue: 0, alpha: 255 }, bold: false, italic: false } },
          { text: '', style: { fontSize: 1000, fontFamily: 'Calibri' } },
        ] }] },
      })),
    ],
  }
}

test('semantic comparison checks every slide, ordered visible text, geometry, and painted fonts', () => {
  for (let index = 0; index < textWorkload.slides; index += 1) assert.deepEqual(textSlideFailures(sceneForSlide(index), index), [])
  const scene = sceneForSlide(9)
  scene.commands[1].frame.paragraphs[0].runs[0].text = '{{text_9_0}}'
  assert.deepEqual(textSlideFailures(scene, 9), ['field 0 text changed or moved'])
  // Requested strings elsewhere in the package cannot stand in for an actual painted command.
  scene.strings.push(textSlide(9)[0].text)
  assert.deepEqual(textSlideFailures(scene, 9), ['field 0 text changed or moved'])
})

test('comparison rejects empty scenes, wrong layout/font/background, and unexpected diagnostics', () => {
  assert(textSlideFailures({ ...sceneForSlide(0), commands: [] }, 0).includes('requested text box count changed'))
  const scene = sceneForSlide(0)
  scene.width += 1
  scene.diagnostics.push({ code: 'unsupported-graphic-frame' })
  scene.commands[0].color.red = 0
  scene.commands[1].bounds.x += 1
  scene.commands[2].frame.paragraphs[0].runs[0].style.fontFamily = 'different font'
  assert.deepEqual(textSlideFailures(scene, 0), ['page dimensions changed', 'unexpected resolver diagnostics', 'background changed', 'field 0 geometry changed', 'field 1 font changed'])
})

function processSamples() {
  return { samples: ['cold', 'warm', 'author', 'raw-author'].flatMap((phase) => [0, 1, 2].map((iteration) => ({
    phase, iteration, output: `${phase}-${iteration}.pptx`, outputSha256: 'a'.repeat(64),
    durationMs: phase === 'author' ? 20 + iteration : 10 + iteration,
    validation: { openXml: { exitCode: phase === 'raw-author' ? 1 : 0 }, semantic: { passed: true } },
  }))) }
}
const configuration = { iterations: 3, processRuns: 1 }

test('eligible output ratios include preparation and correction; raw invalid and warm-reuse timings stay separate', () => {
  const evaluation = comparisonEvaluation([processSamples()], configuration)
  assert.equal(evaluation.passed, true)
  assert.equal(evaluation.comparison.eligible, true)
  assert.equal(evaluation.comparison.ratios.p50, 21 / 11)
  assert.equal(evaluation.comparison.ratios.p95, 22 / 12)
  assert.equal(evaluation.comparison.warmComparison.ratios, null)
  assert.equal(evaluation.comparison.claim, null)
  assert.equal(evaluation.participants.find((participant) => participant.phase === 'raw-author').eligible, false)
})

test('any invalid timed output removes its eligibility and all comparative ratios', () => {
  for (const mutate of [
    (sample) => { sample.validation.openXml.exitCode = 1 },
    (sample) => { sample.validation.semantic.passed = false },
    (sample) => { sample.durationMs = NaN },
    (sample) => { sample.durationMs = 0 },
    (sample) => { delete sample.validation },
    (sample) => { delete sample.outputSha256 },
  ]) {
    const process = processSamples()
    mutate(process.samples[0])
    const evaluation = comparisonEvaluation([process], configuration)
    assert.equal(evaluation.passed, false)
    assert.equal(evaluation.comparison.eligible, false)
    assert.equal(evaluation.comparison.ratios, null)
  }
})

test('missing process runs and duplicated iterations cannot produce a comparison', () => {
  const process = processSamples()
  process.samples[0].iteration = 1
  assert.equal(comparisonEvaluation([process], configuration).comparison.ratios, null)
  assert.equal(comparisonEvaluation([processSamples()], { ...configuration, processRuns: 2 }).comparison.ratios, null)
  assert.equal(comparisonEvaluation([], configuration).comparison.ratios, null)
  assert.deepEqual(timingSummary([20, 1, 10]), { p50Ms: 10, p95Ms: 20 })
})
