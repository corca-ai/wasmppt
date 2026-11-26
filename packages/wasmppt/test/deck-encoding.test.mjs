import assert from 'node:assert/strict'
import test from 'node:test'
import { encodeAsset, encodeDeck } from '../dist/deck/encode.js'

const spec = { key: 'research', slides: [] }

test('document layout has canonical WDSF v6 defaults and distinct purpose identity', async () => {
  const implicit = await encodeDeck(spec)
  const explicit = await encodeDeck({ ...spec, layout: { presentation: 'seminar', imageFit: 'contain' } })
  assert.deepEqual(implicit.bytes, explicit.bytes)
  assert.equal(new TextDecoder().decode(implicit.bytes.slice(0, 4)), 'WDSF')
  assert.equal(new DataView(implicit.bytes.buffer).getUint32(4, true), 6)
  assert.deepEqual(implicit.bytes.slice(24, 26), Uint8Array.of(2, 1))
  for (const [presentation, tag] of [['conference', 1], ['handout', 3]]) {
    const encoded = await encodeDeck({ ...spec, layout: { presentation } })
    assert.notDeepEqual(encoded.bytes, implicit.bytes)
    assert.deepEqual(encoded.bytes.slice(24, 26), Uint8Array.of(tag, 1))
    assert.deepEqual(encoded.bytes.slice(0, 24), implicit.bytes.slice(0, 24), 'semantic document identity is stable')
  }
})

test('the typed boundary rejects unsupported purpose and cropped image policies', async () => {
  for (const layout of [{ presentation: 'poster' }, { imageFit: 'cover' }, { imageFit: 'stretch' }]) {
    await assert.rejects(encodeDeck({ ...spec, layout }), TypeError)
  }
})

test('asset registrations use the same v6 envelope and canonical layout prefix', () => {
  const bytes = encodeAsset('01'.repeat(16), { bytes: Uint8Array.of(1), mediaType: 'image/png' })
  assert.equal(new DataView(bytes.buffer).getUint32(4, true), 6)
  assert.deepEqual(bytes.slice(24, 26), Uint8Array.of(2, 1))
  assert.equal(new DataView(bytes.buffer).getUint32(26, true), 0, 'registration carries no logical slides')
  assert.equal(new DataView(bytes.buffer).getUint32(30, true), 1, 'registration carries exactly one resource')
})
