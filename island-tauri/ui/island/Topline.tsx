import { useEffect, useState } from 'react';
import { dispatch, useIslandState } from '../ipc/island-state';
import { useSettings } from '../stores';
import { minuteDelay } from './navigation';
import { nativePill } from './geometry';
import { companionNotchSlot } from './companionCharacters';
export function Topline({ fail }: { fail(error: unknown): void }) {
  const state = useIslandState(), settings = useSettings(), [now, setNow] = useState(() => new Date());
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout>;
    const schedule = () => { timer = setTimeout(() => { setNow(new Date()); schedule(); }, minuteDelay(Date.now())); };
    schedule(); return () => clearTimeout(timer);
  }, []);
  // Under the panel notch the clock sits exactly where the native notch draws its own
  // (same box, font sizes and weights as PanelPill.qml), so opening and closing never
  // move, hide or double it: the shell simply fades over an identical clock.
  const notch = nativePill() !== null;
  // PanelPill.qml nudges its clock right by (companion slot - 20) px when the companion is shown
  // on that notch (the 26 px characters), so this clock must take the same offset.
  const output = typeof document === 'undefined' ? '' : document.documentElement.dataset.nativePillOutput ?? '';
  const companionHere = settings.companionEnabled !== false && (!settings.companionMonitor || settings.companionMonitor === output);
  const shift = notch && companionHere ? Math.max(0, companionNotchSlot(settings.companionCharacter) - 20) : 0;
  // The clock and the "Hermes" title are home buttons: open (if needed) and go to the terminal.
  const goHome = () => {
    void (async () => { if (state.mode !== 'large') await dispatch({ type: 'pin' }); if (state.view !== 'terminal') await dispatch({ type: 'selectView', view: 'terminal' }); })().catch(fail);
  };
  return <div className="topline" data-notch-clock={notch} onClick={event => {
    if (state.mode === 'large' && !(event.target as HTMLElement).closest('button')) void dispatch({ type: 'collapse' }).catch(fail);
  }}>
    {state.mode !== 'still' && <button type="button" className="muted topline-title" data-testid="topline-home" aria-label="Go to the Hermes terminal" title="Hermes" onClick={goHome}>Hermes</button>}
    <button className="clock" data-testid="clock" aria-label="Open Hermes terminal" data-date={settings.showDate} onClick={goHome}
      style={shift ? { ['--notch-clock-shift' as string]: `${shift}px` } : undefined}>
      <span>{now.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: settings.hourFormat === 'h12' })}</span>
      {settings.showDate && <small>{now.toLocaleDateString([], { weekday: 'short', month: 'short', day: 'numeric' })}</small>}
    </button>
    {state.mode === 'large' && <button className="collapse" data-testid="collapse" aria-label="Minimize island" onClick={() => { void dispatch({ type: 'collapse' }).catch(fail); }}>−</button>}
  </div>;
}
