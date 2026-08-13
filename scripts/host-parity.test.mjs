import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'
import { comparePptx } from './host-parity.mjs'

const root = new URL('../', import.meta.url)

test('host parity accepts exact bytes and classifies the first ZIP mismatch', async () => {
  const first = await readFile(new URL('fixtures/compat/generated-01.pptx', root))
  const second = await readFile(new URL('fixtures/compat/generated-02.pptx', root))
  assert.deepEqual(comparePptx('native', first, 'browser', first), {
    left: 'native',
    right: 'browser',
    identical: true,
    firstDifference: null,
  })
  const mismatch = comparePptx('native', first, 'workerd', second)
  assert.equal(mismatch.identical, false)
  assert.equal(mismatch.firstDifference.entry, 'ppt/slides/slide1.xml')
  assert(['metadata', 'compressed-payload'].includes(mismatch.firstDifference.category))
})
