// Same specifier as ui/legacy/renderer.jsx: Vite must produce ONE Terminal prototype.
import { Terminal } from "@xterm/xterm/lib/xterm.js";
import { createParseAck } from "./xterm-ack-core";

let installed: ReturnType<typeof createParseAck> | null = null;
export function installXtermAckAdapter(ack: (seq: number) => Promise<unknown>, testHooks: boolean) {
  if (installed) return installed;
  const adapter = createParseAck(ack);
  adapter.install(Terminal.prototype);
  installed = adapter;
  if (testHooks) {
    Object.defineProperty(window, "__islandTerminalTest", { value: {
      text() {
        const terminal = adapter.terminal() as Terminal | null;
        if (!terminal) return null;
        const buffer = terminal.buffer.active;
        return Array.from({ length: buffer.length }, (_, i) => buffer.getLine(i)?.translateToString(true) ?? "").join("\n");
      },
      setAckPaused: (value: boolean) => adapter.setAckPaused(value),
    } });
  }
  return adapter;
}
