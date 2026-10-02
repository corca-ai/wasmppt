import assert from 'node:assert/strict'

/** Join smoke declarations to the canonical provenance registry; binaries remain fetch-only. */
export function externalVisualCases(corpus, renderCorpus) {
  const cases = renderCorpus.externalPresentations
  assert(Array.isArray(cases) && cases.length >= 2, 'at least two external visual cases are required')
  const ids = new Set()
  return cases.map((entry) => {
    assert(!ids.has(entry.fixtureId), `duplicate external visual case ${entry.fixtureId}`)
    ids.add(entry.fixtureId)
    const fixture = corpus.fixtures.find((candidate) => candidate.id === entry.fixtureId)
    assert(fixture, `unregistered external visual case ${entry.fixtureId}`)
    assert.equal(fixture.redistribution, 'fetch-only')
    assert.equal(fixture.path, undefined)
    assert.match(fixture.url, /^https:\/\/.+\.pptx$/)
    assert.match(fixture.sha256, /^[0-9a-f]{64}$/)
    for (const field of ['provenance', 'license']) assert(fixture[field]?.length > 0, field)
    for (const field of ['name', 'version', 'evidence']) assert(fixture.producer?.[field]?.length > 0, `producer.${field}`)
    assert(Number.isSafeInteger(entry.slideCount) && entry.slideCount > 0)
    assert.equal(entry.sizeEmu.length, 2)
    assert(entry.sizeEmu.every((value) => Number.isSafeInteger(value) && value > 0))
    for (const value of Object.values(entry.viewport)) assert(Number.isSafeInteger(value) && value > 0 && value <= 2048)
    assert.equal(entry.viewport.width / entry.viewport.height, entry.sizeEmu[0] / entry.sizeEmu[1])
    assert(entry.slides.length > 0)
    const slides = new Set()
    for (const slide of entry.slides) {
      assert(Number.isSafeInteger(slide.slideIndex) && slide.slideIndex >= 0 && slide.slideIndex < entry.slideCount)
      assert(!slides.has(slide.slideIndex), `duplicate external slide ${slide.slideIndex}`)
      slides.add(slide.slideIndex)
      assert(Number.isSafeInteger(slide.minimumCommands) && slide.minimumCommands > 1)
      assert(slide.requiredCommandKinds.length > 0)
      assert(slide.requiredCommandKinds.every((kind) => kind !== 'clear' && kind !== 'draw-unsupported'))
      assert(Array.isArray(slide.expectedDiagnostics))
      assert.equal(slide.background.length, 4)
      assert(slide.background.every((byte) => Number.isInteger(byte) && byte >= 0 && byte <= 255))
      assert(Number.isSafeInteger(slide.minimumNonBackgroundPixels) && slide.minimumNonBackgroundPixels > 0)
      assert(slide.minimumNonBackgroundPixels < entry.viewport.width * entry.viewport.height)
    }
    return Object.assign({}, entry, { fixture })
  })
}

/** Smoke evidence detects blank output and contract drift, without claiming Office pixel fidelity. */
export function externalSlideFailures(entry, slide, actual) {
  const failures = []
  if (actual.slideCount !== entry.slideCount) failures.push('presentation slide count changed')
  if (actual.width !== entry.sizeEmu[0] || actual.height !== entry.sizeEmu[1]) failures.push('slide dimensions changed')
  if (!Number.isSafeInteger(actual.commandCount) || actual.commandCount < slide.minimumCommands) failures.push('empty or undersized display list')
  for (const kind of slide.requiredCommandKinds) {
    if (!(actual.commandKinds[kind] > 0)) failures.push(`missing ${kind} command`)
  }
  if (JSON.stringify(actual.diagnosticCodes.toSorted()) !== JSON.stringify(slide.expectedDiagnostics.toSorted())) {
    failures.push('resolver diagnostics changed')
  }
  if (!actual.finiteBounds) failures.push('invalid command bounds')
  if (actual.resourceFailures.length > 0) failures.push('required image resource failed')
  if (!Number.isSafeInteger(actual.nonBackgroundPixels) || actual.nonBackgroundPixels < slide.minimumNonBackgroundPixels) failures.push('blank or insufficient pixel output')
  if (!/^[0-9a-f]{64}$/.test(actual.pixelSha256) || actual.pixelSha256 !== actual.repeatedPixelSha256) failures.push('unstable repeated render')
  return failures
}
