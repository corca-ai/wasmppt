import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

import {
  ByteBudgetLru,
  FontResolver,
  decodeDisplayList,
  measureTextBatch,
  wrapText,
} from '../dist/index.js'

const textCorpus = JSON.parse(
  await readFile(new URL('../../../fixtures/text-edge-cases.json', import.meta.url), 'utf8'),
)

test('display-list decoder rejects corruption and decodes a bounded scene', () => {
  const bytes = minimalDisplayList()
  const scene = decodeDisplayList(bytes)
  assert.equal(scene.width, 9_144_000)
  assert.equal(scene.height, 6_858_000)
  assert.deepEqual(scene.commands, [
    { kind: 'clear', color: { red: 255, green: 255, blue: 255, alpha: 255 } },
  ])
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
    _font: '',
    get font() {
      return this._font
    },
    set font(value) {
      this._font = value
      assigned.push(value)
    },
    measureText(value) {
      return { width: value.length * (this._font === '20px Exact Korean' ? 2 : 1) }
    },
  }
  assert.deepEqual(
    measureTextBatch(context, [
      { text: '가나', font: '20px Exact Korean' },
      { text: 'abc', font: '18px Exact Latin' },
      { text: '다라', font: '20px Exact Korean' },
    ]),
    [4, 3, 4],
  )
  assert.deepEqual(assigned, ['20px Exact Korean', '18px Exact Latin'])
})

test('byte-budget LRU evicts and disposes deterministically', () => {
  const disposed = []
  const cache = new ByteBudgetLru(5, (value) => disposed.push(value))
  assert.equal(cache.set('a', 'A', 3), true)
  assert.equal(cache.set('b', 'B', 3), true)
  assert.equal(cache.get('a'), undefined)
  assert.equal(cache.residentBytes, 3)
  assert.deepEqual(disposed, ['A'])
  assert.equal(cache.set('large', 'L', 6), false)
  assert.deepEqual(disposed, ['A', 'L'])
  cache.clear()
  assert.equal(cache.residentBytes, 0)
  assert.deepEqual(disposed, ['A', 'L', 'B'])
})

function minimalDisplayList() {
  const bytes = new Uint8Array(45)
  const view = new DataView(bytes.buffer)
  bytes.set(new TextEncoder().encode('WPDL'), 0)
  view.setUint16(4, 1, true)
  view.setBigInt64(8, 9_144_000n, true)
  view.setBigInt64(16, 6_858_000n, true)
  view.setUint32(24, 1, true)
  bytes.set([1, 255, 255, 255, 255], 40)
  return bytes.buffer
}
