import { spawnSync } from 'node:child_process'
import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'

const root = resolve(new URL('../', import.meta.url).pathname)
const manifest = JSON.parse(await readFile(resolve(root, 'fixtures/corpus.json'), 'utf8'))
const all = process.argv.includes('--all')
const outputArgument = process.argv.find((value) => value.startsWith('--output='))
const output = resolve(root, outputArgument?.slice('--output='.length) ?? 'target/corpus-scorecard.json')
const binary = resolve(root, process.platform === 'win32' ? 'target/debug/wasmppt.exe' : 'target/debug/wasmppt')
const fixtures = manifest.fixtures.filter((fixture) =>
  fixture.path?.endsWith('.pptx') && (all
    ? fixture.tier === 'pull-request' || fixture.tier === 'scheduled'
    : fixture.tier === 'pull-request'))
const results = fixtures.map((fixture) => {
  const path = resolve(root, fixture.path)
  const validate = spawnSync(binary, ['validate', path], { encoding: 'utf8' })
  const render = spawnSync(binary, ['resolve', path, '0'], { encoding: 'utf8' })
  return {
    id: fixture.id,
    sha256: fixture.sha256,
    featureTags: fixture.featureTags ?? [],
    open: validate.status === 0 ? 'pass' : 'fail',
    preserve: validate.status === 0 ? 'pass' : 'fail',
    edit: fixture.expected?.edit ?? 'unclassified',
    render: render.status === 0 ? 'pass' : 'fail',
    diagnostics: render.stderr.trim().split('\n').filter(Boolean),
  }
})
const features = new Map()
for (const result of results) {
  for (const feature of result.featureTags) {
    const current = features.get(feature) ?? { cases: 0, passed: 0 }
    current.cases += 1
    if (result.open === 'pass' && result.render === 'pass') current.passed += 1
    features.set(feature, current)
  }
}
const report = {
  schema: 1,
  tier: all ? 'scheduled' : 'pull-request',
  presentations: results,
  features: Object.fromEntries([...features].toSorted(([left], [right]) => left.localeCompare(right))),
}
await mkdir(dirname(output), { recursive: true })
await writeFile(output, `${JSON.stringify(report, null, 2)}\n`)
if (results.some((result) => result.open === 'fail' || result.render === 'fail')) process.exitCode = 1
