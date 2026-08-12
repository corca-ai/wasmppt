import { CanvasDisplayListRenderer, decodeDisplayList } from './lib/canvas.js'
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
let presentationHandle
let outputUrl
let templateEpoch = 0
let generationEpoch = 0
let generationAbort
let regenerationTimer
let outputName = 'wasmppt-report.pptx'
let defaultImageData

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
elements.bindings.addEventListener('input', scheduleGeneration)
elements.bindings.addEventListener('change', scheduleGeneration)
elements.download.addEventListener('click', (event) => {
  if (elements.download.getAttribute('aria-disabled') === 'true') event.preventDefault()
})

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
  cancelGeneration()
  disableDownload()
  setStatus('Reading template…', true)
  try {
    const previous = prepared
    prepared = undefined
    if (previous !== undefined) await client.release(previous.handle)
    await releasePresentation()
    const started = performance.now()
    const next = await client.prepare(bytes, { macroPolicy: 'strip' })
    if (epoch !== templateEpoch) {
      await client.release(next.handle)
      return
    }
    prepared = next
    elements['compile-time'].textContent = formatMs(performance.now() - started)
    renderBindings(next.bindings)
    elements.diagnostics.textContent = next.diagnostics.length === 0
      ? `${next.bindings.length} editable bindings · no diagnostics`
      : next.diagnostics.map((item) => `${item.code}: ${item.message}`).join('\n')
    await generateAndPreview()
  } catch (error) {
    if (epoch === templateEpoch) fail(error)
  }
}

function scheduleGeneration() {
  clearTimeout(regenerationTimer)
  regenerationTimer = setTimeout(() => void generateAndPreview(), 250)
}

async function generateAndPreview() {
  const template = prepared
  if (template === undefined) return
  const epoch = ++generationEpoch
  generationAbort?.abort()
  generationAbort = new AbortController()
  disableDownload()
  setStatus('Refreshing preview…', true)
  try {
    const data = await generationData(template.bindings)
    const started = performance.now()
    const chunks = []
    let length = 0
    for await (const chunk of client.generateStream(template.handle, data, {
      signal: generationAbort.signal,
      onProgress: (phase, completed) => {
        if (phase === 'stream' && epoch === generationEpoch) {
          elements.status.textContent = `Rendering ${formatBytes(completed)}…`
        }
      },
    })) {
      chunks.push(chunk)
      length += chunk.byteLength
    }
    if (epoch !== generationEpoch) return
    const blob = new Blob(chunks, {
      type: 'application/vnd.openxmlformats-officedocument.presentationml.presentation',
    })
    elements['generate-time'].textContent = formatMs(performance.now() - started)
    elements['output-size'].textContent = formatBytes(length)
    const slideCount = await renderPreview(blob, epoch)
    if (epoch !== generationEpoch) return
    enableDownload(blob)
    setStatus(`PPTX ready · ${slideCount} slide${slideCount === 1 ? '' : 's'}`, false)
  } catch (error) {
    if (error?.name !== 'AbortError' && epoch === generationEpoch) fail(error)
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
      input.accept = 'image/png,image/jpeg,image/webp,image/gif'
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

async function renderPreview(blob, epoch) {
  await releasePresentation()
  const opened = await client.openPresentation(await blob.arrayBuffer())
  if (epoch !== generationEpoch) {
    await client.releasePresentation(opened.handle)
    return 0
  }
  presentationHandle = opened.handle
  elements.preview.replaceChildren()
  const deviceScale = Math.min(devicePixelRatio || 1, 2)
  for (let index = 0; index < opened.slideCount; index += 1) {
    const scene = decodeDisplayList(await client.resolveSlide(opened.handle, index))
    if (epoch !== generationEpoch) return 0
    const width = Math.min(scene.width / 9_525, 960)
    const height = width * scene.height / scene.width
    const figure = document.createElement('figure')
    const canvas = document.createElement('canvas')
    canvas.width = Math.max(1, Math.round(width * deviceScale))
    canvas.height = Math.max(1, Math.round(height * deviceScale))
    canvas.style.aspectRatio = `${scene.width} / ${scene.height}`
    canvas.setAttribute('aria-label', `Slide ${index + 1}`)
    const caption = document.createElement('figcaption')
    caption.textContent = `Slide ${index + 1}`
    figure.append(canvas, caption)
    elements.preview.append(figure)
    const context = canvas.getContext('2d', { alpha: false })
    if (context === null) throw new Error('Canvas 2D is unavailable')
    await renderer.render(scene, context, {
      imageResolver: async (image, signal) => {
        if (image.partName === undefined) throw new Error('Image resource has no package part')
        const bytes = await client.presentationResource(opened.handle, image.partName, { signal })
        const bitmap = await createImageBitmap(new Blob([bytes], { type: mediaTypeOf(image.partName) }))
        return {
          source: bitmap,
          residentBytes: bitmap.width * bitmap.height * 4,
          close: () => bitmap.close(),
        }
      },
    })
  }
  return opened.slideCount
}

async function releasePresentation() {
  if (presentationHandle === undefined) return
  const handle = presentationHandle
  presentationHandle = undefined
  await client.releasePresentation(handle)
}

function enableDownload(blob) {
  if (outputUrl !== undefined) URL.revokeObjectURL(outputUrl)
  outputUrl = URL.createObjectURL(blob)
  elements.download.href = outputUrl
  elements.download.download = outputName
  elements.download.classList.remove('is-disabled')
  elements.download.setAttribute('aria-disabled', 'false')
}

function disableDownload() {
  elements.download.removeAttribute('href')
  elements.download.classList.add('is-disabled')
  elements.download.setAttribute('aria-disabled', 'true')
}

function cancelGeneration() {
  clearTimeout(regenerationTimer)
  generationEpoch += 1
  generationAbort?.abort()
}

function setStatus(message, busy) {
  elements.status.textContent = message
  document.body.classList.toggle('is-busy', busy)
}

function fail(error) {
  const message = error instanceof Error ? `${error.name}: ${error.message}` : String(error)
  setStatus('Could not create a preview', false)
  elements.diagnostics.textContent = message
  disableDownload()
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
