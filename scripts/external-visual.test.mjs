import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'
import { externalSlideFailures, externalVisualCases } from './external-visual.mjs'

const corpus = JSON.parse(await readFile(new URL('../fixtures/corpus.json', import.meta.url), 'utf8'))
const renderCorpus = JSON.parse(await readFile(new URL('../fixtures/render/corpus.json', import.meta.url), 'utf8'))
const cases = externalVisualCases(corpus, renderCorpus)
const entry = cases[0]
const slide = entry.slides[0]
const actual = {
  slideCount: entry.slideCount,
  width: entry.sizeEmu[0],
  height: entry.sizeEmu[1],
  commandCount: slide.minimumCommands,
  commandKinds: Object.fromEntries(slide.requiredCommandKinds.map((kind) => [kind, 1])),
  diagnosticCodes: slide.expectedDiagnostics,
  finiteBounds: true,
  resourceFailures: [],
  nonBackgroundPixels: slide.minimumNonBackgroundPixels,
  pixelSha256: 'a'.repeat(64),
  repeatedPixelSha256: 'a'.repeat(64),
}

test('external visual cases retain independent producer evidence and canonical fetch-only provenance', () => {
  assert.equal(cases.length, 2)
  assert(cases.every(({ fixture }) => fixture.producer.name.includes('PowerPoint')))
  const changed = structuredClone(renderCorpus)
  changed.externalPresentations[0].fixtureId = 'missing'
  assert.throws(() => externalVisualCases(corpus, changed), /unregistered external visual case/)
  const committed = structuredClone(corpus)
  committed.fixtures.find((fixture) => fixture.id === entry.fixtureId).redistribution = 'allowed'
  assert.throws(() => externalVisualCases(committed, renderCorpus), /fetch-only/)
  const unknownProducer = structuredClone(corpus)
  delete unknownProducer.fixtures.find((fixture) => fixture.id === entry.fixtureId).producer.version
  assert.throws(() => externalVisualCases(unknownProducer, renderCorpus), /producer.version/)
})

test('external smoke declarations reject unbounded or empty checks', () => {
  for (const mutate of [
    (value) => { value.externalPresentations = [value.externalPresentations[0]] },
    (value) => { value.externalPresentations[0].viewport.width = 100_000 },
    (value) => { value.externalPresentations[0].slides[0].slideIndex = entry.slideCount },
    (value) => { value.externalPresentations[0].slides[0].minimumNonBackgroundPixels = 0 },
    (value) => { value.externalPresentations[0].slides[0].requiredCommandKinds = ['clear'] },
  ]) {
    const changed = structuredClone(renderCorpus)
    mutate(changed)
    assert.throws(() => externalVisualCases(corpus, changed))
  }
})

test('external smoke fails on blank pixels even when display-list structure succeeds', () => {
  assert.deepEqual(externalSlideFailures(entry, slide, actual), [])
  assert.deepEqual(externalSlideFailures(entry, slide, { ...actual, nonBackgroundPixels: 0 }), [
    'blank or insufficient pixel output',
  ])
})

test('external smoke retains unexpected diagnostics, missing content/resources, and unstable rendering as failures', () => {
  assert.deepEqual(externalSlideFailures(entry, slide, {
    ...actual,
    slideCount: 0,
    width: NaN,
    commandCount: 1,
    commandKinds: {},
    diagnosticCodes: ['unsupported-graphic-frame'],
    finiteBounds: false,
    resourceFailures: ['ppt/media/missing.png'],
    repeatedPixelSha256: 'b'.repeat(64),
  }), [
    'presentation slide count changed',
    'slide dimensions changed',
    'empty or undersized display list',
    ...slide.requiredCommandKinds.map((kind) => `missing ${kind} command`),
    'resolver diagnostics changed',
    'invalid command bounds',
    'required image resource failed',
    'unstable repeated render',
  ])
})
