#!/usr/bin/env node
// Fails (exit 1) if ui/ipc/bindings.ts is stale or contains split/nullable-float types.
// The Rust test `bindings_are_current` regenerates into a scratch file and compares byte-for-byte.
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const cargo = spawnSync(
  "cargo",
  ["test", "--locked", "--lib", "--", "bindings_are_current", "generated_types_are_clean", "command_lists_match"],
  { cwd: join(root, "src-tauri"), stdio: "inherit", env: { ...process.env, PATH: `${process.env.HOME}/.cargo/bin:${process.env.PATH}` } },
);
if (cargo.status !== 0) {
  console.error("check-bindings: bindings are stale or unclean (run: cargo test --lib export_bindings -- --ignored)");
  process.exit(1);
}
const text = readFileSync(join(root, "ui/ipc/bindings.ts"), "utf8");
for (const bad of ["_Serialize", "_Deserialize", "bigint"]) {
  if (text.includes(bad)) {
    console.error(`check-bindings: bindings.ts contains ${bad}`);
    process.exit(1);
  }
}
console.log("check-bindings: ok");
