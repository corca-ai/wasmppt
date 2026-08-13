import type { WasmpptErrorEnvelope } from './error.js'

export const WORKER_PROTOCOL_VERSION = 6 as const
export const LEGACY_WORKER_PROTOCOL_VERSION = 5 as const

export type TextBindings = Readonly<Record<string, string>>

export interface TemplateCompilerOptions {
  readonly macroPolicy?: 'strip' | 'reject'
  readonly allowVisibleTokens?: boolean
}

export interface TemplateBinding {
  readonly id: string
  readonly kind: 'text' | 'image' | 'chart'
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

export interface DeckSessionUpdate {
  readonly revision: number
  readonly slideCount: number
  readonly presentableSlides: readonly number[]
  readonly invalidatedSlides: readonly number[]
  readonly invalidatedLogicalSlideIds: readonly string[]
  readonly removedPageIds: readonly string[]
  readonly changedParts: readonly string[]
  readonly reusedPages: number
  readonly fullFallback: boolean
  readonly overlay: {
    readonly logicalParts: number
    readonly materializedParts: number
    readonly materializedBytes: number
    readonly reusedSourceBytes: number
    readonly removedParts: number
  }
}

export interface DeckPageMetadata {
  readonly pageId: string
  readonly logicalSlideId: string
  readonly hidden: boolean
  readonly continuationOrdinal: number
  readonly continuationTotal: number
  readonly continuationLabel?: string
}

export type WorkerRequest =
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'prepare-deck-template'
      readonly template: ArrayBuffer
      readonly plan?: ArrayBuffer
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'create-deck-session'
      readonly templateHandle: number
      readonly spec: ArrayBuffer
      readonly plan?: ArrayBuffer
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'update-deck-session'
      readonly sessionHandle: number
      readonly expectedRevision: number
      readonly nextRevision: number
      readonly spec: ArrayBuffer
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'generate-deck-session'
      readonly sessionHandle: number
      readonly revision: number
      readonly chunkBytes: number
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'resolve-deck-slide'
      readonly sessionHandle: number
      readonly revision: number
      readonly slideIndex: number
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'deck-session-resource' | 'deck-session-resource-fingerprint'
      readonly sessionHandle: number
      readonly revision: number
      readonly partName: string
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'deck-session-cache-telemetry' | 'release-deck-session'
      readonly sessionHandle: number
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'release-deck-template'
      readonly templateHandle: number
    }
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
      readonly type: 'create-live-session'
      readonly templateHandle: number
      readonly payload: ArrayBuffer
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'apply-live-delta'
      readonly sessionHandle: number
      readonly expectedRevision: number
      readonly nextRevision: number
      readonly payload: ArrayBuffer
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'generate-live-session'
      readonly sessionHandle: number
      readonly revision: number
      readonly chunkBytes: number
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'resolve-live-slide'
      readonly sessionHandle: number
      readonly revision: number
      readonly slideIndex: number
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'live-session-resource'
      readonly sessionHandle: number
      readonly revision: number
      readonly partName: string
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'live-session-resource-fingerprint'
      readonly sessionHandle: number
      readonly revision: number
      readonly partName: string
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'live-session-metafile-svg'
      readonly sessionHandle: number
      readonly revision: number
      readonly partName: string
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'live-session-cache-telemetry'
      readonly sessionHandle: number
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'release-live-session'
      readonly sessionHandle: number
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
      readonly type: 'presentation-metafile-svg'
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
      readonly type: 'deck-template-prepared'
      readonly templateHandle: number
      readonly cacheable: boolean
      readonly plan: ArrayBuffer
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'deck-session-created'
      readonly sessionHandle: number
      readonly revision: number
      readonly slideCount: number
      readonly presentableSlides: readonly number[]
      readonly plan: ArrayBuffer
    }
  | ({
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'deck-session-updated'
      readonly sessionHandle: number
    } & DeckSessionUpdate)
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'deck-slide-resolved'
      readonly sessionHandle: number
      readonly revision: number
      readonly slideIndex: number
      readonly fingerprint: string
      readonly page: DeckPageMetadata
      readonly displayList: ArrayBuffer
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'deck-session-resource'
      readonly sessionHandle: number
      readonly revision: number
      readonly partName: string
      readonly fingerprint: string
      readonly bytes: ArrayBuffer
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'deck-session-resource-fingerprint'
      readonly sessionHandle: number
      readonly revision: number
      readonly partName: string
      readonly fingerprint: string
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'deck-session-cache-telemetry'
      readonly residentBytes: number
      readonly peakBytes: number
      readonly entries: number
      readonly hits: number
      readonly misses: number
      readonly evictions: number
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'deck-session-released' | 'deck-template-released'
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'progress'
      readonly phase: 'prepare' | 'session' | 'delta' | 'generate' | 'stream' | 'open' | 'resolve'
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
      readonly type: 'live-session-created'
      readonly sessionHandle: number
      readonly revision: number
      readonly slideCount: number
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'live-session-updated'
      readonly sessionHandle: number
      readonly revision: number
      readonly graphChanged: boolean
      readonly fullFallback: boolean
      readonly invalidationReason: 'topology' | 'dependency' | 'none'
      readonly slideCount: number
      readonly invalidatedSlides: readonly number[]
      readonly changedBindings: readonly string[]
      readonly changedParts: readonly string[]
      readonly overlay: {
        readonly reusedMaterializedParts: number
        readonly logicalParts: number
        readonly materializedParts: number
        readonly materializedBytes: number
        readonly reusedSourceBytes: number
        readonly removedParts: number
      }
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
      readonly error: WasmpptErrorEnvelope
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
      readonly type: 'live-slide-resolved'
      readonly sessionHandle: number
      readonly revision: number
      readonly slideIndex: number
      readonly fingerprint: string
      readonly displayList: ArrayBuffer
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'live-session-resource'
      readonly sessionHandle: number
      readonly revision: number
      readonly partName: string
      readonly fingerprint: string
      readonly bytes: ArrayBuffer
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'live-session-resource-fingerprint'
      readonly sessionHandle: number
      readonly revision: number
      readonly partName: string
      readonly fingerprint: string
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'live-session-metafile-svg'
      readonly sessionHandle: number
      readonly revision: number
      readonly partName: string
      readonly fingerprint: string
      readonly bytes: ArrayBuffer
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'live-session-cache-telemetry'
      readonly residentBytes: number
      readonly peakBytes: number
      readonly entries: number
      readonly hits: number
      readonly misses: number
      readonly evictions: number
    }
  | {
      readonly version: typeof WORKER_PROTOCOL_VERSION
      readonly id: number
      readonly type: 'live-session-released'
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
      readonly type: 'presentation-metafile-svg'
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
      readonly error: WasmpptErrorEnvelope
      /** @deprecated Read `error` machine fields instead. */
      readonly name: string
      /** @deprecated Read `error.message`; messages are informational. */
      readonly message: string
    }

export interface WorkerEngine {
  prepare_deck_template(template: Uint8Array): number
  prepare_deck_template_with_plan(template: Uint8Array, plan: Uint8Array): number
  deck_template_plan(handle: number): Uint8Array
  deck_template_cacheable(handle: number): boolean
  create_deck_session(templateHandle: number, spec: Uint8Array): number
  create_deck_session_with_plan(templateHandle: number, spec: Uint8Array, plan: Uint8Array): number
  deck_session_revision(handle: number): number
  deck_session_plan(handle: number, revision: number): Uint8Array
  deck_session_slide_count(handle: number): number
  deck_session_presentable_slides(handle: number): unknown[]
  deck_session_slide_metadata(handle: number, revision: number, slideIndex: number): unknown[]
  apply_deck_session_spec(
    handle: number,
    expectedRevision: number,
    nextRevision: number,
    spec: Uint8Array,
  ): unknown[]
  resolve_deck_session_slide(handle: number, revision: number, slideIndex: number): Uint8Array
  deck_session_slide_fingerprint(handle: number, revision: number, slideIndex: number): string
  deck_session_resource(handle: number, revision: number, partName: string): Uint8Array
  deck_session_resource_fingerprint(handle: number, revision: number, partName: string): string
  start_deck_session_generation(handle: number, revision: number): number
  deck_session_cache_telemetry(handle: number): unknown[]
  release_deck_template(handle: number): boolean
  release_deck_session(handle: number): boolean
  prepare(template: Uint8Array): number
  prepare_with_options(
    template: Uint8Array,
    macroPolicy: number,
    allowVisibleTokens: boolean,
  ): number
  prepare_with_plan(template: Uint8Array, plan: Uint8Array): number
  prepared_weight(handle: number): bigint
  prepared_plan(handle: number): Uint8Array
  prepared_bindings(handle: number): unknown[]
  prepared_diagnostics(handle: number): unknown[]
  start_generation_payload(handle: number, payload: Uint8Array): number
  create_live_session_payload(templateHandle: number, payload: Uint8Array): number
  live_session_revision(handle: number): number
  live_session_slide_count(handle: number): number
  apply_live_session_payload(
    handle: number,
    expectedRevision: number,
    nextRevision: number,
    payload: Uint8Array,
  ): unknown[]
  resolve_live_session_slide(handle: number, revision: number, slideIndex: number): Uint8Array
  live_session_slide_fingerprint(handle: number, revision: number, slideIndex: number): string
  live_session_resource(handle: number, revision: number, partName: string): Uint8Array
  live_session_resource_fingerprint(handle: number, revision: number, partName: string): string
  start_live_session_generation(handle: number, revision: number): number
  live_session_cache_telemetry(handle: number): unknown[]
  generation_pull(handle: number, maximumBytes: number): Uint8Array
  generation_done(handle: number): boolean
  release_template(handle: number): boolean
  release_generation(handle: number): boolean
  open_presentation(presentation: Uint8Array): number
  presentation_slide_count(handle: number): number
  resolve_presentation_slide(handle: number, slideIndex: number): Uint8Array
  presentation_resource(handle: number, partName: string): Uint8Array
  release_presentation(handle: number): boolean
  release_live_session(handle: number): boolean
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
