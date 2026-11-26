import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { readFile } from 'node:fs/promises'
import test from 'node:test'
import { academicAssets, academicCases, measureAcademicSnapshot } from './academic-layout-evidence.mjs'

test('academic corpus retains validated source-order relations and research attribution', () => {
  const slides = academicCases()
  const cases = new Set(slides.map(slide => slide.key))
  assert.equal(cases.size, slides.length)
  for (const ratio of ['4x1', '16x9', '1x1', '3x4', '1x4']) {
    for (const context of ['alone', 'caption', 'prose', 'list']) assert.ok(cases.has(`${ratio}-${context}`))
  }
  const keys = new Set()
  for (const slide of slides) {
    const nodes = new Map()
    const collect = values => {
      for (const node of values) {
        assert.ok(!keys.has(node.key), `duplicate key ${node.key}`)
        keys.add(node.key); nodes.set(node.key, node)
        assert.equal(node.source.source, `${slide.key}.md`)
        assert.ok(node.source.start < node.source.end)
        if (node.content.kind === 'children') collect(node.content.nodes)
      }
    }
    collect(slide.nodes)
    for (const relation of slide.mediaTextRelations ?? []) {
      const media = nodes.get(relation.mediaNodeKey), copy = nodes.get(relation.textNodeKey)
      assert.equal(media.content.kind, 'image')
      assert.ok(relation.textSide === 'before-media' ? copy.source.end <= media.source.start : media.source.end <= copy.source.start)
      assert.equal(relation.explicitCaption, copy.role === 'caption')
    }
  }
})

test('independent research figures preserve licensed upstream bytes and complete dimensions', async () => {
  const provenance = JSON.parse(await readFile(new URL('../fixtures/deck-gates/research/provenance.json', import.meta.url), 'utf8'))
  assert.equal(provenance.article.license, 'CC-BY-4.0')
  const assets = await academicAssets()
  for (const figure of provenance.figures) {
    const asset = assets.get(`fig${figure.figure}`)
    assert.equal(createHash('sha256').update(asset.bytes).digest('hex'), figure.sha256)
    const header = new DataView(asset.bytes.buffer, asset.bytes.byteOffset, asset.bytes.byteLength)
    assert.equal(header.getUint32(16), figure.width)
    assert.equal(header.getUint32(20), figure.height)
    assert.deepEqual(asset.intrinsicSize, { width: figure.width, height: figure.height })
    assert.equal(figure.alterations, 'none')
  }
})


test('resolved short-title Starter pages expose reclaimed safe body area for every purpose', async () => {
  const { createNodeEngine } = await import('../packages/wasmppt/generated/node.mjs')
  const engine = await createNodeEngine()
  const slide = academicCases().find(value => value.key === 'research-question')
  try {
    for (const presentation of ['conference', 'seminar', 'handout']) {
      const suffix = presentation === 'seminar' ? '' : `-${presentation}`
      const prepared = await engine.prepareDeckTemplate(await readFile(new URL(`../fixtures/deck-gates/starter${suffix}.potx`, import.meta.url)))
      assert.equal(prepared.ok, true)
      const template = prepared.value, deck = engine.createDeck()
      try {
        const accepted = await deck.replace({ template, spec: { key: 'actual-body-test', layout: { presentation, imageFit: 'contain' }, slides: [slide] } })
        assert.equal(accepted.ok, true)
        const snapshot = accepted.value
        try {
          const evidence = await measureAcademicSnapshot(snapshot, slide, template, new Map(), presentation, true)
          assert.deepEqual(evidence.errors, [])
          assert.equal(evidence.pages.length, 1)
          const page = evidence.pages[0]
          assert.ok(page.actualBodySafeAreaRatio >= 0.72)
          assert.ok(page.bodyReclaimOpportunityEmu > 0)
          assert.ok(page.bodyReclaimedEmu > 0)
          assert.ok(page.measuredHeaderHeight < page.preferredHeaderHeight)
          const header = page.fragments.find(fragment => fragment.role === 'title')
          assert.ok(page.actualBodyTop >= header.bounds.y + header.bounds.height)
        } finally { await snapshot.dispose() }
      } finally { await deck.dispose(); await template.dispose() }
    }
  } finally { engine.dispose() }
})

test('continued accepted snapshots paint the upstream title label on every page', async () => {
  const { createNodeEngine } = await import('../packages/wasmppt/generated/node.mjs')
  const engine = await createNodeEngine()
  const prepared = await engine.prepareDeckTemplate(await readFile(new URL('../fixtures/deck-gates/starter.potx', import.meta.url)))
  assert.equal(prepared.ok, true)
  const template = prepared.value, deck = engine.createDeck()
  try {
    const slide = academicCases().find(value => value.key === 'method')
    slide.nodes[1].content.text.runs[0].text = slide.nodes[1].content.text.runs[0].text.repeat(4)
    const accepted = await deck.replace({ template, spec: { key: 'continued-title-test', slides: [slide] } })
    assert.equal(accepted.ok, true)
    const snapshot = accepted.value
    try {
      assert.ok(snapshot.pages.length > 1)
      for (const page of snapshot.pages) {
        const scene = await snapshot.resolvePage(page.id)
        const text = scene.commands.filter(command => command.kind === 'draw-rich-text').flatMap(command => command.frame.paragraphs.map(paragraph => paragraph.runs.map(run => run.text).join(''))).join('\n')
        assert.ok(text.includes(`Learning curves / 학습 곡선 (${page.continuationOrdinal}/${page.continuationTotal})`), text)
        const heading = scene.semantics.find(item => item.source && snapshot.sourceIdentity(item.source.semanticId)?.key === slide.nodes[0].key)
        assert.ok(heading)
        assert.deepEqual(snapshot.sourceIdentity(heading.source.semanticId).source, slide.nodes[0].source)
      }
    } finally { await snapshot.dispose() }
  } finally { await deck.dispose(); await template.dispose(); engine.dispose() }
})

test('public SDK measures canonical source orders and adaptive final page fonts', async () => {
  const { createNodeEngine } = await import('../packages/wasmppt/generated/node.mjs')
  const engine = await createNodeEngine(), assets = await academicAssets(), registered = new Map()
  try {
    for (const [id, input] of assets) registered.set(id, await engine.registerAsset(input))
    for (const presentation of ['conference', 'seminar', 'handout']) {
      const prepared = await engine.prepareDeckTemplate(await readFile(new URL('../fixtures/deck-gates/starter.potx', import.meta.url)))
      assert.equal(prepared.ok, true)
      const template = prepared.value, deck = engine.createDeck(), measured = new Map()
      try {
        for (const slide of academicCases({ expanded: true }).filter(value => /^(canonical|adaptive)-/.test(value.key))) {
          const accepted = await deck.replace({ template, assets: registered, spec: { key: 'expanded-academic', layout: { presentation }, slides: [slide] } })
          assert.equal(accepted.ok, true, JSON.stringify(accepted.diagnostics))
          const snapshot = accepted.value
          try {
            snapshot.assertComplete()
            const evidence = await measureAcademicSnapshot(snapshot, slide, template, assets, presentation, true)
            assert.deepEqual(evidence.errors, [])
            measured.set(slide.key, evidence)
            if (slide.key.startsWith('canonical')) {
              assert.equal(evidence.pages.length, 1)
              const copy = evidence.pages[0].fragments.find(fragment => fragment.role === 'list')
              const figure = evidence.pages[0].fragments.find(fragment => fragment.role === 'figure')
              assert.ok(copy.bounds.x + copy.bounds.width <= figure.bounds.x)
              assert.equal(copy.readingOrder < figure.readingOrder, slide.key === 'canonical-list-image')
            }
            if (evidence.pages.length > 1) for (const page of evidence.pages) {
              const label = `(${page.continuationOrdinal}/${page.continuationTotal})`
              assert.ok(page.paintedText.some(shape => shape.text.endsWith(label)))
            }
          } finally { await snapshot.dispose() }
        }
        const fonts = key => measured.get(key).pages.flatMap(page => page.fragments.filter(fragment => fragment.role === 'list').flatMap(fragment => fragment.fontSizes))
        assert.ok(Math.min(...fonts('adaptive-sparse-two')) > Math.max(...fonts('adaptive-normal-eight')))
        const nominal = { conference: 2400, seminar: 2000, handout: 1800 }[presentation]
        assert.ok(Math.min(...fonts('adaptive-normal-eight')) >= nominal)
        assert.equal(measured.get('adaptive-nearfull-one').pages.length, 1)
        assert.equal(measured.get('adaptive-nearfull-two').pages.length, 1)
        const continued = measured.get('adaptive-continued-spare')
        assert.equal(continued.pages.length, 2)
        assert.ok(continued.pages.every(page => Math.min(...page.fragments.filter(fragment => fragment.role === 'list').flatMap(fragment => fragment.fontSizes)) >= nominal))
        assert.ok(Math.max(...fonts('adaptive-continued-spare')) > Math.min(...fonts('adaptive-nearfull-two')))
      } finally { await deck.dispose(); await template.dispose() }
    }
  } finally { for (const asset of registered.values()) await asset.dispose(); engine.dispose() }
})
