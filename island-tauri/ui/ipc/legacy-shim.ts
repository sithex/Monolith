// Phase 2–6 compatibility shim (SPEC §4.6): implements the frozen `window.island` shape of
// island/preload.cjs on top of the generated bindings, so the unchanged fork renderer runs on Tauri.
// Translating `idle|preview|pinned` <-> `still|hover|large` is this file's job; Rust never sees
// the legacy names (SPEC §0.3).
import {
  aiConnect,
  aiUsage,
  bootstrap,
  newTerminalChannel,
  onAnchorSignal,
  onPaletteChanged,
  paletteGet,
  reportGeometry,
  settingsGet,
  settingsPatch,
  systemStats,
  terminalAck,
  terminalAttach,
  terminalInput,
  terminalResize,
  terminalRestart,
  voiceCommand,
  type AiProvider,
  type AnchorKind,
  type Palette,
  type SettingsPatch,
  type TerminalEvent,
  type TerminalSnapshot,
  type VoiceOp,
} from "./client";
import type { IslandMode } from "./bindings";
import { installXtermAckAdapter } from "./xterm-ack-adapter";
import { listenIslandState, seedIslandState } from "./island-state";
import { listenNotifications, seedNotifications } from "./notifications";
import { toCanonicalMode } from "./legacy-mode";
export { toCanonicalMode, toLegacyMode } from "./legacy-mode";
/** A layout burst is "settled" when no further layout call arrived for this long. */
export const SETTLE_MS = 140;

type Listener<T> = (value: T) => void;
const fanOut = <T>(set: Set<Listener<T>>) => (callback: Listener<T>) => {
  if (typeof callback !== "function") throw new TypeError("Expected callback");
  set.add(callback);
  return () => {
    set.delete(callback);
  };
};



function legacySnapshot(snapshot: TerminalSnapshot) {
  // The renderer's snapshot omits `exitCode` until the process exited (JS `undefined`).
  const { exitCode, ...rest } = snapshot;
  return exitCode === undefined ? rest : { ...rest, exitCode };
}

export async function installLegacyShim(): Promise<void> {
  if (Object.getOwnPropertyDescriptor(window, "island")) return;
  await listenIslandState();
  await listenNotifications();
  const boot = await bootstrap();

  const paletteListeners = new Set<Listener<Palette>>();
  const anchorListeners = new Set<Listener<AnchorKind>>();
  const dataListeners = new Set<Listener<{ data: string; seq: number }>>();
  const exitListeners = new Set<Listener<{ exitCode: number }>>();
  const guarded = <T>(set: Set<Listener<T>>, value: T) => {
    for (const callback of set) {
      try {
        callback(value);
      } catch (error) {
        console.error(error);
      }
    }
  };
  // Registered before `window.island` exists, so no event can be missed between mount and listen.
  await onPaletteChanged((palette) => guarded(paletteListeners, palette));
  await onAnchorSignal((kind) => guarded(anchorListeners, kind));
  // Phase 3: the Rust actor is the IslandState authority. Seed from the bootstrap state
  // (rev-guarded; the event listener was installed BEFORE the bootstrap read) before the
  // renderer module is evaluated.
  seedIslandState(boot.state);
  seedNotifications(boot.notifications);

  // --- terminal: one channel for the lifetime of the page, fanned out to onData/onExit -------
  let channel: ReturnType<typeof newTerminalChannel> | null = null;
  const parseAck = installXtermAckAdapter(terminalAck, boot.runtime.testHooks);
  const onTerminalEvent = (event: TerminalEvent) => {
    if (event.type === "data") {
      parseAck.record(event);
      guarded(dataListeners, { data: event.data, seq: event.seq });
    } else if (event.type === "snapshot") {
      parseAck.recordSnapshot(event);
    } else {
      guarded(exitListeners, { exitCode: event.code ?? -1 });
    }
  };
  const attach = async () => {
    parseAck.reset();
    // Each Rust Channel starts a fresh transport index. Reusing a JS Channel on
    // remount would wait forever for the old index after Rust replaces its sink.
    channel = newTerminalChannel();
    channel.onmessage = onTerminalEvent;
    const snapshot = await terminalAttach(channel);
    parseAck.recordSnapshot(snapshot);
    return legacySnapshot(snapshot);
  };

  // --- layout: legacy per-frame rect calls -> SPEC §4.4 reports ---------------------------------
  // Phase 3: `stateRev` is the ACTOR's revision, passed by the renderer from the store state it
  // rendered (renderer.ownership.patch); the shim no longer invents revisions. Rust accepts only
  // the current rev, several reports per rev are fine, and `settled` follows ~140 ms of quiet.
  let settleTimer: ReturnType<typeof setTimeout> | null = null;
  let last: {
    rect: Record<string, number>;
    mode: IslandMode;
    scale: number;
    stateRev: number;
  } | null = null;
  const send = (settled: boolean) => {
    if (!last) return Promise.resolve(undefined);
    const { rect, mode, scale, stateRev } = last;
    return reportGeometry({
      stateRev,
      mode,
      rects: [
        {
          x: Number(rect["x"]),
          y: Number(rect["y"]),
          width: Number(rect["width"]),
          height: Number(rect["height"]),
          radius: Number(rect["radius"]),
        },
      ],
      scale,
      settled,
    });
  };
  const layout = (rect: Record<string, number>, mode: unknown, scale: number, stateRev: unknown) => {
    const canonical = toCanonicalMode(mode);
    if (!rect || typeof rect !== "object") return Promise.reject(new Error("Invalid overlay layout"));
    if (typeof stateRev !== "number" || !Number.isInteger(stateRev) || stateRev < 0 || stateRev > 0xffffffff) {
      return Promise.reject(new Error("Invalid overlay layout revision"));
    }
    last = { rect, mode: canonical, scale, stateRev };
    if (settleTimer !== null) clearTimeout(settleTimer);
    settleTimer = setTimeout(() => {
      settleTimer = null;
      send(true).catch((error: unknown) => console.error(error));
    }, SETTLE_MS);
    return send(false);
  };

  const api = Object.freeze({
    overlay: boot.runtime.overlay,
    panelAnchor: boot.runtime.panelAnchor,
    onAnchor: fanOut(anchorListeners),
    onPalette: fanOut(paletteListeners),
    onData: fanOut(dataListeners),
    onExit: fanOut(exitListeners),
    layout,
    // Focus-on-pin belongs to the state actor (Phase 3, SPEC §2.5); a click already activates.
    pin: () => Promise.resolve(undefined),
    palette: (...args: unknown[]) =>
      args.length ? Promise.reject(new Error("Palette takes no arguments")) : paletteGet(),
    start: attach,
    restart: async () => {
      parseAck.reset();
      const snapshot = await terminalRestart();
      parseAck.recordSnapshot(snapshot);
      return legacySnapshot(snapshot);
    },
    input: (data: string) => terminalInput(data),
    resize: (cols: number, rows: number) => terminalResize(cols, rows),
    voice: (operation: VoiceOp) => voiceCommand(operation),
    settings: () => settingsGet(),
    setSettings: (patch: SettingsPatch) => settingsPatch(patch),
    // Window skip-taskbar is a side effect of `settings_patch` (SPEC §4.3); only validate here.
    taskbar: (show: unknown) =>
      typeof show === "boolean" ? Promise.resolve(show) : Promise.reject(new Error("Invalid taskbar flag")),
    stats: () => systemStats(),
    aiUsage: () => aiUsage(),
    aiConnect: (provider: AiProvider) => aiConnect(provider),
  });
  Object.defineProperty(window, "island", { value: api, enumerable: true });
}
