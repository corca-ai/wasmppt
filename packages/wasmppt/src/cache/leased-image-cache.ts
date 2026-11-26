import type { DecodedImage } from '../image.js'
import { ByteBudgetLru } from './byte-budget-lru.js'

export interface ImageLease {
  readonly image: DecodedImage
  release(): void
}

class ImageEntry {
  constructor(readonly image: DecodedImage, private owners: number) {}

  retain(): void { this.owners += 1 }

  release(): void {
    this.owners -= 1
    if (this.owners === 0) this.image.close?.()
  }

  lease(): ImageLease {
    let released = false
    return { image: this.image, release: () => {
      if (released) return
      released = true
      this.release()
    } }
  }
}

interface PendingImage {
  promise: Promise<ImageEntry | undefined>
  borrowers: number
  entry?: ImageEntry
}

/** Cache ownership and in-flight paint ownership have independent lifetimes. */
export class LeasedImageCache {
  readonly #cache: ByteBudgetLru<string, ImageEntry>
  readonly #pending = new Map<string, PendingImage>()
  #revision = 0

  constructor(maxBytes: number) {
    this.#cache = new ByteBudgetLru(maxBytes, entry => entry.release())
  }

  get residentBytes(): number { return this.#cache.residentBytes }
  get hitRate(): number { return this.#cache.hitRate }

  async acquire(key: string, load: () => Promise<DecodedImage | undefined>): Promise<ImageLease | undefined> {
    const cached = this.#cache.get(key)
    if (cached !== undefined) {
      cached.retain()
      return cached.lease()
    }
    let pending = this.#pending.get(key)
    if (pending === undefined) {
      const revision = this.#revision
      const created: PendingImage = { borrowers: 1, promise: Promise.resolve(undefined) }
      created.promise = Promise.resolve().then(load).then(image => {
        if (image === undefined) return undefined
        if (revision !== this.#revision) {
          image.close?.()
          return undefined
        }
        if (!Number.isSafeInteger(image.residentBytes) || image.residentBytes < 0) {
          image.close?.()
          throw new RangeError('cache weight is invalid')
        }
        // Reserve every waiting paint's owner before cache admission can evict this entry.
        const entry = new ImageEntry(image, created.borrowers + 1)
        created.entry = entry
        this.#cache.set(key, entry, image.residentBytes)
        return entry
      }).finally(() => {
        if (this.#pending.get(key) === created) this.#pending.delete(key)
      })
      this.#pending.set(key, created)
      pending = created
    } else {
      pending.borrowers += 1
      pending.entry?.retain()
    }
    return (await pending.promise)?.lease()
  }

  clear(): void {
    this.#revision += 1
    this.#pending.clear()
    this.#cache.clear()
  }
}
