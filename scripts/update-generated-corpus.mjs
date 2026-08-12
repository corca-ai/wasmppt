import { createHash } from 'node:crypto'
import { readFile, writeFile } from 'node:fs/promises'

const root = new URL('../', import.meta.url)
const manifestUrl = new URL('fixtures/corpus.json', root)
const manifest = JSON.parse(await readFile(manifestUrl, 'utf8'))
manifest.fixtures = manifest.fixtures.filter((fixture) => !fixture.id.startsWith('generated-compat-'))
for (let caseNumber = 1; caseNumber <= 50; caseNumber += 1) {
  const suffix = String(caseNumber).padStart(2, '0')
  const path = `fixtures/compat/generated-${suffix}.pptx`
  const bytes = await readFile(new URL(path, root))
  const featureTags = [
    'text-multilingual',
    caseNumber % 3 === 0 ? 'text-decoration' : 'text-basic',
    caseNumber % 4 === 0 ? 'gradient' : 'theme-fill',
    caseNumber % 5 === 0 ? 'rtl' : 'ltr',
    caseNumber % 7 === 0 ? 'vertical-text' : 'horizontal-text',
    `preset-${(caseNumber - 1) % 10}`,
  ]
  manifest.fixtures.push({
    id: `generated-compat-${suffix}`,
    path,
    sha256: createHash('sha256').update(bytes).digest('hex'),
    provenance: 'generated-in-repository',
    producer: { name: 'wasmppt', version: 'corpus-v1' },
    generator: 'cargo run -p wasmppt-native --example write_compat_corpus -- fixtures/compat',
    license: 'Apache-2.0',
    redistribution: 'allowed',
    tier: caseNumber <= 10 ? 'pull-request' : 'scheduled',
    featureTags,
    expected: { open: 'pass', preserve: 'pass', edit: 'pass', render: 'pass', diagnostics: [] },
  })
}
await writeFile(manifestUrl, `${JSON.stringify(manifest, null, 2)}\n`)
