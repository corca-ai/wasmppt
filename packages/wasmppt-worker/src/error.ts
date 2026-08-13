export const ERROR_ENVELOPE_VERSION = 1 as const

export type WasmpptErrorDomain =
  | 'package'
  | 'xml'
  | 'template'
  | 'payload'
  | 'generation'
  | 'layout'
  | 'runtime'

export interface WasmpptErrorEnvelope {
  readonly version: typeof ERROR_ENVELOPE_VERSION
  readonly domain: WasmpptErrorDomain
  readonly code: string
  readonly message: string
  readonly partName?: string
  readonly offset?: number
  readonly bindingId?: string
  readonly slideIndex?: number
  readonly causeCode?: string
}

export function normalizeWasmpptError(error: unknown): WasmpptErrorEnvelope {
  const candidate = error instanceof Error
    ? error as Error & { readonly wasmppt?: unknown; readonly envelope?: unknown }
    : undefined
  const structured = candidate?.wasmppt ?? candidate?.envelope
  if (isWasmpptErrorEnvelope(structured)) return Object.freeze({ ...structured })
  if (candidate?.name === 'AbortError') {
    return errorEnvelope('runtime', 'cancelled', candidate.message)
  }
  return Object.freeze({
    version: ERROR_ENVELOPE_VERSION,
    domain: 'runtime',
    code: 'internal',
    message: error instanceof Error ? error.message : String(error),
  })
}

export function errorEnvelope(
  domain: WasmpptErrorDomain,
  code: string,
  message: string,
): WasmpptErrorEnvelope {
  return Object.freeze({ version: ERROR_ENVELOPE_VERSION, domain, code, message })
}

function isWasmpptErrorEnvelope(value: unknown): value is WasmpptErrorEnvelope {
  if (typeof value !== 'object' || value === null) return false
  const candidate = value as Partial<WasmpptErrorEnvelope>
  return candidate.version === ERROR_ENVELOPE_VERSION && isDomain(candidate.domain) &&
    typeof candidate.code === 'string' && candidate.code.length > 0 &&
    typeof candidate.message === 'string'
}

function isDomain(value: unknown): value is WasmpptErrorDomain {
  return value === 'package' || value === 'xml' || value === 'template' || value === 'payload' ||
    value === 'generation' || value === 'layout' || value === 'runtime'
}
