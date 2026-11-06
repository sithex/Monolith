// Records a KDE global shortcut from a real key press. Click Record, press the chord: it is
// converted to KGlobalAccel syntax (shortcut-keys.ts) and saved. Esc cancels, Backspace or
// Clear unbinds (''). Nothing is ever bound unless the user records or types one.
//
// KWin grabs Meta chords it already owns (and Meta alone opens the launcher), so those keys
// never reach the webview. When Meta goes down and up with no chord in between, or the island
// loses focus mid-recording, the "Type it" fallback is offered.
import { useEffect, useRef, useState } from 'react';
import { shortcutError } from '../settings-model';
import { chordFromKey, heldText } from './shortcut-keys';

const message = (e: unknown) => e instanceof Error ? e.message : String(e);

export function ShortcutRecorder({ label, value, onSave }: { label: string; value: string; onSave(value: string): Promise<void> }) {
  const [recording, setRecording] = useState(false), [held, setHeld] = useState(''), [note, setNote] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null), [typing, setTyping] = useState(false), [draft, setDraft] = useState(value);
  const button = useRef<HTMLButtonElement>(null);
  const save = (next: string) => {
    setError(null);
    if (next === value) return;
    onSave(next).catch(e => setError(message(e)));
  };
  useEffect(() => {
    if (!recording) return;
    let chorded = false, metaDown = false;
    const stop = () => { setRecording(false); setHeld(''); };
    const down = (e: KeyboardEvent) => {
      // Capture phase + stop: Esc must not close the island, keys must not reach other handlers.
      e.preventDefault(); e.stopImmediatePropagation();
      if (e.repeat) return;
      const chord = chordFromKey(e);
      if (e.key === 'Meta' || e.key === 'OS' || e.key === 'Super') metaDown = true;
      switch (chord.kind) {
        case 'modifier': setHeld(chord.held); setNote(null); return;
        case 'cancel': stop(); return;
        case 'clear': chorded = true; stop(); save(''); return;
        case 'invalid': chorded = true; setHeld(chord.held); setNote(chord.reason); return;
        case 'chord': {
          chorded = true;
          const bad = shortcutError(chord.text);
          if (bad) { setNote(bad); return; }
          stop(); setNote(null); save(chord.text);
        }
      }
    };
    const up = (e: KeyboardEvent) => {
      e.preventDefault(); e.stopImmediatePropagation();
      // KeyboardEvent fields are prototype getters: copy them explicitly, minus the released key.
      const meta = e.key === 'Meta' || e.key === 'OS' || e.key === 'Super';
      setHeld(heldText({ key: e.key, metaKey: e.metaKey && !meta, ctrlKey: e.ctrlKey && e.key !== 'Control', altKey: e.altKey && e.key !== 'Alt', shiftKey: e.shiftKey && e.key !== 'Shift' }));
      if ((e.key === 'Meta' || e.key === 'OS' || e.key === 'Super') && metaDown && !chorded) {
        setNote('Didn\'t catch that? KDE keeps Meta shortcuts it already uses. Type it instead.');
      }
      if (!e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey) { chorded = false; metaDown = false; }
    };
    const blur = () => { stop(); setNote('The island lost focus (KDE may own that shortcut). Type it instead.'); };
    window.addEventListener('keydown', down, true);
    window.addEventListener('keyup', up, true);
    window.addEventListener('blur', blur);
    return () => { window.removeEventListener('keydown', down, true); window.removeEventListener('keyup', up, true); window.removeEventListener('blur', blur); };
  }, [recording, value]);
  const typedError = shortcutError(draft);
  const commitTyped = () => { if (!typedError) { save(draft.trim()); setTyping(false); } };
  const shown = error ?? note;
  return <div className="shortcut-recorder" data-recording={recording || undefined}>
    <div className="shortcut-line">
      {typing
        ? <input type="text" aria-label={`${label} (typed)`} value={draft} placeholder="Meta+`" maxLength={64} spellCheck={false} autoComplete="off" autoFocus
            aria-invalid={typedError ? true : undefined} onChange={e => setDraft(e.target.value)} onBlur={commitTyped}
            onKeyDown={e => { if (e.key === 'Enter') { e.preventDefault(); commitTyped(); } else if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); setTyping(false); setDraft(value); } }} />
        : <button ref={button} type="button" className="shortcut-keys" aria-label={recording ? `${label}: press the keys` : `Record ${label}`}
            aria-pressed={recording} onClick={() => { setError(null); setNote(null); setHeld(''); setRecording(!recording); }}>
            {recording ? <span className="shortcut-live">{held || 'Press keys…'}</span>
              : value ? <kbd>{value}</kbd> : <span className="muted">Not set</span>}
          </button>}
      {!typing && value && !recording && <button type="button" className="link" aria-label={`Clear ${label}`} onClick={() => save('')}>Clear</button>}
      {!recording && <button type="button" className="link" onClick={() => { setTyping(!typing); setDraft(value); setNote(null); }}>{typing ? 'Done' : 'Type it'}</button>}
    </div>
    {recording && !note && <p className="shortcut-hint">Esc cancels · Backspace clears</p>}
    {typing && typedError && <p className="settings-field-error" role="alert">{typedError}</p>}
    {shown && <p className="settings-field-error" role="alert">{shown}</p>}
  </div>;
}
