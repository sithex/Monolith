import { useId } from 'react';
import type { IslandIntents, TimerSnapshot, TimerTab } from '../models';
export function duration(ms: number): string {
  const seconds = Math.max(0, Math.ceil(ms / 1000));
  const hours = Math.floor(seconds / 3600);
  return `${hours ? `${hours}:` : ''}${String(Math.floor(seconds / 60) % (hours ? 60 : 100000)).padStart(2, '0')}:${String(seconds % 60).padStart(2, '0')}`;
}
export function TimerApp({ snapshot, tab, minutes, intents }: { snapshot: TimerSnapshot; tab: TimerTab; minutes: number; intents: IslandIntents }) {
  const id = useId();
  const value = tab === 'timer' ? snapshot.countdown : tab === 'pomodoro' ? snapshot.pomodoro : snapshot.stopwatch;
  const running = value.status === 'running';
  const tabs: TimerTab[] = ['timer', 'pomodoro', 'stopwatch'];
  return <div className="timer-app tone-amber">
    <div className="timer-top"><div className="timer-tabs" role="tablist" aria-label="Timer mode">{tabs.map((t, index) => <button key={t} id={`${id}-timer-tab-${t}`} role="tab" aria-controls={`${id}-timer-panel-${t}`} aria-selected={tab === t} tabIndex={tab === t ? 0 : -1}
      onKeyDown={e => { if (['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(e.key)) { e.preventDefault(); const next = e.key === 'Home' ? tabs[0] : e.key === 'End' ? tabs[2] : tabs[(index + (e.key === 'ArrowRight' ? 1 : 2)) % 3]; intents.timer({ type: 'tab', tab: next }); document.getElementById(`${id}-timer-tab-${next}`)?.focus(); } }}
      onClick={() => intents.timer({ type: 'tab', tab: t })}>{t === 'timer' ? 'Timer' : t === 'pomodoro' ? 'Pomodoro' : 'Stopwatch'}</button>)}</div>
      <button className="timer-start" onClick={() => intents.timer({ type: running ? 'pause' : value.status === 'paused' ? 'resume' : 'start', tab })}>{running ? 'Pause' : value.status === 'paused' ? 'Resume' : 'Start'}</button>
    </div>
    <div role="tabpanel" id={`${id}-timer-panel-${tab}`} aria-labelledby={`${id}-timer-tab-${tab}`}>
      {tab === 'timer' && <div className="ruler-wrap"><div className="ruler" aria-hidden="true">{Array.from({ length: 30 }, (_, n) => <span key={n} className={`${n < minutes ? 'lit' : ''} ${n % 5 === 4 ? 'major' : ''}`}><i />{n === 0 || n % 5 === 4 ? <small>{n + 1}</small> : null}</span>)}</div><input aria-label="Countdown minutes" type="range" min={1} max={1440} step={1} value={minutes} disabled={value.status !== 'idle'} onChange={e => intents.timer({ type: 'minutes', minutes: Number(e.target.value) })}/><span className="ruler-pointer" aria-hidden="true">▲</span></div>}
      {tab === 'pomodoro' && <p className="phase-label">{snapshot.pomodoro.phase === 'focus' ? 'Focus' : snapshot.pomodoro.phase === 'shortBreak' ? 'Short break' : 'Long break'} · Round {snapshot.pomodoro.round + 1}</p>}
      <div className="countdown" role="timer" aria-label={`${tab} ${value.status}`}>{duration('remainingMs' in value ? value.remainingMs : value.elapsedMs)}</div>
      <div className="timer-footer"><span className="muted">{value.status === 'idle' ? 'Make a little room for focus.' : value.status}</span><button onClick={() => intents.timer({ type: 'reset', tab })}>Reset</button>{tab === 'stopwatch' && <button disabled={!running} onClick={() => intents.timer({ type: 'lap', tab })}>Lap</button>}</div>
      {tab === 'stopwatch' && <ol className="lap-list">{snapshot.stopwatch.laps.map(l => <li key={l.number}>Lap {l.number}<span>{duration(l.elapsedMs)}</span></li>)}</ol>}
    </div>
  </div>;
}
