import assert from 'node:assert/strict'

/** Whole public SDK snapshots, followed by actual Canvas glyph and opaque image paint. */
export async function checkProseGutterBrowserApi(browserPage) {
  const facts = await browserPage.evaluate(async () => {
    const { createBrowserEngine } = await import('/dist/browser.js')
    const { CanvasDisplayListRenderer, decodeRasterImage } = await import('/dist/canvas.js')
    const engine = await createBrowserEngine({
      worker: new Worker('/dist/browser-worker.js', { type: 'module' }), measureInlineText: true,
    })
    const prepared = await engine.prepareDeckTemplate(await (await fetch('/deck-gate-starter.potx')).arrayBuffer())
    if (!prepared.ok) throw new Error(JSON.stringify(prepared.diagnostics))
    const template = prepared.value
    const customPrepared = await engine.prepareDeckTemplate(await (await fetch('/deck-gate-custom.potx')).arrayBuffer())
    if (!customPrepared.ok) throw new Error(JSON.stringify(customPrepared.diagnostics))
    const customTemplate = customPrepared.value
    const deck = engine.createDeck()
    const encoder = new TextEncoder()
    const text = (key, role, value) => ({ key, role, split: 'text', source: { source: 'prose-gutter.md', start: 0, end: encoder.encode(value).length }, content: { kind: 'text', text: { runs: [{ text: value }] } } })
    const first = 'I finally built something to replace Obsidian. I wanted something fast, without folders or tags. Two days after building it, I no longer use Obsidian. One reason this was possible, I think, is that I hardly used any Obsidian plugins.'
    const repeated = 'A personal wiki where I write whatever comes to mind: diaries, technology/programming, HCI/UX/design, philosophy/ethics, and more. Welcome :)'
    const results = []
    const assets = []
    const paint = async (snapshot, imageBytes) => {
      const pages = []
      for (const page of snapshot.pages) {
        const scene = await snapshot.resolvePage(page.id)
        const canvas = document.createElement('canvas')
        canvas.width = Math.ceil(scene.width / 9525); canvas.height = Math.ceil(scene.height / 9525)
        const context = canvas.getContext('2d')
        const glyphs = []
        const fill = context.fillText.bind(context)
        context.fillText = (value, x, y, ...rest) => {
          if (value.trim()) {
            const metrics = context.measureText(value)
            glyphs.push({ value, font: context.font, x: x - metrics.actualBoundingBoxLeft,
              y: y - metrics.actualBoundingBoxAscent,
              right: x + metrics.actualBoundingBoxRight, bottom: y + metrics.actualBoundingBoxDescent })
          }
          fill(value, x, y, ...rest)
        }
        const layouts = []
        const renderer = new CanvasDisplayListRenderer()
        await renderer.render(scene, context, {
          imageResolver: async () => decodeRasterImage(imageBytes.buffer, {}),
          onTextLayout: (index, layout) => {
            const command = scene.commands[index]
            const semantic = scene.semantics.find(item => index >= item.firstCommand && index < item.firstCommand + item.commandCount)
            if (!semantic?.source) return
            layouts.push({ key: snapshot.sourceIdentity(semantic.source.semanticId)?.key,
              frame: command.bounds, rows: [...new Set(layout.runs.filter(run => run.text.trim()).map(run => run.baseline))],
              runs: layout.runs.filter(run => run.text.trim()).map(run => ({ text: run.text, x: run.x,
                right: run.x + run.width, baseline: run.baseline, font: run.font.css, size: run.fontSize })) })
          },
        })
        const collisions = []
        for (let i = 0; i < glyphs.length; i++) for (let j = i + 1; j < glyphs.length; j++) {
          const a = glyphs[i], b = glyphs[j]
          if (Math.min(a.right, b.right) - Math.max(a.x, b.x) > 0.5 &&
              Math.min(a.bottom, b.bottom) - Math.max(a.y, b.y) > 0.5) collisions.push([a, b])
        }
        const overflow = layouts.filter(item => item.runs.some(run =>
          run.baseline + run.size * 0.2 > (item.frame.y + item.frame.height) / 9525 + 0.1))
        pages.push({ layouts, glyphs, collisions, overflow,
          images: scene.commands.filter(command => command.kind === 'draw-image').map(command => command.transform.bounds) })
        renderer.clear()
      }
      return pages
    }
    try {
      for (const presentation of ['conference', 'seminar', 'handout']) {
        for (const hardBreaks of [false, true]) {
          const paragraphs = hardBreaks ? Array(9).fill('One measured line.\nTwo measured lines.\nThree measured lines.\nFour measured lines.') : [first, ...Array(8).fill(repeated)]
          const spec = { key: 'serious-deck', layout: { presentation }, slides: [{ key: 'serious-slide', kind: 'content',
            nodes: [text('heading', 'title', 'Serious'), ...paragraphs.map((value, index) => text(`p${index}`, 'prose', value))] }] }
          const accepted = await deck.replace({ template, spec })
          if (!accepted.ok || accepted.value.failedSlides.length) throw new Error(JSON.stringify(accepted.diagnostics))
          const snapshot = accepted.value
          try {
            const pages = await paint(snapshot)
            const body = pages.flatMap(page => page.layouts).filter(item => item.key?.startsWith('p'))
            const source = paragraphs.map((_, index) => body.filter(item => item.key === `p${index}`)
              .flatMap(item => item.runs.map(run => run.text)).join('').replaceAll(/\s/gu, ''))
            results.push({ kind: 'prose', presentation, hardBreaks, pages, source, expected: paragraphs.map(value => value.replaceAll(/\s/gu, '')) })
          } finally { await snapshot.dispose() }
        }
        const sizes = []
        for (const count of [2, 8]) {
          const fitting = await deck.replace({ template, spec: { key: 'fitting-deck', layout: { presentation }, slides: [{
            key: 'fitting-slide', kind: 'content', nodes: Array.from({ length: count }, (_, index) => text(`copy${index}`, 'prose', 'A short paragraph.')),
          }] } })
          if (!fitting.ok) throw new Error(JSON.stringify(fitting.diagnostics))
          try {
            const pages = await paint(fitting.value)
            sizes.push([...new Set(pages.flatMap(page => page.layouts).flatMap(item => item.runs.map(run => run.size * 72 / 96)))])
          } finally { await fitting.value.dispose() }
        }
        results.push({ kind: 'preferred', presentation, sizes })
      }
      for (const custom of [false, true]) {
        for (const [width, height] of [[400, 100], [160, 90], [100, 100], [75, 100], [100, 400]]) {
          const canvas = document.createElement('canvas'); canvas.width = width; canvas.height = height
          canvas.getContext('2d').fillRect(0, 0, width, height)
          const blob = await new Promise(resolve => canvas.toBlob(resolve))
          const bytes = new Uint8Array(await blob.arrayBuffer())
          const asset = await engine.registerAsset({ bytes, mediaType: 'image/png' }); assets.push(asset)
          {
            for (const imageFirst of [true, false]) {
              const image = { key: 'figure', role: 'figure', content: { kind: 'image', assetId: asset.id } }
              const list = { key: 'bullets', role: 'list', split: 'list-items', content: { kind: 'list', list: {
                ordered: false, start: 1, items: ['Introduce the problem.', 'Explain your approach.'].map((value, index) => ({
                  key: `item${index}`, blocks: [text(`item-text${index}`, 'prose', value)], children: [],
                })),
              } } }
              const accepted = await deck.replace({ template: custom ? customTemplate : template, assets: new Map([[asset.id, asset]]), spec: {
                key: 'gutter-deck', slides: [{ key: 'gutter-slide', kind: 'content', nodes: [text('heading', 'title', 'Key idea'), ...(imageFirst ? [image, list] : [list, image])] }],
              } })
              if (!accepted.ok || accepted.value.failedSlides.length) throw new Error(JSON.stringify(accepted.diagnostics))
              try {
                const pages = await paint(accepted.value, bytes)
                const imageFrame = pages[0].images[0]
                const body = pages[0].layouts.find(item => item.key === 'bullets')
                const minX = Math.min(...body.runs.map(run => run.x))
                const maxX = Math.max(...body.runs.map(run => run.right))
                const gap = imageFirst ? minX - (imageFrame.x + imageFrame.width) / 9525 : imageFrame.x / 9525 - maxX
                results.push({ kind: 'gutter', width, height, imageFirst, custom, gap, pages })
              } finally { await accepted.value.dispose() }
            }
          }
        }
      }
      return results
    } finally {
      for (const asset of assets) await asset.dispose()
      await deck.dispose(); await customTemplate.dispose(); await template.dispose(); engine.dispose()
    }
  })
  for (const fact of facts) {
    if (fact.kind === 'preferred') {
      const maximum = { conference: 28, seminar: 24, handout: 22 }[fact.presentation]
      assert.deepEqual(fact.sizes, [[maximum], [maximum]], JSON.stringify(fact))
      continue
    }
    for (const page of fact.pages) {
      assert.deepEqual(page.collisions, [], `actual painted glyph collisions: ${JSON.stringify(fact)}`)
      assert.deepEqual(page.overflow, [], `painted rows exceed planned fragments: ${JSON.stringify(fact)}`)
    }
    if (fact.kind === 'prose') {
      assert.deepEqual(fact.source, fact.expected, 'all nine paragraphs must paint completely in source order')
      if (fact.hardBreaks) assert.ok(fact.pages.some(page => new Set(page.layouts.filter(item => item.key?.startsWith('p')).map(item => item.frame.x)).size === 2), 'hard-break prose must exercise actual physical two-column refitting')
      for (const page of fact.pages) {
        const sizes = new Set(page.layouts.filter(item => item.key?.startsWith('p')).flatMap(item => item.runs.map(run => run.size)))
        assert.equal(sizes.size, 1, 'comparable physical body columns must share their font')
      }
    } else {
      assert.equal(fact.pages.length, 1)
      assert.ok(fact.gap >= (fact.custom ? 12 : 33.6) - 0.1, `visible image-to-bullet/text gutter: ${JSON.stringify(fact)}`)
      if (fact.custom) {
        const body = fact.pages[0].layouts.find(item => item.key === 'bullets')
        assert.ok(body.runs.every(run => run.size * 72 / 96 === 18), 'custom body retains explicit 18pt')
        if (fact.imageFirst && fact.width === 160) assert.ok(fact.gap < 33.6, 'custom fixture retains its smaller configured gap')
      }
    }
  }
  console.log('Public SDK prose and image/list actual Canvas regression passed', facts.map(({ kind, presentation, gap }) => ({ kind, presentation, gap })))
}
