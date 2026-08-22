import assert from 'node:assert/strict'
import test from 'node:test'

import { decodeRasterImage, inspectRasterImageMetadata } from '../dist/image.js'

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
