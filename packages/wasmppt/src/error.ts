export const ERROR_ENVELOPE_VERSION = 1 as const

export type WasmpptErrorDomain =
  | 'package'
  | 'xml'
  | 'template'
  | 'payload'
  | 'generation'
  | 'layout'
  | 'runtime'

/** Stable machine fields. `message` is informational and may change without a major release. */
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

export class WasmpptError extends Error {
  readonly envelope: WasmpptErrorEnvelope

  constructor(envelope: WasmpptErrorEnvelope, name = 'WasmpptError') {
    super(envelope.message)
    this.name = name
    this.envelope = Object.freeze({ ...envelope })
  }

  get domain(): WasmpptErrorDomain {
    return this.envelope.domain
  }

  get code(): string {
    return this.envelope.code
  }
}

export function normalizeWasmpptError(
  error: unknown,
  fallback: Pick<WasmpptErrorEnvelope, 'domain' | 'code'> = {
    domain: 'runtime',
    code: 'internal',
  },
): { readonly envelope: WasmpptErrorEnvelope; readonly name: string } {
  const candidate = error instanceof Error
    ? error as Error & { readonly wasmppt?: unknown; readonly envelope?: unknown }
    : undefined
  const structured = candidate?.wasmppt ?? candidate?.envelope
  if (isWasmpptErrorEnvelope(structured)) {
    return { envelope: Object.freeze({ ...structured }), name: candidate?.name ?? 'WasmpptError' }
  }
  const message = error instanceof Error ? error.message : String(error)
  return {
    envelope: Object.freeze({
      version: ERROR_ENVELOPE_VERSION,
      domain: fallback.domain,
      code: fallback.code,
      message,
    }),
    name: error instanceof Error ? error.name : 'Error',
  }
}

export function isWasmpptErrorEnvelope(value: unknown): value is WasmpptErrorEnvelope {
  if (typeof value !== 'object' || value === null) return false
  const candidate = value as Partial<WasmpptErrorEnvelope>
  return candidate.version === ERROR_ENVELOPE_VERSION &&
    isDomain(candidate.domain) &&
    typeof candidate.code === 'string' && candidate.code.length > 0 &&
    typeof candidate.message === 'string' &&
    optionalString(candidate.partName) && optionalInteger(candidate.offset) &&
    optionalString(candidate.bindingId) && optionalInteger(candidate.slideIndex) &&
    optionalString(candidate.causeCode)
}

export function cancellationEnvelope(message = 'wasmppt operation was cancelled'): WasmpptErrorEnvelope {
  return Object.freeze({
    version: ERROR_ENVELOPE_VERSION,
    domain: 'runtime',
    code: 'cancelled',
    message,
  })
}

function isDomain(value: unknown): value is WasmpptErrorDomain {
  return value === 'package' || value === 'xml' || value === 'template' || value === 'payload' ||
    value === 'generation' || value === 'layout' || value === 'runtime'
}

function optionalString(value: unknown): boolean {
  return value === undefined || typeof value === 'string'
}

function optionalInteger(value: unknown): boolean {
  return value === undefined || (Number.isSafeInteger(value) && (value as number) >= 0)
}
