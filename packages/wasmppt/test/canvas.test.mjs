import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

import {
  ByteBudgetLru,
  CanvasDisplayListRenderer,
  FontResolver,
  buildRichTextLayout,
  decodeDisplayList,
  decodeRasterImage,
  decodeOoxmlObfuscatedFont,
  inspectOpenTypeEmbedding,
  inspectRasterImageMetadata,
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

test('display-list decoder rejects corruption and decodes a bounded scene', () => {
  const bytes = minimalDisplayList()
  const scene = decodeDisplayList(bytes)
  assert.equal(scene.width, 9_144_000)
  assert.equal(scene.height, 6_858_000)
  assert.deepEqual(scene.commands, [
    { kind: 'clear', color: { red: 255, green: 255, blue: 255, alpha: 255 } },
  ])
  for (const version of [2, 3, 4, 5, 6, 7]) {
    assert.equal(decodeDisplayList(minimalDisplayList(version)).version, version)
  }
  assert.throws(() => decodeDisplayList(bytes.slice(0, -1)), /truncated/)
  const wrongVersion = bytes.slice()
  new DataView(wrongVersion).setUint16(4, 99, true)
  assert.throws(() => decodeDisplayList(wrongVersion), /version 99/)
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
      flow: 'horizontal',
    },
  }
  const context = { font: '', measureText: (value) => ({ width: value.length * 8 }) }
  const plan = await buildRichTextLayout(context, command)
  assert(Math.abs(plan.runs[0].fontSize - 12.8) < 1e-9)
  assert(Math.abs(plan.runs[1].baseline - plan.runs[0].baseline - 15.36) < 1e-9)
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

test('image metadata enforces deterministic PNG, JPEG, GIF, and safe SVG boundaries', () => {
  const png = new Uint8Array(24)
  png.set([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a], 0)
  new DataView(png.buffer).setUint32(8, 13)
  png.set([0x49, 0x48, 0x44, 0x52], 12)
  new DataView(png.buffer).setUint32(16, 640)
  new DataView(png.buffer).setUint32(20, 360)
  assert.deepEqual(inspectRasterImageMetadata(png), {
    format: 'png', width: 640, height: 360, orientation: 1,
  })

  const jpeg = new Uint8Array([
    0xff, 0xd8,
    0xff, 0xc0, 0x00, 0x11, 0x08, 0x01, 0x68, 0x02, 0x80,
    0x03, 0x01, 0x11, 0x00, 0x02, 0x11, 0x00, 0x03, 0x11, 0x00,
    0xff, 0xd9,
  ])
  assert.deepEqual(inspectRasterImageMetadata(jpeg), {
    format: 'jpeg', width: 640, height: 360, orientation: 1,
  })
  const oriented = new Uint8Array([
    0xff, 0xd8, 0xff, 0xe1, 0x00, 0x22,
    0x45, 0x78, 0x69, 0x66, 0x00, 0x00,
    0x49, 0x49, 0x2a, 0x00, 0x08, 0x00, 0x00, 0x00,
    0x01, 0x00, 0x12, 0x01, 0x03, 0x00, 0x01, 0x00, 0x00, 0x00,
    0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ...jpeg.subarray(2),
  ])
  assert.equal(inspectRasterImageMetadata(oriented).orientation, 6)
  const gif = Uint8Array.of(0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x80, 0x02, 0x68, 0x01)
  assert.deepEqual(inspectRasterImageMetadata(gif), {
    format: 'gif', width: 640, height: 360, orientation: 1,
  })
  const svg = new TextEncoder().encode('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 360"/>')
  assert.deepEqual(inspectRasterImageMetadata(svg), {
    format: 'svg', width: 640, height: 360, orientation: 1,
  })
  const unitSvg = new TextEncoder().encode(
    '<svg xmlns="http://www.w3.org/2000/svg" stroke-width="999" width="1in" height="0.5in"/>',
  )
  assert.deepEqual(inspectRasterImageMetadata(unitSvg), {
    format: 'svg', width: 96, height: 48, orientation: 1,
  })
  assert.throws(
    () => inspectRasterImageMetadata(new TextEncoder().encode('<svg width="1" height="1"><script/></svg>')),
    /active or external/,
  )
  for (const unsafe of [
    '<svg width="1" height="1" onload="alert(1)"/>',
    '<svg width="1" height="1"><style>@import url(https://example.com/a.css)</style></svg>',
    '<svg width="1" height="1"><image href="relative.png"/></svg>',
    '<?xml-stylesheet href="https://example.com/a.css"?><svg width="1" height="1"/>',
  ]) {
    assert.throws(
      () => inspectRasterImageMetadata(new TextEncoder().encode(unsafe)),
      /active or external/,
    )
  }
  const lateScript = new TextEncoder().encode(
    `<svg width="1" height="1">${' '.repeat(1024 * 1024)}<script/></svg>`,
  )
  assert.throws(() => inspectRasterImageMetadata(lateScript), /active or external/)
  assert.throws(
    () => inspectRasterImageMetadata(new TextEncoder().encode('<svg width="1e309" height="1"/>')),
    /dimensions/,
  )
  const fakePng = png.slice()
  fakePng[4] = 0
  assert.throws(() => inspectRasterImageMetadata(fakePng), /supported raster image/)
  assert.throws(() => inspectRasterImageMetadata(new Uint8Array([1, 2, 3])), /supported raster image/)
})

test('raster decoding rejects byte limits before browser allocation', async () => {
  await assert.rejects(
    decodeRasterImage(new Uint8Array(25), { maxBytes: 24 }),
    /24-byte decode limit/,
  )
  await assert.rejects(decodeRasterImage(new Uint8Array(), { maxBytes: Number.NaN }), /byte limit/)
  await assert.rejects(decodeRasterImage(new Uint8Array(), { maxPixels: -1 }), /pixel limit/)
})

test('raster decoding closes a created bitmap when cancellation wins the allocation race', async () => {
  const png = new Uint8Array(24)
  png.set([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a], 0)
  new DataView(png.buffer).setUint32(8, 13)
  png.set([0x49, 0x48, 0x44, 0x52], 12)
  new DataView(png.buffer).setUint32(16, 1)
  new DataView(png.buffer).setUint32(20, 1)
  const controller = new AbortController()
  let closed = 0
  const original = globalThis.createImageBitmap
  globalThis.createImageBitmap = async () => {
    controller.abort()
    return { close: () => { closed += 1 } }
  }
  try {
    await assert.rejects(decodeRasterImage(png, {}, controller.signal), { name: 'AbortError' })
    assert.equal(closed, 1)
  } finally {
    if (original === undefined) delete globalThis.createImageBitmap
    else globalThis.createImageBitmap = original
  }
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
