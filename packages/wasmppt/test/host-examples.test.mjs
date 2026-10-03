import assert from 'node:assert/strict'
import test from 'node:test'

import {
  WasmpptError,
  encodeInjectionData,
  isWasmpptErrorEnvelope,
} from '../dist/index.js'

test('Documented R2 payload and error-envelope primitives execute', () => {
  const payload = new Uint8Array(encodeInjectionData({ text: { title: 'Quarterly report' } }))
  assert.equal(new TextDecoder().decode(payload.subarray(0, 4)), 'WPPD')
  const envelope = {
    version: 1,
    domain: 'template',
    code: 'missing-value',
    message: 'informational',
    bindingId: 'title',
  }
  assert.equal(isWasmpptErrorEnvelope(envelope), true)
  assert.equal(new WasmpptError(envelope).code, 'missing-value')
})
