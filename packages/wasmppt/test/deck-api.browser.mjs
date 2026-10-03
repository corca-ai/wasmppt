import assert from 'node:assert/strict'

const spec = text => ({ key: 'browser', slides: [
        { key: 'cover', kind: 'title', nodes: [{ key: 'title', role: 'title', source: { source: 'deck.md', start: 0, end: 10 }, content: { kind: 'text', text: { runs: [{ text }] } } }] },
        { key: 'detail', kind: 'content', nodes: [{ key: 'body', role: 'prose', content: { kind: 'text', text: { runs: [{ text: 'Detail page' }] } } }] },
      ] })


/** Browser-owned contracts: font registration, paint publication, hit testing and closed HTML. */
export async function checkDeckBrowserApi(page) {
  const facts = await page.evaluate(async ({ before, after }) => {
    const { createBrowserEngine, CanvasView, exportHtml } = await import('/dist/browser.js')
    const engine = await createBrowserEngine({ worker: new Worker('/dist/browser-worker.js', { type: 'module' }) })
    const view = new CanvasView(document.body.appendChild(document.createElement('canvas')))
    try {
      const prepared = await engine.prepareDeckTemplate(await (await fetch('/deck-gate-starter.potx')).arrayBuffer())
      if (!prepared.ok) throw new Error(JSON.stringify(prepared.diagnostics))
      const template = prepared.value
      const fontBytes = new Uint8Array(await (await fetch('/font.ttf')).arrayBuffer())
      const fonts = await engine.registerFonts(template.describe().requiredFonts.map(family => ({ family, bytes: fontBytes })))
      const deck = engine.createDeck()
      const old = await deck.replace({ template, fonts, spec: before })
      const next = await deck.replace({ template, fonts, spec: after })
      if (!old.ok || !next.ok) throw new Error('test deck was rejected')
      const first = old.value
      const second = next.value
      const cancelled = view.render(first, { pageId: first.pages[0].id, width: 800 }).then(() => false, error => error.name === 'AbortError')
      const rendered = await view.render(second, { pageId: second.pages[0].id, width: 800 })
      const rect = view.canvas.getBoundingClientRect()
      const source = rendered.scene.semantics.find(item => item.source !== undefined)
      const hit = view.hitTest({
        x: rect.left + (source.bounds.x + source.bounds.width / 2) / rendered.scene.width * rect.width,
        y: rect.top + (source.bounds.y + source.bounds.height / 2) / rendered.scene.height * rect.height,
      })
      const selected = [first.pages[1].id, first.pages[0].id]
      const html = await exportHtml(first, { pageIds: selected })
      const invalidSelection = await exportHtml(first, { pageIds: ['missing-page'] }).then(() => false, error => error instanceof RangeError)
      const duplicateSelection = await exportHtml(first, { pageIds: [selected[0], selected[0]] }).then(() => false, error => error instanceof RangeError)
      const documentCopy = new DOMParser().parseFromString(html.html, 'text/html')
      const result = {
        cancelled: await cancelled, hitKey: hit?.nodeKey, hitRevision: hit?.revision,
        width: view.canvas.width, height: view.canvas.height,
        text: rendered.scene.commands.filter(command => command.kind === 'draw-rich-text').flatMap(command => command.frame.paragraphs.flatMap(paragraph => paragraph.runs.map(run => run.text))).join(''),
        selected, htmlPages: html.pageIds, htmlRevision: html.revision,
        htmlText: documentCopy.body.textContent, hasEmbeddedFont: /@font-face/.test(html.html) && /data:font\//.test(html.html),
        invalidSelection, duplicateSelection, registeredFontsRemaining: document.fonts.size,
      }
      await first.dispose(); await second.dispose(); await fonts.dispose(); await template.dispose(); await deck.dispose()
      result.accountedAfterRelease = await engine.accountedBytes()
      return result
    } finally { await view.dispose(); view.canvas.remove(); engine.dispose() }
  }, { before: spec('Before'), after: spec('After') })
  assert.equal(facts.cancelled, true)
  assert.equal(facts.hitKey, 'title')
  assert.equal(facts.hitRevision, 1)
  assert.equal(facts.width, 800)
  assert.ok(facts.height > 0)
  assert.ok(facts.text.includes('After'))
  assert.deepEqual(facts.htmlPages, facts.selected)
  assert.equal(facts.htmlRevision, 0)
  assert.ok(facts.htmlText.includes('Before'))
  assert.equal(facts.hasEmbeddedFont, true)
  assert.equal(facts.invalidSelection, true)
  assert.equal(facts.duplicateSelection, true)
  assert.equal(facts.registeredFontsRemaining, 0)
  assert.equal(facts.accountedAfterRelease, 0)
  console.log('Semantic SDK browser contracts passed')
}
