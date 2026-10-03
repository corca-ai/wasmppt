import type { DeckSourceRange } from './model.js'

/** Convert editor UTF-16 offsets without coupling the semantic model to an editor implementation. */
export class DeckSourceText {
  readonly #utf16: number[] = [0]
  readonly #utf8: number[] = [0]
  constructor(readonly source: string, readonly text: string, readonly version?: string) {
    let utf16 = 0
    let utf8 = 0
    const encoder = new TextEncoder()
    for (const scalar of text) {
      utf16 += scalar.length; utf8 += encoder.encode(scalar).length
      this.#utf16.push(utf16); this.#utf8.push(utf8)
    }
  }
  toUtf8(offset: number): number { return this.#convert(offset, this.#utf16, this.#utf8) }
  toUtf16(offset: number): number { return this.#convert(offset, this.#utf8, this.#utf16) }
  range(start: number, end: number): DeckSourceRange {
    if (end < start) throw new RangeError('source range is reversed')
    return { source: this.source, start: this.toUtf8(start), end: this.toUtf8(end),
      ...(this.version === undefined ? {} : { version: this.version }) }
  }
  #convert(offset: number, from: readonly number[], to: readonly number[]): number {
    if (!Number.isSafeInteger(offset) || offset < 0 || offset > from[from.length - 1]!) throw new RangeError('source offset is out of bounds')
    let low = 0; let high = from.length - 1
    while (low <= high) {
      const middle = (low + high) >>> 1
      if (from[middle] === offset) return to[middle]!
      if (from[middle]! < offset) low = middle + 1
      else high = middle - 1
    }
    throw new RangeError('source offset splits a Unicode scalar')
  }
}
