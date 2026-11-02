import { useEffect, useId, useRef, useState } from 'react';
import { AnimatePresence, animate, motion, useMotionValue, useMotionValueEvent, useTransform, type AnimationPlaybackControls } from 'framer-motion';
import type { AppViewIntents, TimerSnapshot, TimerTab, CompletedTimerKind } from '../app-presentations';
import { timerKind } from '../app-presentations';
import { useReducedMotionPreference } from './motion-preference';
export function duration(ms: number): string {
  const seconds = Math.max(0, Math.ceil(ms / 1000));
  const hours = Math.floor(seconds / 3600);
  return `${hours ? `${hours}:` : ''}${String(Math.floor(seconds / 60) % (hours ? 60 : 100000)).padStart(2, '0')}:${String(seconds % 60).padStart(2, '0')}`;
}

/** Digits roll vertically when they change (Vorssaint numeric-text transition, 0.3 s). */
function RollingText({ text, reduced }: { text: string; reduced: boolean }) {
  const chars = text.split('');
  return <span aria-hidden="true" style={{ display: 'inline-flex', overflow: 'hidden' }}>{chars.map((c, i) => {
    const slot = chars.length - i;  // keyed from the right so seconds keep their slot when hours appear
    return <span key={slot} style={{ position: 'relative', display: 'inline-block' }}>
      <AnimatePresence initial={false} mode="popLayout">
        <motion.span key={c} className="digit" initial={reduced ? false : { y: '-70%', opacity: 0, filter: 'blur(2px)' }} animate={{ y: '0%', opacity: 1, filter: 'blur(0px)' }}
          exit={reduced ? undefined : { y: '70%', opacity: 0, filter: 'blur(2px)' }} transition={{ duration: 0.3, ease: [0.22, 1, 0.36, 1] }}>{c}</motion.span>
      </AnimatePresence></span>;
  })}</span>;
}

const TICK = 14, MIN = 1, MAX = 180;
/**
 * Horizontal scroll ruler (Vorssaint timer picker): drag with momentum, scroll wheel and
 * arrow keys move it; it always settles on a whole minute.
 */
function MinuteWheel({ value, onChange, disabled, reduced }: { value: number; onChange(v: number): void; disabled: boolean; reduced: boolean }) {
  const offset = useMotionValue(-(value - MIN) * TICK);
  const [live, setLive] = useState(value);
  const drag = useRef<{ x: number; start: number; last: number; t: number; v: number } | null>(null);
  const anim = useRef<AnimationPlaybackControls | null>(null), wheelAcc = useRef(0);
  const toValue = (o: number) => Math.max(MIN, Math.min(MAX, Math.round(-o / TICK) + MIN));
  useMotionValueEvent(offset, 'change', o => { const v = toValue(o); setLive(prev => prev === v ? prev : v); });
  const settle = (target: number, velocity = 0) => {
    anim.current?.stop();
    const clamped = Math.max(MIN, Math.min(MAX, target));
    anim.current = reduced ? (offset.set(-(clamped - MIN) * TICK), null) : animate(offset, -(clamped - MIN) * TICK, { type: 'spring', stiffness: 420, damping: 42, velocity });
    if (clamped !== value) onChange(clamped);
  };
  useEffect(() => { if (!drag.current && toValue(offset.get()) !== value) settle(value); }, [value]);
  const x = useTransform(offset, o => o);
  return <div className="wheel" role="slider" tabIndex={disabled ? -1 : 0} aria-label="Countdown minutes" aria-valuemin={MIN} aria-valuemax={MAX} aria-valuenow={live} aria-valuetext={`${live} minutes`} aria-disabled={disabled}
    style={{ opacity: disabled ? 0.45 : 1, pointerEvents: disabled ? 'none' : undefined }}
    onPointerDown={e => {
      if (e.button !== 0) return; anim.current?.stop(); e.currentTarget.setPointerCapture(e.pointerId);
      drag.current = { x: e.clientX, start: offset.get(), last: e.clientX, t: performance.now(), v: 0 };
    }}
    onPointerMove={e => {
      const d = drag.current; if (!d) return;
      const now = performance.now(), dt = Math.max(1, now - d.t);
      d.v = (e.clientX - d.last) / dt * 1000; d.last = e.clientX; d.t = now;
      const raw = d.start + (e.clientX - d.x), lo = -(MAX - MIN) * TICK;
      // Rubber-band past either end.
      offset.set(raw > 0 ? raw * 0.3 : raw < lo ? lo + (raw - lo) * 0.3 : raw);
    }}
    onPointerUp={e => {
      const d = drag.current; drag.current = null; if (!d) return;
      if (Math.abs(e.clientX - d.x) < 3) {   // click: jump to the tick under the pointer
        const r = e.currentTarget.getBoundingClientRect();
        settle(toValue(offset.get()) + Math.round((e.clientX - (r.left + r.width / 2)) / TICK)); return;
      }
      const projected = offset.get() + d.v * 0.18;   // momentum, then snap to a whole minute
      settle(toValue(projected), d.v);
    }}
    onPointerCancel={() => { drag.current = null; settle(toValue(offset.get())); }}
    onWheel={e => {
      wheelAcc.current += (Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY) * (e.deltaMode === 1 ? 20 : 1);
      const steps = Math.trunc(wheelAcc.current / 24); if (!steps) return;
      wheelAcc.current -= steps * 24; settle(toValue(offset.get()) + steps);
    }}
    onKeyDown={e => {
      const step = e.key === 'ArrowRight' || e.key === 'ArrowUp' ? 1 : e.key === 'ArrowLeft' || e.key === 'ArrowDown' ? -1 : e.key === 'PageUp' ? 5 : e.key === 'PageDown' ? -5 : 0;
      if (e.key === 'Home') { e.preventDefault(); settle(MIN); } else if (e.key === 'End') { e.preventDefault(); settle(MAX); }
      else if (step) { e.preventDefault(); e.stopPropagation(); settle(live + step); }
    }}>
    <motion.div className="wheel-track" style={{ x }}>
      {Array.from({ length: MAX - MIN + 1 }, (_, i) => {
        const minute = i + MIN, major = minute % 5 === 0 || minute === 1;
        return <span key={minute}>
          <i className={`wheel-tick ${major ? 'major' : ''} ${minute <= live ? 'lit' : ''}`} style={{ left: i * TICK }} />
          {major && <span className="wheel-label" style={{ left: i * TICK }}>{minute}</span>}
        </span>;
      })}
    </motion.div>
    <span className="wheel-pointer" aria-hidden="true" />
  </div>;
}

export function TimerApp({ snapshot, intents, completedKind = snapshot.countdownStatus === 'finished' ? 'countdown' : null }: { snapshot: TimerSnapshot; intents: Pick<AppViewIntents, 'timer'>; completedKind?: CompletedTimerKind | null }) {
  const id = useId(), reduced = useReducedMotionPreference();
  const [tab, setTab] = useState<TimerTab>('timer');
  const [minutes, setMinutes] = useState(25);
  const status = tab === 'timer' ? snapshot.countdownStatus : tab === 'pomodoro' ? snapshot.pomodoroStatus : snapshot.stopwatchStatus;
  const ms = tab === 'timer' ? (status === 'idle' ? minutes * 60000 : snapshot.remainingMs) : tab === 'pomodoro' ? snapshot.phaseRemainingMs : snapshot.elapsedMs;
  const running = status === 'running';
  const tabs: TimerTab[] = ['timer', 'pomodoro', 'stopwatch'];
  const start = () => {
    // Core permits SetCountdown only while idle; Finished restarts its duration.
    if (tab === 'timer' && status === 'idle') intents.timer({ type: 'setCountdown', durationMs: minutes * 60000 });
    intents.timer({ type: running ? 'pause' : status === 'paused' ? 'resume' : 'start', kind: timerKind(tab) });
  };
  return <div className="timer-app">
    <div className="timer-top"><div className="timer-tabs" role="tablist" aria-label="Timer mode">{tabs.map((t, index) => <button key={t} id={`${id}-timer-tab-${t}`} role="tab" aria-controls={`${id}-timer-panel-${t}`} aria-selected={tab === t} tabIndex={tab === t ? 0 : -1}
      onKeyDown={e => { if (['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(e.key)) { e.preventDefault(); const next = e.key === 'Home' ? tabs[0] : e.key === 'End' ? tabs[2] : tabs[(index + (e.key === 'ArrowRight' ? 1 : 2)) % 3]; if (next) { setTab(next); document.getElementById(`${id}-timer-tab-${next}`)?.focus(); } } }}
      onClick={() => setTab(t)}>{t === 'timer' ? 'Timer' : t === 'pomodoro' ? 'Pomodoro' : 'Stopwatch'}</button>)}</div>
      <button className="timer-start" disabled={!snapshot.available} onClick={start}>{running ? 'Pause' : status === 'paused' ? 'Resume' : 'Start'}</button>
    </div>
    {!snapshot.available && <p className="unavailable">Unavailable · {snapshot.detail}</p>}
    <div role="tabpanel" id={`${id}-timer-panel-${tab}`} aria-labelledby={`${id}-timer-tab-${tab}`} className="app-stack">
      {tab === 'timer' && <MinuteWheel value={minutes} onChange={setMinutes} disabled={status !== 'idle'} reduced={reduced} />}
      {tab === 'pomodoro' && <p className="phase-label">{snapshot.phase === 'focus' ? 'Focus' : snapshot.phase === 'shortBreak' ? 'Short break' : 'Long break'} · Round {snapshot.round + 1}</p>}
      <div className="countdown" role="timer" aria-label={`${tab} ${status} ${duration(ms)}`}><RollingText text={duration(ms)} reduced={reduced} /></div>
      <div className="timer-footer"><span className="muted">{status === 'idle' ? (tab === 'timer' ? 'Drag or scroll the ruler to set minutes.' : 'Make a little room for focus.') : status}</span><button disabled={!snapshot.available} onClick={() => intents.timer({ type: 'cancel', kind: timerKind(tab) })}>Reset</button>{tab === 'stopwatch' && <button disabled={!running || !snapshot.available} onClick={() => intents.timer({ type: 'lap' })}>Lap</button>}</div>
      {completedKind !== null && <button data-testid="timer-dismiss" disabled={!snapshot.available} onClick={() => intents.timer({ type: 'cancel', kind: completedKind })}>Dismiss completed timer</button>}
      {tab === 'stopwatch' && <ol className="lap-list">{snapshot.laps.map(l => <li key={l.number}>Lap {l.number}<span>{duration(l.elapsedMs)}</span></li>)}</ol>}
    </div>
  </div>;
}
