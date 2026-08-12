import { CanvasDisplayListRenderer, decodeDisplayList, decodeSvgImage } from './lib/canvas.js'
import { WasmpptWorkerClient } from './lib/worker-client.js'

const templateDefinitions = [
  {
    id: 'atlas',
    name: 'Atlas Report',
    source: './fixtures/report.potx',
    outputName: 'wasmppt-atlas-report.pptx',
  },
  {
    id: 'garden',
    name: 'Signal Garden',
    source: './fixtures/garden.potx',
    outputName: 'wasmppt-signal-garden.pptx',
  },
]
const preferredBindings = ['title', 'subtitle', 'metrics.label', 'metrics.value']
const bindingLabels = new Map([
  ['title', ['Headline', 'The main story both templates will typeset']],
  ['subtitle', ['Supporting line', 'A little context beneath the headline']],
  ['metrics.label', ['Metric label', 'The label used on each second slide']],
  ['metrics.value', ['Metric value', 'The number or phrase to emphasize']],
])

const elements = {
  bindings: document.getElementById('bindings'),
  status: document.getElementById('status'),
  diagnostics: document.getElementById('diagnostics'),
}
const worker = new Worker('./worker.js', { type: 'module' })
const client = new WasmpptWorkerClient(worker)
const decks = templateDefinitions.map((definition) => createDeck(definition))

let pendingDelta = emptyDelta()
let updateFrame
let updateRunning = false
let defaultImageData
const liveSettleWaiters = new Set()

worker.addEventListener('message', (event) => {
  if (event.data?.type === 'host-ready') void initializeGarden()
  else if (event.data?.type === 'host-init-error') fail(new Error(event.data.message))
})
worker.addEventListener('error', (event) => fail(new Error(event.message)))
elements.bindings.addEventListener('input', queueBindingEdit)
for (const deck of decks) {
  deck.download.addEventListener('click', (event) => void downloadCurrentRevision(deck, event))
}

function createDeck(definition) {
  const element = document.querySelector(`[data-deck="${definition.id}"]`)
  if (!(element instanceof HTMLElement)) throw new Error(`Missing ${definition.id} deck host`)
  return {
    ...definition,
    element,
    preview: element.querySelector('[data-preview]'),
    download: element.querySelector('[data-download]'),
    compileTime: element.querySelector('[data-compile-time]'),
    refreshTime: element.querySelector('[data-refresh-time]'),
    outputSize: element.querySelector('[data-output-size]'),
    status: element.querySelector('[data-deck-status]'),
    renderer: new CanvasDisplayListRenderer(),
    prepared: undefined,
    session: undefined,
    preparationDiagnostics: [],
    slideDiagnostics: new Map(),
    slideHosts: new Map(),
    visibleSlides: new Set(),
    dirtySlides: new Set(),
    renderAbort: undefined,
    renderEpoch: 0,
    outputUrl: undefined,
    outputBlob: undefined,
    outputRevision: -1,
    exportTimer: undefined,
    exportAbort: undefined,
    exportPromise: undefined,
    exportPromiseRevision: -1,
    abandonedWork: 0,
  }
}

async function initializeGarden() {
  setStatus('Preparing two PowerPoint templates in parallel…', true)
  try {
    await Promise.all(decks.map(async (deck) => {
      const started = performance.now()
      const bytes = await fetch(deck.source).then(required).then((response) => response.arrayBuffer())
      deck.prepared = await client.prepare(bytes, { macroPolicy: 'strip' })
      deck.preparationDiagnostics = deck.prepared.diagnostics
      deck.compileTime.textContent = formatMs(performance.now() - started)
      deck.status.textContent = `${deck.prepared.bindings.length} bindings discovered`
    }))

    const sharedBindings = commonTextBindings()
    renderBindings(sharedBindings)
    await Promise.all(decks.map(async (deck) => {
      const data = await generationData(deck.prepared.bindings)
      deck.session = await client.createLiveSession(deck.prepared.handle, data)
      setupPreview(deck, deck.session.slideCount)
      enableDownloadAction(deck)
    }))
    await Promise.all(decks.map((deck) => renderDirtySlides(deck)))
    await Promise.all(decks.map((deck) => exportRevision(deck, deck.session.revision)))
    showDiagnostics()
    setStatus('Both decks are live · edit any field to update them together', false)
  } catch (error) {
    fail(error)
  }
}

function commonTextBindings() {
  const bindingMaps = decks.map((deck) => new Map(
    deck.prepared.bindings.map((binding) => [binding.id, binding]),
  ))
  const bindings = preferredBindings.flatMap((id) => {
    const binding = bindingMaps[0].get(id)
    return binding?.kind === 'text' && bindingMaps.every((map) => map.get(id)?.kind === 'text')
      ? [binding]
      : []
  })
  if (bindings.length === 0) throw new Error('The bundled templates have no shared text bindings')
  return bindings
}

function renderBindings(bindings) {
  elements.bindings.replaceChildren()
  for (const binding of bindings) {
    const [name, hint] = bindingLabels.get(binding.id) ?? [binding.id, 'Shared by both templates']
    const wrapper = document.createElement('div')
    wrapper.className = 'field'
    const label = document.createElement('label')
    const input = document.createElement('input')
    const description = document.createElement('small')
    input.id = `binding-${binding.id.replaceAll(/[^a-zA-Z0-9_-]/g, '-')}`
    input.type = 'text'
    input.value = defaultText(binding.id)
    input.dataset.binding = binding.id
    label.htmlFor = input.id
    label.textContent = name
    description.textContent = hint
    wrapper.append(label, input, description)
    elements.bindings.append(wrapper)
  }
}

function queueBindingEdit(event) {
  const input = event.target
  if (!(input instanceof HTMLInputElement) || input.dataset.binding === undefined) return
  pendingDelta.text[input.dataset.binding] = input.value
  if (updateFrame !== undefined) return
  updateFrame = requestAnimationFrame(() => {
    updateFrame = undefined
    void flushLiveUpdate()
  })
}

async function flushLiveUpdate() {
  if (updateRunning || deltaEmpty(pendingDelta) || decks.some((deck) => deck.session === undefined)) return
  const delta = pendingDelta
  pendingDelta = emptyDelta()
  updateRunning = true
  const sessions = decks.map((deck) => deck.session)
  for (const deck of decks) {
    deck.renderAbort?.abort()
    if (deck.exportPromise !== undefined) deck.abandonedWork += 1
    deck.exportAbort?.abort()
  }
  const nextRevision = sessions[0].revision + 1
  setStatus(`Applying shared revision ${nextRevision} to both templates…`, true)
  const started = performance.now()
  try {
    const updates = await Promise.all(decks.map((deck, index) =>
      client.applyLiveDelta(sessions[index].handle, sessions[index].revision, delta),
    ))
    if (decks.some((deck, index) => deck.session?.handle !== sessions[index].handle)) return

    for (const [index, deck] of decks.entries()) {
      const update = updates[index]
      deck.session = {
        handle: update.handle,
        revision: update.revision,
        slideCount: update.slideCount,
      }
      if (update.fullFallback) setupPreview(deck, update.slideCount)
      else for (const slideIndex of update.invalidatedSlides) deck.dirtySlides.add(slideIndex)
      deck.refreshTime.textContent = formatMs(performance.now() - started)
      deck.status.textContent = `${update.invalidatedSlides.length} slide${update.invalidatedSlides.length === 1 ? '' : 's'} invalidated · ${update.overlay.reusedMaterializedParts} overlay parts reused`
    }

    await Promise.all(decks.map((deck) => renderDirtySlides(deck)))
    for (const deck of decks) scheduleExport(deck, deck.session.revision)
    const elapsed = performance.now() - started
    setStatus(`Both previews reached revision ${nextRevision} in ${formatMs(elapsed)}`, false)
  } catch (error) {
    fail(error)
  } finally {
    updateRunning = false
    if (!deltaEmpty(pendingDelta)) {
      if (liveSettleWaiters.size > 0) void flushLiveUpdate()
      else {
        updateFrame = requestAnimationFrame(() => {
          updateFrame = undefined
          void flushLiveUpdate()
        })
      }
    } else {
      resolveLiveSettleWaiters()
    }
  }
}

async function generationData(bindings) {
  const text = {}
  const images = {}
  for (const binding of bindings) {
    if (binding.kind === 'text') {
      const input = document.querySelector(`[data-binding="${CSS.escape(binding.id)}"]`)
      text[binding.id] = input?.value ?? defaultText(binding.id)
    } else if (binding.kind === 'image') {
      images[binding.id] = await defaultImage()
    }
  }
  return { text, images }
}

function setupPreview(deck, slideCount) {
  deck.renderAbort?.abort()
  deck.preview.replaceChildren()
  deck.visibleSlides.clear()
  deck.dirtySlides.clear()
  deck.slideHosts.clear()
  deck.slideDiagnostics.clear()
  for (let index = 0; index < slideCount; index += 1) {
    const figure = document.createElement('figure')
    figure.dataset.slideIndex = String(index)
    figure.style.aspectRatio = '16 / 9'
    const caption = document.createElement('figcaption')
    caption.textContent = `${deck.name} · Slide ${index + 1}`
    figure.append(caption)
    deck.preview.append(figure)
    deck.slideHosts.set(index, figure)
    deck.visibleSlides.add(index)
    deck.dirtySlides.add(index)
  }
}

async function renderDirtySlides(deck) {
  const session = deck.session
  if (session === undefined) return
  const indices = [...deck.visibleSlides].filter((index) =>
    index >= 0 && index < session.slideCount &&
    (deck.dirtySlides.has(index) || deck.slideHosts.get(index)?.querySelector('canvas') === null),
  )
  if (indices.length === 0) return
  const epoch = ++deck.renderEpoch
  deck.renderAbort?.abort()
  deck.renderAbort = new AbortController()
  const { signal } = deck.renderAbort
  try {
    await Promise.all(indices.map((index) => renderLiveSlide(deck, session, index, epoch, signal)))
  } catch (error) {
    if (signal.aborted || epoch !== deck.renderEpoch || error?.name === 'WasmpptRevisionError') return
    throw error
  }
  if (epoch === deck.renderEpoch) {
    deck.element.dataset.renderRevision = String(session.revision)
    showDiagnostics()
  }
}

async function renderLiveSlide(deck, session, index, epoch, signal) {
  const started = performance.now()
  const resolved = await client.resolveLiveSlide(session.handle, session.revision, index, { signal })
  if (signal.aborted || epoch !== deck.renderEpoch || deck.session?.revision !== session.revision) return
  const scene = decodeDisplayList(resolved.displayList)
  deck.slideDiagnostics.set(index, scene.diagnostics)
  const figure = deck.slideHosts.get(index)
  if (figure === undefined) return
  const deviceScale = Math.min(devicePixelRatio || 1, 2)
  const width = Math.min(scene.width / 9_525, 960)
  const height = width * scene.height / scene.width
  const canvas = document.createElement('canvas')
  canvas.width = Math.max(1, Math.round(width * deviceScale))
  canvas.height = Math.max(1, Math.round(height * deviceScale))
  canvas.style.aspectRatio = `${scene.width} / ${scene.height}`
  canvas.setAttribute('aria-label', `${deck.name} slide ${index + 1}`)
  figure.style.aspectRatio = `${scene.width} / ${scene.height}`
  figure.querySelector('canvas')?.remove()
  figure.prepend(canvas)
  const context = canvas.getContext('2d', { alpha: false })
  if (context === null) throw new Error('Canvas 2D is unavailable')
  await deck.renderer.render(scene, context, {
    signal,
    resolutionMs: performance.now() - started,
    imageCacheKey: async (image, imageSignal) => {
      if (image.partName === undefined) throw new Error('Image resource has no package part')
      const fingerprint = await client.liveSessionResourceFingerprint(
        session.handle,
        session.revision,
        image.partName,
        { signal: imageSignal },
      )
      return /\.(?:emf|wmf)$/i.test(image.partName)
        ? `${fingerprint}:metafile-svg-v1`
        : fingerprint
    },
    imageResolver: async (image, imageSignal) => {
      if (image.partName === undefined) throw new Error('Image resource has no package part')
      const metafile = /\.(?:emf|wmf)$/i.test(image.partName)
      const resource = metafile
        ? await client.liveSessionMetafileSvg(session.handle, session.revision, image.partName, { signal: imageSignal })
        : await client.liveSessionResource(session.handle, session.revision, image.partName, { signal: imageSignal })
      try {
        if (metafile) return await decodeSvgImage(resource.bytes, imageSignal)
        const bitmap = await createImageBitmap(
          new Blob([resource.bytes], { type: mediaTypeOf(image.partName) }),
        )
        return {
          source: bitmap,
          residentBytes: bitmap.width * bitmap.height * 4,
          close: () => bitmap.close(),
        }
      } catch (error) {
        console.warn(`Cannot decode ${image.partName}; a placeholder will be shown`, error)
        return undefined
      }
    },
  })
  if (!signal.aborted && epoch === deck.renderEpoch && deck.session?.revision === session.revision) {
    deck.dirtySlides.delete(index)
  }
}

function showDiagnostics() {
  const diagnostics = decks.flatMap((deck) => [
    ...deck.preparationDiagnostics.map((item) => ({ deck: deck.name, item })),
    ...[...deck.slideDiagnostics.values()].flat().map((item) => ({ deck: deck.name, item })),
  ])
  const unique = [...new Map(diagnostics.map(({ deck, item }) => [
    `${deck}\0${item.code}\0${item.partName ?? ''}\0${item.shapeId ?? ''}\0${item.message}`,
    { deck, item },
  ])).values()]
  elements.diagnostics.textContent = unique.length === 0
    ? 'Two templates · shared bindings · no diagnostics'
    : unique.map(({ deck, item }) => `${deck} · ${item.code}: ${item.message}`).join('\n')
}

function enableDownloadAction(deck) {
  deck.download.classList.remove('is-disabled')
  deck.download.setAttribute('aria-disabled', 'false')
}

function enableDownload(deck, blob, revision) {
  if (deck.outputUrl !== undefined) URL.revokeObjectURL(deck.outputUrl)
  deck.outputBlob = blob
  deck.outputRevision = revision
  deck.outputUrl = URL.createObjectURL(blob)
  deck.download.href = deck.outputUrl
  deck.download.download = deck.outputName
  deck.download.dataset.revision = String(revision)
  enableDownloadAction(deck)
}

function disableDownload(deck) {
  if (deck.outputUrl !== undefined) URL.revokeObjectURL(deck.outputUrl)
  deck.outputUrl = undefined
  deck.outputBlob = undefined
  deck.outputRevision = -1
  deck.download.removeAttribute('href')
  delete deck.download.dataset.revision
  deck.download.classList.add('is-disabled')
  deck.download.setAttribute('aria-disabled', 'true')
}

function scheduleExport(deck, revision) {
  clearTimeout(deck.exportTimer)
  deck.exportTimer = setTimeout(() => void exportRevision(deck, revision), 200)
}

async function exportRevision(deck, revision) {
  const session = deck.session
  if (session === undefined || session.revision !== revision) return undefined
  if (deck.outputRevision === revision && deck.outputBlob !== undefined) return deck.outputBlob
  if (deck.exportPromiseRevision === revision && deck.exportPromise !== undefined) {
    return deck.exportPromise
  }
  deck.exportAbort?.abort()
  deck.exportAbort = new AbortController()
  const { signal } = deck.exportAbort
  deck.exportPromiseRevision = revision
  deck.exportPromise = (async () => {
    const started = performance.now()
    const chunks = []
    let length = 0
    for await (const chunk of client.generateLiveStream(session.handle, revision, { signal })) {
      chunks.push(chunk)
      length += chunk.byteLength
    }
    if (signal.aborted || deck.session?.revision !== revision) return undefined
    const blob = new Blob(chunks, {
      type: 'application/vnd.openxmlformats-officedocument.presentationml.presentation',
    })
    enableDownload(deck, blob, revision)
    deck.outputSize.textContent = formatBytes(length)
    deck.status.textContent = `Revision ${revision} PPTX ready in ${formatMs(performance.now() - started)} · ${deck.abandonedWork} stale exports abandoned`
    return blob
  })().catch((error) => {
    if (error?.name !== 'AbortError' && deck.session?.handle === session.handle) fail(error)
    return undefined
  }).finally(() => {
    if (deck.exportPromiseRevision === revision) {
      deck.exportPromise = undefined
      deck.exportPromiseRevision = -1
    }
  })
  return deck.exportPromise
}

async function downloadCurrentRevision(deck, event) {
  let session = deck.session
  if (session === undefined || deck.download.getAttribute('aria-disabled') === 'true') {
    event.preventDefault()
    return
  }
  if (updateRunning || !deltaEmpty(pendingDelta)) {
    event.preventDefault()
    await settleLiveEdits()
    session = deck.session
    if (session === undefined) return
  }
  if (deck.outputRevision === session.revision && deck.outputUrl !== undefined) return
  event.preventDefault()
  const blob = await exportRevision(deck, session.revision)
  if (blob === undefined || deck.session?.revision !== session.revision) return
  const link = document.createElement('a')
  link.href = URL.createObjectURL(blob)
  link.download = deck.outputName
  link.click()
  setTimeout(() => URL.revokeObjectURL(link.href), 0)
}

function settleLiveEdits() {
  if (!updateRunning && deltaEmpty(pendingDelta)) return Promise.resolve()
  if (updateFrame !== undefined) {
    cancelAnimationFrame(updateFrame)
    updateFrame = undefined
  }
  const settled = new Promise((resolve) => liveSettleWaiters.add(resolve))
  if (!updateRunning) void flushLiveUpdate()
  return settled
}

function resolveLiveSettleWaiters() {
  for (const resolve of liveSettleWaiters) resolve()
  liveSettleWaiters.clear()
}

function emptyDelta() {
  return { text: {} }
}

function deltaEmpty(delta) {
  return Object.keys(delta.text).length === 0
}

function setStatus(message, busy) {
  elements.status.textContent = message
  document.body.classList.toggle('is-busy', busy)
}

function fail(error) {
  const message = error instanceof Error ? `${error.name}: ${error.message}` : String(error)
  setStatus('Could not keep both previews in sync', false)
  elements.diagnostics.textContent = message
  for (const deck of decks) {
    if (deck.session === undefined) disableDownload(deck)
  }
  console.error(error)
}

function defaultText(id) {
  if (id === 'title') return 'One story, two visual worlds'
  if (id === 'subtitle') return 'Live PowerPoint templates, rendered side by side'
  if (id === 'metrics.label') return 'Template refresh latency'
  if (id === 'metrics.value') return 'Under 16 ms'
  return id.split('.').at(-1)?.replaceAll(/[-_]/g, ' ') ?? id
}

async function defaultImage() {
  if (defaultImageData !== undefined) return defaultImageData
  const canvas = document.createElement('canvas')
  canvas.width = 384
  canvas.height = 384
  const context = canvas.getContext('2d')
  if (context === null) throw new Error('Canvas 2D is unavailable')
  const background = context.createLinearGradient(0, 0, 384, 384)
  background.addColorStop(0, '#71ffb1')
  background.addColorStop(1, '#10233d')
  context.fillStyle = background
  context.fillRect(0, 0, 384, 384)
  context.fillStyle = '#ff6b4a'
  context.beginPath()
  context.arc(280, 105, 72, 0, Math.PI * 2)
  context.fill()
  context.fillStyle = 'rgba(255, 255, 255, .88)'
  context.fillRect(54, 205, 216, 34)
  context.fillRect(54, 259, 148, 20)
  const blob = await new Promise((resolve, reject) => {
    canvas.toBlob(
      (value) => value === null ? reject(new Error('Cannot encode bundled artwork')) : resolve(value),
      'image/png',
    )
  })
  defaultImageData = {
    bytes: new Uint8Array(await blob.arrayBuffer()),
    extension: 'png',
    contentType: 'image/png',
  }
  return defaultImageData
}

function mediaTypeOf(name) {
  const extension = name.split('.').at(-1)?.toLowerCase()
  if (extension === 'jpg' || extension === 'jpeg') return 'image/jpeg'
  if (extension === 'gif') return 'image/gif'
  if (extension === 'webp') return 'image/webp'
  if (extension === 'svg') return 'image/svg+xml'
  if (extension === 'bmp') return 'image/bmp'
  if (extension === 'tif' || extension === 'tiff') return 'image/tiff'
  return 'image/png'
}

function required(response) {
  if (!response.ok) throw new Error(`Cannot load bundled template: HTTP ${response.status}`)
  return response
}

function formatMs(value) { return `${value.toFixed(1)} ms` }
function formatBytes(value) {
  return value < 1024 ? `${value} B` : `${(value / 1024).toFixed(1)} KiB`
}
