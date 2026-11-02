// Phase 5 sibling portal. Legacy DOM/CSS remain untouched; all controls stay inside its shape.
import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { dispatch, useIslandState } from "../ipc/island-state";
import { bootstrap, clipboardList, clipboardGet, clipboardCopy, clipboardPin, clipboardDelete, clipboardClear,
  clipboardSetPaused, onClipboardAdded, onClipboardStatus, onSettingsChanged, settingsPatch,
  type Settings, type ClipboardStatus, type ClipSummary, type ClipContent, type SettingsPatch } from "../ipc/client";

export function ClipboardSurface() {
  const state = useIslandState();
  const [target, setTarget] = useState<Element | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [status, setStatus] = useState<ClipboardStatus | null>(null);
  const [rows, setRows] = useState<ClipSummary[]>([]);
  const [content, setContent] = useState<ClipContent | null>(null);
  const [thumbnail, setThumbnail] = useState<string | null>(null);
  const statusVersion = useRef(0);
  useEffect(() => {
    setThumbnail(null);
    if (content?.type !== "image") return;
    let live = true;
    const image = new Image();
    image.onload = () => {
      if (!live) return;
      const scale = Math.min(1, 160 / Math.max(image.width, image.height));
      const canvas = document.createElement("canvas");
      canvas.width = Math.max(1, Math.round(image.width * scale));
      canvas.height = Math.max(1, Math.round(image.height * scale));
      const context = canvas.getContext("2d");
      if (context) { context.drawImage(image, 0, 0, canvas.width, canvas.height); setThumbnail(canvas.toDataURL("image/png")); }
    };
    image.src = content.pngDataUrl;
    return () => { live = false; image.onload = null; };
  }, [content]);
  const [patterns, setPatterns] = useState("");
  const dirty = useRef(false);
  const refreshing = useRef(false);
  const pending = useRef(false);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    const sync = () => setTarget(document.querySelector(state.view === "settings" ? ".island .settings-panel" : ".island .view-pane"));
    sync();
    const observer = new MutationObserver(sync);
    observer.observe(document.getElementById("root") ?? document.body, { childList: true, subtree: true });
    return () => observer.disconnect();
  }, [state.view]);
  async function refresh() {
    pending.current = true;
    if (refreshing.current) return;
    refreshing.current = true;
    try {
      while (pending.current) {
        pending.current = false;
        const version = statusVersion.current;
        const [snapshot, items] = await Promise.all([bootstrap(), clipboardList()]);
        setSettings(snapshot.settings);
        if (version === statusVersion.current) setStatus(snapshot.capabilities.clipboardStatus);
        setRows(items);
        if (!dirty.current) setPatterns((snapshot.settings.clipboardDenyPatterns ?? []).join("\n"));
      }
    } finally { refreshing.current = false; }
  }
  useEffect(() => {
    let live = true;
    const offs: (() => void)[] = [];
    void (async () => {
      for (const subscribe of [onClipboardAdded, onSettingsChanged]) {
        const off = await subscribe(() => { if (live) void refresh().catch((e: Error) => setError(e.message)); });
        if (!live) { off(); return; } offs.push(off);
      }
      const off = await onClipboardStatus((next) => { if (live) { statusVersion.current += 1; setStatus(next); } });
      if (!live) { off(); return; } offs.push(off);
      if (live) await refresh();
    })().catch((e: Error) => { if (live) setError(e.message); });
    return () => { live = false; offs.forEach((off) => off()); };
  }, []);
  useEffect(() => { if (state.mode === "large" && ["settings", "clipboard"].includes(state.view)) void refresh().catch((e: Error) => setError(e.message)); }, [state.mode, state.view]);
  async function run(work: () => Promise<unknown>) {
    if (busy) return;
    setBusy(true); setError("");
    try { await work(); await refresh(); }
    catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    finally { setBusy(false); }
  }
  const save = (patch: SettingsPatch) => run(() => settingsPatch(patch));
  if (!target || state.mode !== "large" || !["settings", "clipboard"].includes(state.view)) return null;
  return createPortal(<section data-testid="clipboard-surface" aria-label="Clipboard history" style={{ padding: state.view === "clipboard" ? 18 : 0, overflow: "auto", maxHeight: "100%", boxSizing: "border-box" }}>
    <h3 className="settings-section">Clipboard history</h3>
    {error && <p role="alert">{error}</p>}
    <p data-testid="clipboard-status">{!settings?.clipboardHistory ? "Monitoring off" : status?.paused ? "Paused" : status?.available ? "Capturing clipboard changes" : status?.detail ?? "Connecting…"}</p>
    <fieldset disabled={busy} style={{ border: 0, padding: 0, margin: 0, minWidth: 0 }}>
      {settings && <>
        <label className="setting-row"><span>Enable clipboard history (private, opt-in)</span><input aria-label="Enable clipboard history" type="checkbox" checked={settings.clipboardHistory} onChange={(e) => { void save({ clipboardHistory: e.target.checked }); }} /></label>
        <label className="setting-row"><span>Persist pinned entries only</span><input aria-label="Persist pinned clipboard entries" type="checkbox" checked={settings.clipboardPersist} onChange={(e) => { void save({ clipboardPersist: e.target.checked }); }} /></label>
        <label className="setting-row"><span>History size (10–500)</span><input aria-label="Clipboard history size" type="number" min={10} max={500} value={settings.clipboardHistorySize} onChange={(e) => { if (e.target.value !== "") void save({ clipboardHistorySize: Number(e.target.value) }); }} /></label>
        <button disabled={!settings.clipboardHistory} onClick={() => { void run(() => clipboardSetPaused(!status?.paused)); }}>{status?.paused ? "Resume clipboard" : "Pause clipboard"}</button>
        {state.view === "settings" && <button onClick={() => { void run(() => dispatch({ type: "selectView", view: "clipboard" })); }}>Open clipboard history</button>}
        <form onSubmit={(e) => { e.preventDefault(); e.stopPropagation(); void run(async () => { await settingsPatch({ clipboardDenyPatterns: patterns === "" ? [] : patterns.split("\n") }); dirty.current = false; }); }}>
          <label>Denied text patterns (one regex per line, up to 32, 256 characters each)<textarea aria-label="Clipboard deny patterns" rows={3} value={patterns} onChange={(e) => { dirty.current = true; setPatterns(e.target.value); }} style={{ width: "100%", boxSizing: "border-box", background: "transparent", color: "inherit" }} /></label>
          <button type="submit">Save clipboard privacy</button>
        </form>
      </>}
      {state.view === "clipboard" && <>
        <button onClick={() => { void run(() => clipboardClear()); }}>Clear unpinned</button>
        <button onClick={() => { void run(refresh); }}>Refresh</button>
        <ul>{rows.map((row) => <li key={row.id}>
          <button onClick={() => { void run(async () => setContent(await clipboardGet(row.id))); }}>{row.preview}</button>
          <button onClick={() => { void run(() => clipboardCopy(row.id)); }}>Copy</button>
          <button onClick={() => { void run(() => clipboardPin(row.id, !row.pinned)); }}>{row.pinned ? "Unpin" : "Pin"}</button>
          <button onClick={() => { setContent(null); void run(() => clipboardDelete(row.id)); }}>Delete</button>
        </li>)}</ul>
        {content?.type === "text" && <pre style={{ whiteSpace: "pre-wrap", userSelect: "text" }}>{content.text}</pre>}
        {content?.type === "files" && <pre style={{ whiteSpace: "pre-wrap", userSelect: "text" }}>{content.uris.join("\n")}</pre>}
        {content?.type === "image" && thumbnail && <img src={thumbnail} alt="Clipboard image" style={{ maxWidth: 160, maxHeight: 160, objectFit: "contain" }} />}
      </>}
    </fieldset>
  </section>, target);
}
