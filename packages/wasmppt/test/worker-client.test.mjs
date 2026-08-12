import assert from 'node:assert/strict'
import test from 'node:test'

import {
  WORKER_PROTOCOL_VERSION,
  WasmpptWorkerClient,
  installWorkerRuntime,
} from '../dist/index.js'

class FakeWorker extends EventTarget {
  messages = []
  transfers = []
  terminated = false

  postMessage(message, transfer = []) {
    this.messages.push(message)
    this.transfers.push(transfer)
  }

  terminate() {
    this.terminated = true
  }

  respond(message) {
    this.dispatchEvent(new MessageEvent('message', { data: message }))
  }
}

test('prepare transfers the caller-owned ArrayBuffer', async () => {
  const worker = new FakeWorker()
  const client = new WasmpptWorkerClient(worker)
  const template = new ArrayBuffer(16)
  const pending = client.prepare(template)
  const request = worker.messages[0]
  assert.equal(request.type, 'prepare')
  assert.deepEqual(worker.transfers[0], [template])
  worker.respond({
    version: WORKER_PROTOCOL_VERSION,
    id: request.id,
    type: 'prepared',
    templateHandle: 7,
    residentBytes: 32,
    plan: new ArrayBuffer(8),
    bindings: [],
    diagnostics: [],
  })
  assert.deepEqual(await pending, {
    handle: 7,
    residentBytes: 32,
    plan: new ArrayBuffer(8),
    bindings: [],
    diagnostics: [],
  })
  client.terminate()
})

test('an unknown Worker response cannot consume a live request ID', async () => {
  const worker = new FakeWorker()
  const client = new WasmpptWorkerClient(worker)
  const pending = client.prepare(new ArrayBuffer(4))
  const request = worker.messages[0]
  worker.respond({ version: WORKER_PROTOCOL_VERSION, id: request.id, type: 'mystery' })
  worker.respond({
    version: WORKER_PROTOCOL_VERSION,
    id: request.id,
    type: 'prepared',
    templateHandle: 8,
    residentBytes: 16,
    plan: new ArrayBuffer(4),
    bindings: [],
    diagnostics: [],
  })
  assert.equal((await pending).handle, 8)
  client.terminate()
})

test('termination rejects every pending request and stream', async () => {
  const worker = new FakeWorker()
  const client = new WasmpptWorkerClient(worker)
  const prepare = client.prepare(new ArrayBuffer(4))
  const streamRead = client.generateStream(9).getReader().read()
  client.terminate()
  await assert.rejects(prepare, /terminated/)
  await assert.rejects(streamRead, /terminated/)
  assert.equal(worker.terminated, true)
})

test('presentation, display lists, and lazy resources cross the Worker boundary once', async () => {
  const worker = new FakeWorker()
  const client = new WasmpptWorkerClient(worker)
  const presentation = new ArrayBuffer(32)
  const opening = client.openPresentation(presentation)
  const openRequest = worker.messages[0]
  assert.equal(openRequest.type, 'open-presentation')
  assert.deepEqual(worker.transfers[0], [presentation])
  worker.respond({
    version: WORKER_PROTOCOL_VERSION,
    id: openRequest.id,
    type: 'presentation-opened',
    presentationHandle: 11,
    slideCount: 5,
  })
  assert.deepEqual(await opening, { handle: 11, slideCount: 5 })

  const resolving = client.resolveSlide(11, 2)
  const resolveRequest = worker.messages[1]
  assert.deepEqual(resolveRequest, {
    version: WORKER_PROTOCOL_VERSION,
    id: resolveRequest.id,
    type: 'resolve-slide',
    presentationHandle: 11,
    slideIndex: 2,
  })
  const displayList = new ArrayBuffer(40)
  worker.respond({
    version: WORKER_PROTOCOL_VERSION,
    id: resolveRequest.id,
    type: 'slide-resolved',
    slideIndex: 2,
    displayList,
  })
  assert.equal(await resolving, displayList)

  const readingResource = client.presentationResource(11, 'ppt/media/photo.png')
  const duplicateResource = client.presentationResource(11, 'ppt/media/photo.png')
  const resourceRequest = worker.messages[2]
  assert.deepEqual(resourceRequest, {
    version: WORKER_PROTOCOL_VERSION,
    id: resourceRequest.id,
    type: 'presentation-resource',
    presentationHandle: 11,
    partName: 'ppt/media/photo.png',
  })
  const resource = new ArrayBuffer(24)
  worker.respond({
    version: WORKER_PROTOCOL_VERSION,
    id: resourceRequest.id,
    type: 'presentation-resource',
    partName: 'ppt/media/photo.png',
    bytes: resource,
  })
  assert.equal((await readingResource).byteLength, resource.byteLength)
  assert.equal((await duplicateResource).byteLength, resource.byteLength)
  assert.equal(worker.messages.filter((message) => message.type === 'presentation-resource').length, 1)
  assert.equal(client.resourceCacheBytes, resource.byteLength)

  const readingMetafile = client.presentationMetafileSvg(11, 'ppt/media/diagram.emf')
  const duplicateMetafile = client.presentationMetafileSvg(11, 'ppt/media/diagram.emf')
  const metafileRequest = worker.messages[3]
  assert.deepEqual(metafileRequest, {
    version: WORKER_PROTOCOL_VERSION,
    id: metafileRequest.id,
    type: 'presentation-metafile-svg',
    presentationHandle: 11,
    partName: 'ppt/media/diagram.emf',
  })
  const svg = new TextEncoder().encode('<svg xmlns="http://www.w3.org/2000/svg"></svg>').buffer
  worker.respond({
    version: WORKER_PROTOCOL_VERSION,
    id: metafileRequest.id,
    type: 'presentation-metafile-svg',
    partName: 'ppt/media/diagram.emf',
    bytes: svg,
  })
  assert.deepEqual(new Uint8Array(await readingMetafile), new Uint8Array(svg))
  assert.deepEqual(new Uint8Array(await duplicateMetafile), new Uint8Array(svg))
  assert.equal(worker.messages.filter((message) => message.type === 'presentation-metafile-svg').length, 1)
  const releasing = client.releasePresentation(11)
  const releaseRequest = worker.messages.at(-1)
  worker.respond({
    version: WORKER_PROTOCOL_VERSION,
    id: releaseRequest.id,
    type: 'presentation-released',
  })
  await releasing
  assert.equal(client.resourceCacheBytes, 0)
  client.terminate()
})

test('a Worker crash rejects all outstanding promises', async () => {
  const worker = new FakeWorker()
  const client = new WasmpptWorkerClient(worker)
  const first = client.prepare(new ArrayBuffer(1))
  const second = client.release(3)
  worker.dispatchEvent(new Event('error'))
  await assert.rejects(first, /unexpectedly/)
  await assert.rejects(second, /unexpectedly/)
  client.terminate()
})

test('runtime cancellation is observed between transferable output chunks', async () => {
  class Scope extends EventTarget {
    responses = []

    postMessage(message) {
      this.responses.push(message)
      if (message.type === 'chunk') {
        this.dispatchEvent(
          new MessageEvent('message', {
            data: { version: WORKER_PROTOCOL_VERSION, id: 99, type: 'cancel', targetId: 42 },
          }),
        )
      }
    }
  }
  const scope = new Scope()
  const output = new Uint8Array(12)
  let offset = 0
  installWorkerRuntime(scope, {
    prepare: () => 1,
    prepare_with_options: () => 1,
    prepare_with_plan: () => 1,
    prepared_weight: () => 1n,
    prepared_plan: () => new Uint8Array(),
    prepared_bindings: () => [],
    prepared_diagnostics: () => [],
    start_generation_payload: () => 2,
    generation_done: () => offset === output.byteLength,
    generation_pull: (_handle, length) => {
      const chunk = output.slice(offset, offset + length)
      offset += chunk.byteLength
      return chunk
    },
    release_template: () => true,
    release_generation: () => true,
  })
  scope.dispatchEvent(
    new MessageEvent('message', {
      data: {
        version: WORKER_PROTOCOL_VERSION,
        id: 42,
        type: 'generate',
        templateHandle: 1,
        payload: new ArrayBuffer(28),
        chunkBytes: 4,
      },
    }),
  )
  await new Promise((resolve) => setTimeout(resolve, 30))
  assert.equal(scope.responses.filter((message) => message.type === 'chunk').length, 1)
  assert.equal(scope.responses.at(-1).type, 'cancelled')
})

test('runtime preserves chart binding metadata returned by Wasm', async () => {
  class Scope extends EventTarget {
    responses = []

    postMessage(message) {
      this.responses.push(message)
    }
  }
  const scope = new Scope()
  installWorkerRuntime(scope, {
    prepare: () => 7,
    prepare_with_options: () => 7,
    prepare_with_plan: () => 7,
    prepared_weight: () => 32n,
    prepared_plan: () => new Uint8Array(8),
    prepared_bindings: () => [[
      'sales', 'chart', 'ppt/slides/slide1.xml', 'shape-metadata', 4, 'Sales chart',
    ]],
    prepared_diagnostics: () => [],
    release_template: () => true,
  })
  scope.dispatchEvent(new MessageEvent('message', {
    data: {
      version: WORKER_PROTOCOL_VERSION,
      id: 73,
      type: 'prepare',
      template: new ArrayBuffer(16),
      options: {},
    },
  }))
  await new Promise((resolve) => setTimeout(resolve, 0))
  assert.equal(scope.responses.at(-1).type, 'prepared')
  assert.deepEqual(scope.responses.at(-1).bindings, [{
    id: 'sales',
    kind: 'chart',
    partName: 'ppt/slides/slide1.xml',
    source: 'shape-metadata',
    shapeId: 4,
    shapeName: 'Sales chart',
  }])
})

test('runtime releases a prepared handle when metadata conversion fails', async () => {
  class Scope extends EventTarget {
    responses = []

    postMessage(message) {
      this.responses.push(message)
    }
  }
  const scope = new Scope()
  const released = []
  installWorkerRuntime(scope, {
    prepare: () => 41,
    prepare_with_options: () => 41,
    prepare_with_plan: () => 41,
    prepared_weight: () => 32n,
    prepared_plan: () => new Uint8Array(8),
    prepared_bindings: () => [['broken']],
    prepared_diagnostics: () => [],
    release_template: (handle) => { released.push(handle); return true },
  })
  scope.dispatchEvent(new MessageEvent('message', {
    data: {
      version: WORKER_PROTOCOL_VERSION,
      id: 74,
      type: 'prepare',
      template: new ArrayBuffer(16),
      options: {},
    },
  }))
  await new Promise((resolve) => setTimeout(resolve, 0))
  assert.equal(scope.responses.at(-1).type, 'error')
  assert.deepEqual(released, [41])
})

test('runtime releases a presentation handle when opening metadata fails', async () => {
  class Scope extends EventTarget {
    responses = []

    postMessage(message) {
      this.responses.push(message)
    }
  }
  const scope = new Scope()
  const released = []
  installWorkerRuntime(scope, {
    open_presentation: () => 51,
    presentation_slide_count: () => { throw new Error('broken slide metadata') },
    release_presentation: (handle) => { released.push(handle); return true },
  })
  scope.dispatchEvent(new MessageEvent('message', {
    data: {
      version: WORKER_PROTOCOL_VERSION,
      id: 75,
      type: 'open-presentation',
      presentation: new ArrayBuffer(16),
    },
  }))
  await new Promise((resolve) => setTimeout(resolve, 0))
  assert.equal(scope.responses.at(-1).type, 'error')
  assert.deepEqual(released, [51])
})

test('releasing a presentation prevents an in-flight resource from repopulating its cache', async () => {
  const worker = new FakeWorker()
  const client = new WasmpptWorkerClient(worker)
  const reading = client.presentationResource(11, 'ppt/media/late.png')
  const resourceRequest = worker.messages[0]
  const releasing = client.releasePresentation(11)
  const releaseRequest = worker.messages[1]
  worker.respond({
    version: WORKER_PROTOCOL_VERSION,
    id: resourceRequest.id,
    type: 'presentation-resource',
    partName: 'ppt/media/late.png',
    bytes: new ArrayBuffer(24),
  })
  worker.respond({
    version: WORKER_PROTOCOL_VERSION,
    id: releaseRequest.id,
    type: 'presentation-released',
  })
  await reading
  await releasing
  assert.equal(client.resourceCacheBytes, 0)
  client.terminate()
})
