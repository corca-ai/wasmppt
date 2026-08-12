/* tslint:disable */
/* eslint-disable */

/**
 * Runtime-independent capabilities. Correctness always uses the scalar path.
 */
export class EngineCapabilities {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    readonly simd: boolean;
    readonly threads: boolean;
}

/**
 * Instance-local handle table. No request or document state is process-global.
 */
export class WasmpptEngine {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Report optional acceleration detected by the JavaScript adapter.
     *
     * The current baseline artifact intentionally reports scalar-only support.
     */
    capabilities(): EngineCapabilities;
    /**
     * Generate into an engine-owned output buffer and return an opaque handle.
     * Hosts drain that buffer in bounded transferable chunks.
     */
    generate_text(template_handle: number, ids: Array<any>, values: Array<any>): number;
    constructor();
    /**
     * Copy one bounded chunk into a JavaScript `Uint8Array`.
     */
    output_chunk(output_handle: number, offset: number, length: number): Uint8Array;
    output_len(output_handle: number): number;
    /**
     * Compile an immutable template and return an opaque instance-local handle.
     */
    prepare(template: Uint8Array): number;
    prepared_weight(handle: number): bigint;
    release_output(handle: number): boolean;
    release_template(handle: number): boolean;
}

/**
 * Returns the engine package version embedded in the Wasm module.
 */
export function engine_version(): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_enginecapabilities_free: (a: number, b: number) => void;
    readonly __wbg_wasmpptengine_free: (a: number, b: number) => void;
    readonly engine_version: (a: number) => void;
    readonly enginecapabilities_simd: (a: number) => number;
    readonly enginecapabilities_threads: (a: number) => number;
    readonly wasmpptengine_capabilities: (a: number) => number;
    readonly wasmpptengine_generate_text: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly wasmpptengine_new: () => number;
    readonly wasmpptengine_output_chunk: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly wasmpptengine_output_len: (a: number, b: number, c: number) => void;
    readonly wasmpptengine_prepare: (a: number, b: number, c: number, d: number) => void;
    readonly wasmpptengine_prepared_weight: (a: number, b: number, c: number) => void;
    readonly wasmpptengine_release_output: (a: number, b: number) => number;
    readonly wasmpptengine_release_template: (a: number, b: number) => number;
    readonly __wbindgen_export: (a: number, b: number) => number;
    readonly __wbindgen_export2: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
    readonly __wbindgen_export3: (a: number, b: number, c: number) => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
