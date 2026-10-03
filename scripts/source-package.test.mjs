import assert from 'node:assert/strict'
import { access, readFile } from 'node:fs/promises'
import test from 'node:test'
import { resolve } from 'node:path'

const root = resolve(import.meta.dirname, '..')

test('root package exports browser, semantic and compiled Node entry points', async () => {
  const packageJson = JSON.parse(await readFile(resolve(root, 'package.json'), 'utf8'))

  assert.deepEqual(packageJson.exports, {
    '.': './packages/wasmppt/src/index.ts',
    './browser-worker': './packages/wasmppt/src/browser-worker.ts',
    './deck': './packages/wasmppt/src/deck.ts',
    './browser': './packages/wasmppt/src/browser.ts',
    './node': { types: './packages/wasmppt/src/node.ts', import: './packages/wasmppt/generated/node.mjs' },
  })
  assert.equal(packageJson.scripts?.prepare, undefined)
  await Promise.all(
    Object.values(packageJson.exports).flatMap(entry => typeof entry === 'string' ? [entry] : Object.values(entry))
      .map(entry => access(resolve(root, entry))),
  )
})
