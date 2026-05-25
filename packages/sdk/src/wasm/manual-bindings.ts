import type * as wasmModule from '../../wasm-web/polyglot_sql_wasm.js';
import initWasm from '../../wasm-web/polyglot_sql_wasm.js';

export * from '../../wasm-web/polyglot_sql_wasm.js';

type WasmInitInput = string | URL;

let initialized = false;
let initPromise: Promise<void> | undefined;

export interface ManualWasmInitOptions {
  wasmUrl: WasmInitInput;
}

export async function initWasmModule(
  options: ManualWasmInitOptions,
): Promise<void> {
  if (initialized) return;
  if (initPromise) return initPromise;

  initPromise = initWasm({ module_or_path: options.wasmUrl })
    .then(() => {
      initialized = true;
    })
    .catch((error: unknown) => {
      initPromise = undefined;
      throw error;
    });

  return initPromise;
}

export function isWasmModuleInitialized(): boolean {
  return initialized;
}

export type ManualWasmBindings = typeof wasmModule;
