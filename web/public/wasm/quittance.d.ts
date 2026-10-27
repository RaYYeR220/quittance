/* tslint:disable */
/* eslint-disable */

/**
 * What a disclosure claims, before anything is checked.
 */
export function inspect(text: string): string;

/**
 * Builds the verifying key ahead of time (it takes a moment); optional.
 */
export function prepare(): void;

/**
 * A deliberately broken copy of a disclosure, for showing what refusals look
 * like: `output-key`, `proof`, `signature`, or `challenge` (rewrites the
 * challenge text without re-signing).
 */
export function tamper(text: string, how: string, new_challenge?: string | null): string;

/**
 * Verifies `text` against the transaction `raw_tx_hex` mined at `mined_height`
 * (`undefined` if the chain has it only in the mempool or not at all). An
 * empty `raw_tx_hex` uses the transaction attached to the disclosure.
 */
export function verify(text: string, raw_tx_hex: string, mined_height: number | null | undefined, expected_challenge: string | null | undefined, network: string | null | undefined, now_unix: number): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly inspect: (a: number, b: number) => [number, number, number, number];
    readonly prepare: () => void;
    readonly tamper: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number, number];
    readonly verify: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number) => [number, number, number, number];
    readonly rustsecp256k1_v0_14_default_error_callback_fn: (a: number, b: number) => void;
    readonly rustsecp256k1_v0_14_default_illegal_callback_fn: (a: number, b: number) => void;
    readonly rustsecp256k1_v0_14_context_destroy: (a: number) => void;
    readonly rustsecp256k1_v0_14_context_create: (a: number) => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_start: () => void;
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
