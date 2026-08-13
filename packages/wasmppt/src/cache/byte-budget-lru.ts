export class ByteBudgetLru<Key, Value> {
  readonly #entries = new Map<Key, { readonly value: Value; readonly weight: number }>()
  readonly #maxBytes: number
  readonly #dispose?: (value: Value) => void
  #residentBytes = 0
  #hits = 0
  #misses = 0

  constructor(maxBytes: number, dispose?: (value: Value) => void) {
    if (!Number.isSafeInteger(maxBytes) || maxBytes < 0) {
      throw new RangeError('byte budget must be a non-negative safe integer')
    }
    this.#maxBytes = maxBytes
    this.#dispose = dispose
  }

  get residentBytes(): number { return this.#residentBytes }
  get size(): number { return this.#entries.size }
  get hits(): number { return this.#hits }
  get misses(): number { return this.#misses }

  get hitRate(): number {
    const requests = this.#hits + this.#misses
    return requests === 0 ? 0 : this.#hits / requests
  }

  get(key: Key): Value | undefined {
    const entry = this.#entries.get(key)
    if (entry === undefined) {
      this.#misses += 1
      return undefined
    }
    this.#hits += 1
    this.#entries.delete(key)
    this.#entries.set(key, entry)
    return entry.value
  }

  set(key: Key, value: Value, weight: number): boolean {
    if (!Number.isSafeInteger(weight) || weight < 0) throw new RangeError('cache weight is invalid')
    const previous = this.#entries.get(key)
    if (previous !== undefined && Object.is(previous.value, value)) {
      if (weight > this.#maxBytes) {
        this.#remove(key, previous)
        return false
      }
      this.#entries.delete(key)
      this.#residentBytes -= previous.weight
      this.#entries.set(key, { value, weight })
      this.#residentBytes += weight
      this.#evictToBudget()
      return Object.is(this.#entries.get(key)?.value, value)
    }
    if (previous !== undefined) this.#remove(key, previous)
    if (weight > this.#maxBytes) {
      this.#dispose?.(value)
      return false
    }
    this.#entries.set(key, { value, weight })
    this.#residentBytes += weight
    this.#evictToBudget()
    return Object.is(this.#entries.get(key)?.value, value)
  }

  clear(): void {
    for (const entry of this.#entries.values()) this.#dispose?.(entry.value)
    this.#entries.clear()
    this.#residentBytes = 0
    this.#hits = 0
    this.#misses = 0
  }

  #evictToBudget(): void {
    while (this.#residentBytes > this.#maxBytes) {
      const oldest = this.#entries.entries().next().value as
        | [Key, { readonly value: Value; readonly weight: number }]
        | undefined
      if (oldest === undefined) break
      this.#remove(oldest[0], oldest[1])
    }
  }

  #remove(key: Key, entry: { readonly value: Value; readonly weight: number }): void {
    this.#entries.delete(key)
    this.#residentBytes -= entry.weight
    this.#dispose?.(entry.value)
  }
}
