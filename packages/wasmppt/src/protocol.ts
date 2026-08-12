export const WORKER_PROTOCOL_VERSION = 1 as const

export type TextBindings = Readonly<Record<string, string>>

export type WorkerRequest =
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'prepare'
      readonly template: ArrayBuffer
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'generate'
      readonly templateHandle: number
      readonly text: TextBindings
      readonly chunkBytes: number
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'release'
      readonly templateHandle: number
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'cancel'
      readonly targetId: number
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'open-presentation'
      readonly presentation: ArrayBuffer
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'resolve-slide'
      readonly presentationHandle: number
      readonly slideIndex: number
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'release-presentation'
      readonly presentationHandle: number
    }

export type WorkerResponse =
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'progress'
      readonly phase: 'prepare' | 'generate' | 'stream' | 'open' | 'resolve'
      readonly completed: number
      readonly total: number
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'prepared'
      readonly templateHandle: number
      readonly residentBytes: number
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'chunk'
      readonly offset: number
      readonly bytes: ArrayBuffer
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'generated'
      readonly byteLength: number
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'released'
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'cancelled'
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'presentation-opened'
      readonly presentationHandle: number
      readonly slideCount: number
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'slide-resolved'
      readonly slideIndex: number
      readonly displayList: ArrayBuffer
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'presentation-released'
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'error'
      readonly name: string
      readonly message: string
    }

export interface WorkerEngine {
  prepare(template: Uint8Array): number
  prepared_weight(handle: number): bigint
  generate_text(handle: number, ids: readonly string[], values: readonly string[]): number
  output_len(handle: number): number
  output_chunk(handle: number, offset: number, length: number): Uint8Array
  release_template(handle: number): boolean
  release_output(handle: number): boolean
  open_presentation(presentation: Uint8Array): number
  presentation_slide_count(handle: number): number
  resolve_presentation_slide(handle: number, slideIndex: number): Uint8Array
  release_presentation(handle: number): boolean
}

export interface RuntimeCapabilities {
  readonly simd: boolean
  readonly threads: boolean
}

/** Optional acceleration probes; the scalar engine remains the correctness baseline. */
export function detectRuntimeCapabilities(): RuntimeCapabilities {
  const simdProbe = new Uint8Array([
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x05, 0x01, 0x60, 0x00, 0x01,
    0x7b, 0x03, 0x02, 0x01, 0x00, 0x0a, 0x0a, 0x01, 0x08, 0x00, 0x41, 0x00, 0xfd, 0x0f,
    0x0b,
  ])
  let simd = false
  try {
    simd = WebAssembly.validate(simdProbe)
  } catch {
    simd = false
  }
  const threads =
    typeof SharedArrayBuffer !== 'undefined' &&
    typeof crossOriginIsolated === 'boolean' &&
    crossOriginIsolated
  return { simd, threads }
}
