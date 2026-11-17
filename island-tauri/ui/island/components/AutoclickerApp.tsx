import { Switch } from './Switch';
import { useEffect, useRef, useState } from 'react';
import { AnimatePresence, motion } from 'framer-motion';
import type { AutoclickerChanged, AutoclickerConfig, AutoclickerOp, ClickButton, ClickKind, ClickTarget, TriggerMode } from '../../ipc/client';
import { ShortcutRecorder } from '../../views/settings/ShortcutRecorder';
import { DraftRange, ServiceNotice } from './AppPrimitives';
import { useReducedMotionAll } from './motion-preference';
import { RollingText } from './RollingText';
import { DUR, EASE_OUT } from '../motion-profile';

type SpeedUnit = 'cps' | 'ms';
const MAX_MS = 3_600_000;
type Tab = 'basic' | 'advanced';
const TAB_KEY = 'monolith.autoclicker.tab';
const clampInt = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, Math.round(v)));
const cpsOf = (ms: number) => 1000 / ms;
const fmtCps = (cps: number) => cps >= 100 ? cps.toFixed(0) : cps >= 10 ? cps.toFixed(1).replace(/\.0$/, '') : cps.toFixed(2).replace(/\.?0+$/, '');
const plural = (n: number, word: string) => `${n.toLocaleString()} ${word}${n === 1 ? '' : 's'}`;
/** 4.2 s · 3m 05s · 1h 12m */
export function fmtDuration(ms: number) {
  const s = Math.max(0, ms) / 1000;
  if (s < 60) return `${s < 10 ? s.toFixed(1) : Math.floor(s)} s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ${String(Math.floor(s % 60)).padStart(2, '0')}s`;
  return `${Math.floor(m / 60)}h ${String(m % 60).padStart(2, '0')}m`;
}
const BUTTON_NAME: Record<ClickButton, string> = { left: 'Left', middle: 'Middle', right: 'Right' };

/** What one click does, in a few words: "Left click · 10/s", "Holds E". */
function summary(c: AutoclickerConfig) {
  const what = c.target === 'key' ? (c.key || 'No key') : `${BUTTON_NAME[c.button]} ${c.kind === 'hold' ? 'button' : 'click'}`;
  if (c.kind === 'hold') return `Holds ${c.target === 'key' ? what : what.toLowerCase()}`;
  return `${c.target === 'key' ? `Presses ${what}` : what}${c.kind === 'double' ? ' ×2' : ''} · ${fmtCps(cpsOf(c.intervalMs))}/s`;
}

function readTab(): Tab { try { return localStorage.getItem(TAB_KEY) === 'advanced' ? 'advanced' : 'basic'; } catch { return 'basic'; } }
function writeTab(tab: Tab) { try { localStorage.setItem(TAB_KEY, tab); } catch { /* a remembered tab is a convenience only */ } }

/** Segmented control (single choice) with the island's `.segmented` look. */
function Choice<T extends string>({ label, value, options, onChange }: { label: string; value: T; options: [T, string][]; onChange(v: T): void }) {
  return <div className="segmented" role="group" aria-label={label}>
    {options.map(([v, text]) => <button key={v} type="button" aria-pressed={value === v} onClick={() => v !== value && onChange(v)}>{text}</button>)}
  </div>;
}

/** Number field that keeps a draft while typing and commits on blur / Enter. */
function NumberField({ label, value, min, max, step = 1, suffix, onCommit }: { label: string; value: number; min: number; max: number; step?: number; suffix?: string; onCommit(v: number): void }) {
  const [draft, setDraft] = useState(String(value));
  useEffect(() => setDraft(String(value)), [value]);
  const commit = () => {
    const n = Number(draft);
    if (!Number.isFinite(n) || draft.trim() === '') { setDraft(String(value)); return; }
    const next = Math.max(min, Math.min(max, n));
    setDraft(String(next));
    if (next !== value) onCommit(next);
  };
  return <label className="ac-number">
    <input type="number" inputMode="decimal" aria-label={label} min={min} max={max} step={step} value={draft}
      onChange={e => setDraft(e.target.value)} onBlur={commit}
      onKeyDown={e => { if (e.key === 'Enter') { e.preventDefault(); commit(); (e.target as HTMLInputElement).blur(); } }} />
    {suffix && <span className="muted">{suffix}</span>}
  </label>;
}

function Row({ title, hint, children }: { title: string; hint?: string; children: React.ReactNode }) {
  return <div className="ac-row"><div className="ac-row-label"><strong>{title}</strong>{hint && <small className="muted">{hint}</small>}</div><div className="ac-row-control">{children}</div></div>;
}

const hotkeyNote = (m: AutoclickerChanged) => {
  const k = m.config.hotkey;
  switch (m.hotkeyState) {
    case 'bound': return m.config.mode === 'hold' ? `Hold ${k} to click` : m.config.mode === 'burst' ? `Press ${k} for ${plural(m.config.burstCount, 'click')}` : `Press ${k} to start or stop`;
    case 'off': return 'Off. Turn it on to start clicking from any app.';
    case 'pending': return 'Registering the hotkey with KDE…';
    default: return m.hotkeyDetail || 'Hotkey unavailable';
  }
};

/** Autoclicker: a virtual mouse and keyboard click wherever the pointer is (Wayland-safe, via uinput). */
export function AutoclickerApp({ model, intents }: { model: AutoclickerChanged; intents: { autoclicker(op: AutoclickerOp): void } }) {
  const c = model.config;
  const [unit, setUnit] = useState<SpeedUnit>(() => (c.intervalMs >= 1000 ? 'ms' : 'cps'));
  const [tab, setTabState] = useState<Tab>(readTab);
  const reduced = useReducedMotionAll();
  const advancedRef = useRef<HTMLDivElement>(null);
  // Only a switch made here scrolls the new cards into view (not a remembered Advanced on open).
  const revealed = useRef(false);
  const send = intents.autoclicker;
  const set = (patch: Partial<AutoclickerConfig>) => send({ type: 'configure', config: { ...c, ...patch } });
  const setTab = (next: Tab) => { revealed.current = next === 'advanced'; setTabState(next); writeTab(next); };
  const holding = c.kind === 'hold';
  const status = !model.running ? 'Idle' : model.starting ? 'Starting… move the pointer into place' : holding ? 'Holding' : 'Clicking';
  const avg = model.sessionMs >= 500 && !holding ? ` · ${fmtCps(model.clicks / (model.sessionMs / 1000))}/s avg` : '';
  const session = model.running || model.clicks > 0
    ? `${plural(model.clicks, 'click')} · ${fmtDuration(model.sessionMs)}${avg}` : summary(c);
  const warn = (s: string) => s === 'conflict' || s === 'invalid' || s === 'unavailable';
  const stopWarn = model.running && warn(model.stopHotkeyState);
  return <div className="autoclicker-app app-stack">
    {/* Advanced keeps every Basic option and fades its extra cards in below them. */}
    <div className="segmented ac-tabs" role="group" aria-label="Autoclicker options">
      {([['basic', 'Basic'], ['advanced', 'Advanced']] as const).map(([id, name]) =>
        <button key={id} type="button" aria-pressed={tab === id} aria-controls="ac-advanced" onClick={() => tab !== id && setTab(id)}>{name}</button>)}
    </div>
    {!model.available && <ServiceNotice icon="cursor" reason={model.detail} inline />}

    <section className="card ac-hero" data-running={model.running || undefined}>
      <div className="ac-status">
        <span className="ac-dot" aria-hidden="true" />
        <div><strong>{status}</strong><small className="muted" aria-live="off">{model.running || model.clicks > 0
          ? <><span className="sr-only">{session}</span><RollingText text={session} reduced={reduced} fast /></> : session}</small></div>
      </div>
      <button type="button" className="ac-go" data-variant={model.running ? 'danger' : 'primary'} disabled={!model.available} onClick={() => send(model.running ? { type: 'stop' } : { type: 'start' })}>
        {model.running ? 'Stop' : c.startDelayS ? `Start in ${c.startDelayS}s` : 'Start'}</button>
    </section>
    {model.available && model.detail && <p className="warning app-caption" role="alert">{model.detail}</p>}
    {stopWarn && <p className="warning app-caption">Stop key: {model.stopHotkeyDetail}</p>}

    <section className="card ac-card">
      <Row title="Hotkey active" hint={hotkeyNote(model)}>
        <Switch aria-label="Hotkey active" checked={model.armed} onChange={e => send({ type: 'arm', armed: e.target.checked })} />
      </Row>
      {warn(model.hotkeyState) && model.armed && <p className="warning app-caption">{model.hotkeyDetail}</p>}
      <Row title="Hotkey" hint={model.armed ? 'Turn the hotkey off to record a new one' : undefined}>
        <ShortcutRecorder label="Autoclicker hotkey" value={c.hotkey} onSave={async hotkey => set({ hotkey })} />
      </Row>
      <Row title="Stop key" hint="Always stops clicking. Only active while clicking.">
        <ShortcutRecorder label="Stop key" value={c.stopHotkey} onSave={async stopHotkey => set({ stopHotkey })} />
      </Row>
      <Row title="Activation">
        <Choice<TriggerMode> label="Activation" value={c.mode} options={[['toggle', 'Toggle'], ['hold', 'Hold'], ['burst', 'Burst']]} onChange={mode => set({ mode })} />
      </Row>
      {c.mode === 'burst' && <Row title="Clicks per press">
        <NumberField label="Clicks per burst" value={c.burstCount} min={1} max={1000} suffix="clicks" onCommit={v => set({ burstCount: Math.round(v) })} />
      </Row>}
    </section>

    <section className="card ac-card">
      <Row title="Press">
        <Choice<ClickTarget> label="What to press" value={c.target} options={[['mouse', 'Mouse'], ['key', 'Key']]} onChange={target => set({ target })} />
      </Row>
      {c.target === 'mouse'
        ? <Row title="Button">
            <Choice<ClickButton> label="Mouse button" value={c.button} options={[['left', 'Left'], ['middle', 'Middle'], ['right', 'Right']]} onChange={button => set({ button })} />
          </Row>
        : <Row title="Key" hint="Can't be one of the hotkeys">
            <ShortcutRecorder label="Key to press" value={c.key} onSave={async key => set({ key })} />
          </Row>}
      <Row title="Type" hint={holding ? 'Pressed once and kept down until it stops' : undefined}>
        <Choice<ClickKind> label="Click type" value={c.kind} options={[['single', 'Single'], ['double', 'Double'], ['hold', 'Hold down']]} onChange={kind => set({ kind })} />
      </Row>
      {!holding && <Row title="Speed" hint={unit === 'cps' ? `Every ${c.intervalMs} ms` : `${fmtCps(cpsOf(c.intervalMs))} per second`}>
        <div className="ac-speed">
          {unit === 'cps'
            ? <NumberField label="Clicks per second" value={Number(fmtCps(cpsOf(c.intervalMs)))} min={0.001} max={1000} step={1} suffix="/s"
                onCommit={cps => set({ intervalMs: clampInt(1000 / cps, 1, MAX_MS) })} />
            : <NumberField label="Interval in milliseconds" value={c.intervalMs} min={1} max={MAX_MS} suffix="ms"
                onCommit={ms => set({ intervalMs: clampInt(ms, 1, MAX_MS) })} />}
          <Choice<SpeedUnit> label="Speed unit" value={unit} options={[['cps', 'CPS'], ['ms', 'ms']]} onChange={setUnit} />
        </div>
      </Row>}
    </section>

    <AnimatePresence initial={false}>{tab === 'advanced' && <motion.div key="advanced" id="ac-advanced" ref={advancedRef} className="ac-advanced"
      initial={reduced ? { opacity: 0 } : { opacity: 0, height: 0, y: -6 }} animate={reduced ? { opacity: 1 } : { opacity: 1, height: 'auto', y: 0 }}
      exit={reduced ? { opacity: 0 } : { opacity: 0, height: 0, y: -6 }}
      transition={{ duration: DUR.slow, ease: EASE_OUT, opacity: { duration: DUR.slow, ease: 'easeOut', delay: reduced ? 0 : .04 } }}
      onAnimationComplete={() => { if (revealed.current) { revealed.current = false; advancedRef.current?.scrollIntoView({ block: 'nearest', behavior: reduced ? 'auto' : 'smooth' }); } }}>
      <section className="card ac-card">
        {!holding && <Row title="Randomize" hint={c.jitterPercent ? `Each gap varies ±${c.jitterPercent}%` : 'Exact timing'}>
          <DraftRange className="ac-range" aria-label="Randomize interval" min={0} max={50} step={1} value={c.jitterPercent} live={false}
            onCommit={(v, final) => { if (final) set({ jitterPercent: v }); }} />
        </Row>}
        {!holding && <Row title="Press length" hint={c.pressMs ? 'How long each press is held' : 'Instant (some games need longer)'}>
          <NumberField label="Press length in milliseconds" value={c.pressMs} min={0} max={1000} suffix="ms" onCommit={v => set({ pressMs: Math.round(v) })} />
        </Row>}
      </section>
      <section className="card ac-card">
        <Row title="Stop after clicks" hint="0 = never">
          <NumberField label="Stop after clicks" value={c.clickLimit} min={0} max={10_000_000} suffix="clicks" onCommit={v => set({ clickLimit: Math.round(v) })} />
        </Row>
        <Row title="Stop after time" hint="0 = never">
          <NumberField label="Stop after seconds" value={c.timeLimitS} min={0} max={86_400} suffix="s" onCommit={v => set({ timeLimitS: Math.round(v) })} />
        </Row>
        <Row title="Start delay" hint="For the Start button, so you can move the pointer">
          <NumberField label="Start delay in seconds" value={c.startDelayS} min={0} max={10} suffix="s" onCommit={v => set({ startDelayS: Math.round(v) })} />
        </Row>
      </section>
      <section className="card ac-card">
        <Row title="All time" hint={`${plural(model.lifetimeClicks, 'click')} · ${fmtDuration(model.lifetimeMs)} clicking`}>
          <button type="button" className="link" disabled={!model.lifetimeClicks} onClick={() => send({ type: 'resetStats' })}>Reset</button>
        </Row>
      </section>
    </motion.div>}</AnimatePresence>
    <p className="muted app-caption">Clicks land wherever your pointer is, in any app.</p>
  </div>;
}
