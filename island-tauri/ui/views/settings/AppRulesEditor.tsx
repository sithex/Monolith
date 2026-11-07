// Per-app notification rules: one line per app (popup / sound switches, urgency), an app-name
// combobox suggesting apps seen in notifications, add and remove. Saves the whole list as one
// `notificationAppRules` patch; Rust's `check_app_rules` has the final word.
import { useId, useMemo, useState } from 'react';
import type { NotificationAppRule, RuleUrgency } from '../../ipc/bindings';
import { Dropdown } from '../../island/components/AppPrimitives';
import { SoundPicker } from './SoundPicker';
import { addRule, appNameError, appSuggestions, removeRule, updateRule, URGENCY_OPTIONS } from './app-rules';

const message = (e: unknown) => e instanceof Error ? e.message : String(e);

export function AppRulesEditor({ rules, seenApps, onSave, globalSound = '' }: {
  rules: readonly NotificationAppRule[]; seenApps: readonly string[]; onSave(rules: NotificationAppRule[]): Promise<void>;
  /** The global sound choice, previewed for "Same as global". */
  globalSound?: string;
}) {
  const [name, setName] = useState(''), [error, setError] = useState<string | null>(null), [listOpen, setListOpen] = useState(false), [focus, setFocus] = useState(-1);
  const id = useId();
  const suggestions = useMemo(() => appSuggestions(seenApps, rules, name), [seenApps, rules, name]);
  const nameError = name.trim() ? appNameError(name, rules) : null;
  const save = (next: NotificationAppRule[]) => { setError(null); onSave(next).catch(e => setError(message(e))); };
  const add = (app = name) => {
    const bad = appNameError(app, rules);
    if (bad) { setError(bad); return; }
    save(addRule(rules, app)); setName(''); setListOpen(false); setFocus(-1);
  };
  return <div className="app-rules" aria-label="Per-app notification rules">
    {rules.length > 0 && <ul className="app-rule-list">
      {rules.map(rule => <li key={rule.app} className="app-rule">
        <span className="app-rule-name" title={rule.app}>{rule.app}</span>
        <label className="app-rule-toggle"><input className="switch" type="checkbox" aria-label={`${rule.app} popups`} checked={rule.popup}
          onChange={e => save(updateRule(rules, rule.app, { popup: e.target.checked }))} />Popup</label>
        <label className="app-rule-toggle"><input className="switch" type="checkbox" aria-label={`${rule.app} sound`} checked={rule.sound}
          onChange={e => save(updateRule(rules, rule.app, { sound: e.target.checked }))} />Sound</label>
        <SoundPicker label={`${rule.app} sound choice`} value={rule.soundId ?? ''} emptyLabel="Same as global" previewEmpty={globalSound} disabled={!rule.sound}
          onChange={id => save(updateRule(rules, rule.app, { soundId: id }))} />
        <Dropdown label={`${rule.app} urgency`} value={rule.urgency} options={URGENCY_OPTIONS}
          onChange={v => save(updateRule(rules, rule.app, { urgency: v as RuleUrgency }))} />
        <button type="button" className="icon-button app-rule-remove" aria-label={`Remove rule for ${rule.app}`} onClick={() => save(removeRule(rules, rule.app))}>✕</button>
      </li>)}
    </ul>}
    <div className="app-rule-add">
      <div className="app-rule-combo" data-open={listOpen && suggestions.length > 0}>
        <input type="text" role="combobox" aria-label="App name" aria-autocomplete="list" aria-expanded={listOpen && suggestions.length > 0} aria-controls={id}
          aria-activedescendant={focus >= 0 ? `${id}-${focus}` : undefined} aria-invalid={nameError ? true : undefined}
          value={name} placeholder={seenApps.length ? 'App name (pick or type)' : 'App name, e.g. Discord'} maxLength={256} spellCheck={false} autoComplete="off"
          onChange={e => { setName(e.target.value); setError(null); setListOpen(true); setFocus(-1); }}
          onFocus={() => setListOpen(true)} onBlur={() => setTimeout(() => setListOpen(false), 120)}
          onKeyDown={e => {
            if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
              e.preventDefault(); setListOpen(true);
              if (suggestions.length) setFocus(f => { const n = f + (e.key === 'ArrowDown' ? 1 : -1); return n >= suggestions.length ? -1 : n < -1 ? suggestions.length - 1 : n; });
            } else if (e.key === 'Enter') { e.preventDefault(); add(focus >= 0 && suggestions[focus] ? suggestions[focus] : name); }
            else if (e.key === 'Escape' && (listOpen || name)) { e.preventDefault(); e.stopPropagation(); if (listOpen && suggestions.length) setListOpen(false); else setName(''); }
          }} />
        {listOpen && suggestions.length > 0 && <ul className="dropdown-menu app-rule-suggestions" role="listbox" id={id} aria-label="Apps seen in notifications">
          {suggestions.map((app, i) => <li key={app} id={`${id}-${i}`} role="option" aria-selected={i === focus} data-focus={i === focus}
            onPointerDown={e => e.preventDefault()} onPointerEnter={() => setFocus(i)} onClick={() => add(app)}><span>{app}</span></li>)}
        </ul>}
      </div>
      <button type="button" className="pill-button primary" disabled={!name.trim() || !!nameError} onClick={() => add()}>Add</button>
    </div>
    {(error ?? nameError) && <p className="settings-field-error" role="alert">{error ?? nameError}</p>}
  </div>;
}
