import { defineConfig } from 'vite';
import { resolve } from 'path';
import wasm from 'vite-plugin-wasm';
import { wasmExtractPlugin } from './plugins/wasmExtractPlugin';

export default defineConfig({
  plugins: [
    wasm(),
    wasmExtractPlugin({
      wasmFilename: 'polyglot_sql.wasm',
      wasmRelativePath: './polyglot_sql.wasm',
      extractWasm: false,
      injectNodeCompat: true,
    }),
  ],
  build: {
    lib: {
      entry: resolve(__dirname, 'src/index.ts'),
      name: 'PolyglotSQL',
      formats: ['es'],
      fileName: () => 'index.node.js',
    },
    rollupOptions: {
      external: ['node:fs', 'node:url'],
      output: {
        exports: 'named',
      },
    },
    target: 'node22',
    sourcemap: false,
    minify: false,
    emptyOutDir: false,
  },
  assetsInclude: ['**/*.wasm'],
  optimizeDeps: {
    exclude: ['./wasm/polyglot_sql_wasm.js'],
  },
});
