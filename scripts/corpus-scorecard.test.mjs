import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'
import {
  compareEdit,
  comparePreservation,
  outcomesFromStages,
  resultMatchesExpected,
  scoreFixture,
} from './corpus-scorecard.mjs'

const root = new URL('../', import.meta.url)
const first = new URL('fixtures/compat/generated-01.pptx', root)
const second = new URL('fixtures/compat/generated-02.pptx', root)

test('preservation evidence compares actual compressed package payloads', async () => {
  const declaration = {
    unknownXmlParts: ['docProps/custom.xml'],
    relationshipParts: ['_rels/.rels', 'ppt/_rels/presentation.xml.rels'],
    opaqueParts: [],
  }
  const unchanged = await comparePreservation(first, first, declaration)
  assert.deepEqual(unchanged.failures, [])
  assert.equal(unchanged.entries.unchanged, unchanged.entries.source)

  const broken = await comparePreservation(first, second, declaration)
  assert(broken.failures.length > 0)
  assert(broken.differences.some((difference) => difference.name === 'ppt/slides/slide1.xml'))
})

test('edit evidence fails closed when no declared edit occurred', async () => {
  const evidence = await compareEdit(first, first, {
    binding: 'case_01',
    value: 'Scorecard 01 & verified',
    part: 'ppt/slides/slide1.xml',
    slide: 0,
  })
  assert.equal(evidence.changed, false)
  assert.equal(evidence.escapedValuePresent, false)
  assert(evidence.failures.length >= 2)
})

test('broken open, preserve, edit, and render evidence fails independently', () => {
  const expected = { open: 'pass', preserve: 'pass', edit: 'pass', render: 'pass' }
  for (const brokenDimension of Object.keys(expected)) {
    const stages = Object.fromEntries(Object.keys(expected).map((dimension) => [
      dimension,
      { status: dimension === brokenDimension ? 'fail' : 'pass' },
    ]))
    const outcomes = outcomesFromStages(stages)
    assert.equal(outcomes[brokenDimension], 'fail')
    for (const dimension of Object.keys(expected)) {
      if (dimension !== brokenDimension) assert.equal(outcomes[dimension], 'pass')
    }
    assert.equal(resultMatchesExpected(outcomes, expected, true), false)
  }
  assert.equal(resultMatchesExpected(expected, expected, false), false)
})

test('a deliberately broken fixture executes and fails each stage independently', async () => {
  const corpus = JSON.parse(await readFile(new URL('fixtures/corpus.json', root), 'utf8'))
  const fixture = corpus.fixtures.find((value) => value.id === 'generated-compat-01')
  for (const brokenDimension of ['open', 'preserve', 'edit', 'render']) {
    const execute = (_binary, arguments_) => {
      const [command, input, output] = arguments_
      const dimension = command === 'inject-text'
        ? output.includes('-preserve.') ? 'preserve' : 'edit'
        : command === 'resolve' && input.includes('-edit.')
          ? 'edit'
          : command === 'resolve'
            ? 'render'
            : input.endsWith('generated-01.pptx')
              ? 'open'
              : input.includes('-preserve.')
                ? 'preserve'
                : 'edit'
      const failed = dimension === brokenDimension
      return {
        command: ['fake-wasmppt', ...arguments_],
        exitCode: failed ? 1 : 0,
        stdout: dimension === 'edit' && !failed ? fixture.scorecard.edit.value : '',
        stderr: failed ? `deliberate ${dimension} failure` : '',
        failure: failed ? `deliberate ${dimension} failure` : null,
      }
    }
    const result = await scoreFixture(fixture, {
      binary: 'fake-wasmppt',
      temporary: '/tmp/wasmppt-deliberately-broken',
      execute,
      comparePreservationEvidence: async () => ({ failures: [] }),
      compareEditEvidence: async () => ({ failures: [] }),
    })
    assert.equal(result[brokenDimension], 'fail')
    for (const dimension of ['open', 'preserve', 'edit', 'render']) {
      if (dimension !== brokenDimension) assert.equal(result[dimension], 'pass')
    }
  }
})
