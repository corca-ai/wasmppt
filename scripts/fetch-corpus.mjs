import { createHash } from 'node:crypto'
import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { basename, resolve } from 'node:path'

const root = resolve(import.meta.dirname, '..')
const outputDirectory = resolve(process.argv[2] ?? 'target/corpus')
const requested = new Set(process.argv.slice(3))
const manifest = JSON.parse(await readFile(resolve(root, 'fixtures/corpus.json'), 'utf8'))
const fixtures = manifest.fixtures.filter((fixture) =>
  fixture.url !== undefined && (requested.size === 0 || requested.has(fixture.id)),
)
if (requested.size !== 0 && fixtures.length !== requested.size) {
  const found = new Set(fixtures.map((fixture) => fixture.id))
  throw new Error(`unknown corpus IDs: ${[...requested].filter((id) => !found.has(id)).join(', ')}`)
}
await mkdir(outputDirectory, { recursive: true })
for (const fixture of fixtures) {
  const response = await fetch(fixture.url)
  if (!response.ok) throw new Error(`cannot fetch ${fixture.id}: HTTP ${response.status}`)
  const bytes = new Uint8Array(await response.arrayBuffer())
  const actual = createHash('sha256').update(bytes).digest('hex')
  if (actual !== fixture.sha256) throw new Error(`${fixture.id} hash ${actual} != ${fixture.sha256}`)
  const path = resolve(outputDirectory, basename(new URL(fixture.url).pathname))
  await writeFile(path, bytes)
  console.log(`${fixture.id}\t${path}\t${bytes.byteLength}`)
}
