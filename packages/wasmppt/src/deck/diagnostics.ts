import { WasmpptError } from '../error.js'
import type { DeckDiagnostic as WireDiagnostic } from '../protocol.js'
import type { DeckDiagnostic } from './model.js'
import type { SourceIdentity } from './encode.js'

function category(code: string): DeckDiagnostic['category'] {
  if (code === 'plan-font-risk') return 'font'
  if (code.startsWith('template-')) return 'template'
  if (code === 'missing-resource') return 'resource'
  if (['plan-source-loss', 'plan-source-duplication', 'plan-source-reordered', 'plan-target-drift',
    'plan-invalid-geometry', 'plan-invalid-continuation', 'plan-unstable-id'].includes(code)) return 'engine'
  return code.startsWith('plan-') ? 'layout' : 'content'
}

export function diagnostics(
  values: readonly WireDiagnostic[],
  identities: ReadonlyMap<string, SourceIdentity> = new Map(),
): readonly DeckDiagnostic[] {
  return Object.freeze(values.map(value => {
    const code = value.name ?? `unknown-${value.code}`
    const identity = value.nodeId === undefined ? undefined : identities.get(value.nodeId)
    const source = identity?.source ?? (value.source?.source === 'wasmppt:untracked' ? undefined : value.source)
    return Object.freeze({ code, category: category(code), severity: value.severity, message: value.message,
      ...(identity === undefined ? {} : { nodeKey: identity.key }),
      ...(source === undefined ? {} : { source: Object.freeze({ ...source }) }),
      ...(value.pageId === undefined ? {} : { pageId: value.pageId }) })
  }))
}

/** Validation failures are values; transport, disposal and resource-budget failures remain errors. */
export function inputFailure(error: unknown): readonly DeckDiagnostic[] {
  if (error instanceof WasmpptError && error.domain !== 'runtime') {
    return [Object.freeze({ code: error.code, category: error.domain === 'template' ? 'template' : 'content',
      severity: 'error', message: error.message })]
  }
  throw error
}

export function invalidInput(error: Error): readonly DeckDiagnostic[] {
  return Object.freeze([Object.freeze({ code: 'invalid-deck-input', category: 'content', severity: 'error', message: error.message })])
}
