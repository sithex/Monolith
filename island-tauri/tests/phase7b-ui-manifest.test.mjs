import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { MANIFEST_NAME, contractHash, hashTree, writeManifest } from "../scripts/write-ui-manifest.mjs";

const sha = (text) => createHash("sha256").update(text).digest("hex");
const scratch = () => mkdtempSync(join(process.env.TMPDIR ?? tmpdir(), "ui-manifest-"));

function fixture() {
  const dir = scratch();
  mkdirSync(join(dir, "assets"));
  writeFileSync(join(dir, "island.html"), "<html></html>");
  writeFileSync(join(dir, "assets/island-abc.js"), "console.log(1)\n");
  return dir;
}

test("the manifest lists every file with its sha256 and the bindings contract hash", () => {
  const dir = fixture();
  try {
    const manifest = writeManifest(dir);
    assert.deepEqual(manifest.files, {
      "assets/island-abc.js": sha("console.log(1)\n"),
      "island.html": sha("<html></html>"),
    });
    assert.equal(manifest.version, 1);
    assert.equal(manifest.contractHash, contractHash());
    const bindings = readFileSync(new URL("../ui/ipc/bindings.ts", import.meta.url));
    assert.equal(manifest.contractHash, createHash("sha256").update(bindings).digest("hex"));
    assert.deepEqual(JSON.parse(readFileSync(join(dir, MANIFEST_NAME), "utf8")).files, manifest.files);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("rewriting the manifest never lists the manifest itself", () => {
  const dir = fixture();
  try {
    writeManifest(dir);
    assert.equal(Object.keys(writeManifest(dir).files).includes(MANIFEST_NAME), false);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("group/world-writable output is tightened so the binary accepts it", () => {
  const dir = fixture();
  try {
    chmodSync(join(dir, "island.html"), 0o666);
    chmodSync(join(dir, "assets"), 0o777);
    writeManifest(dir);
    assert.equal(statSync(join(dir, "island.html")).mode & 0o022, 0);
    assert.equal(statSync(join(dir, "assets")).mode & 0o022, 0);
    assert.equal(statSync(join(dir, MANIFEST_NAME)).mode & 0o022, 0);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("symlinks and empty trees are refused", () => {
  const dir = fixture();
  try {
    symlinkSync("island.html", join(dir, "alias.html"));
    assert.throws(() => hashTree(dir), /not a regular file/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
  const empty = scratch();
  try {
    assert.throws(() => writeManifest(empty), /empty/);
  } finally {
    rmSync(empty, { recursive: true, force: true });
  }
});
