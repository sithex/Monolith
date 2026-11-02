import { useEffect, useRef } from 'react';
import { Terminal } from '@xterm/xterm/lib/xterm.js';
import { FitAddon } from '@xterm/addon-fit';
import { newTerminalChannel, terminalAttach, terminalInput, terminalResize, terminalAck, type Palette } from '../ipc/client';
import { runtimeStore } from '../stores';
import { createTerminalFlow } from './terminal-flow';
import { keepIslandColours } from './terminal-colours';
import '@xterm/xterm/css/xterm.css';
export function TerminalView({ active, palette, fontSize, fail }: { active: boolean; palette: Palette; fontSize: number; fail(error: unknown): void }) {
  const host = useRef<HTMLDivElement>(null), term = useRef<Terminal | null>(null);
  const visible = useRef(active), resize = useRef(() => {});
  visible.current = active;
  // Deliberately independent of mode/view: collapse and view changes NEVER recreate xterm/PTY.
  useEffect(() => {
    if (!host.current) return;
    const terminal = new Terminal({ cursorBlink: true, fontFamily: '"DejaVu Sans Mono", monospace', fontSize,
      scrollback: 5000, theme: theme(palette), allowProposedApi: false, allowTransparency: true });
    const fitter = new FitAddon(); terminal.loadAddon(fitter); terminal.open(host.current); term.current = terminal;
    const osc = keepIslandColours(terminal);
    let disposed = false, running = false, frame: number | null = null, retry: ReturnType<typeof setTimeout> | null = null, deferred = 0;
    const flow = createTerminalFlow(terminal, terminalAck, fail);
    const channel = newTerminalChannel();
    channel.onmessage = event => {
      if (disposed) return;
      if (event.type === 'snapshot') running = event.running;
      if (event.type === 'exit') { running = false; terminal.write(`\r\n[Hermes exited: ${event.code ?? 'unknown'}]\r\n`); }
      flow.event(event);
    };
    const input = terminal.onData(data => { if (running) void terminalInput(data).catch(fail); });
    resize.current = () => {
      if (!visible.current || disposed || frame !== null) return;
      // Never refit mid-morph: every intermediate size reflowed xterm AND resized the PTY,
      // making the Hermes TUI redraw several times per open. Fit once when the shell settles.
      if (host.current?.closest('.island')?.getAttribute('data-morphing') === 'true' && deferred < 25) {
        deferred++;
        if (retry === null) retry = setTimeout(() => { retry = null; resize.current(); }, 90);
        return;
      }
      deferred = 0;
      frame = requestAnimationFrame(() => {
        frame = null;
        if (!visible.current || !host.current || host.current.clientHeight < 80) return;
        const size = fitter.proposeDimensions();
        if (size && size.cols >= 2 && size.rows >= 2 && (terminal.cols !== size.cols || terminal.rows !== size.rows)) {
          terminal.resize(size.cols, size.rows); void terminalResize(size.cols, size.rows).catch(fail);
        }
        terminal.focus();
      });
    };
    const observer = new ResizeObserver(() => resize.current()); observer.observe(host.current);
    void terminalAttach(channel).then(snapshot => {
      if (disposed) return;
      running = snapshot.running; flow.snapshot(snapshot); resize.current();
    }).catch(fail);
    if (runtimeStore.getSnapshot().testHooks) {
      Object.defineProperty(window, '__islandTerminalTest', { configurable: true, value: {
        identity: terminal,
        text: () => Array.from({ length: terminal.buffer.active.length }, (_, i) => terminal.buffer.active.getLine(i)?.translateToString(true) ?? '').join('\n'),
      } });
    }
    return () => { disposed = true; osc.forEach(handler => handler.dispose()); flow.dispose(); observer.disconnect(); input.dispose(); terminal.dispose(); term.current = null; if (frame !== null) cancelAnimationFrame(frame); if (retry !== null) clearTimeout(retry); };
    // The first palette/font are supplied by bootstrap; subsequent changes update options below.
  }, []);
  useEffect(() => { if (term.current) { term.current.options.fontSize = fontSize; term.current.options.theme = theme(palette); resize.current(); } }, [palette, fontSize]);
  useEffect(() => { if (active) resize.current(); }, [active]);
  return <div className="terminal" ref={host} aria-label="Hermes interactive terminal" />;
}
// Transparent: the island surface (incl. its blur/tint) shows through, so a
// partial last row never paints a strip in another colour.
function theme(palette: Palette) { return { background: '#00000000', foreground: palette.text, cursor: palette.accent, selectionBackground: palette.border }; }
