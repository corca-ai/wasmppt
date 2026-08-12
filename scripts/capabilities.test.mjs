import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

const matrix = JSON.parse(
  await readFile(new URL('../capabilities/presentationml.json', import.meta.url), 'utf8'),
)

test('every feature independently classifies read, preserve, edit, and render support', () => {
  assert.equal(matrix.schema, 1)
  assert.equal(matrix.displayListVersion, 3)
  assert(matrix.features.length >= 16)
  const identifiers = new Set()
  for (const feature of matrix.features) {
    assert.equal(typeof feature.id, 'string')
    assert(!identifiers.has(feature.id), `duplicate feature ID ${feature.id}`)
    identifiers.add(feature.id)
    for (const dimension of ['read', 'preserve', 'edit', 'render']) {
      assert.equal(typeof feature[dimension], 'string', `${feature.id}.${dimension} is missing`)
      assert(feature[dimension].length > 0, `${feature.id}.${dimension} is empty`)
    }
  }
})
