import type { WorkerEngine } from '../protocol.js'
import type { DeckSemanticRole, DeckTemplateDescription } from './model.js'
import { ROLE_CODES } from './encode.js'
import type { DeckOperation, DeckOperationResult } from './transport.js'
import { decodeDeckDiagnostics, decodeDeckPageMetadata } from './metadata.js'

/** One dispatch shared by real browser Workers and the Node in-process adapter. */
export function executeDeckOperation(engine: WorkerEngine, operation: DeckOperation): DeckOperationResult {
  switch (operation.kind) {
    case 'configure': engine.configure_deck_budget(operation.maxRetainedBytes); return { kind: 'done' }
    case 'register-asset': return { kind: 'handle', handle: engine.register_deck_asset(new Uint8Array(operation.bytes)) }
    case 'register-font': return { kind: 'handle', handle: engine.register_deck_font(operation.family, 0, new Uint8Array(operation.bytes)) }
    case 'release-input': engine.release_deck_input(operation.handle); return { kind: 'done' }
    case 'describe-template': return describe(engine.describe_deck_template(operation.handle))
    case 'accounting': return { kind: 'accounting', bytes: engine.deck_accounted_bytes() }
    case 'output-start': return { kind: 'handle', handle: engine.start_deck_session_generation(operation.handle, operation.revision) }
    case 'output-pull': {
      if (!Number.isInteger(operation.chunkBytes) || operation.chunkBytes <= 0 || operation.chunkBytes > 1024 * 1024) {
        throw new RangeError('output chunk must contain at most 1 MiB')
      }
      const chunk = engine.generation_pull(operation.handle, operation.chunkBytes)
      return { kind: 'chunk', bytes: new Uint8Array(chunk).buffer, done: engine.generation_done(operation.handle) }
    }
    case 'output-release': engine.release_generation(operation.handle); return { kind: 'done' }
    case 'snapshot': {
      let handle: number
      try {
        handle = engine.create_deck_snapshot(operation.template, new Uint8Array(operation.spec),
          new Uint32Array(operation.assets), new Uint32Array(operation.fonts), operation.previous)
      } catch (error) {
        if (typeof error === 'object' && error !== null && 'diagnostics' in error && Array.isArray(error.diagnostics)) {
          return { kind: 'rejected', diagnostics: decodeDeckDiagnostics(error.diagnostics) }
        }
        throw error
      }
      try {
        const revision = engine.deck_session_revision(handle)
        const pages = Array.from({ length: engine.deck_session_slide_count(handle) }, (_, index) =>
          decodeDeckPageMetadata(engine.deck_session_slide_metadata(handle, revision, index)))
        const sources = engine.deck_snapshot_sources(handle, revision).map(value => {
          const [fragment, node] = list(value)
          return [string(fragment), string(node)] as const
        })
        return { kind: 'snapshot', handle, revision, pages, sources,
          diagnostics: decodeDeckDiagnostics(engine.deck_session_diagnostics(handle, revision)) }
      } catch (error) { engine.release_deck_session(handle); throw error }
    }
  }
}

function list(value: unknown): unknown[] {
  if (!Array.isArray(value)) throw new TypeError('invalid template inventory')
  return value
}
function string(value: unknown): string {
  if (typeof value !== 'string') throw new TypeError('invalid template string')
  return value
}
function number(value: unknown): number {
  if (typeof value !== 'number' || !Number.isSafeInteger(value)) throw new TypeError('invalid template coordinate')
  return value
}
function role(code: unknown): DeckSemanticRole {
  const entry = Object.entries(ROLE_CODES).find(([, value]) => value === code)
  if (entry === undefined) throw new TypeError('invalid template semantic role')
  return entry[0] as DeckSemanticRole
}
function describe(values: unknown[]): DeckOperationResult {
  const [width, height, rawLayouts, rawFonts, diagnostics] = values
  const layouts: DeckTemplateDescription['layouts'] = list(rawLayouts).map(value => {
    const [id, capability, matchingName, rawRegions] = list(value)
    if (capability !== 'title' && capability !== 'statement' && capability !== 'content-envelope') throw new TypeError('invalid template capability')
    return { id: string(id), capability, matchingName: string(matchingName),
      regions: list(rawRegions).map(rawRegion => {
        const [regionId, rawRole, rawFrame, accepts] = list(rawRegion)
        const [x, y, regionWidth, regionHeight] = list(rawFrame)
        return { id: string(regionId), role: string(rawRole), frame: { x: number(x), y: number(y), width: number(regionWidth), height: number(regionHeight) }, accepts: list(accepts).map(role) }
      }) }
  })
  return { kind: 'description', description: {
    pageSize: { width: number(width), height: number(height) }, layouts,
    requiredFonts: [...new Set(list(rawFonts).map(string))],
  }, diagnostics: decodeDeckDiagnostics(list(diagnostics)) }
}
