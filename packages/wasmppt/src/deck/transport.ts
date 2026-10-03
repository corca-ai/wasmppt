import type { DeckDiagnostic, DeckPageMetadata } from '../protocol.js'
import type { DeckTemplateDescription } from './model.js'

/** SDK transport, deliberately separate from the authoring data model. */
export type DeckOperation =
  | { readonly kind: 'configure'; readonly maxRetainedBytes: number }
  | { readonly kind: 'register-asset'; readonly bytes: ArrayBuffer }
  | { readonly kind: 'register-font'; readonly family: string; readonly bytes: ArrayBuffer }
  | { readonly kind: 'release-input'; readonly handle: number }
  | { readonly kind: 'describe-template'; readonly handle: number }
  | { readonly kind: 'snapshot'; readonly template: number; readonly spec: ArrayBuffer; readonly assets: readonly number[]; readonly fonts: readonly number[]; readonly previous: number }
  | { readonly kind: 'output-start'; readonly handle: number; readonly revision: number }
  | { readonly kind: 'output-pull'; readonly handle: number; readonly chunkBytes: number }
  | { readonly kind: 'output-release'; readonly handle: number }
  | { readonly kind: 'accounting' }

export type DeckOperationResult =
  | { readonly kind: 'done' }
  | { readonly kind: 'handle'; readonly handle: number }
  | { readonly kind: 'accounting'; readonly bytes: number }
  | { readonly kind: 'description'; readonly description: Omit<DeckTemplateDescription, 'diagnostics'>; readonly diagnostics: readonly DeckDiagnostic[] }
  | { readonly kind: 'rejected'; readonly diagnostics: readonly DeckDiagnostic[] }
  | { readonly kind: 'chunk'; readonly bytes: ArrayBuffer; readonly done: boolean }
  | {
      readonly sources: readonly (readonly [string, string])[]
      readonly kind: 'snapshot'
      readonly handle: number
      readonly revision: number
      readonly pages: readonly DeckPageMetadata[]
      readonly diagnostics: readonly DeckDiagnostic[]
    }
