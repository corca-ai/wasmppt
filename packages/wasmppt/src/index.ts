/** Package identity exposed without loading the Wasm engine. */
export const packageName = '@corca-ai/wasmppt' as const

/** Browser-owned locations for separately emitted engine and worker assets. */
export interface BrowserEngineAssets {
  readonly wasmUrl: string | URL
  readonly workerUrl: string | URL
}

/** A transferable binary input accepted by the future browser adapter. */
export type BrowserBinaryInput = ArrayBuffer
