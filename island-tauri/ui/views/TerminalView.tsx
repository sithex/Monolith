import { useEffect, useRef } from 'react';
import { Terminal } from '@xterm/xterm/lib/xterm.js';
import { FitAddon } from '@xterm/addon-fit';
import { newTerminalChannel, terminalAttach, terminalInput, terminalCopy, terminalResize, terminalAck, terminalRestart, type Palette, type TerminalEvent } from '../ipc/client';
import { runtimeStore } from '../stores';
import { createTerminalFlow } from './terminal-flow';
import { keepIslandColours } from './terminal-colours';
import { terminalShortcut, keepTerminalClipboard, trackAppSelection } from './terminal-shortcuts';
import { createAutoRestart } from './terminal-respawn';
import { applyTerminalOptions, terminalOptions, type TerminalLook } from './terminal-options';
import '@xterm/xterm/css/xterm.css';
export function TerminalView({ active, palette, fontSize, fail, fontFamily = '', scrollback = 5000, cursorStyle = 'block', cursorBlink = true }:
  { active: boolean; palette: Palette; fontSize: number; fail(error: unknown): void } & Partial<TerminalLook>) {
  const host = useRef<HTMLDivElement>(null), term = useRef<Terminal | null>(null);
  const visible = useRef(active), resize = useRef(() => {});
  visible.current = active;
  // Deliberately independent of mode/view: collapse and view changes NEVER recreate xterm/PTY.
  useEffect(() => {
    if (!host.current) return;
    const terminal = new Terminal({ ...terminalOptions({ fontFamily, scrollback, cursorStyle, cursorBlink }), fontSize,
      theme: theme(palette), allowProposedApi: false, allowTransparency: true });
    const fitter = new FitAddon(); terminal.loadAddon(fitter); terminal.open(host.current); term.current = terminal;
    const appSelection = trackAppSelection(terminal, host.current);
    terminal.attachCustomKeyEventHandler(event => terminalShortcut(event, data => terminal.input(data, true),
      { selection: () => terminal.getSelection(), copy: terminalCopy, fail, appSelection: appSelection.active, appCopied: appSelection.clear }));
    const osc = keepIslandColours(terminal);
    osc.push(keepTerminalClipboard(terminal, terminalCopy, fail), appSelection);
    let disposed = false, running = false, frame: number | null = null, retry: ReturnType<typeof setTimeout> | null = null, deferred = 0;
    const flow = createTerminalFlow(terminal, terminalAck, fail);
    // A Tauri channel numbers its messages per Rust handle, so re-attaching the SAME channel
    // made the new run's events wait forever behind the old index. Each attach gets a fresh one.
    let channel = newTerminalChannel();
    const respawn = createAutoRestart({ fail, notice: text => terminal.write(`\r\n\x1b[2m${text}\x1b[0m\r\n`),
      restart: async () => {
        await terminalRestart();
        if (disposed) return;
        channel.onmessage = () => {};
        channel = newTerminalChannel(); channel.onmessage = receive;
        flow.reset();
        const snapshot = await terminalAttach(channel);
        if (disposed) return;
        running = snapshot.running;
        void terminalResize(terminal.cols, terminal.rows).catch(fail);
      } });
    function receive(event: TerminalEvent) {
      if (disposed) return;
      if (event.type === 'snapshot') running = event.running;
      if (event.type === 'exit') { running = false; terminal.write(`\r\n[Hermes exited: ${event.code ?? 'unknown'}]\r\n`); respawn.exited(); }
      flow.event(event);
    }
    channel.onmessage = receive;
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
        respawn: () => respawn.trace(),
        text: () => Array.from({ length: terminal.buffer.active.length }, (_, i) => terminal.buffer.active.getLine(i)?.translateToString(true) ?? '').join('\n'),
      } });
    }
    return () => { disposed = true; respawn.dispose(); osc.forEach(handler => handler.dispose()); flow.dispose(); observer.disconnect(); input.dispose(); terminal.dispose(); term.current = null; if (frame !== null) cancelAnimationFrame(frame); if (retry !== null) clearTimeout(retry); };
    // The first palette/font are supplied by bootstrap; subsequent changes update options below.
  }, []);
  useEffect(() => { if (term.current) { term.current.options.fontSize = fontSize; term.current.options.theme = theme(palette); resize.current(); } }, [palette, fontSize]);
  // Nov 5: font family / scrollback / cursor apply to the live instance (never recreated).
  useEffect(() => {
    if (term.current && applyTerminalOptions(term.current.options as unknown as Record<string, unknown>, { fontFamily, scrollback, cursorStyle, cursorBlink })) resize.current();
  }, [fontFamily, scrollback, cursorStyle, cursorBlink]);
  useEffect(() => { if (active) resize.current(); }, [active]);
  return <div className="terminal" ref={host} aria-label="Hermes interactive terminal" />;
}
// Transparent: the island surface (incl. its blur/tint) shows through, so a
// partial last row never paints a strip in another colour.
function theme(palette: Palette) { return { background: '#00000000', foreground: palette.text, cursor: palette.accent, selectionBackground: palette.border }; }
