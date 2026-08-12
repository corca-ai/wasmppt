import assert from 'node:assert/strict'
import test from 'node:test'

import { encodeInjectionData } from '../dist/injection.js'

test('structured injection payload is deterministic and versioned', () => {
  const payload = new Uint8Array(encodeInjectionData({
    text: { title: '분기 보고서' },
    images: {
      hero: {
        bytes: Uint8Array.of(1, 2, 3),
        extension: 'png',
        contentType: 'image/png',
        crop: { left: 1, top: 2, right: 3, bottom: 4 },
      },
    },
    tables: { revenue: [{ region: '서울' }] },
    slides: { 'ppt/slides/slide2.xml': 3 },
    charts: {
      'ppt/charts/chart1.xml': {
        categories: ['Q1', 'Q2'],
        series: [{ name: 'Sales', values: [1.5, 2.5] }],
      },
    },
  }))
  assert.deepEqual([...payload.subarray(0, 8)], [0x57, 0x50, 0x50, 0x44, 2, 0, 0, 0])
  assert.equal(toHex(payload), GOLDEN_HEX)
})

test('structured injection payload rejects unsafe numeric values', () => {
  assert.throws(
    () => encodeInjectionData({ slides: { 'ppt/slides/slide1.xml': -1 } }),
    /unsigned 32-bit integer/,
  )
  assert.throws(
    () => encodeInjectionData({
      charts: { 'ppt/charts/chart1.xml': { categories: ['Q1'], series: [{ name: 'x', values: [NaN] }] } },
    }),
    /must be finite/,
  )
})

const GOLDEN_HEX = '575050440200000001000000050000007469746c6510000000ebb684eab8b020ebb3b4eab3a0ec849c01000000040000006865726f03000000706e6709000000696d6167652f706e67010100000002000000030000000400000003000000010203000100000007000000726576656e7565010000000100000006000000726567696f6e06000000ec849cec9ab801000000150000007070742f736c696465732f736c696465322e786d6c0300000001000000150000007070742f6368617274732f6368617274312e786d6c02000000020000005131020000005132010000000500000053616c657302000000000000000000f83f00000000000004400000000000000000'

function toHex(bytes) {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('')
}
