import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import frozenXterm from "./scripts/phase7b-frozen-xterm.mjs";

// Tauri serves `dist/` (frontendDist) in production and http://localhost:5173 in `cargo tauri dev`.
export default defineConfig({
  plugins: [frozenXterm(), react()],
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    target: "es2022",
    sourcemap: false,
    // index.html = Phase 1 geometry harness (kept for the Phase 1/1.5 GUI tests);
    // legacy.html = the unchanged fork renderer behind the legacy shim (Phase 2);
    // spike.html = Phase 2.5 panel-overlap spike page (test-only, loaded only by --spike-panel).
    rollupOptions: { input: { index: "index.html", legacy: "legacy.html", spike: "spike.html", island: "island.html" } },
  },
});
