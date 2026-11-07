import { useEffect, useRef, useState } from 'react';
import { commands, type GeoPlace } from '../../ipc/bindings';
import type { Settings, SettingsPatch } from '../../ipc/client';
import { Dropdown } from '../../island/components/AppPrimitives';

const message = (e: unknown) => e instanceof Error ? e.message : String(e);

/** Explicit save keeps invalid regex drafts out of persisted settings; Rust is authoritative. */
export function ClipboardPatterns({ value, onSave }: { value: string[]; onSave(patch: SettingsPatch): Promise<unknown> }) {
  const [draft, setDraft] = useState(value.join('\n')), [error, setError] = useState(''), [busy, setBusy] = useState(false);
  const previous = useRef(value.join('\n'));
  useEffect(() => { const next = value.join('\n'); if (draft === previous.current) setDraft(next); previous.current = next; }, [value]);
  return <form className="settings-draft-field" onSubmit={event => {
    event.preventDefault(); setBusy(true); setError('');
    void onSave({ clipboardDenyPatterns: draft === '' ? [] : draft.split('\n') }).catch(e => setError(message(e))).finally(() => setBusy(false));
  }}>
    <textarea aria-label="Clipboard exclusion patterns" rows={3} spellCheck={false} value={draft} onChange={event => { setDraft(event.target.value); setError(''); }} />
    <button type="submit" className="link" disabled={busy || draft === value.join('\n')}>Save patterns</button>
    {error && <p className="settings-field-error" role="alert">{error}</p>}
  </form>;
}

/** Search runs only on submit, not while typing, enabling weather, or opening Settings. */
export function WeatherLocation({ value, onSave }: { value: GeoPlace | null; onSave(patch: SettingsPatch): Promise<unknown> }) {
  const [query, setQuery] = useState(''), [places, setPlaces] = useState<GeoPlace[]>([]), [error, setError] = useState(''), [busy, setBusy] = useState(false);
  const generation = useRef(0);
  useEffect(() => () => { generation.current++; }, []);
  const save = async (place: GeoPlace | null) => {
    setError('');
    try { await onSave({ weatherLocation: place }); setPlaces([]); }
    catch (e) { setError(message(e)); }
  };
  return <div className="settings-draft-field">
    <span className="muted">{value ? `${value.name} · ${value.latitude}, ${value.longitude} · ${value.country}` : 'No location selected'}</span>
    <form onSubmit={event => {
      event.preventDefault(); const id = ++generation.current; setError(''); setBusy(true);
      void commands.weatherGeocode(query.trim()).then(result => {
        if (id !== generation.current) return;
        if (result.status === 'error') throw new Error(message(result.error));
        setPlaces(result.data); if (!result.data.length) setError('No matching places');
      }).catch(e => { if (id === generation.current) setError(message(e)); }).finally(() => { if (id === generation.current) setBusy(false); });
    }}>
      <input type="search" aria-label="Find weather location" placeholder="City or place" value={query} maxLength={256} onChange={event => setQuery(event.target.value)} />
      <button type="submit" className="link" disabled={busy || !query.trim()}>{busy ? 'Searching…' : 'Find'}</button>
    </form>
    {places.length > 0 && <div className="settings-place-results" aria-label="Weather locations">{places.map((place, index) =>
      <button type="button" className="link" key={index} onClick={() => { void save(place); }}>{place.name} · {place.latitude}, {place.longitude} · {place.country}</button>)}</div>}
    <button type="button" className="link" disabled={!value} onClick={() => { void save(null); }}>Clear location</button>
    {error && <p className="settings-field-error" role="alert">{error}</p>}
  </div>;
}

export function DisplayChoice({ value, outputs, onChange }: { value: Settings['displayId']; outputs: string[] | null; onChange(value: string | null): void }) {
  const names = [...new Set([...(outputs ?? []), ...(value ? [value] : [])])];
  return <Dropdown label="Island display" value={value ?? ''} options={[{ value: '', label: 'Default display' }, ...names.map(name => ({ value: name, label: name }))]}
    onChange={name => onChange(name || null)} />;
}
