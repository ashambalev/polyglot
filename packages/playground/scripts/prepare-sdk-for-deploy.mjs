import { execSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

function readJson(filePath) {
  return JSON.parse(readFileSync(filePath, "utf8"));
}

function run(command, cwd) {
  execSync(command, { cwd, stdio: "inherit" });
}

async function readDistVersion(distIndexPath) {
  if (!existsSync(distIndexPath)) return null;
  try {
    const mod = await import(`${pathToFileURL(distIndexPath).href}?t=${Date.now()}`);
    if (typeof mod.getVersion === "function") {
      return String(mod.getVersion());
    }
  } catch {
    return null;
  }
  return null;
}

const scriptDir = dirname(fileURLToPath(import.meta.url));
const rootDir = resolve(scriptDir, "../../..");
const sdkDir = resolve(rootDir, "packages/sdk");

const sdkPackageJsonPath = resolve(sdkDir, "package.json");
const sdkWasmPackageJsonPath = resolve(sdkDir, "wasm/package.json");
const sdkDistIndexPath = resolve(sdkDir, "dist/index.node.js");
const requiredDistFiles = [
  "dist/index.js",
  "dist/index.node.js",
  "dist/index.cjs",
  "dist/manual.js",
  "dist/manual.d.ts",
  "dist/polyglot_sql.wasm",
  "dist/polyglot_sql.wasm.d.ts",
  "dist/cdn/polyglot.esm.js",
];

const expectedVersion = readJson(sdkPackageJsonPath).version;
const currentWasmVersion = existsSync(sdkWasmPackageJsonPath)
  ? readJson(sdkWasmPackageJsonPath).version
  : null;
const currentDistVersion = await readDistVersion(sdkDistIndexPath);
const hasRequiredDistFiles = requiredDistFiles.every((file) =>
  existsSync(resolve(sdkDir, file)),
);

const needsBuild =
  !hasRequiredDistFiles ||
  currentWasmVersion !== expectedVersion ||
  currentDistVersion !== expectedVersion;

if (needsBuild) {
  console.log(
    `[prepare:sdk] Rebuilding SDK artifacts (expected=${expectedVersion}, wasm=${currentWasmVersion ?? "missing"}, dist=${currentDistVersion ?? "missing"})`,
  );
  run("pnpm run build:wasm", sdkDir);
  run("pnpm run build", sdkDir);
} else {
  console.log(`[prepare:sdk] SDK artifacts already at version ${expectedVersion}`);
}

const finalWasmVersion = existsSync(sdkWasmPackageJsonPath)
  ? readJson(sdkWasmPackageJsonPath).version
  : null;
const finalDistVersion = await readDistVersion(sdkDistIndexPath);
const missingDistFiles = requiredDistFiles.filter((file) => !existsSync(resolve(sdkDir, file)));

if (
  missingDistFiles.length > 0 ||
  finalWasmVersion !== expectedVersion ||
  finalDistVersion !== expectedVersion
) {
  throw new Error(
    `[prepare:sdk] SDK artifact check failed after prepare step: expected=${expectedVersion}, wasm=${finalWasmVersion ?? "missing"}, dist=${finalDistVersion ?? "missing"}, missing=${missingDistFiles.join(", ") || "none"}`,
  );
}

console.log(`[prepare:sdk] SDK artifacts ready at version ${expectedVersion}`);
