import assert from 'node:assert/strict'
import test from 'node:test'

import {
  WORKER_PROTOCOL_VERSION,
  WasmpptError,
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

test('machine-readable errors survive Wasm, protocol v5, and the browser client', async () => {
  const worker = new FakeWorker()
  const client = new WasmpptWorkerClient(worker)
  const pending = client.prepare(new ArrayBuffer(4))
  const request = worker.messages[0]
  worker.respond({
    version: WORKER_PROTOCOL_VERSION,
    id: request.id,
    type: 'error',
    error: {
      version: 1,
      domain: 'package',
      code: 'invalid-signature',
      message: 'not a ZIP package',
      causeCode: 'invalid-signature',
    },
    name: 'WasmpptPackageError',
    message: 'not a ZIP package',
  })

  await assert.rejects(pending, (error) => {
    assert(error instanceof WasmpptError)
    assert.equal(error.name, 'WasmpptPackageError')
    assert.equal(error.domain, 'package')
    assert.equal(error.code, 'invalid-signature')
    assert.equal(error.envelope.causeCode, 'invalid-signature')
    return true
  })
  client.terminate()
})

test('protocol v5 client decodes legacy v4 errors without treating messages as codes', async () => {
  const worker = new FakeWorker()
  const client = new WasmpptWorkerClient(worker)
  const pending = client.prepare(new ArrayBuffer(4))
  const request = worker.messages[0]
  worker.respond({
    version: 4,
    id: request.id,
    type: 'error',
    name: 'WasmpptCompileError',
    message: 'legacy detail',
  })

  await assert.rejects(pending, (error) => {
    assert(error instanceof WasmpptError)
    assert.equal(error.code, 'legacy-error')
    assert.equal(error.message, 'legacy detail')
    return true
  })
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
  assert.equal(scope.responses.at(-1).error.code, 'cancelled')
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

test('runtime transports a Wasm error envelope without rewriting machine fields', async () => {
  class Scope extends EventTarget {
    responses = []
    postMessage(message) { this.responses.push(message) }
  }
  const scope = new Scope()
  const error = new Error('not a ZIP package')
  error.name = 'WasmpptPackageError'
  error.wasmppt = Object.freeze({
    version: 1,
    domain: 'package',
    code: 'invalid-signature',
    message: error.message,
    causeCode: 'invalid-signature',
  })
  installWorkerRuntime(scope, {
    prepare_with_options: () => { throw error },
  })
  scope.dispatchEvent(new MessageEvent('message', {
    data: {
      version: WORKER_PROTOCOL_VERSION,
      id: 76,
      type: 'prepare',
      template: new ArrayBuffer(8),
      options: {},
    },
  }))
  await new Promise((resolve) => setTimeout(resolve, 0))

  assert.deepEqual(scope.responses.at(-1), {
    version: WORKER_PROTOCOL_VERSION,
    id: 76,
    type: 'error',
    error: error.wasmppt,
    name: 'WasmpptPackageError',
    message: 'not a ZIP package',
  })
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

test('live deltas expose exact invalidation and content-addressed resources survive A-B-A edits', async () => {
  const worker = new FakeWorker()
  const client = new WasmpptWorkerClient(worker)
  const partName = 'ppt/media/hero.png'

  const apply = async (expectedRevision, fingerprintLabel) => {
    const pending = client.applyLiveDelta(17, expectedRevision, { title: fingerprintLabel })
    const request = worker.messages.at(-1)
    assert.equal(request.type, 'apply-live-delta')
    assert.deepEqual(worker.transfers.at(-1), [request.payload])
    worker.respond({
      version: WORKER_PROTOCOL_VERSION,
      id: request.id,
      type: 'live-session-updated',
      sessionHandle: 17,
      revision: expectedRevision + 1,
      graphChanged: false,
      fullFallback: false,
      invalidationReason: 'dependency',
      slideCount: 2,
      invalidatedSlides: [0],
      changedBindings: ['title'],
      changedParts: [partName],
      overlay: {
        reusedMaterializedParts: 7,
        logicalParts: 20,
        materializedParts: 8,
        materializedBytes: 1024,
        reusedSourceBytes: 2048,
        removedParts: 0,
      },
    })
    const update = await pending
    assert.deepEqual(update.changedBindings, ['title'])
    assert.deepEqual(update.invalidatedSlides, [0])
    assert.equal(update.overlay.reusedMaterializedParts, 7)
  }

  const load = async (revision, fingerprint, fill) => {
    const pending = client.liveSessionResource(17, revision, partName)
    await Promise.resolve()
    const fingerprintRequest = worker.messages.at(-1)
    assert.equal(fingerprintRequest.type, 'live-session-resource-fingerprint')
    worker.respond({
      version: WORKER_PROTOCOL_VERSION,
      id: fingerprintRequest.id,
      type: 'live-session-resource-fingerprint',
      sessionHandle: 17,
      revision,
      partName,
      fingerprint,
    })
    await new Promise((resolve) => setImmediate(resolve))
    const resourceRequest = worker.messages.at(-1)
    if (resourceRequest.type === 'live-session-resource') {
      const bytes = Uint8Array.of(fill).buffer
      worker.respond({
        version: WORKER_PROTOCOL_VERSION,
        id: resourceRequest.id,
        type: 'live-session-resource',
        sessionHandle: 17,
        revision,
        partName,
        fingerprint,
        bytes,
      })
    }
    return pending
  }

  await apply(0, 'A')
  assert.deepEqual(new Uint8Array((await load(1, 'sha-A', 0xaa)).bytes), Uint8Array.of(0xaa))
  await apply(1, 'B')
  assert.deepEqual(new Uint8Array((await load(2, 'sha-B', 0xbb)).bytes), Uint8Array.of(0xbb))
  await apply(2, 'A')
  assert.deepEqual(new Uint8Array((await load(3, 'sha-A', 0xcc)).bytes), Uint8Array.of(0xaa))
  assert.equal(worker.messages.filter((message) => message.type === 'live-session-resource').length, 2)
  client.terminate()
})
