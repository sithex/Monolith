import { useEffect, useRef, useState } from 'react';

/** Editing a launch preference never starts, stops, or writes to the active PTY. */
export function TerminalCommandSettings({ value, onSave }: {
  value: string; onSave(command: string): Promise<unknown>;
}) {
  const [draft, setDraft] = useState(value), [busy, setBusy] = useState(false);
  const [error, setError] = useState(''), [notice, setNotice] = useState('');
  const previous = useRef(value), saving = useRef(false);
  useEffect(() => {
    // Accept external settings updates without destroying a locally edited draft.
    if (draft === previous.current) setDraft(value);
    previous.current = value;
  }, [value]);
  const edit = (command: string) => { setDraft(command); setError(''); setNotice(''); };
  const save = async (command: string) => {
    if (saving.current) return;
    saving.current = true; setBusy(true); setError(''); setNotice('');
    try {
      await onSave(command);
      setNotice('Saved. Applies to the next terminal session.');
    } catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)); }
    finally { saving.current = false; setBusy(false); }
  };
  return <section className="settings-card terminal-command" aria-label="Terminal launch settings">
    <h3>Terminal</h3>
    <form onSubmit={event => { event.preventDefault(); void save(draft); }}>
      <label htmlFor="terminal-launch-command">Hermes launch command</label>
      <input id="terminal-launch-command" type="text" aria-label="Hermes launch command" value={draft}
        placeholder="Built-in Hermes TUI launcher" maxLength={4096} autoComplete="off" spellCheck={false}
        onChange={event => edit(event.target.value)} />
      <div className="terminal-command-presets" role="group" aria-label="Launch command examples">
        {['hermes', 'hermes --tui', 'hermes --cli', 'hermes-local'].map(command =>
          <button key={command} type="button" className="pill-button" onClick={() => edit(command)}>{command}</button>)}
      </div>
      <p className="muted">Leave blank for the built-in launcher. Plain <code>hermes</code> follows your Hermes interface setting;
        <code> --tui</code> and <code> --cli</code> override it. Flags are lowercase.</p>
      <p className="muted">Runs an executable and quoted arguments, not a shell. Shell aliases, pipes and variable expansion are not supported.
        Use a full executable path if it is not found.</p>
      <p className="muted">Saving does not end the current chat. The command is used after Hermes exits and you restart the terminal, or after the island restarts.
        A custom wrapper may omit your usual voice or startup customizations.</p>
      <div className="terminal-command-actions">
        <button type="submit" className="pill-button primary" disabled={busy || draft === value}>{busy ? 'Saving…' : 'Save command'}</button>
        <button type="button" className="link" disabled={busy || (value === '' && draft === '')}
          onClick={() => { edit(''); void save(''); }}>Reset to built-in</button>
      </div>
      {error && <p role="alert">{error}</p>}
      {notice && <p role="status" className="muted">{notice}</p>}
    </form>
  </section>;
}
