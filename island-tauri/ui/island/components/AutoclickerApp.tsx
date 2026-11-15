import { useEffect, useState } from 'react';
import type { AutoclickerChanged, AutoclickerConfig, AutoclickerOp, ClickButton, ClickKind, TriggerMode } from '../../ipc/client';
import { ShortcutRecorder } from '../../views/settings/ShortcutRecorder';
import { DraftRange, ServiceNotice } from './AppPrimitives';

type SpeedUnit = 'cps' | 'ms';
const MAX_MS = 3_600_000;
const clampInt = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, Math.round(v)));
const cpsOf = (ms: number) => 1000 / ms;
const fmtCps = (cps: number) => cps >= 100 ? cps.toFixed(0) : cps >= 10 ? cps.toFixed(1).replace(/\.0$/, '') : cps.toFixed(2).replace(/\.?0+$/, '');

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
  switch (m.hotkeyState) {
    case 'bound': return m.config.mode === 'hold' ? `Hold ${m.config.hotkey} to click` : `Press ${m.config.hotkey} to start or stop`;
    case 'off': return 'Hotkey is off. Arm it to start clicking from any app.';
    case 'pending': return 'Registering the hotkey with KDE…';
    default: return m.hotkeyDetail || 'Hotkey unavailable';
  }
};

/** Autoclicker: a virtual mouse clicks wherever the pointer is (Wayland-safe, via uinput). */
export function AutoclickerApp({ model, intents }: { model: AutoclickerChanged; intents: { autoclicker(op: AutoclickerOp): void } }) {
  const c = model.config;
  const [unit, setUnit] = useState<SpeedUnit>(() => (c.intervalMs >= 1000 ? 'ms' : 'cps'));
  const send = intents.autoclicker;
  const set = (patch: Partial<AutoclickerConfig>) => send({ type: 'configure', config: { ...c, ...patch } });
  const status = !model.running ? 'Idle' : model.starting ? 'Starting… move the pointer into place' : 'Clicking';
  const warn = model.hotkeyState === 'conflict' || model.hotkeyState === 'invalid' || model.hotkeyState === 'unavailable';
  return <div className="autoclicker-app app-stack">
    {!model.available && <ServiceNotice icon="cursor" reason={model.detail} inline />}

    <section className="card ac-hero" data-running={model.running || undefined}>
      <div className="ac-status">
        <span className="ac-dot" aria-hidden="true" />
        <div><strong>{status}</strong>
          <small className="muted">{model.clicks.toLocaleString()} click{model.clicks === 1 ? '' : 's'} · {fmtCps(cpsOf(c.intervalMs))}/s{c.jitterPercent ? ` ±${c.jitterPercent}%` : ''}</small></div>
      </div>
      <button type="button" className="ac-go" disabled={!model.available} onClick={() => send(model.running ? { type: 'stop' } : { type: 'start' })}>
        {model.running ? 'Stop' : c.startDelayS ? `Start in ${c.startDelayS}s` : 'Start'}</button>
    </section>
    {model.available && model.detail && <p className="warning app-caption" role="alert">{model.detail}</p>}

    <section className="card ac-card">
      <Row title="Hotkey active" hint={hotkeyNote(model)}>
        <input className="switch" type="checkbox" aria-label="Hotkey active" checked={model.armed} onChange={e => send({ type: 'arm', armed: e.target.checked })} />
      </Row>
      {warn && model.armed && <p className="warning app-caption">{model.hotkeyDetail}</p>}
      <Row title="Hotkey" hint={model.armed ? 'Turn the hotkey off to record a new one' : undefined}>
        <ShortcutRecorder label="Autoclicker hotkey" value={c.hotkey} onSave={async hotkey => set({ hotkey })} />
      </Row>
      <Row title="Activation" hint={c.mode === 'hold' ? 'Clicks while the hotkey is held down' : 'Press once to start, again to stop'}>
        <Choice<TriggerMode> label="Activation" value={c.mode} options={[['toggle', 'Toggle'], ['hold', 'Hold']]} onChange={mode => set({ mode })} />
      </Row>
    </section>

    <section className="card ac-card">
      <Row title="Speed" hint={unit === 'cps' ? `Every ${c.intervalMs} ms` : `${fmtCps(cpsOf(c.intervalMs))} clicks per second`}>
        <div className="ac-speed">
          {unit === 'cps'
            ? <NumberField label="Clicks per second" value={Number(fmtCps(cpsOf(c.intervalMs)))} min={0.001} max={1000} step={1} suffix="/s"
                onCommit={cps => set({ intervalMs: clampInt(1000 / cps, 1, MAX_MS) })} />
            : <NumberField label="Interval in milliseconds" value={c.intervalMs} min={1} max={MAX_MS} suffix="ms"
                onCommit={ms => set({ intervalMs: clampInt(ms, 1, MAX_MS) })} />}
          <Choice<SpeedUnit> label="Speed unit" value={unit} options={[['cps', 'CPS'], ['ms', 'ms']]} onChange={setUnit} />
        </div>
      </Row>
      <Row title="Randomize" hint={c.jitterPercent ? `Each gap varies ±${c.jitterPercent}%` : 'Exact timing'}>
        <DraftRange className="ac-range" aria-label="Randomize interval" min={0} max={50} step={1} value={c.jitterPercent} live={false}
          onCommit={(v, final) => { if (final) set({ jitterPercent: v }); }} />
      </Row>
      <Row title="Button">
        <Choice<ClickButton> label="Mouse button" value={c.button} options={[['left', 'Left'], ['middle', 'Middle'], ['right', 'Right']]} onChange={button => set({ button })} />
      </Row>
      <Row title="Click type">
        <Choice<ClickKind> label="Click type" value={c.kind} options={[['single', 'Single'], ['double', 'Double']]} onChange={kind => set({ kind })} />
      </Row>
    </section>

    <section className="card ac-card">
      <Row title="Stop after clicks" hint="0 = never">
        <NumberField label="Stop after clicks" value={c.clickLimit} min={0} max={10_000_000} suffix="clicks" onCommit={v => set({ clickLimit: Math.round(v) })} />
      </Row>
      <Row title="Stop after time" hint="0 = never">
        <NumberField label="Stop after seconds" value={c.timeLimitS} min={0} max={86_400} suffix="s" onCommit={v => set({ timeLimitS: Math.round(v) })} />
      </Row>
      <Row title="Start delay" hint="For the Start button only, so you can move the pointer">
        <NumberField label="Start delay in seconds" value={c.startDelayS} min={0} max={10} suffix="s" onCommit={v => set({ startDelayS: Math.round(v) })} />
      </Row>
    </section>
    <p className="muted app-caption">Clicks land wherever your pointer is, in any app. The hotkey only works while it's active.</p>
  </div>;
}
