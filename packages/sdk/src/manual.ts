import autoDefault from './index';
import {
  initWasmModule,
  isWasmModuleInitialized,
  type ManualWasmInitOptions,
} from './wasm/manual-bindings';

export * from './index';
export type { ManualWasmInitOptions };

export async function init(options: ManualWasmInitOptions): Promise<void> {
  await initWasmModule(options);
}

export function isInitialized(): boolean {
  return isWasmModuleInitialized();
}

export default {
  ...autoDefault,
  init,
  isInitialized,
};
