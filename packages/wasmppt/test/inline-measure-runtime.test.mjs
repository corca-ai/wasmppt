import assert from 'node:assert/strict'
import test from 'node:test'
import { installWorkerRuntime, WORKER_PROTOCOL_VERSION } from '../dist/index.js'

class Scope extends EventTarget {
  next = 1
  pending = new Map()
  postMessage(message) { this.pending.get(message.id)?.resolve(message); this.pending.delete(message.id) }
  call(operation) {
    const id = this.next++
    const result = Promise.withResolvers()
    this.pending.set(id, result)
    this.dispatchEvent(new MessageEvent('message', { data: { version: WORKER_PROTOCOL_VERSION, id, type: 'deck-api', operation } }))
    return { id, result: result.promise }
  }
  cancel(id) { this.dispatchEvent(new MessageEvent('message', { data: { version: WORKER_PROTOCOL_VERSION, id: this.next++, type: 'cancel', targetId: id } })) }
}
const snapshot = (measureInlineText, fonts = []) => ({ kind: 'snapshot', template: 1,
  spec: new ArrayBuffer(), assets: [], fonts, previous: 0, measureInlineText })
const measure = () => 42

test('optional inline measurement loads only on demand and is shared across snapshots', async () => {
  const scope = new Scope()
  const loaded = Promise.withResolvers()
  let loads = 0
  const callbacks = []
  installWorkerRuntime(scope, {
    deck_accounted_bytes: () => 0,
    create_deck_snapshot: (...args) => { callbacks.push(args.at(-1)); throw new Error('snapshot boundary reached') },
  }, { loadInlineTextMeasure: () => { loads++; return loaded.promise } })
  await scope.call({ kind: 'accounting' }).result
  await scope.call(snapshot(false)).result
  await scope.call(snapshot(true, [7])).result
  assert.equal(loads, 0, 'ordinary and exact-font requests do not load browser measurement')
  assert.deepEqual(callbacks, [undefined, undefined])
  const first = scope.call(snapshot(true))
  const second = scope.call(snapshot(true))
  const cancelled = scope.call(snapshot(true))
  scope.cancel(cancelled.id)
  assert.equal(loads, 1)
  assert.equal(callbacks.length, 2, 'no snapshot is created before measurement is available')
  loaded.resolve(measure)
  await Promise.all([first.result, second.result])
  assert.equal((await cancelled.result).type, 'cancelled')
  await scope.call(snapshot(true)).result
  assert.equal(loads, 1)
  assert.deepEqual(callbacks, [undefined, undefined, measure, measure, measure])
})

test('measurement load failure reports an error without creating a snapshot', async () => {
  const scope = new Scope()
  let creates = 0
  installWorkerRuntime(scope, { create_deck_snapshot: () => { creates++; return 1 } }, {
    loadInlineTextMeasure: async () => { throw new Error('measurement module unavailable') },
  })
  const result = await scope.call(snapshot(true)).result
  assert.equal(result.type, 'error')
  assert.match(result.message, /measurement module unavailable/)
  assert.equal(creates, 0)
})
