import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { resolve } from 'node:path'

const root = resolve(import.meta.dirname, '..')
const documents = ['docs/browser.md', 'docs/hosts.md', 'docs/semantic-sdk.md']
await mkdir(resolve(root, 'target'), { recursive: true })
const directory = await mkdtemp(resolve(root, 'target/doc-examples-'))
try {
  const files = []
  for (const document of documents) {
    const source = await readFile(resolve(root, document), 'utf8')
    const blocks = [...source.matchAll(/^```ts\r?\n([\s\S]*?)^```\s*$/gm)]
    assert(blocks.length > 0, `${document} has no runnable TypeScript examples`)
    for (const [index, block] of blocks.entries()) {
      const file = resolve(directory, `${document.replaceAll('/', '-')}-${index + 1}.ts`)
      await writeFile(file, block[1])
      files.push(file)
    }
  }
  const config = resolve(directory, 'tsconfig.json')
  await writeFile(config, JSON.stringify({
    extends: resolve(root, 'packages/wasmppt/tsconfig.json'),
    compilerOptions: {
      noEmit: true,
      rootDir: root,
      paths: {
        '@corca-ai/wasmppt': [resolve(root, 'packages/wasmppt/src/index.ts')],
        '@corca-ai/wasmppt/*': [resolve(root, 'packages/wasmppt/src/*')],
      },
    },
    include: files,
  }))
  execFileSync(process.execPath, [resolve(root, 'node_modules/typescript/bin/tsc'), '-p', config], {
    cwd: root,
    stdio: 'inherit',
  })
  console.log(`TypeScript examples checked from ${documents.join(', ')}`)
} finally {
  await rm(directory, { recursive: true, force: true })
}
