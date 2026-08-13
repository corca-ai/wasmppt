import assert from 'node:assert/strict'
import test from 'node:test'

import { WasmFontShaper, decodeLineBreakTokens, decodeShapedFontRun } from '../dist/shaper.js'

function encodedRun() {
  const bytes = new Uint8Array(12 + 25)
  bytes.set(new TextEncoder().encode('WPSH'))
  const view = new DataView(bytes.buffer)
  view.setUint16(4, 1, true)
  view.setUint16(6, 1000, true)
  view.setUint32(8, 1, true)
  view.setUint32(12, 42, true)
  view.setUint32(16, 3, true)
  view.setInt32(20, 640, true)
  view.setInt32(24, -12, true)
  view.setInt32(28, 7, true)
  view.setInt32(32, 9, true)
  bytes[36] = 1
  return bytes
}

function encodedBreaks(text) {
  const offsets = []
  let offset = 0
  for (const character of text) {
    offset += new TextEncoder().encode(character).byteLength
    offsets.push(offset)
  }
  const bytes = new Uint8Array(10 + offsets.length * 5)
  bytes.set(new TextEncoder().encode('WPLB'))
  const view = new DataView(bytes.buffer)
  view.setUint16(4, 1, true)
  view.setUint32(6, offsets.length, true)
  offsets.forEach((value, index) => {
    view.setUint32(10 + index * 5, value, true)
    bytes[14 + index * 5] = index === offsets.length - 1 ? 1 : 0
  })
  return bytes
}

test('font-byte shaper decodes the bounded backend-neutral contract and caches repeats', async () => {
  let calls = 0
  const module = {
    line_breaks: encodedBreaks,
    shape_font(font, faceIndex, text, direction, language, script, features, variations, maxFontBytes, maxTextBytes, maxGlyphs) {
      calls += 1
      assert.deepEqual([...font], calls === 1 ? [1, 2, 3] : [9, 2, 3])
      assert.equal(faceIndex, 0)
      assert.equal(text, 'office')
      assert.equal(direction, 0)
      assert.equal(language, 'en-US')
      assert.equal(script, 'Latn')
      assert.equal(features, 'kern\0liga')
      assert.equal(variations, 'wght=500')
      assert.equal(maxFontBytes, 32 * 1024 * 1024)
      assert.equal(maxTextBytes, 1024 * 1024)
      assert.equal(maxGlyphs, 1_048_576)
      return encodedRun()
    },
  }
  const shaper = new WasmFontShaper(module)
  const request = {
    fontBytes: new Uint8Array([1, 2, 3]),
    text: 'office',
    direction: 'ltr',
    language: 'en-US',
    script: 'Latn',
    features: ['kern', 'liga'],
    variations: ['wght=500'],
  }
  const first = await shaper.shape(request)
  const second = await shaper.shape(request)
  assert.equal(first, second)
  assert.equal(calls, 1)
  assert.deepEqual(first, {
    unitsPerEm: 1000,
    glyphs: [{
      glyphId: 42,
      cluster: 3,
      xAdvance: 640,
      yAdvance: -12,
      xOffset: 7,
      yOffset: 9,
      safeToBreak: true,
    }],
  })
  request.fontBytes[0] = 9
  await shaper.shape(request)
  assert.equal(calls, 2, 'mutating caller-owned font bytes must not reuse stale glyphs')
  await assert.rejects(
    shaper.shape({ ...request, features: Array(65).fill('kern') }),
    /exceeds 64 entries/,
  )
})

test('UAX14 break plans share the bounded cache with shaped runs', async () => {
  let calls = 0
  const module = {
    shape_font: encodedRun,
    line_breaks(text) {
      calls += 1
      return encodedBreaks(text)
    },
  }
  const shaper = new WasmFontShaper(module, { maxCacheBytes: 1024 })
  const first = await shaper.breakText('ab')
  const second = await shaper.breakText('ab')
  assert.equal(first, second)
  assert.equal(calls, 1)
  shaper.clear()
  await shaper.breakText('ab')
  assert.equal(calls, 2)
})

test('font-byte shaped-run decoder rejects corruption before allocation', () => {
  assert.throws(() => decodeShapedFontRun(new Uint8Array(11)), /truncated/)
  const badMagic = encodedRun()
  badMagic[0] = 0
  assert.throws(() => decodeShapedFontRun(badMagic), /magic/)
  const badCount = encodedRun()
  new DataView(badCount.buffer).setUint32(8, 2, true)
  assert.throws(() => decodeShapedFontRun(badCount), /bounds/)
})

test('UAX14 line-break decoder maps UTF-8 offsets to JS source tokens', () => {
  const text = 'A 日本\nB'
  const offsets = [2, 5, 9, 10]
  const bytes = new Uint8Array(10 + offsets.length * 5)
  bytes.set(new TextEncoder().encode('WPLB'))
  const view = new DataView(bytes.buffer)
  view.setUint16(4, 1, true)
  view.setUint32(6, offsets.length, true)
  offsets.forEach((offset, index) => {
    view.setUint32(10 + index * 5, offset, true)
    bytes[14 + index * 5] = index === 2 || index === 3 ? 1 : 0
  })
  assert.deepEqual(decodeLineBreakTokens(text, bytes), ['A', ' ', '日', '本', '\n', 'B'])
  view.setUint32(10, 3, true)
  assert.throws(() => decodeLineBreakTokens(text, bytes), /boundary/)
})
