// Writes dist/.monolith-manifest.json: every file under dist/ with its SHA-256, plus the hash of
// ui/ipc/bindings.ts (the IPC contract the UI was built against). A release binary serves
// dist/ from disk only when this manifest verifies and the contract hash equals the one
// compiled into the binary (src-tauri/src/ui_assets.rs); anything else falls back to the
// assets embedded at compile time.
//
// Runs as a Vite plugin (vite.config.ts), so every `npm run build:ui` writes it. By hand:
//   node scripts/write-ui-manifest.mjs [dist-dir]   write the manifest for an existing dist/
//   node scripts/write-ui-manifest.mjs --check      print the contract hash of ui/ipc/bindings.ts
import { createHash } from 'node:crypto';
import { chmodSync, lstatSync, readFileSync, readdirSync, renameSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

export const MANIFEST_NAME = '.monolith-manifest.json';
const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');

export function contractHash(appRoot = root) {
  return sha256(readFileSync(join(appRoot, 'ui/ipc/bindings.ts')));
}

/** Relative path -> sha256, sorted; symlinks and special files are refused. */
export function hashTree(dist) {
  const files = {};
  const walk = (dir) => {
    for (const entry of readdirSync(dir, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
      const full = join(dir, entry.name);
      const info = lstatSync(full);
      // The binary refuses group/world-writable entries, so a loose umask must not break it.
      chmodSync(full, info.mode & 0o7755);
      if (info.isDirectory()) walk(full);
      else if (info.isFile()) {
        if (entry.name !== MANIFEST_NAME || dir !== dist) files[relative(dist, full).split(sep).join('/')] = sha256(readFileSync(full));
      } else throw new Error(`${full} is not a regular file or directory`);
    }
  };
  walk(dist);
  return files;
}

export function writeManifest(dist = join(root, 'dist'), appRoot = root) {
  chmodSync(dist, lstatSync(dist).mode & 0o7755);
  const files = hashTree(dist);
  if (Object.keys(files).length === 0) throw new Error(`${dist} is empty: run vite build first`);
  const manifest = { version: 1, contractHash: contractHash(appRoot), generatedAt: new Date().toISOString(), files };
  const target = join(dist, MANIFEST_NAME);
  writeFileSync(`${target}.tmp`, `${JSON.stringify(manifest, null, 1)}\n`, { mode: 0o644 });
  renameSync(`${target}.tmp`, target);
  return manifest;
}

export default function uiManifestPlugin() {
  let outDir = join(root, 'dist');
  return {
    name: 'monolith-ui-manifest',
    apply: 'build',
    configResolved(config) {
      outDir = resolve(config.root, config.build.outDir);
    },
    closeBundle: {
      order: 'post',
      handler() {
        const manifest = writeManifest(outDir);
        console.log(`ui manifest: ${Object.keys(manifest.files).length} files, contract ${manifest.contractHash.slice(0, 12)}`);
      },
    },
  };
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  if (process.argv.includes('--check')) console.log(contractHash());
  else {
    const manifest = writeManifest(process.argv[2] ? resolve(process.argv[2]) : undefined);
    console.log(`ui manifest: ${Object.keys(manifest.files).length} files, contract ${manifest.contractHash.slice(0, 12)}`);
  }
}
