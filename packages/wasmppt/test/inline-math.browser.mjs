import assert from 'node:assert/strict'

/** Compare formula geometry with the real positioned text plan, including long Korean prefixes. */
export async function checkInlineMathBrowserApi(page, measureInlineText = true, templatePath = '/deck-gate-starter.potx') {
  const facts = await page.evaluate(async ({ enabled, templateUrl }) => {
    const { createBrowserEngine, CanvasView, exportHtml } = await import('/dist/browser.js')
    const engine = await createBrowserEngine({ worker: new Worker('/dist/browser-worker.js', { type: 'module' }), measureInlineText: enabled })
    const canvas = document.body.appendChild(document.createElement('canvas'))
    const view = new CanvasView(canvas)
    const results = []
    try {
      const prepared = await engine.prepareDeckTemplate(await (await fetch(templateUrl)).arrayBuffer())
      if (!prepared.ok) throw new Error(JSON.stringify(prepared.diagnostics))
      const template = prepared.value
      const formula = await engine.registerAsset({ mediaType: 'image/svg+xml', bytes: new TextEncoder().encode(
        '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><text y="12">x</text></svg>'), intrinsicSize: { width: 16, height: 16 } })
      const pngCanvas = document.createElement('canvas')
      pngCanvas.width = 16; pngCanvas.height = 16
      const png = await new Promise(resolve => pngCanvas.toBlob(resolve))
      const fallback = await engine.registerAsset({ mediaType: 'image/png', bytes: new Uint8Array(await png.arrayBuffer()) })
      const deck = engine.createDeck()
      try {
        for (const [prefix, marks] of [
          ['text ', {}], ['text '.repeat(7), {}], ['한글 ', {}], ['한글 '.repeat(7), {}],
          ['Mixed 한글 text 한글 ', { bold: true, italic: true }], ['code '.repeat(5), { inlineCode: true }],
          ['text '.repeat(23), {}], ['한글 '.repeat(23), {}],
        ]) {
          const spec = { key: 'inline', slides: [{ key: 'slide', kind: 'content', nodes: [{
            key: 'paragraph', role: 'prose', split: 'children', content: { kind: 'children', nodes: [
              ...Array.from(prefix.matchAll(/\s*\S+(?:\s+|$)/gu), (match, index) => ({
                key: `prefix-${index}`, role: 'prose', content: { kind: 'text', text: { runs: [{ text: match[0], marks }] } },
              })),
              { key: 'math', role: 'prose', content: { kind: 'svg', assetId: 'formula', fallbackAssetId: 'fallback', sourceText: '$x$' } },
              { key: 'suffix', role: 'prose', split: 'text', content: { kind: 'text', text: { runs: [{ text: ' suffix' }] } } },
            ] },
          }] }] }
          const accepted = await deck.replace({ template, spec, assets: new Map([['formula', formula], ['fallback', fallback]]) })
          if (!accepted.ok) throw new Error(JSON.stringify(accepted.diagnostics))
          const snapshot = accepted.value
          try {
            const { scene } = await view.render(snapshot, { pageId: snapshot.pages[0].id })
            const math = scene.semantics.find(item => item.source && snapshot.sourceIdentity(item.source.semanticId)?.key === 'math')
            const prefixCommand = scene.commands.slice(0, math.firstCommand).findLast(command => command.kind === 'draw-rich-text')
            const { buildRichTextLayout, FontResolver } = await import('/dist/canvas.js')
            const layout = await buildRichTextLayout(canvas.getContext('2d'), prefixCommand, new FontResolver())
            const right = Math.max(...layout.runs.map(run => run.x + run.width)) * 9525
            const sameRow = Math.abs(prefixCommand.bounds.y + prefixCommand.bounds.height / 2
              - math.bounds.y - math.bounds.height / 2) < 1
            const html = await exportHtml(snapshot)
            const copy = new DOMParser().parseFromString(html.html, 'text/html')
            results.push({ prefix, wrapped: !sameRow,
              prefixRows: new Set(scene.commands.slice(0, math.firstCommand)
                .filter(command => command.kind === 'draw-rich-text').map(command => command.bounds.y)).size,
              gap: (math.bounds.x - (sameRow ? right : prefixCommand.bounds.x)) / 9525,
              lineCount: new Set(layout.runs.map(run => run.baseline)).size,
              htmlHasFormula: copy.querySelector('image') !== null,
              exportedBytes: (await new Response(snapshot.exportPptx()).arrayBuffer()).byteLength })
          } finally { await snapshot.dispose() }
        }
      } finally { await deck.dispose(); await formula.dispose(); await fallback.dispose(); await template.dispose() }
    } finally { await view.dispose(); canvas.remove(); engine.dispose() }
    return results
  }, { enabled: measureInlineText, templateUrl: templatePath })
  console.log('Inline formula prefix advances', JSON.stringify(facts))
  for (const fact of facts) {
    assert.ok(Math.abs(fact.gap) < 0.1, JSON.stringify(fact))
    assert.equal(fact.lineCount, 1, 'the renderer must preserve the planner-owned inline row')
    assert.equal(fact.htmlHasFormula, true)
    assert.ok(fact.exportedBytes > 0)
  }
  assert.equal(facts.some(fact => fact.wrapped || fact.prefixRows > 1), true,
    'long prose must wrap at planned atom boundaries')
}
