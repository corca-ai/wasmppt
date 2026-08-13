import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

const root = new URL('../', import.meta.url)
const corpus = JSON.parse(await readFile(new URL('fixtures/corpus.json', root), 'utf8'))

test('corpus provenance and redistribution policy are machine-checkable', async () => {
  assert.equal(corpus.schema, 2)
  const identifiers = new Set()
  for (const fixture of corpus.fixtures) {
    assert.match(fixture.id, /^[a-z0-9-]+$/)
    assert(!identifiers.has(fixture.id), `duplicate corpus ID ${fixture.id}`)
    identifiers.add(fixture.id)
    assert.match(fixture.sha256, /^[0-9a-f]{64}$/)
    assert.equal(fixture.license, 'Apache-2.0')
    assert(['allowed', 'fetch-only'].includes(fixture.redistribution))
    assert.equal(typeof fixture.provenance, 'string')
    if (fixture.path !== undefined) {
      assert.equal(fixture.redistribution, 'allowed')
      assert.equal(typeof fixture.generator, 'string')
      const bytes = await readFile(new URL(fixture.path, root))
      assert.equal(createHash('sha256').update(bytes).digest('hex'), fixture.sha256)
      if (fixture.id.startsWith('generated-compat-')) {
        assert(['pull-request', 'scheduled'].includes(fixture.tier))
        assert(fixture.featureTags.length >= 5)
        assert.equal(typeof fixture.producer.name, 'string')
        assert.deepEqual(fixture.scorecard.slides, [0])
        assert.deepEqual(fixture.scorecard.featureRegions[0].tags, fixture.featureTags)
        assert.equal(fixture.scorecard.edit.binding, `case_${fixture.id.slice(-2)}`)
        assert.equal(fixture.scorecard.edit.part, 'ppt/slides/slide1.xml')
        assert.equal(fixture.scorecard.preserve.unknownXmlParts[0], 'docProps/custom.xml')
        assert(fixture.scorecard.preserve.relationshipParts.length >= 2)
        for (const dimension of ['open', 'preserve', 'edit', 'render']) {
          assert.equal(fixture.expected[dimension], 'pass')
        }
      }
    } else {
      assert.equal(fixture.redistribution, 'fetch-only')
      assert.match(fixture.url, /^https:\/\/raw\.githubusercontent\.com\/apache\/poi\//)
    }
  }
  assert(corpus.fixtures.filter((fixture) => fixture.path?.endsWith('.pptx')).length >= 50)
})
