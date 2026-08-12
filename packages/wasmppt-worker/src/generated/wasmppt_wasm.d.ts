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
     * Atomically apply a partial WPPD payload and return compact revision metadata.
     */
    apply_live_session_payload(handle: number, expected_revision: number, next_revision: number, payload: Uint8Array): Array<any>;
    /**
     * Report optional acceleration detected by the JavaScript adapter.
     *
     * The current baseline artifact intentionally reports scalar-only support.
     */
    capabilities(): EngineCapabilities;
    /**
     * Create a revision-zero live session from one prepared template and complete
     * initial generation data. The logical package is opened directly, without a
     * generated PPTX buffer.
     */
    create_live_session_payload(template_handle: number, payload: Uint8Array): number;
    /**
     * Text-only compatibility entry point returning a pull cursor handle.
     */
    generate_text(template_handle: number, ids: Array<any>, values: Array<any>): number;
    generation_done(generation_handle: number): boolean;
    generation_pull(generation_handle: number, maximum_bytes: number): Uint8Array;
    live_session_cache_telemetry(handle: number): Array<any>;
    live_session_resource(handle: number, revision: number, part_name: string): Uint8Array;
    live_session_resource_fingerprint(handle: number, revision: number, part_name: string): string;
    live_session_revision(handle: number): number;
    live_session_slide_count(handle: number): number;
    live_session_slide_fingerprint(handle: number, revision: number, slide_index: number): string;
    constructor();
    /**
     * Index a presentation once and retain its compressed package behind an opaque handle.
     */
    open_presentation(presentation: Uint8Array): number;
    /**
     * Compile an immutable template and return an opaque instance-local handle.
     */
    prepare(template: Uint8Array): number;
    /**
     * Compile with explicit stable v1 option tags.
     */
    prepare_with_options(template: Uint8Array, macro_policy: number, compatibility: number, compression: number, allow_visible_tokens: boolean): number;
    /**
     * Restore a previously compiled plan after verifying its source identity.
     */
    prepare_with_plan(template: Uint8Array, plan: Uint8Array): number;
    /**
     * Return compact binding tuples: id, kind, part, source, shape ID, shape name.
     */
    prepared_bindings(handle: number): Array<any>;
    /**
     * Return compact diagnostic tuples: code, binding ID, part, message.
     */
    prepared_diagnostics(handle: number): Array<any>;
    prepared_plan(handle: number): Uint8Array;
    prepared_weight(handle: number): bigint;
    /**
     * Read one display-list resource without eagerly decoding unrelated media.
     */
    presentation_resource(presentation_handle: number, part_name: string): Uint8Array;
    presentation_slide_count(handle: number): number;
    release_generation(handle: number): boolean;
    release_live_session(handle: number): boolean;
    release_presentation(handle: number): boolean;
    release_template(handle: number): boolean;
    resolve_live_session_slide(handle: number, revision: number, slide_index: number): Uint8Array;
    /**
     * Resolve exactly one requested slide to the compact display-list wire format.
     */
    resolve_presentation_slide(presentation_handle: number, slide_index: number): Uint8Array;
    /**
     * Generate from the versioned binary structured-injection payload.
     */
    start_generation_payload(template_handle: number, payload: Uint8Array): number;
    start_live_session_generation(handle: number, revision: number): number;
}

/**
 * Stable signature used to compare native and Wasm display-list structure.
 */
export function display_list_signature(presentation: Uint8Array, slide_index: number): string;

/**
 * Returns the engine package version embedded in the Wasm module.
 */
export function engine_version(): string;

/**
 * Resolve one slide to the compact backend-neutral display-list wire format.
 */
export function resolve_display_list(presentation: Uint8Array, slide_index: number): Uint8Array;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_enginecapabilities_free: (a: number, b: number) => void;
    readonly __wbg_wasmpptengine_free: (a: number, b: number) => void;
    readonly display_list_signature: (a: number, b: number, c: number, d: number) => void;
    readonly engine_version: (a: number) => void;
    readonly enginecapabilities_simd: (a: number) => number;
    readonly enginecapabilities_threads: (a: number) => number;
    readonly resolve_display_list: (a: number, b: number, c: number, d: number) => void;
    readonly wasmpptengine_apply_live_session_payload: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
    readonly wasmpptengine_capabilities: (a: number) => number;
    readonly wasmpptengine_create_live_session_payload: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly wasmpptengine_generate_text: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly wasmpptengine_generation_done: (a: number, b: number, c: number) => void;
    readonly wasmpptengine_generation_pull: (a: number, b: number, c: number, d: number) => void;
    readonly wasmpptengine_live_session_cache_telemetry: (a: number, b: number, c: number) => void;
    readonly wasmpptengine_live_session_resource: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly wasmpptengine_live_session_resource_fingerprint: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly wasmpptengine_live_session_revision: (a: number, b: number, c: number) => void;
    readonly wasmpptengine_live_session_slide_count: (a: number, b: number, c: number) => void;
    readonly wasmpptengine_live_session_slide_fingerprint: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly wasmpptengine_new: () => number;
    readonly wasmpptengine_open_presentation: (a: number, b: number, c: number, d: number) => void;
    readonly wasmpptengine_prepare: (a: number, b: number, c: number, d: number) => void;
    readonly wasmpptengine_prepare_with_options: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => void;
    readonly wasmpptengine_prepare_with_plan: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly wasmpptengine_prepared_bindings: (a: number, b: number, c: number) => void;
    readonly wasmpptengine_prepared_diagnostics: (a: number, b: number, c: number) => void;
    readonly wasmpptengine_prepared_plan: (a: number, b: number, c: number) => void;
    readonly wasmpptengine_prepared_weight: (a: number, b: number, c: number) => void;
    readonly wasmpptengine_presentation_resource: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly wasmpptengine_presentation_slide_count: (a: number, b: number, c: number) => void;
    readonly wasmpptengine_release_generation: (a: number, b: number) => number;
    readonly wasmpptengine_release_live_session: (a: number, b: number) => number;
    readonly wasmpptengine_release_presentation: (a: number, b: number) => number;
    readonly wasmpptengine_release_template: (a: number, b: number) => number;
    readonly wasmpptengine_resolve_live_session_slide: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly wasmpptengine_resolve_presentation_slide: (a: number, b: number, c: number, d: number) => void;
    readonly wasmpptengine_start_generation_payload: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly wasmpptengine_start_live_session_generation: (a: number, b: number, c: number, d: number) => void;
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
