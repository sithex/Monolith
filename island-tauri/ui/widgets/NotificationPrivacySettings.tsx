// A sibling-root portal adds Phase 4 controls without changing the pinned legacy renderer.
import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useIslandState } from "../ipc/island-state";
import { onSettingsChanged, settingsGet, settingsPatch, type Settings, type SettingsPatch } from "../ipc/client";

const message = (error: unknown) => error instanceof Error ? error.message : String(error);
export function NotificationPrivacySettings() {
  const state = useIslandState();
  const [target, setTarget] = useState<Element | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [draft, setDraft] = useState("");
  const dirty = useRef(false);
  const saving = useRef(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [status, setStatus] = useState("");
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    const sync = () => setTarget(document.querySelector(".island .settings-panel:not([data-own-privacy])"));
    sync();
    const observer = new MutationObserver(sync);
    observer.observe(document.getElementById("root") ?? document.body, { childList: true, subtree: true });
    return () => observer.disconnect();
  }, []);
  useEffect(() => {
    let active = true;
    let off: (() => void) | undefined;
    let eventSeen = false;
    setError("");
    const accept = (next: Settings) => {
      if (!active) return;
      setSettings(next);
      if (!dirty.current) setDraft((next.notificationDeniedApps ?? []).join("\n"));
    };
    // Subscribe before reading; an intervening change wins over the older get response.
    void (async () => {
      off = await onSettingsChanged((next) => { eventSeen = true; accept(next); });
      if (!active) { off(); return; }
      const next = await settingsGet();
      if (!eventSeen) accept(next);
    })().catch((e: unknown) => { if (active) setError(message(e)); });
    return () => { active = false; off?.(); };
  }, [attempt]);
  async function save(patch: SettingsPatch, commitDraft = false) {
    if (saving.current) return;
    saving.current = true;
    setBusy(true); setError(""); setStatus("");
    try {
      const next = await settingsPatch(patch);
      setSettings(next);
      if (commitDraft) { dirty.current = false; setDraft((next.notificationDeniedApps ?? []).join("\n")); }
      setStatus("Notification privacy settings saved.");
    } catch (e: unknown) { setError(message(e)); }
    finally { saving.current = false; setBusy(false); }
  }
  if (!target || state.mode !== "large" || state.view !== "settings") return null;
  return createPortal(<section data-testid="notification-privacy-settings" aria-label="Notification privacy">
    <h3 className="settings-section">Notification privacy</h3>
    {error && <p role="alert">{error}</p>}
    {!settings ? <button type="button" onClick={() => setAttempt((n) => n + 1)}>Retry notification settings</button> :
      <fieldset disabled={busy} style={{ border: 0, margin: 0, padding: 0, minWidth: 0 }}>
        {([
          ["notificationsEnabled", "Hermes notifications", "Allow Hermes ingest and system alerts"],
          ["systemNotificationMirror", "Mirror desktop notifications", "Opt in to passive desktop mirroring; no notification history is saved"],
          ["redactBodies", "Hide notification bodies", "Irreversibly scrub stored bodies; turning off does not restore them"],
          ["dnd", "Do Not Disturb", "Store arrivals without showing a toast"],
        ] as const).map(([key, label, hint]) => <label className="setting-row" key={key}>
          <span className="setting-text"><span className="setting-label">{label}</span><span className="setting-hint">{hint}</span></span>
          <input type="checkbox" aria-label={label} checked={settings[key]}
            onChange={(event) => { void save({ [key]: event.target.checked }); }} />
        </label>)}
        <form onSubmit={(event) => {
          event.preventDefault(); event.stopPropagation();
          // Preserve exact identifiers. Rust rejects blank lines, padding, controls, duplicates
          // and byte/count overflow; no trimming, normalization or silent removal here.
          void save({ notificationDeniedApps: draft === "" ? [] : draft.split("\n") }, true);
        }}>
          <label htmlFor="notification-denied-apps" className="setting-label">Denied notification apps</label>
          <p id="notification-denied-apps-hint" className="settings-note">
            Exact app names, one per line (case-sensitive). Up to 128 names, 256 UTF-8 bytes each;
            no blank lines, padding or duplicates. Saving removes matching stored notifications;
            allowing an app again does not restore them. Empty clears the list.
          </p>
          <textarea id="notification-denied-apps" aria-describedby="notification-denied-apps-hint"
            rows={4} spellCheck={false} value={draft} maxLength={32896}
            style={{ width: "100%", boxSizing: "border-box", color: "inherit", background: "transparent", font: "inherit" }}
            onChange={(event) => { dirty.current = true; setDraft(event.target.value); setStatus(""); }} />
          <button type="submit">Save denied apps</button>
        </form>
      </fieldset>}
    {busy && <p role="status">Saving…</p>}
    {!busy && status && <p role="status">{status}</p>}
  </section>, target);
}
