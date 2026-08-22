import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

import {
  ByteBudgetLru,
  CanvasDisplayListRenderer,
  FontResolver,
  buildRichTextLayout,
  decodeDisplayList,
  decodeOoxmlObfuscatedFont,
  inspectOpenTypeEmbedding,
  hitTestDisplayScene,
  measureTextBatch,
  renderOffscreenThumbnail,
  wrapText,
} from '../dist/index.js'

test('embedded font decoding and permission inspection are bounded and deterministic', () => {
  const source = Uint8Array.from({ length: 40 }, (_, index) => index)
  const guid = '00112233-4455-6677-8899-aabbccddeeff'
  const encoded = decodeOoxmlObfuscatedFont(source, guid)
  assert.deepEqual(decodeOoxmlObfuscatedFont(encoded, guid), source)
  assert.throws(() => decodeOoxmlObfuscatedFont(source, 'bad'), /GUID/)

  const font = new Uint8Array(38)
  const view = new DataView(font.buffer)
  view.setUint16(4, 1)
  font.set(new TextEncoder().encode('OS/2'), 12)
  view.setUint32(20, 28)
  view.setUint32(24, 10)
  view.setUint16(36, 0x0004)
  assert.deepEqual(inspectOpenTypeEmbedding(font), {
    fsType: 0x0004,
    permitted: true,
    reason: 'preview-print',
  })
  view.setUint16(36, 0x0002)
  assert.equal(inspectOpenTypeEmbedding(font).permitted, false)
})

const textCorpus = JSON.parse(
  await readFile(new URL('../../../fixtures/text-edge-cases.json', import.meta.url), 'utf8'),
)

function codePointLength(value) {
  return [...value].length
}

function sourceSemantic(semanticId, zOrder, readingOrder, bounds) {
  return {
    firstCommand: 0,
    commandCount: 0,
    shapeId: zOrder + 1,
    zOrder,
    readingOrder,
    kind: 'shape',
    bounds,
    name: semanticId,
    source: { semanticId, source: 'deck.md', start: 10, end: 20 },
  }
}

test('display-list decoder rejects corruption and decodes a bounded scene', () => {
  const bytes = minimalDisplayList()
  const scene = decodeDisplayList(bytes)
  assert.equal(scene.width, 9_144_000)
  assert.equal(scene.height, 6_858_000)
  assert.deepEqual(scene.commands, [
    { kind: 'clear', color: { red: 255, green: 255, blue: 255, alpha: 255 } },
  ])
  for (const version of [2, 3, 4, 5, 6, 7, 8, 9, 10, 11]) {
    assert.equal(decodeDisplayList(minimalDisplayList(version)).version, version)
  }
  assert.throws(() => decodeDisplayList(bytes.slice(0, -1)), /truncated/)
  const wrongVersion = bytes.slice()
  new DataView(wrongVersion).setUint16(4, 99, true)
  assert.throws(() => decodeDisplayList(wrongVersion), /version 99/)
})

test('source hit testing returns the topmost stable semantic target', () => {
  const scene = {
    semantics: [
      sourceSemantic('bottom', 1, 0, { x: 0, y: 0, width: 100, height: 100 }),
      sourceSemantic('top', 2, 1, { x: 20, y: 20, width: 50, height: 50 }),
    ],
  }
  assert.equal(hitTestDisplayScene(scene, 30, 30)?.source.semanticId, 'top')
  assert.equal(hitTestDisplayScene(scene, 5, 5)?.source.semanticId, 'bottom')
  assert.equal(hitTestDisplayScene(scene, 500, 500), undefined)
})

test('Korean and CJK wrapping permits deterministic character boundaries', () => {
  assert.deepEqual(wrapText('가나다라마바사', 3, (value) => [...value].length), [
    '가나다',
    '라마바',
    '사',
  ])
  assert.deepEqual(wrapText('漢字かなカナ', 2, (value) => [...value].length), ['漢字', 'かな', 'カナ'])
})

test('Latin wrapping uses whitespace and falls back for an oversized word', () => {
  assert.deepEqual(wrapText('alpha beta gamma', 10, codePointLength), ['alpha beta', 'gamma'])
  assert.deepEqual(wrapText('extraordinary', 5, codePointLength), ['extra', 'ordin', 'ary'])
})

test('Unicode wrapping preserves grapheme clusters, nonbreaking spaces, and kinsoku punctuation', () => {
  assert.deepEqual(wrapText('👨🏽‍💻👩‍👧‍👦', 4, codePointLength), ['👨🏽‍💻', '👩‍👧‍👦'])
  assert.deepEqual(wrapText('A\u00A0B C', 3, codePointLength), ['A\u00A0B', 'C'])
  assert.deepEqual(wrapText('「日本語」、テスト', 4, codePointLength), ['「日本', '語」、テ', 'スト'])
  assert.deepEqual(wrapText('ภาษาไทย', 3, codePointLength), ['ภาษ', 'าไท', 'ย'])
})

test('rich-text layout wraps Latin titles inside the text frame', async () => {
  const style = {
    fontSize: 1_200,
    color: { red: 0, green: 0, blue: 0, alpha: 255 },
    bold: false,
    italic: false,
    underline: false,
    strike: false,
    characterSpacing: 0,
    baseline: 0,
    alignment: 'left',
    verticalAlignment: 'top',
    marginLeft: 0,
    marginTop: 0,
    marginRight: 0,
    marginBottom: 0,
  }
  const command = (text, width) => ({
    kind: 'draw-rich-text',
    bounds: { x: 0, y: 0, width: width * 9_525, height: 2_000_000 },
    frame: {
      paragraphs: [{
        runs: [{ text, style }],
        alignment: 'left',
        level: 0,
        marginLeft: 0,
        indent: 0,
        direction: 'ltr',
        tabs: [],
      }],
      verticalAlignment: 'top',
      marginLeft: 0,
      marginTop: 0,
      marginRight: 0,
      marginBottom: 0,
      wrap: true,
      autofit: 'none',
      flow: 'horizontal',
    },
  })
  const context = { font: '', measureText: (value) => ({ width: codePointLength(value) }) }
  const words = await buildRichTextLayout(context, command('alpha beta gamma', 10))
  const longWord = await buildRichTextLayout(context, command('extraordinary', 5))
  assert.equal(new Set(words.runs.map((run) => run.baseline)).size, 2)
  assert.equal(words.contentWidth, 10)
  assert.equal(new Set(longWord.runs.map((run) => run.baseline)).size, 3)
  assert.equal(longWord.contentWidth, 5)
})

test('shrink-text autofit reflows at the largest font scale that fits', async () => {
  const style = {
    fontSize: 1_200,
    color: { red: 0, green: 0, blue: 0, alpha: 255 },
    bold: false,
    italic: false,
    underline: false,
    strike: false,
    characterSpacing: 0,
    baseline: 0,
    alignment: 'left',
    verticalAlignment: 'top',
    marginLeft: 0,
    marginTop: 0,
    marginRight: 0,
    marginBottom: 0,
  }
  const command = {
    kind: 'draw-rich-text',
    bounds: { x: 0, y: 0, width: 952_500, height: 190_500 },
    frame: {
      paragraphs: [{
        runs: [{ text: 'alpha beta gamma delta epsilon', style }],
        alignment: 'left',
        level: 0,
        marginLeft: 0,
        indent: 0,
        direction: 'ltr',
        tabs: [],
      }],
      verticalAlignment: 'top',
      marginLeft: 0,
      marginTop: 0,
      marginRight: 0,
      marginBottom: 0,
      wrap: true,
      autofit: 'shrink-text',
      flow: 'horizontal',
    },
  }
  let measurementCount = 0
  const context = {
    font: '',
    measureText(value) {
      measurementCount += 1
      return { width: codePointLength(value) * 10 }
    },
  }
  const plan = await buildRichTextLayout(context, command)
  assert.equal(new Set(plan.runs.map((run) => run.baseline)).size, 2)
  assert(plan.runs[0].fontSize > 8)
  assert(plan.contentHeight <= 20)
  assert(plan.contentWidth <= 100)
  assert.equal(measurementCount, 6)
})

test('font resolver uses an exact supplied CJK font and documents fallback', async () => {
  const loaded = []
  const resolver = new FontResolver({
    theme: { eastAsian: 'Supplied Korean' },
    webFonts: [{ family: 'Supplied Korean', source: new ArrayBuffer(8) }],
    fallback: { 'east-asian': 'Documented CJK Fallback' },
    host: {
      async load(font) {
        loaded.push(font.family)
      },
      check(css, text) {
        return css.includes('Supplied Korean') && text.includes('한')
      },
    },
  })
  const exact = await resolver.resolve('한글')
  assert.equal(exact.script, 'east-asian')
  assert.equal(exact.family, 'Supplied Korean')
  assert.equal(exact.exact, true)
  assert.deepEqual(loaded, ['Supplied Korean'])

  const fallback = await new FontResolver({
    theme: { eastAsian: 'Missing Korean' },
    fallback: { 'east-asian': 'Documented CJK Fallback' },
    host: { load: async () => {}, check: () => false },
  }).resolve('漢字')
  assert.equal(fallback.family, 'Documented CJK Fallback')
  assert.equal(fallback.exact, false)
})

test('RTL, emoji, and deliberately missing-font corpus cases select stable fallbacks', async () => {
  assert.equal(textCorpus.schema, 1)
  const resolver = new FontResolver({
    theme: {
      eastAsian: textCorpus.missingFont,
      complexScript: textCorpus.missingFont,
    },
    fallback: {
      'east-asian': 'Documented CJK Fallback',
      complex: 'Documented Complex-Script Fallback',
    },
    host: { load: async () => {}, check: () => false },
  })
  for (const fixture of textCorpus.cases) {
    const resolved = await resolver.resolve(fixture.text)
    assert.equal(resolved.script, fixture.script, fixture.id)
    assert.equal(resolved.exact, false, fixture.id)
  }
})

test('text measurement is grouped by exact font without changing result order', () => {
  const assigned = []
  const context = {
    fontValue: '',
    get font() {
      return this.fontValue
    },
    set font(value) {
      this.fontValue = value
      assigned.push(value)
    },
    measureText(value) {
      return { width: value.length * (this.fontValue === '20px Exact Korean' ? 2 : 1) }
    },
  }
  let measurementCount = 0
  const originalMeasure = context.measureText
  context.measureText = function (value) {
    measurementCount += 1
    return originalMeasure.call(this, value)
  }
  assert.deepEqual(
    measureTextBatch(context, [
      { text: '가나', font: '20px Exact Korean' },
      { text: 'abc', font: '18px Exact Latin' },
      { text: '다라', font: '20px Exact Korean' },
      { text: '가나', font: '20px Exact Korean' },
    ]),
    [4, 3, 4, 4],
  )
  assert.deepEqual(assigned, ['20px Exact Korean', '18px Exact Latin'])
  assert.equal(measurementCount, 3)
})

test('shared rich-text plan carries exact font-byte advances and clusters', async () => {
  let shapeCalls = 0
  const shaper = {
    async breakText(text) { return [text] },
    async shape(request) {
      shapeCalls += 1
      assert.equal(request.text, 'office')
      return {
        unitsPerEm: 1_000,
        glyphs: [...request.text].map((_, cluster) => ({
          glyphId: cluster + 1,
          cluster,
          xAdvance: 100,
          yAdvance: 0,
          xOffset: 0,
          yOffset: 0,
          safeToBreak: true,
        })),
      }
    },
  }
  const fontBytes = new ArrayBuffer(32)
  const resolver = new FontResolver({
    shaper,
    webFonts: [{ family: 'Exact', source: fontBytes }],
    host: { load: async () => {}, check: () => true },
  })
  const style = {
    fontSize: 1_200,
    color: { red: 0, green: 0, blue: 0, alpha: 255 },
    fontFamily: 'Exact',
    bold: false,
    italic: false,
    underline: false,
    strike: false,
    characterSpacing: 0,
    baseline: 0,
    alignment: 'left',
    verticalAlignment: 'top',
    marginLeft: 0,
    marginTop: 0,
    marginRight: 0,
    marginBottom: 0,
  }
  const command = {
    kind: 'draw-rich-text',
    bounds: { x: 0, y: 0, width: 952_500, height: 952_500 },
    frame: {
      paragraphs: [{
        runs: [{ text: 'office', style }],
        alignment: 'left',
        level: 0,
        marginLeft: 0,
        indent: 0,
        direction: 'ltr',
        tabs: [],
      }],
      verticalAlignment: 'top',
      marginLeft: 0,
      marginTop: 0,
      marginRight: 0,
      marginBottom: 0,
      wrap: true,
      autofit: 'none',
      flow: 'horizontal',
    },
  }
  const context = { font: '', measureText: () => ({ width: 999 }) }
  const plan = await buildRichTextLayout(context, command, resolver)
  assert.equal(shapeCalls, 1)
  assert.equal(plan.runs.length, 1)
  assert.equal(plan.runs[0].width, 9.6)
  assert.deepEqual(plan.runs[0].shaped.glyphs.map((glyph) => glyph.cluster), [0, 1, 2, 3, 4, 5])
})

test('paragraph space-before shifts every line from the start of the paragraph', async () => {
  const context = {
    font: '',
    measureText: (value) => ({ width: value.length * 10 }),
  }
  const style = {
    fontSize: 1_200,
    color: { red: 0, green: 0, blue: 0, alpha: 255 },
    bold: false,
    italic: false,
    underline: false,
    strike: false,
    characterSpacing: 0,
    baseline: 0,
    alignment: 'left',
    verticalAlignment: 'top',
    marginLeft: 0,
    marginTop: 0,
    marginRight: 0,
    marginBottom: 0,
  }
  const command = (spaceBefore) => ({
    kind: 'draw-rich-text',
    bounds: { x: 0, y: 0, width: 2_000_000, height: 2_000_000 },
    frame: {
      paragraphs: [{
        runs: [{ text: 'first\nsecond', style }],
        alignment: 'left',
        level: 0,
        marginLeft: 0,
        indent: 0,
        spaceBefore,
        direction: 'ltr',
        tabs: [],
      }],
      verticalAlignment: 'top',
      marginLeft: 0,
      marginTop: 0,
      marginRight: 0,
      marginBottom: 0,
      wrap: true,
      autofit: 'none',
      flow: 'horizontal',
    },
  })
  const baseline = await buildRichTextLayout(context, command(undefined))
  const spaced = await buildRichTextLayout(context, command({ kind: 'points', value: 1_200 }))
  assert.equal(spaced.runs.length, 2)
  assert(Math.abs(spaced.runs[0].baseline - baseline.runs[0].baseline - 16) < 1e-9)
  assert(Math.abs(spaced.runs[1].baseline - baseline.runs[1].baseline - 16) < 1e-9)
  const lineSpacedCommand = command(undefined)
  lineSpacedCommand.frame.paragraphs[0].lineSpacing = { kind: 'points', value: 2_400 }
  const lineSpaced = await buildRichTextLayout(context, lineSpacedCommand)
  assert(Math.abs(lineSpaced.runs[1].baseline - lineSpaced.runs[0].baseline - 32) < 1e-9)
})

test('normAutofit honors authored scale and percentage line-spacing reduction', async () => {
  const style = {
    fontSize: 1_200,
    color: { red: 0, green: 0, blue: 0, alpha: 255 },
    bold: false,
    italic: false,
    underline: false,
    strike: false,
    characterSpacing: 0,
    baseline: 0,
    alignment: 'left',
    verticalAlignment: 'top',
    marginLeft: 0,
    marginTop: 0,
    marginRight: 0,
    marginBottom: 0,
  }
  const command = {
    kind: 'draw-rich-text',
    bounds: { x: 0, y: 0, width: 952_500, height: 952_500 },
    frame: {
      paragraphs: [{
        runs: [{ text: 'first\nsecond', style }],
        alignment: 'left',
        level: 0,
        marginLeft: 0,
        indent: 0,
        lineSpacing: { kind: 'percent', value: 120_000 },
        direction: 'ltr',
        tabs: [],
      }],
      verticalAlignment: 'top',
      marginLeft: 0,
      marginTop: 0,
      marginRight: 0,
      marginBottom: 0,
      wrap: true,
      autofit: 'shrink-text',
      autofitFontScale: 80_000,
      autofitLineSpacingReduction: 20_000,
      autofitRecompute: false,
      flow: 'horizontal',
    },
  }
  const context = { font: '', measureText: (value) => ({ width: value.length * 8 }) }
  const plan = await buildRichTextLayout(context, command)
  assert(Math.abs(plan.runs[0].fontSize - 12.8) < 1e-9)
  assert(Math.abs(plan.runs[1].baseline - plan.runs[0].baseline - 15.36) < 1e-9)

  command.frame.autofitRecompute = true
  const edited = await buildRichTextLayout(context, command)
  assert.equal(edited.runs[0].fontSize, 16)
  assert(edited.runs[0].fontSize > plan.runs[0].fontSize)

  command.bounds.height = 250_000
  const editedOverflow = await buildRichTextLayout(context, command)
  assert(editedOverflow.runs[0].fontSize < 16)
  assert(editedOverflow.contentHeight <= command.bounds.height / 9_525)
})

test('shape-resize autofit keeps font size and expands the effective bounds', async () => {
  const style = {
    fontSize: 1_200,
    color: { red: 0, green: 0, blue: 0, alpha: 255 },
    bold: false,
    italic: false,
    underline: false,
    strike: false,
    characterSpacing: 0,
    baseline: 0,
    alignment: 'left',
    verticalAlignment: 'top',
    marginLeft: 0,
    marginTop: 0,
    marginRight: 0,
    marginBottom: 0,
  }
  const command = {
    kind: 'draw-rich-text',
    bounds: { x: 100, y: 200, width: 952_500, height: 190_500 },
    frame: {
      paragraphs: [{
        runs: [{ text: 'first\nsecond', style }],
        alignment: 'left',
        level: 0,
        marginLeft: 0,
        indent: 0,
        direction: 'ltr',
        tabs: [],
      }],
      verticalAlignment: 'top',
      marginLeft: 0,
      marginTop: 0,
      marginRight: 0,
      marginBottom: 0,
      wrap: true,
      autofit: 'resize-shape',
      flow: 'horizontal',
    },
  }
  const context = { font: '', measureText: (value) => ({ width: value.length * 8 }) }
  const plan = await buildRichTextLayout(context, command)
  assert.equal(plan.runs[0].fontSize, 16)
  assert.equal(plan.effectiveBounds.x, command.bounds.x)
  assert.equal(plan.effectiveBounds.y, command.bounds.y)
  assert.equal(plan.effectiveBounds.width, command.bounds.width)
  assert(Math.abs(plan.effectiveBounds.height / 9_525 - 38.4) < 1e-9)

  const rotatedCommand = structuredClone(command)
  rotatedCommand.frame.flow = 'vertical'
  rotatedCommand.frame.verticalAlignment = 'center'
  rotatedCommand.frame.paragraphs[0].runs[0].text = Array.from(
    { length: 20 },
    (_, index) => `line ${index}`,
  ).join('\n')
  const rotated = await buildRichTextLayout(context, rotatedCommand)
  assert(rotated.effectiveBounds.width > rotatedCommand.bounds.width)
  assert(rotated.effectiveBounds.x < rotatedCommand.bounds.x)
  assert.equal(rotated.effectiveBounds.y, rotatedCommand.bounds.y)
  assert.equal(rotated.rotationDegrees, 90)

  const pathologicalCommand = structuredClone(command)
  pathologicalCommand.frame.wrap = false
  pathologicalCommand.frame.paragraphs[0].runs[0].text = 'W'.repeat(20_000)
  const pathological = await buildRichTextLayout(context, pathologicalCommand)
  assert.equal(pathological.effectiveBounds.width, 91_440_000)
  assert(Number.isFinite(pathological.effectiveBounds.height))
})

test('multi-column text flows lines into bounded column rectangles', async () => {
  const style = {
    fontSize: 750,
    color: { red: 0, green: 0, blue: 0, alpha: 255 },
    bold: false,
    italic: false,
    underline: false,
    strike: false,
    characterSpacing: 0,
    baseline: 0,
    alignment: 'left',
    verticalAlignment: 'top',
    marginLeft: 0,
    marginTop: 0,
    marginRight: 0,
    marginBottom: 0,
  }
  const command = {
    kind: 'draw-rich-text',
    bounds: { x: 0, y: 0, width: 1_905_000, height: 228_600 },
    frame: {
      paragraphs: [{
        runs: [{ text: 'one\ntwo\nthree', style }],
        alignment: 'left',
        level: 0,
        marginLeft: 0,
        indent: 0,
        direction: 'ltr',
        tabs: [],
      }],
      verticalAlignment: 'top',
      marginLeft: 0,
      marginTop: 0,
      marginRight: 0,
      marginBottom: 0,
      wrap: true,
      autofit: 'none',
      flow: 'horizontal',
      columnCount: 2,
      columnSpacing: 95_250,
    },
  }
  const context = { font: '', measureText: (value) => ({ width: value.length * 5 }) }
  const plan = await buildRichTextLayout(context, command)
  assert.equal(plan.runs.length, 3)
  assert.equal(plan.runs[0].x, plan.runs[1].x)
  assert(plan.runs[2].x > plan.runs[1].x + 90)
  assert.equal(plan.runs[2].baseline, plan.runs[0].baseline)
  assert(plan.contentHeight <= 24)
  command.frame.warp = { preset: 'wave1', adjustment: 50_000 }
  const warped = await buildRichTextLayout(context, command)
  assert(warped.runs.some((run) => Math.abs(run.warpRotation) > 0.1))
  assert(warped.runs.some((run, index) => run.baseline !== plan.runs[index].baseline))
  delete command.frame.warp
  command.frame.paragraphs[0].bulletImageResource = 0
  const pictureBullet = await buildRichTextLayout(context, command)
  assert.equal(pictureBullet.runs[0].bulletImageResource, 0)
})


test('clearing a renderer disposes late image decodes without repopulating its cache', async () => {
  const renderer = new CanvasDisplayListRenderer()
  let resolveImage
  let markResolverStarted
  const resolverStarted = new Promise((resolve) => { markResolverStarted = resolve })
  let closed = 0
  const fontResolver = { resolve: async () => ({ css: '12px sans-serif' }) }
  const canvas = { width: 1, height: 1 }
  const context = {
    canvas,
    save() {},
    restore() {},
    setTransform() {},
  }
  const rendered = renderer.render(
    {
      version: 5,
      width: 9_525,
      height: 9_525,
      commands: [],
      groups: [],
      strings: [],
      images: [{ partName: 'ppt/media/late.png', relationshipId: 'rId1' }],
      semantics: [],
      diagnostics: [],
      byteLength: 1,
    },
    context,
    {
      fontResolver,
      imageResolver: () => {
        markResolverStarted()
        return new Promise((resolve) => { resolveImage = resolve })
      },
    },
  )
  await resolverStarted
  assert.equal(typeof resolveImage, 'function')
  renderer.clear()
  resolveImage({ source: {}, residentBytes: 64, close: () => { closed += 1 } })
  await rendered
  assert.equal(renderer.decodedImageBytes, 0)
  assert.equal(closed, 1)
})

test('byte-budget LRU evicts and disposes deterministically', () => {
  const disposed = []
  const cache = new ByteBudgetLru(5, (value) => disposed.push(value))
  assert.equal(cache.set('a', 'A', 3), true)
  assert.equal(cache.set('b', 'B', 3), true)
  assert.equal(cache.get('a'), undefined)
  assert.equal(cache.misses, 1)
  assert.equal(cache.residentBytes, 3)
  assert.deepEqual(disposed, ['A'])
  assert.equal(cache.set('large', 'L', 6), false)
  assert.deepEqual(disposed, ['A', 'L'])
  cache.clear()
  assert.equal(cache.residentBytes, 0)
  assert.equal(cache.hitRate, 0)
  assert.deepEqual(disposed, ['A', 'L', 'B'])
})

test('byte-budget LRU does not dispose a resident value while updating its weight', () => {
  const disposed = []
  const value = { close: () => disposed.push('closed') }
  const cache = new ByteBudgetLru(8, (entry) => entry.close())
  assert.equal(cache.set('image', value, 3), true)
  assert.equal(cache.set('image', value, 5), true)
  assert.deepEqual(disposed, [])
  assert.equal(cache.residentBytes, 5)
  assert.equal(cache.get('image'), value)
  cache.clear()
  assert.deepEqual(disposed, ['closed'])
})

test('byte-budget LRU deletes one invalidated revision entry without flushing siblings', () => {
  const cache = new ByteBudgetLru(8)
  cache.set(0, 'first', 3)
  cache.set(1, 'second', 3)
  assert.equal(cache.delete(0), true)
  assert.equal(cache.delete(0), false)
  assert.equal(cache.get(1), 'second')
  assert.equal(cache.residentBytes, 3)
})

test('offscreen thumbnails reject invalid dimensions before host capability checks', async () => {
  const scene = { width: 9_144_000, height: 6_858_000 }
  await assert.rejects(renderOffscreenThumbnail(scene, 0), /maximum width must be positive/)
  await assert.rejects(renderOffscreenThumbnail(scene, Number.NaN), /maximum width must be positive/)
  await assert.rejects(
    renderOffscreenThumbnail({ ...scene, width: 0 }, 320),
    /scene dimensions must be positive/,
  )
})

test('byte-budget LRU remains bounded across a 1000-slide scroll trace', () => {
  const cache = new ByteBudgetLru(4 * 1024)
  for (let slideIndex = 0; slideIndex < 1000; slideIndex += 1) {
    cache.set(slideIndex, Object.freeze({ slideIndex }), 1024)
    assert(cache.residentBytes <= 4 * 1024)
    assert(cache.size <= 4)
  }
  assert.deepEqual([...Array(4).keys()].map((offset) => cache.get(996 + offset)?.slideIndex), [
    996, 997, 998, 999,
  ])
  assert.equal(cache.hitRate, 1)
})

function minimalDisplayList(version = 1) {
  const commandOffset = version >= 7 ? 52 : version >= 2 ? 48 : 40
  const bytes = new Uint8Array(commandOffset + 5)
  const view = new DataView(bytes.buffer)
  bytes.set(new TextEncoder().encode('WPDL'), 0)
  view.setUint16(4, version, true)
  view.setBigInt64(8, 9_144_000n, true)
  view.setBigInt64(16, 6_858_000n, true)
  view.setUint32(24, 1, true)
  bytes.set([1, 255, 255, 255, 255], commandOffset)
  return bytes.buffer
}
