export const WORKER_PROTOCOL_VERSION = 2 as const

export type TextBindings = Readonly<Record<string, string>>

export interface TemplateCompilerOptions {
  readonly macroPolicy?: 'strip' | 'reject' | 'preserve-as-pptm'
  readonly compatibility?: 'powerpoint-2016' | 'microsoft-365'
  readonly compression?: 'balanced-deflate-6' | 'store-media'
  readonly allowVisibleTokens?: boolean
}

export interface TemplateBinding {
  readonly id: string
  readonly kind: 'text' | 'image'
  readonly partName: string
  readonly source: 'visible-token' | 'shape-metadata' | 'manifest'
  readonly shapeId?: number
  readonly shapeName?: string
}

export interface TemplateDiagnostic {
  readonly code:
    | 'missing-target'
    | 'duplicate-id'
    | 'ambiguous-target'
    | 'unsupported-kind'
    | 'invalid-manifest'
    | 'invalid-slide'
    | 'unknown'
  readonly bindingId?: string
  readonly partName?: string
  readonly message: string
}

export type WorkerRequest =
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'prepare'
      readonly template: ArrayBuffer
      readonly options: TemplateCompilerOptions
      readonly plan?: ArrayBuffer
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'generate'
      readonly templateHandle: number
      readonly payload: ArrayBuffer
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
      readonly type: 'presentation-resource'
      readonly presentationHandle: number
      readonly partName: string
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
      readonly plan: ArrayBuffer
      readonly bindings: readonly TemplateBinding[]
      readonly diagnostics: readonly TemplateDiagnostic[]
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
      readonly type: 'presentation-resource'
      readonly partName: string
      readonly bytes: ArrayBuffer
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
  prepare_with_options(
    template: Uint8Array,
    macroPolicy: number,
    compatibility: number,
    compression: number,
    allowVisibleTokens: boolean,
  ): number
  prepare_with_plan(template: Uint8Array, plan: Uint8Array): number
  prepared_weight(handle: number): bigint
  prepared_plan(handle: number): Uint8Array
  prepared_bindings(handle: number): unknown[]
  prepared_diagnostics(handle: number): unknown[]
  start_generation_payload(handle: number, payload: Uint8Array): number
  generation_pull(handle: number, maximumBytes: number): Uint8Array
  generation_done(handle: number): boolean
  release_template(handle: number): boolean
  release_generation(handle: number): boolean
  open_presentation(presentation: Uint8Array): number
  presentation_slide_count(handle: number): number
  resolve_presentation_slide(handle: number, slideIndex: number): Uint8Array
  presentation_resource(handle: number, partName: string): Uint8Array
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
