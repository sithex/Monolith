// Agent Live uses Nick's typed, Result-unwrapping IPC client, never raw Tauri invoke.
export { agentLiveRaw, agentLiveBlocks } from '../ipc/client';
export type { AgentLiveRaw, AgentLiveRequest, AgentLiveBlocksOp } from '../ipc/client';
import { terminalCopy } from '../ipc/client';

/** Copy for `/resume`: async clipboard, then the island's GTK copy command, then execCommand. */
export async function copyText(text: string): Promise<boolean> {
  try { if (navigator.clipboard?.writeText) { await navigator.clipboard.writeText(text); return true; } } catch { /* WebKitGTK may refuse */ }
  try { await terminalCopy(text); return true; } catch { /* unavailable or rejected */ }
  let area: HTMLTextAreaElement | null = null;
  const focused = document.activeElement;
  try {
    area = document.createElement('textarea');
    area.value = text; area.setAttribute('readonly', ''); area.style.position = 'fixed'; area.style.opacity = '0';
    document.body.appendChild(area); area.select();
    return document.execCommand('copy');
  } catch { return false; }
  finally { area?.remove(); if (focused instanceof HTMLElement) focused.focus({ preventScroll: true }); }
}
