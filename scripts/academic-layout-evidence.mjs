import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { execFileSync } from 'node:child_process'
import { arch, cpus, platform, release, totalmem } from 'node:os'
import { readFile, mkdir, writeFile } from 'node:fs/promises'
import { resolve } from 'node:path'
import { pathToFileURL } from 'node:url'
import { deflateSync } from 'node:zlib'

const root = new URL('../', import.meta.url)
const hash = bytes => createHash('sha256').update(bytes).digest('hex')
const text = (key, role, value, split = role === 'prose' ? 'text' : 'never') => ({ key, role, split, content: { kind: 'text', text: { runs: [{ text: value }] } } })
const image = (key, assetId) => ({ key, role: 'figure', content: { kind: 'image', assetId, altText: key } })
const relation = (mediaNodeKey, textNodeKey, explicitCaption = false) => ({ mediaNodeKey, textNodeKey, proximity: 'adjacent-blocks', textSide: 'after-media', explicitCaption })
const paragraph = 'A deterministic analysis retains every observation, confidence interval, axis label and legend. We compare measured learning curves across sessions, report uncertainty, and distinguish the statistical method from its interpretation. '
const korean = '측정한 학습 곡선을 비교하고 불확실성을 보고한다. 축과 범례를 보존하며 방법과 결과를 명확히 구분한다. '
const list = (key, count, verbose = false) => ({ key, role: 'list', split: 'list-items', content: { kind: 'list', list: { ordered: false, start: 1, items: Array.from({ length: count }, (_, i) => ({ key: `${key}-item-${i}`, blocks: i === count - 1 ? [] : [text(`${key}-block-${i}`, 'listItem', verbose ? paragraph : `Result ${i + 1}: reproducible estimates retain uncertainty.`)], children: [] })) } } })

/** One shared semantic corpus for the pre-change and post-change engine, without host geometry. */
export function academicCases({ expanded = false } = {}) {
  const slides = []
  const add = (key, nodes, relations = [], title = 'Learning curves / 학습 곡선') => slides.push({ key, kind: 'content', nodes: [text(`${key}-title`, 'title', title), ...nodes], mediaTextRelations: relations })
  slides.push({ key: 'cover', kind: 'title', nodes: [text('cover-title', 'title', 'Learning curves / 학습 곡선'), text('cover-subtitle', 'subtitle', 'Research seminar · methods and reproducible evidence')] })
  add('research-question', [text('question', 'prose', 'How does the analysis window affect estimates of learning across sessions?'), list('question-list', 4)])
  add('method', [text('method-prose', 'prose', paragraph.repeat(5)), list('method-list', 5)])
  add('results-ko', [text('results-ko-prose', 'prose', korean.repeat(10)), list('results-ko-list', 4)], [], '연구 결과: 세션 간 학습 변화와 불확실성을 함께 비교하는 긴 제목의 측정 및 자동 줄바꿈')
  add('mixed-long-title', [text('mixed-prose', 'prose', (paragraph + korean).repeat(5))], [], 'A long research heading explains uncertainty across learning sessions / 한국어와 English 혼합 제목의 안전한 줄바꿈 검증')
  add('formula', [text('formula-prefix', 'prose', 'The logistic model connects trial-level predictors with a bounded success probability.'), { key: 'formula-image', role: 'displayMath', content: { kind: 'svg', assetId: 'formula', fallbackAssetId: 'ratio-4x1', sourceText: 'p = 1 / (1 + exp(-eta))' } }, text('formula-after', 'prose', paragraph)])
  const table = { columns: ['Method', 'Trials', 'Estimate'].map((_, i) => ({ key: `table-col-${i}`, alignment: i === 0 ? 'start' : 'end' })), headerRows: 1, rows: Array.from({ length: 15 }, (_, r) => ({ key: `table-row-${r}`, cells: (r === 0 ? ['Method', 'Trials', 'Estimate'] : [`Window ${r}`, String(r * 11), '0.72 ± 0.04']).map((value, c) => ({ key: `table-cell-${r}-${c}`, content: { runs: [{ text: value }] } })) })) }
  add('table', [{ key: 'results-table', role: 'table', split: 'table-rows', content: { kind: 'table', table } }])
  for (const ratio of ['4x1', '16x9', '1x1', '3x4', '1x4']) {
    for (const context of ['alone', 'caption', 'prose', 'list']) {
      const key = `${ratio}-${context}`
      const figure = image(`${key}-figure`, `ratio-${ratio}`)
      const copy = context === 'caption' ? text(`${key}-caption`, 'caption', 'Measured observations, with axes and legend preserved.') : context === 'prose' ? text(`${key}-prose`, 'prose', paragraph.repeat(7)) : context === 'list' ? list(`${key}-list`, 10, true) : undefined
      add(key, copy ? [figure, copy] : [figure], copy && context !== 'list' ? [relation(figure.key, copy.key, context === 'caption')] : [])
    }
  }
  for (const name of ['fig3', 'fig7']) {
    const key = `research-${name}`
    add(key, [image(`${key}-figure`, name), text(`${key}-caption`, 'caption', `Deliano et al. (2016), ${name === 'fig3' ? 'Figure 3: window and model comparisons.' : 'Figure 7: population analysis.'} CC BY 4.0.`)], [relation(`${key}-figure`, `${key}-caption`, true)])
  }
  for (const count of [2, 4, 6]) {
    const key = `multipanel-${count}`
    const nodes = [], relations = []
    for (let i = 0; i < count; i++) {
      const figure = image(`${key}-figure-${i}`, i % 2 ? 'fig7' : 'fig3')
      const caption = text(`${key}-caption-${i}`, 'caption', `Panel ${i + 1}: Deliano et al., 2016. CC BY 4.0.`)
      nodes.push(figure, caption); relations.push(relation(figure.key, caption.key, true))
    }
    add(key, [{ key: `${key}-gallery`, role: 'gallery', split: 'children', content: { kind: 'children', nodes } }], relations)
  }
  if (expanded) {
    for (const order of ['list-image', 'image-list']) {
      const key = `canonical-${order}`
      const copy = list(`${key}-list`, 2), figure = image(`${key}-figure`, 'ratio-16x9')
      // Both items are real short bullets; the legacy corpus separately exercises empty input.
      copy.content.list.items[1].blocks = [text(`${key}-block-last`, 'listItem', 'B')]
      copy.content.list.items[0].blocks[0].content.text.runs[0].text = 'A'
      add(key, order === 'list-image' ? [copy, figure] : [figure, copy], [], 'Canonical pair')
    }
    for (const [label, count, repetitions] of [['sparse-two', 2, 0], ['normal-eight', 8, 0], ['nearfull-one', 14, 0], ['sparse-two-columns', 20, 0], ['nearfull-two', 28, 0], ['continued-spare', 44, 0]]) {
      const key = `adaptive-${label}`, copy = list(`${key}-list`, count)
      for (let index = 0; index < count; index++) copy.content.list.items[index].blocks = [text(`${key}-text-${index}`, 'listItem', repetitions ? paragraph : `Observation ${index + 1}`)]
      add(key, [copy], [], 'Measured body density')
    }
  }
  for (const slide of slides) {
    let position = 1
    const sourced = nodes => {
      for (const node of nodes) {
        const start = position++
        if (node.content.kind === 'children') sourced(node.content.nodes)
        if (node.content.kind === 'list') for (const item of node.content.list.items) {
          const itemStart = position++
          sourced(item.blocks)
          item.source = { source: `${slide.key}.md`, start: itemStart, end: position++ }
        }
        node.source = { source: `${slide.key}.md`, start, end: position++ }
      }
    }
    sourced(slide.nodes)
    slide.source = { source: `${slide.key}.md`, start: 0, end: position }
  }
  return slides
}

function crc32(bytes) {
  let crc = 0xffffffff
  for (const byte of bytes) {
    crc ^= byte
    for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ (0xedb88320 & -(crc & 1))
  }
  return (crc ^ 0xffffffff) >>> 0
}
function chunk(name, bytes) {
  const type = Buffer.from(name), size = Buffer.alloc(4), crc = Buffer.alloc(4)
  size.writeUInt32BE(bytes.length); crc.writeUInt32BE(crc32(Buffer.concat([type, bytes])))
  return Buffer.concat([size, type, bytes, crc])
}
function ratioPng(width, height) {
  const header = Buffer.alloc(13); header.writeUInt32BE(width); header.writeUInt32BE(height, 4); header[8] = 8; header[9] = 2
  const pixels = Buffer.alloc((width * 3 + 1) * height, 255)
  for (let y = 0; y < height; y++) {
    pixels[y * (width * 3 + 1)] = 0
    for (let x = 0; x < width; x++) {
      const axis = x < 4 || y > height - 5 || Math.abs(y - height * (0.8 - 0.6 * x / width)) < 2
      const offset = y * (width * 3 + 1) + 1 + x * 3
      if (axis) pixels.fill(35, offset, offset + 3)
    }
  }
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk('IHDR', header), chunk('IDAT', deflateSync(pixels)), chunk('IEND', Buffer.alloc(0))])
}
export async function academicAssets() {
  const assets = new Map()
  for (const [name, width, height] of [['4x1', 800, 200], ['16x9', 640, 360], ['1x1', 400, 400], ['3x4', 300, 400], ['1x4', 200, 800]]) assets.set(`ratio-${name}`, { bytes: ratioPng(width, height), mediaType: 'image/png', intrinsicSize: { width, height } })
  for (const [name, width, height] of [['fig3', 4389, 3221], ['fig7', 2700, 4680]]) assets.set(name, { bytes: new Uint8Array(await readFile(new URL(`fixtures/deck-gates/research/deliano-2016-${name}.png`, root))), mediaType: 'image/png', intrinsicSize: { width, height } })
  assets.set('formula', { bytes: new TextEncoder().encode('<svg xmlns="http://www.w3.org/2000/svg" width="360" height="40"><text x="1" y="30" font-size="28">p = 1 / (1 + exp(−η))</text></svg>'), mediaType: 'image/svg+xml', intrinsicSize: { width: 360, height: 40 } })
  return assets
}
const overlap = (a, b) => a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
const area = rect => rect.width * rect.height
function inventory(nodes, result = new Map()) {
  for (const node of nodes) {
    result.set(node.key, node)
    if (node.content.kind === 'children') inventory(node.content.nodes, result)
  }
  return result
}
export async function measureAcademicSnapshot(snapshot, slide, template, assets, presentation, academic) {
  const [title, bodySize, floor, caption, cover] = { conference: [3000, 2400, 2200, 1800, 4000], seminar: [2800, 2000, 1800, 1600, 3600], handout: [2600, 1800, 1600, 1400, 3200] }[presentation]
  const nodes = inventory(slide.nodes), pages = []
  const errors = []
  for (const page of snapshot.pages) {
    const scene = await snapshot.resolvePage(page.id)
    const paintedText = scene.commands.filter(command => command.kind === 'draw-rich-text').map(command => ({ bounds: command.bounds, text: command.frame.paragraphs.map(textParagraph => textParagraph.runs.map(run => run.text).join('')).join('\n'), fontSizes: [...new Set(command.frame.paragraphs.flatMap(textParagraph => textParagraph.runs.filter(run => run.text.trim()).map(run => run.style.fontSize)))] }))
    const source = scene.semantics.filter(item => item.source)
    const fragments = source.map(item => {
      const identity = snapshot.sourceIdentity(item.source.semanticId)
      const node = nodes.get(identity?.key)
      const commands = scene.commands.slice(item.firstCommand, item.firstCommand + item.commandCount)
      const fonts = commands.flatMap(command => command.kind === 'draw-rich-text' ? command.frame.paragraphs.flatMap(textParagraph => textParagraph.runs.filter(run => run.text.trim().length > 0).map(run => run.style.fontSize)) : command.kind === 'draw-text' && scene.strings[command.text]?.trim().length > 0 ? [command.style.fontSize] : [])
      if (academic && fonts.length > 0) {
        const minimum = node?.role === 'caption' ? caption : node?.role === 'title' ? title : floor
        const maximum = node?.role === 'caption' ? caption : node?.role === 'title' ? (slide.kind === 'title' ? cover : title) : bodySize + 400
        for (const size of fonts) if (size < minimum || size > maximum) errors.push(`${node?.key}: painted font ${size} outside purpose range ${minimum}..${maximum}`)
      }
      const pictures = commands.filter(command => command.kind === 'draw-image').map(command => ({ frame: command.transform.bounds, crop: command.crop }))
      if (node?.content.kind === 'image') {
        const intrinsic = assets.get(node.content.assetId)?.intrinsicSize
        for (const picture of pictures) {
          if (picture.crop.some(value => value !== 0)) errors.push(`${node.key}: cropped image`)
          if (intrinsic && Math.abs(picture.frame.width * intrinsic.height - picture.frame.height * intrinsic.width) / Math.max(picture.frame.width * intrinsic.height, 1) > 0.00005) errors.push(`${node.key}: distorted image`)
        }
      }
      return { key: identity?.key, role: node?.role, bounds: item.bounds, readingOrder: item.readingOrder, fontSizes: [...new Set(fonts)], images: pictures }
    })
    for (let i = 0; i < fragments.length; i++) for (let j = i + 1; j < fragments.length; j++) if (overlap(fragments[i].bounds, fragments[j].bounds)) errors.push(`page ${page.index}: overlap ${fragments[i].key}/${fragments[j].key}`)
    const covered = fragments.reduce((sum, fragment) => sum + area(fragment.bounds), 0)
    const contentLayout = template.describe().layouts.find(layout => layout.capability === 'content-envelope')
    const body = contentLayout?.regions.find(region => region.role === 'body')?.frame
    const preferredHeader = contentLayout?.regions.find(region => region.role === 'title')?.frame
    const header = fragments.find(fragment => fragment.role === 'title')
    const firstBody = fragments.find(fragment => fragment.key === 'question' && fragment.role === 'prose')
    const bodyFragments = fragments.filter(fragment => fragment.role !== 'title')
    // This dedicated text probe has a first, full-width prose block followed by a same-width list.
    // Its source shape begins at the actual Stack lane top. Centered media or split lanes do not
    // expose that allocation boundary and must never be presented as an inferred safe envelope.
    const fullWidthTextProbe = slide.key === 'research-question' && page.continuationOrdinal === 1 &&
      body && header && firstBody && bodyFragments.every(fragment =>
        Math.abs(fragment.bounds.x - body.x) <= 2 && Math.abs(fragment.bounds.width - body.width) <= 2 && fragment.images.length === 0)
    const actualBodyTop = fullWidthTextProbe ? firstBody.bounds.y : null
    const actualBodySafeAreaRatio = fullWidthTextProbe ? body.width * (body.y + body.height - actualBodyTop) / (scene.width * scene.height) : null
    const bodyReclaimedEmu = fullWidthTextProbe ? body.y - actualBodyTop : null
    const bodyReclaimOpportunityEmu = fullWidthTextProbe && preferredHeader ? Math.max(0, preferredHeader.height - header.bounds.height) : null
    const actualBodyMetricReason = fullWidthTextProbe ? 'Resolved first full-width prose source shape is the Stack lane top; authored safe bottom and width bound its available body envelope.' : 'Unavailable: this page is not the dedicated full-width text probe; centered media and split lanes do not reveal the body allocation top.'
    if (academic && slide.key === 'research-question') {
      if (!fullWidthTextProbe) errors.push('short-title actual body envelope is not observable as a full-width text Stack')
      if (actualBodySafeAreaRatio !== null && actualBodySafeAreaRatio < 0.72) errors.push('single-line title actual body safe area is below 72%')
      if (bodyReclaimOpportunityEmu > 0 && bodyReclaimedEmu <= 0) errors.push('measured short title did not reclaim available preferred header reservation')
      if (firstBody && header && firstBody.bounds.y < header.bounds.y + header.bounds.height) errors.push('actual body begins before measured title bottom')
    }
    pages.push({ paintedText, continuationTotal: page.continuationTotal, index: page.index, continuationOrdinal: page.continuationOrdinal, pageWidth: scene.width, pageHeight: scene.height, preferredBodyAreaRatio: body ? area(body) / (scene.width * scene.height) : null, actualBodyTop, actualBodySafeAreaRatio, bodyReclaimedEmu, bodyReclaimOpportunityEmu, measuredHeaderHeight: fullWidthTextProbe ? header.bounds.height : null, preferredHeaderHeight: fullWidthTextProbe ? preferredHeader?.height ?? null : null, actualBodyMetricReason, contentBoundsAreaRatio: covered / (scene.width * scene.height), whitespaceRatio: 1 - covered / (scene.width * scene.height), fragments })
  }
  if (slide.key.startsWith('canonical-')) {
    const copy = pages[0]?.fragments.find(fragment => fragment.role === 'list')
    const figure = pages[0]?.fragments.find(fragment => fragment.role === 'figure')
    if (pages.length !== 1 || !copy || !figure || copy.bounds.x + copy.bounds.width > figure.bounds.x) errors.push('canonical pair must place complete text left and image right')
  }
  if (slide.key === 'adaptive-continued-spare') for (const page of pages) {
    if (!page.paintedText.some(shape => shape.text.endsWith(`(${page.continuationOrdinal}/${page.continuationTotal})`))) errors.push(`page ${page.continuationOrdinal}: missing visible accepted title suffix`)
  }
  for (const authoredRelation of slide.mediaTextRelations ?? []) if (authoredRelation.explicitCaption && !pages.some(page => page.fragments.some(fragment => fragment.key === authoredRelation.mediaNodeKey) && page.fragments.some(fragment => fragment.key === authoredRelation.textNodeKey))) errors.push(`detached caption ${authoredRelation.textNodeKey}`)
  for (const node of nodes.values()) if (!['gallery'].includes(node.role) && !pages.some(page => page.fragments.some(fragment => fragment.key === node.key))) errors.push(`missing content ${node.key}`)
  return { pages, errors }
}

async function main() {
  const args = new Map(process.argv.slice(2).filter(value => value.startsWith('--') && value.includes('=')).map(value => value.slice(2).split(/=(.*)/s).slice(0, 2)))
  const enginePath = resolve(args.get('engine') ?? 'packages/wasmppt/generated/node.mjs')
  const output = resolve(args.get('output') ?? 'target/academic-layout/after.json')
  const themes = resolve(args.get('themes') ?? 'fixtures/deck-gates')
  const names = (args.get('names') ?? 'starter').split(',')
  const purposes = ['conference', 'seminar', 'handout']
  const { createNodeEngine } = await import(pathToFileURL(enginePath))
  const inputs = await academicAssets(), slides = academicCases({ expanded: args.has('expanded') })
  const report = { schema: 1, label: args.get('label') ?? 'after', corpusHash: hash(JSON.stringify(slides)), runtime: process.version, engineBundleHash: hash(await readFile(enginePath)), engineWasmHash: hash(await readFile(new URL('../../wasmppt-worker/src/generated/wasmppt_wasm_bg.wasm', pathToFileURL(enginePath)))), sourceRevision: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim(), sourceDirty: execFileSync('git', ['status', '--porcelain'], { cwd: root, encoding: 'utf8' }).trim().length > 0, machine: { platform: platform(), release: release(), arch: arch(), cpu: cpus()[0]?.model, totalMemoryBytes: totalmem() }, memoryScope: 'Sampled process RSS/heap after each snapshot; retained engine admission bytes are separate. These are not OS high-water or transient peak allocation measurements.', assets: [...inputs].map(([id, value]) => ({ id, sha256: hash(value.bytes), intrinsicSize: value.intrinsicSize })), candidateCount: null, candidateCountReason: 'Public SDK does not expose per-case planner work counters. Actual native aggregate work counters are retained separately in planner-metrics.json; unchanged work bounds remain authoritative.', results: [], processPeakRssBytes: 0, processPeakHeapBytes: 0, maxAccountedBytes: 0 }
  const engine = await createNodeEngine(), registered = new Map()
  try {
    for (const [id, input] of inputs) registered.set(id, await engine.registerAsset(input))
    for (const name of names) for (const presentation of purposes) {
      const filename = presentation === 'seminar' || args.has('baseline') ? `${name}.potx` : `${name}-${presentation}.potx`
      let templateBytes
      try { templateBytes = await readFile(resolve(themes, filename)) } catch { templateBytes = await readFile(resolve(themes, `${name}.potx`)) }
      const prepared = await engine.prepareDeckTemplate(templateBytes)
      assert.equal(prepared.ok, true, JSON.stringify(prepared.diagnostics))
      const template = prepared.value, deck = engine.createDeck()
      try {
        for (const slide of slides) {
          const start = performance.now()
          const result = await deck.replace({ template, assets: registered, spec: { key: 'academic-layout-v1', layout: { presentation, imageFit: 'contain' }, slides: [slide] } })
          const elapsedMs = performance.now() - start
          if (!result.ok) { report.results.push({ theme: name, presentation, case: slide.key, elapsedMs, rejected: true, diagnostics: result.diagnostics }); continue }
          const snapshot = result.value
          try {
            const evidence = await measureAcademicSnapshot(snapshot, slide, template, inputs, presentation, args.has('academic') && name !== 'custom')
            report.results.push({ theme: name, templateHash: hash(templateBytes), presentation, case: slide.key, elapsedMs, pageCount: snapshot.pages.length, failedSlides: snapshot.failedSlides, diagnostics: snapshot.diagnostics, ...evidence })
            report.maxAccountedBytes = Math.max(report.maxAccountedBytes, await engine.accountedBytes())
            const memory = process.memoryUsage(); report.processPeakRssBytes = Math.max(report.processPeakRssBytes, memory.rss); report.processPeakHeapBytes = Math.max(report.processPeakHeapBytes, memory.heapUsed)
          } finally { await snapshot.dispose() }
        }
        process.stdout.write(`${name}/${presentation}: ${slides.length} cases\n`)
      } finally { await deck.dispose(); await template.dispose() }
    }
  } finally { for (const asset of registered.values()) await asset.dispose(); engine.dispose() }
  await mkdir(resolve(output, '..'), { recursive: true }); await writeFile(output, `${JSON.stringify(report, null, 2)}\n`)
  const failed = report.results.filter(result => result.rejected || result.failedSlides?.length || result.errors?.length)
  process.stdout.write(`${report.results.length} results, ${failed.length} cases with findings; report ${output}\n`)
  if (process.argv.includes('--strict')) assert.equal(failed.length, 0, failed.map(result => `${result.theme}/${result.presentation}/${result.case}: ${result.errors?.join(';') ?? 'failed'}`).join('\n'))
}
if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) await main()
