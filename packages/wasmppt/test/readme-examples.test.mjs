import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

import {
  WasmpptError,
  encodeInjectionData,
  isWasmpptErrorEnvelope,
} from '../dist/index.js'

const readme = await readFile(new URL('../../../README.md', import.meta.url), 'utf8')

test('README publishes the tested generation, rendering, R2, and lifecycle quickstarts', () => {
  for (const heading of [
    '## Prerequisites and build',
    '## Browser generation quickstart',
    '## Browser rendering quickstart',
    '## Cloudflare R2 generation and errors',
    '## Ownership and lifecycle rules',
  ]) assert(readme.includes(heading), heading)
  for (const symbol of [
    'WasmpptWorkerClient',
    'CanvasDisplayListRenderer',
    'encodeInjectionData',
    'releasePresentation',
    'client.terminate()',
  ]) assert(readme.includes(symbol), symbol)
})

test('README R2 payload and error-envelope primitives execute', () => {
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
