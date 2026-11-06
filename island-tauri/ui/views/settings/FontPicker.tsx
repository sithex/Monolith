// Searchable dropdown of installed font families (Rust `font_families`, fontconfig). Each
// entry is drawn in its own font, but only the visible window is rendered (font-list.ts
// caps it at FONT_MAX_RENDERED rows), so hundreds of fonts never load at once.
import { useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from 'react';
import type { FontFamily } from '../../ipc/bindings';
import { DEFAULT_FONT_LABEL, FONT_ROW_PX, fontEntries, fontWindow, previewFamily, scrollToRow } from './font-list';

const LIST_PX = 256;

export function FontPicker({ label, value, fonts, monoDefault = true, onChange, error }: {
  label: string; value: string; fonts: readonly FontFamily[] | null; monoDefault?: boolean;
  onChange(value: string): void; error?: string | null;
}) {
  const [open, setOpen] = useState(false), [query, setQuery] = useState(''), [monoOnly, setMonoOnly] = useState(monoDefault);
  const [focus, setFocus] = useState(0), [scrollTop, setScrollTop] = useState(0), [up, setUp] = useState(false);
  const root = useRef<HTMLDivElement>(null), list = useRef<HTMLDivElement>(null), search = useRef<HTMLInputElement>(null), trigger = useRef<HTMLButtonElement>(null);
  const id = useId();
  const entries = useMemo(() => fontEntries(fonts ?? [], query, monoOnly, value), [fonts, query, monoOnly, value]);
  const { start, end } = fontWindow(scrollTop, LIST_PX, entries.length);
  useEffect(() => {
    if (!open) return;
    const away = (e: PointerEvent) => { if (!root.current?.contains(e.target as Node)) setOpen(false); };
    document.addEventListener('pointerdown', away, true);
    return () => document.removeEventListener('pointerdown', away, true);
  }, [open]);
  useLayoutEffect(() => { if (open) search.current?.focus(); }, [open]);
  // A new filter restarts at the top with the first match focused.
  useEffect(() => { setFocus(0); setScrollTop(0); if (list.current) list.current.scrollTop = 0; }, [query, monoOnly]);
  const show = () => {
    const r = root.current?.getBoundingClientRect(), pane = root.current?.closest('.app-pane, .settings-view')?.getBoundingClientRect();
    const need = LIST_PX + 60;
    setUp(!!r && !!pane && pane.bottom - r.bottom < need && r.top - pane.top > pane.bottom - r.bottom);
    setQuery('');
    const at = Math.max(0, fontEntries(fonts ?? [], '', monoOnly, value).findIndex(e => e.value === value));
    setFocus(at);
    const top = Math.max(0, at * FONT_ROW_PX - LIST_PX / 2);
    setScrollTop(top); setOpen(true);
    requestAnimationFrame(() => { if (list.current) list.current.scrollTop = top; });
  };
  const pick = (v: string) => { setOpen(false); trigger.current?.focus(); if (v !== value) onChange(v); };
  const move = (to: number) => {
    const index = Math.max(0, Math.min(entries.length - 1, to));
    setFocus(index);
    const el = list.current;
    if (el) { const top = scrollToRow(index, el.scrollTop, el.clientHeight || LIST_PX); el.scrollTop = top; setScrollTop(top); }
  };
  const current = value ? value : DEFAULT_FONT_LABEL;
  return <div className="dropdown font-picker" ref={root} data-open={open} data-up={up} onKeyDown={e => {
    if (!open) { if (['ArrowDown', 'ArrowUp', 'Enter', ' '].includes(e.key) && e.target === trigger.current) { e.preventDefault(); show(); } return; }
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') { e.preventDefault(); move(focus + (e.key === 'ArrowDown' ? 1 : -1)); }
    else if (e.key === 'PageDown' || e.key === 'PageUp') { e.preventDefault(); move(focus + (e.key === 'PageDown' ? 8 : -8)); }
    else if ((e.key === 'Home' || e.key === 'End') && e.target !== search.current) { e.preventDefault(); move(e.key === 'Home' ? 0 : entries.length - 1); }
    else if (e.key === 'Enter') { e.preventDefault(); const entry = entries[focus]; if (entry) pick(entry.value); }
    else if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); setOpen(false); trigger.current?.focus(); }
    else if (e.key === 'Tab') setOpen(false);
  }}>
    <button ref={trigger} type="button" className="dropdown-trigger" aria-label={label} aria-haspopup="listbox" aria-expanded={open} aria-controls={id}
      aria-invalid={error ? true : undefined} onClick={() => open ? setOpen(false) : show()}>
      <span className="dropdown-value" style={{ fontFamily: value ? `${value}, monospace` : undefined }}>{current}</span>
      <svg className="dropdown-chevron" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="m6 9 6 6 6-6" /></svg>
    </button>
    {open && <div className="dropdown-menu font-menu">
      <div className="font-search">
        <input ref={search} type="search" aria-label={`Search ${label}`} placeholder={fonts ? `Search ${fonts.length} fonts` : 'Loading fonts…'} value={query}
          role="combobox" aria-expanded="true" aria-controls={id} aria-activedescendant={entries[focus] ? `${id}-${focus}` : undefined}
          spellCheck={false} autoComplete="off" onChange={e => setQuery(e.target.value)} />
        <label className="font-mono-toggle"><input type="checkbox" checked={monoOnly} onChange={e => setMonoOnly(e.target.checked)} />Monospace</label>
      </div>
      <div className="font-list" ref={list} role="listbox" id={id} aria-label={label} style={{ maxHeight: LIST_PX }}
        onScroll={e => setScrollTop(e.currentTarget.scrollTop)}>
        <div style={{ height: entries.length * FONT_ROW_PX, position: 'relative' }}>
          {entries.slice(start, end).map((entry, i) => {
            const index = start + i;
            return <div key={entry.value || '\u0000default'} id={`${id}-${index}`} role="option" aria-selected={entry.value === value} data-focus={index === focus}
              className="font-option" style={{ top: index * FONT_ROW_PX, height: FONT_ROW_PX, fontFamily: previewFamily(entry) }}
              onPointerEnter={() => setFocus(index)} onClick={() => pick(entry.value)}>
              <span>{entry.label}</span>
              {entry.value === value && <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="m5 12 5 5 9-10" /></svg>}
            </div>;
          })}
        </div>
        {!entries.length && <p className="font-empty">{fonts ? 'No matching fonts' : 'Loading…'}</p>}
      </div>
    </div>}
  </div>;
}
