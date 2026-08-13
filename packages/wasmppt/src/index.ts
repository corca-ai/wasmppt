/** Package identity exposed without loading the Wasm engine. */
export const packageName = '@corca-ai/wasmppt' as const

export * from './protocol.js'
export * from './injection.js'
export * from './worker-client.js'
export * from './worker-runtime.js'
export * from './canvas.js'
export * from './dom-svg.js'
export * from './shaper.js'

/** Browser-owned locations for separately emitted engine and worker assets. */
export interface BrowserEngineAssets {
  readonly wasmUrl: string | URL
  readonly workerUrl: string | URL
}

/** A transferable binary input accepted by the future browser adapter. */
export type BrowserBinaryInput = ArrayBuffer

/** Host-owned persistence port for serialized, versioned TemplatePlan bytes. */
export interface TemplatePlanStore {
  load(key: Uint8Array): Promise<ArrayBuffer | undefined>
  store(key: Uint8Array, plan: ArrayBuffer): Promise<void>
}

/** IndexedDB adapter; the Rust core remains unaware of browser storage APIs. */
export class IndexedDbTemplatePlanStore implements TemplatePlanStore {
  readonly #databaseName: string
  readonly #storeName: string
  #database: Promise<IDBDatabase> | undefined

  constructor(databaseName = 'wasmppt', storeName = 'template-plans') {
    this.#databaseName = databaseName
    this.#storeName = storeName
  }

  async load(key: Uint8Array): Promise<ArrayBuffer | undefined> {
    const database = await this.#open()
    const request = database
      .transaction(this.#storeName, 'readonly')
      .objectStore(this.#storeName)
      .get(toHex(key))
    const value: unknown = await requestResult(request)
    if (value === undefined) return undefined
    if (!(value instanceof ArrayBuffer)) {
      throw new TypeError('stored TemplatePlan is not an ArrayBuffer')
    }
    return value
  }

  async store(key: Uint8Array, plan: ArrayBuffer): Promise<void> {
    const database = await this.#open()
    const transaction = database.transaction(this.#storeName, 'readwrite')
    transaction.objectStore(this.#storeName).put(plan, toHex(key))
    await transactionComplete(transaction)
  }

  close(): void {
    void this.#database?.then((database) => database.close())
    this.#database = undefined
  }

  #open(): Promise<IDBDatabase> {
    if (this.#database !== undefined) return this.#database
    this.#database = new Promise((resolve, reject) => {
      const request = indexedDB.open(this.#databaseName, 1)
      request.addEventListener('upgradeneeded', () => {
        if (!request.result.objectStoreNames.contains(this.#storeName)) {
          request.result.createObjectStore(this.#storeName)
        }
      })
      request.addEventListener('success', () => resolve(request.result))
      request.addEventListener('error', () =>
        reject(request.error ?? new Error('failed to open TemplatePlan database')),
      )
      request.addEventListener('blocked', () =>
        reject(new Error('TemplatePlan database upgrade is blocked')),
      )
    })
    return this.#database
  }
}

function toHex(bytes: Uint8Array): string {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('')
}

function requestResult<T>(request: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    request.addEventListener('success', () => resolve(request.result))
    request.addEventListener('error', () =>
      reject(request.error ?? new Error('IndexedDB request failed')),
    )
  })
}

function transactionComplete(transaction: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => {
    transaction.addEventListener('complete', () => resolve())
    transaction.addEventListener('abort', () =>
      reject(transaction.error ?? new Error('IndexedDB transaction aborted')),
    )
    transaction.addEventListener('error', () =>
      reject(transaction.error ?? new Error('IndexedDB transaction failed')),
    )
  })
}
