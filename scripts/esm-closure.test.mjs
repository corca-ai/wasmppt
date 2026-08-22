import assert from 'node:assert/strict'
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { join, resolve } from 'node:path'
import { tmpdir } from 'node:os'
import test from 'node:test'

import { copyEsmClosure } from './esm-closure.mjs'

test('copies the complete local runtime module closure and explicit assets', async (context) => {
  const temporaryRoot = await mkdtemp(join(tmpdir(), 'wasmppt-esm-closure-'))
  context.after(() => rm(temporaryRoot, { recursive: true, force: true }))
  const appSource = resolve(temporaryRoot, 'source/app')
  const librarySource = resolve(temporaryRoot, 'source/library')
  const output = resolve(temporaryRoot, 'output')
  await writeFiles([
    [resolve(appSource, 'app.js'), "import './lib/first.js'\n"],
    [resolve(librarySource, 'first.js'), "export { value } from './second.js?cache'\nvoid import('./lazy.js')\n"],
    [resolve(librarySource, 'second.js'), 'export const value = 42\n'],
    [resolve(librarySource, 'lazy.js'), 'export const lazy = true\n'],
    [resolve(librarySource, 'payload.wasm'), 'wasm bytes'],
    [resolve(librarySource, 'unused.js'), 'throw new Error("must not be copied")\n'],
  ])

  const copied = await copyEsmClosure({
    entries: [resolve(output, 'app.js'), resolve(output, 'lib/payload.wasm')],
    mounts: [
      { sourceRoot: appSource, outputRoot: output },
      { sourceRoot: librarySource, outputRoot: resolve(output, 'lib') },
    ],
    outputRoot: output,
  })

  assert.deepEqual(copied, [
    'app.js',
    'lib/first.js',
    'lib/lazy.js',
    'lib/payload.wasm',
    'lib/second.js',
  ])
  assert.equal(await readFile(resolve(output, 'lib/payload.wasm'), 'utf8'), 'wasm bytes')
  await assert.rejects(readFile(resolve(output, 'lib/unused.js')), { code: 'ENOENT' })
})

test('fails the build when a local runtime dependency has no source file', async (context) => {
  const temporaryRoot = await mkdtemp(join(tmpdir(), 'wasmppt-esm-missing-'))
  context.after(() => rm(temporaryRoot, { recursive: true, force: true }))
  const source = resolve(temporaryRoot, 'source')
  const output = resolve(temporaryRoot, 'output')
  await writeFiles([[resolve(source, 'entry.js'), "export * from './missing.js'\n"]])

  await assert.rejects(
    copyEsmClosure({
      entries: [resolve(output, 'entry.js')],
      mounts: [{ sourceRoot: source, outputRoot: output }],
      outputRoot: output,
    }),
    /Local ESM dependency is missing: missing\.js/u,
  )
})

test('fails the build when a dynamic import cannot be derived statically', async (context) => {
  const temporaryRoot = await mkdtemp(join(tmpdir(), 'wasmppt-esm-dynamic-'))
  context.after(() => rm(temporaryRoot, { recursive: true, force: true }))
  const source = resolve(temporaryRoot, 'source')
  const output = resolve(temporaryRoot, 'output')
  await writeFiles([[resolve(source, 'entry.js'), 'const name = "optional"\nvoid import(`./${name}.js`)\n']])

  await assert.rejects(
    copyEsmClosure({
      entries: [resolve(output, 'entry.js')],
      mounts: [{ sourceRoot: source, outputRoot: output }],
      outputRoot: output,
    }),
    /Cannot derive computed dynamic ESM dependency/u,
  )
})

async function writeFiles(files) {
  for (const [path, contents] of files) {
    await mkdir(resolve(path, '..'), { recursive: true })
    await writeFile(path, contents)
  }
}
