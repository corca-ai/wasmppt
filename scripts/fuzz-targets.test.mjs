import assert from 'node:assert/strict'
import { readdir, readFile } from 'node:fs/promises'
import test from 'node:test'

test('scheduled fuzz runner enumerates every checked-in target', async () => {
  const targetDirectory = new URL('../crates/wasmppt-opc/fuzz/fuzz_targets/', import.meta.url)
  const targets = (await readdir(targetDirectory))
    .filter((name) => name.endsWith('.rs'))
    .map((name) => name.slice(0, -3))
    .toSorted()
  const runner = await readFile(new URL('run-fuzz-ci.sh', import.meta.url), 'utf8')
  const declared = runner.match(/for target in ([^;]+); do/)?.[1].trim().split(/\s+/).toSorted()

  assert.deepEqual(declared, targets)
})
