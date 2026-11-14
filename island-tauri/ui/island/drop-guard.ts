// Files and links dropped on the island page.
//
// WebKitGTK loads a dropped file or link when the page does not handle the drop, which
// replaces the whole island with that file. The page takes no drops (the notch is the drop
// target), so a window-level guard refuses every drag that carries a file or a URL. Plain
// text drags are left alone so text can still be dropped into a text field. Rust adds a
// second layer: the webview may only navigate within the app's own origin.
//
// Loaded standalone by node tests: no relative imports.

const BLOCKED = ['files', 'text/uri-list', 'text/x-moz-url', '_netscape_url', 'application/x-moz-file'];

/** True when a drag with these `dataTransfer.types` could make the page load something. */
export function shouldBlockDrag(types: ArrayLike<string> | null | undefined): boolean {
  if (!types) return false;
  for (let i = 0; i < types.length; i++) {
    if (BLOCKED.includes(String(types[i]).toLowerCase())) return true;
  }
  return false;
}

type DragLike = { dataTransfer: { types: ArrayLike<string>; dropEffect: string } | null; preventDefault(): void };

/** Handles one `dragover`/`drop`/`dragenter`; returns whether it was refused. */
export function refuseDrag(event: DragLike): boolean {
  if (!shouldBlockDrag(event.dataTransfer?.types)) return false;
  event.preventDefault();
  if (event.dataTransfer) event.dataTransfer.dropEffect = 'none';
  return true;
}

type Target = { addEventListener(type: string, listener: (event: DragLike) => void, options?: { capture?: boolean }): void;
  removeEventListener(type: string, listener: (event: DragLike) => void, options?: { capture?: boolean }): void };

/** Installs the guard on `window` (capture phase, so no page handler can skip it). */
export function installDropGuard(target: Target): () => void {
  const listener = (event: DragLike) => { refuseDrag(event); };
  const types = ['dragenter', 'dragover', 'drop'];
  for (const type of types) target.addEventListener(type, listener, { capture: true });
  return () => { for (const type of types) target.removeEventListener(type, listener, { capture: true }); };
}
