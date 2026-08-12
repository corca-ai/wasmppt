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
  assert.equal(await readingResource, resource)
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
