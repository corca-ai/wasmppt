export const INJECTION_SCHEMA_VERSION = 1 as const

export interface ImageCrop {
  readonly left: number
  readonly top: number
  readonly right: number
  readonly bottom: number
}

export interface ImageBinding {
  readonly bytes: Uint8Array
  readonly extension: string
  readonly contentType: string
  readonly crop?: ImageCrop
}

export interface ChartSeries {
  readonly name: string
  readonly values: readonly number[]
}

export interface ChartBinding {
  readonly categories: readonly string[]
  readonly series: readonly ChartSeries[]
}

export interface GenerationData {
  readonly text?: Readonly<Record<string, string>>
  readonly images?: Readonly<Record<string, ImageBinding>>
  readonly tables?: Readonly<Record<string, readonly Readonly<Record<string, string>>[]>>
  readonly slides?: Readonly<Record<string, number>>
  readonly charts?: Readonly<Record<string, ChartBinding>>
}

/** Encode one structured generation request without JSON or base64 on the Wasm boundary. */
export function encodeInjectionData(data: GenerationData = {}): ArrayBuffer {
  const writer = new BinaryWriter()
  writer.bytes(new Uint8Array([0x57, 0x50, 0x50, 0x44]))
  writer.u32(INJECTION_SCHEMA_VERSION)

  const text = sortedEntries(data.text)
  writer.count(text.length, 'text bindings')
  for (const [id, value] of text) {
    writer.string(id)
    writer.string(value)
  }

  const images = sortedEntries(data.images)
  writer.count(images.length, 'image bindings')
  for (const [id, image] of images) {
    if (!(image.bytes instanceof Uint8Array)) throw new TypeError(`image ${id} bytes must be Uint8Array`)
    writer.string(id)
    writer.string(image.extension)
    writer.string(image.contentType)
    if (image.crop === undefined) {
      writer.u8(0)
    } else {
      writer.u8(1)
      writer.i32(image.crop.left, `${id}.crop.left`)
      writer.i32(image.crop.top, `${id}.crop.top`)
      writer.i32(image.crop.right, `${id}.crop.right`)
      writer.i32(image.crop.bottom, `${id}.crop.bottom`)
    }
    writer.sizedBytes(image.bytes)
  }

  const tables = sortedEntries(data.tables)
  writer.count(tables.length, 'table bindings')
  for (const [id, rows] of tables) {
    writer.string(id)
    writer.count(rows.length, `${id} rows`)
    for (const row of rows) {
      const fields = sortedEntries(row)
      writer.count(fields.length, `${id} row fields`)
      for (const [field, value] of fields) {
        writer.string(field)
        writer.string(value)
      }
    }
  }

  const slides = sortedEntries(data.slides)
  writer.count(slides.length, 'slide copy bindings')
  for (const [partName, copies] of slides) {
    writer.string(partName)
    writer.u32(copies, `${partName} copies`)
  }

  const charts = sortedEntries(data.charts)
  writer.count(charts.length, 'chart bindings')
  for (const [partName, chart] of charts) {
    writer.string(partName)
    writer.count(chart.categories.length, `${partName} categories`)
    for (const category of chart.categories) writer.string(category)
    writer.count(chart.series.length, `${partName} series`)
    for (const series of chart.series) {
      writer.string(series.name)
      writer.count(series.values.length, `${partName} series values`)
      for (const value of series.values) writer.f64(value, `${partName} series value`)
    }
  }
  return writer.finish()
}

class BinaryWriter {
  readonly #chunks: Uint8Array[] = []
  #length = 0

  u8(value: number): void {
    this.bytes(Uint8Array.of(value))
  }

  u32(value: number, label = 'value'): void {
    if (!Number.isSafeInteger(value) || value < 0 || value > 0xffff_ffff) {
      throw new RangeError(`${label} must be an unsigned 32-bit integer`)
    }
    const bytes = new Uint8Array(4)
    new DataView(bytes.buffer).setUint32(0, value, true)
    this.bytes(bytes)
  }

  i32(value: number, label: string): void {
    if (!Number.isSafeInteger(value) || value < -0x8000_0000 || value > 0x7fff_ffff) {
      throw new RangeError(`${label} must be a signed 32-bit integer`)
    }
    const bytes = new Uint8Array(4)
    new DataView(bytes.buffer).setInt32(0, value, true)
    this.bytes(bytes)
  }

  f64(value: number, label: string): void {
    if (!Number.isFinite(value)) throw new RangeError(`${label} must be finite`)
    const bytes = new Uint8Array(8)
    new DataView(bytes.buffer).setFloat64(0, value, true)
    this.bytes(bytes)
  }

  count(value: number, label: string): void {
    this.u32(value, label)
  }

  string(value: string): void {
    if (typeof value !== 'string') throw new TypeError('injection strings must be strings')
    this.sizedBytes(new TextEncoder().encode(value))
  }

  sizedBytes(value: Uint8Array): void {
    this.u32(value.byteLength, 'byte length')
    this.bytes(value)
  }

  bytes(value: Uint8Array): void {
    if (this.#length + value.byteLength > 0xffff_ffff) {
      throw new RangeError('injection payload exceeds the Wasm 32-bit address space')
    }
    this.#chunks.push(value)
    this.#length += value.byteLength
  }

  finish(): ArrayBuffer {
    const output = new Uint8Array(this.#length)
    let offset = 0
    for (const chunk of this.#chunks) {
      output.set(chunk, offset)
      offset += chunk.byteLength
    }
    return output.buffer
  }
}

function sortedEntries<Value>(
  record: Readonly<Record<string, Value>> | undefined,
): [string, Value][] {
  return Object.entries(record ?? {}).sort(([left], [right]) => left.localeCompare(right))
}
