import assert from 'node:assert/strict'
import test from 'node:test'

import {
  WASMPPT_BROWSER_WORKER_ERROR,
  WASMPPT_BROWSER_WORKER_READY,
  WasmpptWorkerClient,
  connectWasmpptBrowserWorker,
} from '../dist/index.js'

class FakeWorker extends EventTarget {
  terminated = false

  postMessage() {}

  terminate() {
    this.terminated = true
  }

  emitMessage(data) {
    this.dispatchEvent(new MessageEvent('message', { data }))
  }
}

test('connectWasmpptBrowserWorker exposes a client only after ready', async () => {
  const worker = new FakeWorker()
  const connection = connectWasmpptBrowserWorker(worker, 100)

  worker.emitMessage({ type: 'unrelated' })
  worker.emitMessage({ type: WASMPPT_BROWSER_WORKER_READY })

  const client = await connection
  assert.ok(client instanceof WasmpptWorkerClient)
  assert.equal(worker.terminated, false)
  client.terminate()
  assert.equal(worker.terminated, true)
})

test('connectWasmpptBrowserWorker terminates explicit startup failures', async () => {
  const worker = new FakeWorker()
  const connection = connectWasmpptBrowserWorker(worker, 100)

  worker.emitMessage({
    type: WASMPPT_BROWSER_WORKER_ERROR,
    message: 'Wasm unavailable',
  })

  await assert.rejects(connection, /Wasm unavailable/)
  assert.equal(worker.terminated, true)
})

test('connectWasmpptBrowserWorker terminates timeouts and invalid budgets', async () => {
  const timedOutWorker = new FakeWorker()
  await assert.rejects(
    connectWasmpptBrowserWorker(timedOutWorker, 1),
    /initialization timed out/,
  )
  assert.equal(timedOutWorker.terminated, true)

  const invalidWorker = new FakeWorker()
  await assert.rejects(
    connectWasmpptBrowserWorker(invalidWorker, 0),
    /positive safe integer/,
  )
  assert.equal(invalidWorker.terminated, true)
})
