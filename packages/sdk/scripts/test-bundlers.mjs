import * as esbuild from 'esbuild';
import { mkdtempSync, readFileSync, readdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createRequire } from 'node:module';

const root = new URL('..', import.meta.url);
const dist = new URL('dist/', root);
const resolveDir = new URL('.', root).pathname;

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

function readDist(name) {
  return readFileSync(new URL(name, dist), 'utf8');
}

function readFirstJs(outdir) {
  const jsFile = readdirSync(outdir).find((file) => file.endsWith('.js'));
  assert(jsFile, `expected a JS bundle in ${outdir}`);
  return readFileSync(join(outdir, jsFile), 'utf8');
}

function assertDistFiles() {
  for (const file of [
    'index.js',
    'index.node.js',
    'index.cjs',
    'manual.js',
    'manual.d.ts',
    'polyglot_sql.wasm',
    'polyglot_sql.wasm.d.ts',
    'cdn/polyglot.esm.js',
  ]) {
    readFileSync(new URL(file, dist));
  }
}

async function testDefaultBrowserBundle() {
  const outdir = mkdtempSync(join(tmpdir(), 'polyglot-esbuild-default-'));
  await esbuild.build({
    stdin: {
      contents: [
        "import { getVersion } from '@polyglot-sql/sdk';",
        'console.log(getVersion());',
      ].join('\n'),
      sourcefile: 'default-entry.js',
      resolveDir,
    },
    bundle: true,
    format: 'esm',
    platform: 'browser',
    target: 'es2022',
    outdir,
    write: true,
  });

  const bundled = readFirstJs(outdir);
  assert(!bundled.includes('node:fs'), 'browser bundle must not contain node:fs');
  assert(!bundled.includes('node:url'), 'browser bundle must not contain node:url');
  assert(!bundled.includes('readFileSync'), 'browser bundle must not contain fs fallback code');
}

async function testManualEsbuildBundle() {
  const outdir = mkdtempSync(join(tmpdir(), 'polyglot-esbuild-manual-'));
  await esbuild.build({
    stdin: {
      contents: [
        "import wasmUrl from '@polyglot-sql/sdk/polyglot_sql.wasm';",
        "import { init, getVersion, transpile, Dialect } from '@polyglot-sql/sdk/manual';",
        'await init({ wasmUrl });',
        "const result = transpile('SELECT IFNULL(a, b)', Dialect.MySQL, Dialect.PostgreSQL);",
        'console.log(getVersion(), result.success);',
      ].join('\n'),
      sourcefile: 'manual-entry.js',
      resolveDir,
    },
    bundle: true,
    format: 'esm',
    platform: 'browser',
    target: 'es2022',
    loader: {
      '.wasm': 'file',
    },
    outdir,
    write: true,
  });

  const files = readdirSync(outdir);
  assert(files.some((file) => file.endsWith('.wasm')), 'manual esbuild bundle must emit wasm asset');
}

async function testNodeEntries() {
  const nodeEntry = await import('@polyglot-sql/sdk');
  assert(typeof nodeEntry.getVersion() === 'string', 'Node ESM entry should expose getVersion()');

  const require = createRequire(import.meta.url);
  const cjsEntry = require('@polyglot-sql/sdk');
  assert(cjsEntry.isInitialized() === false, 'CJS entry should defer initialization');
  await cjsEntry.init();
  assert(cjsEntry.isInitialized() === true, 'CJS entry should initialize');
  assert(typeof cjsEntry.getVersion() === 'string', 'CJS entry should expose getVersion()');
}

assertDistFiles();
assert(!readDist('index.js').includes('node:fs'), 'dist/index.js must not contain node:fs');
assert(!readDist('index.js').includes('node:url'), 'dist/index.js must not contain node:url');

await testDefaultBrowserBundle();
await testManualEsbuildBundle();
await testNodeEntries();

console.log('Bundler smoke tests passed');
