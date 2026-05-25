import { defineConfig } from 'vite';
import type { Plugin } from 'vite';
import { resolve } from 'path';

function manualWasmAlias(): Plugin {
  return {
    name: 'polyglot-manual-wasm-alias',
    enforce: 'pre' as const,
    resolveId(source: string) {
      if (source.endsWith('wasm/polyglot_sql_wasm.js')) {
        return resolve(__dirname, 'src/wasm/manual-bindings.ts');
      }
      return null;
    },
    transform(code: string, id: string) {
      if (!id.endsWith('/wasm-web/polyglot_sql_wasm.js')) {
        return null;
      }

      return code.replace(
        "module_or_path = new URL('polyglot_sql_wasm_bg.wasm', import.meta.url);",
        "throw new Error('Manual WASM initialization requires init({ wasmUrl }).');",
      );
    },
  };
}

function manualDts(): Plugin {
  return {
    name: 'polyglot-manual-dts',
    generateBundle() {
      this.emitFile({
        type: 'asset',
        fileName: 'manual.d.ts',
        source: [
          "export * from './index';",
          '',
          'export interface ManualWasmInitOptions {',
          '  wasmUrl: string | URL;',
          '}',
          '',
          'export declare function init(options: ManualWasmInitOptions): Promise<void>;',
          'export declare function isInitialized(): boolean;',
          '',
          "declare const _default: typeof import('./index').default & {",
          '  init: typeof init;',
          '  isInitialized: typeof isInitialized;',
          '};',
          '',
          'export default _default;',
          '',
        ].join('\n'),
      });
    },
  };
}

export default defineConfig({
  plugins: [
    manualWasmAlias(),
    manualDts(),
  ],
  build: {
    lib: {
      entry: resolve(__dirname, 'src/manual.ts'),
      name: 'PolyglotSQLManual',
      formats: ['es'],
      fileName: () => 'manual.js',
    },
    rollupOptions: {
      output: {
        exports: 'named',
      },
    },
    target: 'esnext',
    sourcemap: false,
    minify: false,
    emptyOutDir: false,
  },
});
