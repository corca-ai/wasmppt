/** Package identity exposed without instantiating a Cloudflare Worker. */
export const packageName = '@corca-ai/wasmppt-worker' as const

/** Byte budgets are adapter configuration, not mutable process-global state. */
export interface WorkerMemoryBudget {
  readonly maxInputBytes: number
  readonly maxOutputChunkBytes: number
  readonly maxCachedPlanBytes: number
}
