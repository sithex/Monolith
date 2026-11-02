// Phase 2–6 entry for the unchanged fork renderer (SPEC §4.6, §6).
// `window.island` must exist before renderer.jsx evaluates (`const api = window.island`), so the
// shim is installed first and the renderer is imported dynamically afterwards.
import { installLegacyShim } from "./ipc/legacy-shim";
import { createRoot } from "react-dom/client";
import { Toast } from "./widgets/Toast";
import { ClipboardSurface } from "./widgets/ClipboardSurface";

await installLegacyShim();
await import("./legacy/renderer.jsx");
const toasts = document.createElement("div");
toasts.id = "notification-root";
document.body.append(toasts);
createRoot(toasts).render(<Toast />);
const clipboard = document.createElement("div");
clipboard.id = "clipboard-root";
document.body.append(clipboard);
createRoot(clipboard).render(<ClipboardSurface />);
