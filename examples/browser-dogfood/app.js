import { WasmpptWorkerClient } from './lib/worker-client.js'

const elements = Object.fromEntries(
  ['template', 'prepare', 'generate', 'bindings', 'advanced', 'status', 'diagnostics', 'compile-time', 'generate-time', 'output-size']
    .map((id) => [id, document.getElementById(id)]),
)
const worker = new Worker('./worker.js', { type: 'module' })
const client = new WasmpptWorkerClient(worker)
let prepared

worker.addEventListener('message', (event) => {
  if (event.data?.type === 'host-ready') {
    elements.status.textContent = 'WebAssembly ready — bundled template selected'
    elements.prepare.disabled = false
  } else if (event.data?.type === 'host-init-error') {
    fail(new Error(event.data.message))
  }
})
worker.addEventListener('error', (event) => fail(new Error(event.message)))
elements.prepare.disabled = true
elements.prepare.addEventListener('click', () => void prepareTemplate())
elements.generate.addEventListener('click', () => void generatePresentation())

async function prepareTemplate() {
  setBusy(true, 'Compiling template…')
  try {
    if (prepared !== undefined) await client.release(prepared.handle)
    const file = elements.template.files[0]
    const bytes = file === undefined
      ? await fetch('./fixtures/report.potx').then(required).then((response) => response.arrayBuffer())
      : await file.arrayBuffer()
    const started = performance.now()
    prepared = await client.prepare(bytes, { macroPolicy: 'strip' })
    elements['compile-time'].textContent = formatMs(performance.now() - started)
    renderBindings(prepared.bindings)
    elements.diagnostics.textContent = prepared.diagnostics.length === 0
      ? `Plan ${prepared.plan.byteLength.toLocaleString()} bytes · ${prepared.bindings.length} bindings · no diagnostics`
      : prepared.diagnostics.map((item) => `${item.code}: ${item.message}`).join('\n')
    elements.generate.disabled = false
    elements.status.textContent = `Template compiled · ${(prepared.residentBytes / 1024).toFixed(1)} KiB resident`
  } catch (error) {
    fail(error)
  } finally {
    setBusy(false)
  }
}

async function generatePresentation() {
  if (prepared === undefined) return
  setBusy(true, 'Generating and streaming PPTX…')
  try {
    const advanced = JSON.parse(elements.advanced.value)
    const text = {}
    const images = {}
    for (const binding of prepared.bindings) {
      const input = document.querySelector(`[data-binding="${CSS.escape(binding.id)}"]`)
      if (input === null) continue
      if (binding.kind === 'text') text[binding.id] = input.value
      else {
        const file = input.files[0]
        images[binding.id] = file === undefined ? defaultImage() : {
          bytes: new Uint8Array(await file.arrayBuffer()),
          extension: extensionOf(file.name),
          contentType: file.type || `image/${extensionOf(file.name)}`,
        }
      }
    }
    const started = performance.now()
    const chunks = []
    let length = 0
    for await (const chunk of client.generateStream(prepared.handle, {
      ...advanced,
      text,
      images,
    }, {
      onProgress: (phase, completed) => {
        if (phase === 'stream') elements.status.textContent = `Streaming ${(completed / 1024).toFixed(1)} KiB…`
      },
    })) {
      chunks.push(chunk)
      length += chunk.byteLength
    }
    const blob = new Blob(chunks, { type: 'application/vnd.openxmlformats-officedocument.presentationml.presentation' })
    const url = URL.createObjectURL(blob)
    const link = Object.assign(document.createElement('a'), { href: url, download: 'wasmppt-report.pptx' })
    link.click()
    setTimeout(() => URL.revokeObjectURL(url), 30_000)
    elements['generate-time'].textContent = formatMs(performance.now() - started)
    elements['output-size'].textContent = `${(length / 1024).toFixed(1)} KiB`
    elements.status.textContent = 'PPTX generated locally and downloaded'
  } catch (error) {
    fail(error)
  } finally {
    setBusy(false)
  }
}

function renderBindings(bindings) {
  elements.bindings.replaceChildren()
  for (const binding of bindings) {
    if (binding.id.startsWith('metrics.')) continue
    const wrapper = document.createElement('div')
    wrapper.className = 'field'
    const label = document.createElement('label')
    label.textContent = binding.kind === 'image' ? `${binding.id} — image` : binding.id
    const input = document.createElement('input')
    input.dataset.binding = binding.id
    if (binding.kind === 'image') {
      input.type = 'file'
      input.accept = 'image/png,image/jpeg,image/webp,image/gif'
    } else {
      input.type = 'text'
      input.value = binding.id === 'title' ? 'wasmppt quarterly report' : 'Generated at the speed of WebAssembly'
    }
    const source = document.createElement('small')
    source.textContent = `${binding.source} · ${binding.partName}`
    wrapper.append(label, input, source)
    elements.bindings.append(wrapper)
  }
}

function defaultImage() {
  const binary = atob('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M/wHwAF/gL+XHI4ZAAAAABJRU5ErkJggg==')
  return { bytes: Uint8Array.from(binary, (value) => value.charCodeAt(0)), extension: 'png', contentType: 'image/png' }
}

function extensionOf(name) {
  const extension = name.split('.').at(-1)?.toLowerCase()
  if (!extension || !/^[a-z0-9]+$/.test(extension)) throw new Error('Image needs a safe file extension')
  return extension === 'jpg' ? 'jpeg' : extension
}

function required(response) {
  if (!response.ok) throw new Error(`Cannot load bundled template: HTTP ${response.status}`)
  return response
}

function setBusy(busy, message) {
  elements.prepare.disabled = busy
  elements.generate.disabled = busy || prepared === undefined
  if (message !== undefined) elements.status.textContent = message
}

function fail(error) {
  const message = error instanceof Error ? `${error.name}: ${error.message}` : String(error)
  elements.status.textContent = 'Operation failed'
  elements.diagnostics.textContent = message
  console.error(error)
}

function formatMs(value) { return `${value.toFixed(1)} ms` }
