import { CanvasDisplayListRenderer, decodeDisplayList, decodeSvgImage } from './lib/canvas.js'
import { WasmpptWorkerClient } from './lib/worker-client.js'

const elementIds = [
  'template', 'drop-zone', 'use-sample', 'file-name', 'bindings', 'preview', 'download',
  'status', 'diagnostics', 'compile-time', 'generate-time', 'output-size',
]
const elements = Object.fromEntries(elementIds.map((id) => [id, document.getElementById(id)]))
const worker = new Worker('./worker.js', { type: 'module' })
const client = new WasmpptWorkerClient(worker)
const renderer = new CanvasDisplayListRenderer()

let prepared
let liveSession
let outputUrl
let outputBlob
let outputRevision = -1
let templateEpoch = 0
let updateFrame
let updateRunning = false
let pendingDelta = emptyDelta()
let renderEpoch = 0
let renderAbort
let exportTimer
let exportAbort
let exportPromise
let exportPromiseRevision = -1
let visibilityObserver
const visibleSlides = new Set()
const dirtySlides = new Set()
const slideHosts = new Map()
const slideDiagnostics = new Map()
const bindingEditVersions = new Map()
let outputName = 'wasmppt-report.pptx'
let defaultImageData
let preparationDiagnostics = []
const liveSettleWaiters = new Set()
let activeRenderBatches = 0
let abandonedWork = 0

worker.addEventListener('message', (event) => {
  if (event.data?.type === 'host-ready') void useBundledTemplate()
  else if (event.data?.type === 'host-init-error') fail(new Error(event.data.message))
})
worker.addEventListener('error', (event) => fail(new Error(event.message)))

elements.template.addEventListener('change', () => {
  const file = elements.template.files[0]
  if (file !== undefined) void useFile(file)
})
elements['use-sample'].addEventListener('click', () => void useBundledTemplate())
elements.bindings.addEventListener('input', (event) => void queueBindingEdit(event))
elements.bindings.addEventListener('change', (event) => void queueBindingEdit(event))
elements.download.addEventListener('click', (event) => void downloadCurrentRevision(event))

for (const type of ['dragenter', 'dragover']) {
  elements['drop-zone'].addEventListener(type, (event) => {
    event.preventDefault()
    if (hasFiles(event.dataTransfer)) elements['drop-zone'].classList.add('is-dragging')
  })
}
for (const type of ['dragleave', 'drop']) {
  elements['drop-zone'].addEventListener(type, (event) => {
    event.preventDefault()
    elements['drop-zone'].classList.remove('is-dragging')
  })
}
elements['drop-zone'].addEventListener('drop', (event) => {
  const file = [...(event.dataTransfer?.files ?? [])].find(isPowerPointFile)
  if (file === undefined) {
    fail(new TypeError('Drop a .potx, .potm, or .pptx file'))
    return
  }
  void useFile(file)
})

async function useBundledTemplate() {
  const response = await fetch('./fixtures/report.potx').then(required)
  elements['file-name'].textContent = 'Bundled report template'
  outputName = 'wasmppt-report.pptx'
  await prepareTemplate(await response.arrayBuffer())
}

async function useFile(file) {
  if (!isPowerPointFile(file)) {
    fail(new TypeError('Choose a .potx, .potm, or .pptx file'))
    return
  }
  elements['file-name'].textContent = `${file.name} · ${formatBytes(file.size)}`
  outputName = `${file.name.replace(/\.(potx|potm|pptx)$/i, '') || 'wasmppt'}.pptx`
  await prepareTemplate(await file.arrayBuffer())
}

async function prepareTemplate(bytes) {
  const epoch = ++templateEpoch
  cancelLiveWork()
  disableDownload()
  setStatus('Reading template…', true)
  try {
    const previous = prepared
    prepared = undefined
    await releaseLiveSession()
    if (previous !== undefined) await client.release(previous.handle)
    const started = performance.now()
    const next = await client.prepare(bytes, { macroPolicy: 'strip' })
    if (epoch !== templateEpoch) {
      await client.release(next.handle)
      return
    }
    prepared = next
    abandonedWork = 0
    elements['compile-time'].textContent = formatMs(performance.now() - started)
    renderBindings(next.bindings)
    preparationDiagnostics = next.diagnostics
    showDiagnostics([], next.bindings.length)
    const data = await generationData(next.bindings)
    const session = await client.createLiveSession(next.handle, data)
    if (epoch !== templateEpoch) {
      await client.releaseLiveSession(session.handle)
      return
    }
    liveSession = session
    setupPreview(session.slideCount)
    enableDownloadAction()
    await renderDirtySlides()
    await exportRevision(session.revision)
  } catch (error) {
    if (epoch === templateEpoch) fail(error)
  }
}

async function queueBindingEdit(event) {
  const input = event.target
  if (!(input instanceof HTMLInputElement) || input.dataset.binding === undefined) return
  const binding = prepared?.bindings.find((candidate) => candidate.id === input.dataset.binding)
  if (binding === undefined) return
  const editVersion = (bindingEditVersions.get(binding.id) ?? 0) + 1
  bindingEditVersions.set(binding.id, editVersion)
  if (binding.kind === 'text') {
    if (event.type !== 'input') return
    pendingDelta.text[binding.id] = input.value
  } else if (event.type === 'change') {
    const file = input.files?.[0]
    const image = file === undefined ? await defaultImage() : {
      bytes: new Uint8Array(await file.arrayBuffer()),
      extension: extensionOf(file.name),
      contentType: file.type || contentTypeOf(file.name),
    }
    if (bindingEditVersions.get(binding.id) !== editVersion) return
    pendingDelta.images[binding.id] = image
  } else {
    return
  }
  scheduleLiveUpdate()
}

function scheduleLiveUpdate() {
  if (updateFrame !== undefined) return
  updateFrame = requestAnimationFrame(() => {
    updateFrame = undefined
    void flushLiveUpdate()
  })
}

async function flushLiveUpdate() {
  if (updateRunning || liveSession === undefined || deltaEmpty(pendingDelta)) return
  const session = liveSession
  const delta = pendingDelta
  pendingDelta = emptyDelta()
  updateRunning = true
  if (activeRenderBatches > 0) abandonedWork += activeRenderBatches
  renderAbort?.abort()
  setStatus(`Applying revision ${session.revision + 1}…`, true)
  const started = performance.now()
  try {
    const update = await client.applyLiveDelta(session.handle, session.revision, delta)
    if (liveSession?.handle !== session.handle) return
    liveSession = { handle: update.handle, revision: update.revision, slideCount: update.slideCount }
    if (update.fullFallback) setupPreview(update.slideCount)
    else for (const index of update.invalidatedSlides) dirtySlides.add(index)
    const interactiveMs = performance.now() - started
    elements['generate-time'].textContent = formatMs(interactiveMs)
    await renderDirtySlides()
    scheduleExport(update.revision)
    const telemetry = await client.liveSessionCacheTelemetry(update.handle)
    setStatus(
      `Preview revision ${update.revision} · ${update.invalidatedSlides.length} slide${update.invalidatedSlides.length === 1 ? '' : 's'} updated in ${formatMs(interactiveMs)} (${update.invalidationReason}) · ${update.overlay.reusedMaterializedParts} overlay parts reused · ${telemetry.hits}/${telemetry.hits + telemetry.misses || 0} scene hits · ${abandonedWork} stale jobs abandoned`,
      false,
    )
  } catch (error) {
    if (error?.name !== 'AbortError' && liveSession?.handle === session.handle) fail(error)
  } finally {
    updateRunning = false
    if (!deltaEmpty(pendingDelta)) {
      if (liveSettleWaiters.size > 0) void flushLiveUpdate()
      else scheduleLiveUpdate()
    } else {
      resolveLiveSettleWaiters()
    }
  }
}

async function generationData(bindings) {
  const text = {}
  const images = {}
  for (const binding of bindings) {
    const input = document.querySelector(`[data-binding="${CSS.escape(binding.id)}"]`)
    if (binding.kind === 'text') {
      text[binding.id] = input?.value ?? defaultText(binding.id)
    } else {
      const file = input?.files?.[0]
      images[binding.id] = file === undefined ? await defaultImage() : {
        bytes: new Uint8Array(await file.arrayBuffer()),
        extension: extensionOf(file.name),
        contentType: file.type || contentTypeOf(file.name),
      }
    }
  }
  // Only submit capabilities discovered from this template. In particular, do not leak
  // bundled-template table or slide defaults into an unrelated uploaded template.
  return { text, images }
}

function renderBindings(bindings) {
  elements.bindings.replaceChildren()
  if (bindings.length === 0) {
    const message = document.createElement('p')
    message.className = 'muted'
    message.textContent = 'No editable bindings found. The template is ready as-is.'
    elements.bindings.append(message)
    return
  }
  for (const binding of bindings) {
    const wrapper = document.createElement('div')
    wrapper.className = 'field'
    const label = document.createElement('label')
    label.textContent = binding.kind === 'image' ? `${binding.id} — image` : binding.id
    const input = document.createElement('input')
    input.id = `binding-${binding.id.replaceAll(/[^a-zA-Z0-9_-]/g, '-')}`
    input.dataset.binding = binding.id
    label.htmlFor = input.id
    if (binding.kind === 'image') {
      input.type = 'file'
      input.accept = 'image/png,image/jpeg,image/webp,image/gif,image/svg+xml'
    } else {
      input.type = 'text'
      input.value = defaultText(binding.id)
    }
    const source = document.createElement('small')
    source.textContent = `${binding.source} · ${binding.partName}`
    wrapper.append(label, input, source)
    elements.bindings.append(wrapper)
  }
}

function setupPreview(slideCount) {
  renderAbort?.abort()
  visibilityObserver?.disconnect()
  elements.preview.replaceChildren()
  visibleSlides.clear()
  dirtySlides.clear()
  slideHosts.clear()
  slideDiagnostics.clear()
  visibilityObserver = new IntersectionObserver((entries) => {
    for (const entry of entries) {
      const index = Number(entry.target.dataset.slideIndex)
      if (entry.isIntersecting) visibleSlides.add(index)
      else {
        visibleSlides.delete(index)
        const canvas = slideHosts.get(index)?.querySelector('canvas')
        canvas?.remove()
      }
    }
    void renderDirtySlides()
  }, { rootMargin: '240px 0px' })
  for (let index = 0; index < slideCount; index += 1) {
    const figure = document.createElement('figure')
    figure.dataset.slideIndex = String(index)
    figure.className = 'slide-host'
    figure.style.aspectRatio = '4 / 3'
    const caption = document.createElement('figcaption')
    caption.textContent = `Slide ${index + 1}`
    figure.append(caption)
    elements.preview.append(figure)
    slideHosts.set(index, figure)
    dirtySlides.add(index)
    visibilityObserver.observe(figure)
  }
  // Paint the first slide immediately even when the preview starts below the fold.
  if (slideCount > 0) visibleSlides.add(0)
}

async function renderDirtySlides() {
  const session = liveSession
  if (session === undefined) return
  const indices = [...visibleSlides].filter((index) =>
    index >= 0 && index < session.slideCount &&
    (dirtySlides.has(index) || slideHosts.get(index)?.querySelector('canvas') === null),
  )
  if (indices.length === 0) return
  const epoch = ++renderEpoch
  renderAbort?.abort()
  renderAbort = new AbortController()
  const { signal } = renderAbort
  activeRenderBatches += 1
  try {
    await Promise.all(indices.map((index) => renderLiveSlide(session, index, epoch, signal)))
  } catch (error) {
    if (signal.aborted || epoch !== renderEpoch || error?.name === 'WasmpptRevisionError') return
    throw error
  } finally {
    activeRenderBatches -= 1
  }
  if (epoch === renderEpoch) {
    showDiagnostics([...slideDiagnostics.values()].flat(), prepared?.bindings.length ?? 0)
  }
}

async function renderLiveSlide(session, index, epoch, signal) {
  const started = performance.now()
  const resolved = await client.resolveLiveSlide(session.handle, session.revision, index, { signal })
  if (signal.aborted || epoch !== renderEpoch || liveSession?.revision !== session.revision) return
  const scene = decodeDisplayList(resolved.displayList)
  slideDiagnostics.set(index, scene.diagnostics)
  const figure = slideHosts.get(index)
  if (figure === undefined) return
  const deviceScale = Math.min(devicePixelRatio || 1, 2)
  const width = Math.min(scene.width / 9_525, 960)
  const height = width * scene.height / scene.width
  const canvas = document.createElement('canvas')
  canvas.width = Math.max(1, Math.round(width * deviceScale))
  canvas.height = Math.max(1, Math.round(height * deviceScale))
  canvas.style.aspectRatio = `${scene.width} / ${scene.height}`
  canvas.setAttribute('aria-label', `Slide ${index + 1}`)
  figure.style.aspectRatio = `${scene.width} / ${scene.height}`
  figure.querySelector('canvas')?.remove()
  figure.prepend(canvas)
  const context = canvas.getContext('2d', { alpha: false })
  if (context === null) throw new Error('Canvas 2D is unavailable')
  await renderer.render(scene, context, {
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
  if (!signal.aborted && epoch === renderEpoch && liveSession?.revision === session.revision) {
    dirtySlides.delete(index)
  }
}

function showDiagnostics(renderDiagnostics, bindingCount) {
  const diagnostics = [...preparationDiagnostics, ...renderDiagnostics]
  const unique = [...new Map(diagnostics.map((item) => [
    `${item.code}\0${item.partName ?? ''}\0${item.shapeId ?? ''}\0${item.message}`,
    item,
  ])).values()]
  elements.diagnostics.textContent = unique.length === 0
    ? `${bindingCount} editable bindings · no diagnostics`
    : unique.map((item) => `${item.code}: ${item.message}`).join('\n')
}

async function releaseLiveSession() {
  if (liveSession === undefined) return
  const handle = liveSession.handle
  liveSession = undefined
  await client.releaseLiveSession(handle)
}

function enableDownload(blob, revision) {
  if (outputUrl !== undefined) URL.revokeObjectURL(outputUrl)
  outputBlob = blob
  outputRevision = revision
  outputUrl = URL.createObjectURL(blob)
  elements.download.href = outputUrl
  elements.download.download = outputName
  elements.download.dataset.revision = String(revision)
  elements.download.classList.remove('is-disabled')
  elements.download.setAttribute('aria-disabled', 'false')
}

function enableDownloadAction() {
  elements.download.classList.remove('is-disabled')
  elements.download.setAttribute('aria-disabled', 'false')
}

function disableDownload() {
  if (outputUrl !== undefined) URL.revokeObjectURL(outputUrl)
  outputUrl = undefined
  outputBlob = undefined
  outputRevision = -1
  elements.download.removeAttribute('href')
  delete elements.download.dataset.revision
  elements.download.classList.add('is-disabled')
  elements.download.setAttribute('aria-disabled', 'true')
}

function scheduleExport(revision) {
  clearTimeout(exportTimer)
  exportTimer = setTimeout(() => void exportRevision(revision), 200)
}

async function exportRevision(revision) {
  const session = liveSession
  if (session === undefined || session.revision !== revision) return undefined
  if (outputRevision === revision && outputBlob !== undefined) return outputBlob
  if (exportPromiseRevision === revision && exportPromise !== undefined) return exportPromise
  if (exportPromise !== undefined && exportPromiseRevision !== revision) abandonedWork += 1
  exportAbort?.abort()
  exportAbort = new AbortController()
  const { signal } = exportAbort
  exportPromiseRevision = revision
  exportPromise = (async () => {
    const started = performance.now()
    const chunks = []
    let length = 0
    for await (const chunk of client.generateLiveStream(session.handle, revision, {
      signal,
      onProgress: (phase, completed) => {
        if (phase === 'stream' && liveSession?.revision === revision) {
          elements.status.textContent = `Preparing revision ${revision} download · ${formatBytes(completed)}…`
        }
      },
    })) {
      chunks.push(chunk)
      length += chunk.byteLength
    }
    if (signal.aborted || liveSession?.revision !== revision) return undefined
    const blob = new Blob(chunks, {
      type: 'application/vnd.openxmlformats-officedocument.presentationml.presentation',
    })
    enableDownload(blob, revision)
    elements['generate-time'].textContent = formatMs(performance.now() - started)
    elements['output-size'].textContent = formatBytes(length)
    setStatus(`PPTX ready · ${session.slideCount} slide${session.slideCount === 1 ? '' : 's'}`, false)
    return blob
  })().catch((error) => {
    if (error?.name !== 'AbortError' && liveSession?.handle === session.handle) fail(error)
    return undefined
  }).finally(() => {
    if (exportPromiseRevision === revision) {
      exportPromise = undefined
      exportPromiseRevision = -1
    }
  })
  return exportPromise
}

async function downloadCurrentRevision(event) {
  let session = liveSession
  if (session === undefined || elements.download.getAttribute('aria-disabled') === 'true') {
    event.preventDefault()
    return
  }
  if (updateRunning || !deltaEmpty(pendingDelta)) {
    event.preventDefault()
    await settleLiveEdits()
    session = liveSession
    if (session === undefined) return
  }
  if (outputRevision === session.revision && outputUrl !== undefined) return
  event.preventDefault()
  const blob = await exportRevision(session.revision)
  if (blob === undefined || liveSession?.revision !== session.revision) return
  const link = document.createElement('a')
  link.href = URL.createObjectURL(blob)
  link.download = outputName
  link.click()
  setTimeout(() => URL.revokeObjectURL(link.href), 0)
}

function cancelLiveWork() {
  if (updateFrame !== undefined) cancelAnimationFrame(updateFrame)
  updateFrame = undefined
  clearTimeout(exportTimer)
  pendingDelta = emptyDelta()
  bindingEditVersions.clear()
  renderEpoch += 1
  renderAbort?.abort()
  exportAbort?.abort()
  visibilityObserver?.disconnect()
  visibleSlides.clear()
  dirtySlides.clear()
  slideHosts.clear()
  slideDiagnostics.clear()
  resolveLiveSettleWaiters()
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
  return { text: {}, images: {} }
}

function deltaEmpty(delta) {
  return Object.keys(delta.text).length === 0 && Object.keys(delta.images).length === 0
}

function setStatus(message, busy) {
  elements.status.textContent = message
  document.body.classList.toggle('is-busy', busy)
}

function fail(error) {
  const message = error instanceof Error ? `${error.name}: ${error.message}` : String(error)
  setStatus('Could not create a preview', false)
  elements.diagnostics.textContent = message
  if (liveSession === undefined) disableDownload()
  console.error(error)
}

function hasFiles(dataTransfer) {
  return dataTransfer?.types?.includes('Files') ?? false
}

function isPowerPointFile(file) {
  return /\.(potx|potm|pptx)$/i.test(file.name)
}

function defaultText(id) {
  if (id === 'title') return 'wasmppt quarterly report'
  if (id === 'subtitle') return 'Generated and previewed entirely in your browser'
  const field = id.split('.').at(-1) ?? id
  return field.replaceAll(/[-_]/g, ' ').replace(/^./, (value) => value.toUpperCase())
}

async function defaultImage() {
  if (defaultImageData !== undefined) return defaultImageData
  const canvas = document.createElement('canvas')
  canvas.width = 64
  canvas.height = 64
  const context = canvas.getContext('2d')
  if (context === null) throw new Error('Canvas 2D is unavailable')
  context.fillStyle = '#71ffb1'
  context.fillRect(0, 0, 64, 64)
  context.fillStyle = '#07120c'
  context.fillRect(16, 16, 32, 32)
  const blob = await new Promise((resolve, reject) => {
    canvas.toBlob((value) => value === null ? reject(new Error('Cannot encode fallback image')) : resolve(value), 'image/png')
  })
  defaultImageData = {
    bytes: new Uint8Array(await blob.arrayBuffer()),
    extension: 'png',
    contentType: 'image/png',
  }
  return defaultImageData
}

function extensionOf(name) {
  const extension = name.split('.').at(-1)?.toLowerCase()
  if (!extension || !/^[a-z0-9]+$/.test(extension)) throw new Error('Image needs a safe file extension')
  return extension === 'jpeg' ? 'jpg' : extension
}

function contentTypeOf(name) {
  const extension = extensionOf(name)
  return extension === 'jpg' ? 'image/jpeg' : `image/${extension}`
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
