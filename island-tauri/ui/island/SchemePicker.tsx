import { useMemo, useRef, useState, type KeyboardEvent } from 'react';
import { AnimatePresence, motion } from 'framer-motion';
import { useReducedMotionAll } from './components/motion-preference';
import type { Appearance, Settings, SettingsPatch } from '../ipc/client';
import { customPalette, initialOpenFamily, navTarget, schemeFamilies, type Family, type Scheme } from './tokens/schemes';
import { ColorSwatch } from './components/ColorField';
import { useAlbumPreview } from './album-scheme';
import { DUR, SPRING, tween } from './motion-profile';

type SeedKey = 'customAccent' | 'customBackground' | 'customText';
const SEEDS: [SeedKey, string, 'accent' | 'background' | 'text'][] = [
  ['customAccent', 'Accent', 'accent'], ['customBackground', 'Background', 'background'], ['customText', 'Text', 'text']];
const NAV = '[data-scheme-nav]';

/**
 * Colour scheme list, one row per family (Nov 6). Families with variants (Monolith presets,
 * Catppuccin) expand in place, one at a time; the active scheme's family starts open, and a
 * collapsed family shows its active variant. Single-scheme families are picked directly.
 */
export function SchemePicker({ value, onPick, settings, patch }: {
  value: Appearance; onPick(id: Appearance): void; settings: Settings; patch(value: SettingsPatch): void;
}) {
  const custom = customPalette(settings), album = useAlbumPreview();
  const families = useMemo(() => schemeFamilies(), []);
  const [open, setOpen] = useState<string | null>(() => initialOpenFamily(value, families));
  const reduced = useReducedMotionAll() || settings.reduceMotion === true;
  const list = useRef<HTMLDivElement>(null);
  const keyboard = typeof document !== 'undefined' && document.documentElement.dataset.navVia === 'key';
  const fade = { ...tween(DUR.base), duration: Math.min(.3, tween(DUR.base).duration) };
  const expand = keyboard || reduced ? { duration: 0 } : fade;
  const glide = reduced || keyboard ? { duration: 0 } : SPRING.ui();
  // Accordion rows push the list below, so height animates (rare, user-initiated) to avoid a jump.
  const hidden = { opacity: 0, height: 0 };
  const variants = { closed: ({ keyboard }: { keyboard: boolean; reduced: boolean }) => ({
    opacity: 0, height: 0, transition: keyboard ? { duration: 0 } : fade,
  }) };

  const look = (s: Scheme): { dots: readonly string[]; note: string } => s.id === 'custom' ? { dots: [custom.surface, custom.accent, custom.text], note: `accent ${custom.accent}` }
    : s.id === 'album' ? { dots: album ? [album.surface, album.accent, album.text] : s.dots, note: album ? `accent ${album.accent} · from artwork` : 'no artwork · follows Hermes skin' }
    : { dots: s.dots, note: s.note };
  const dotsOf = (dots: readonly string[]) => <span className="scheme-dots" aria-hidden="true">{dots.map((c, i) => <i key={i} style={{ background: c }} />)}</span>;
  const pick = (id: Appearance) => { if (id !== value) onPick(id); };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const rows = [...(list.current?.querySelectorAll<HTMLElement>(NAV) ?? [])];
    const current = rows.indexOf(document.activeElement as HTMLElement);
    const target = event.target as HTMLElement;
    const family = target.dataset.family;
    if ((event.key === 'ArrowRight' || event.key === 'ArrowLeft') && family) {
      const header = list.current?.querySelector<HTMLElement>(`[data-family-header="${family}"]`);
      if (event.key === 'ArrowRight' && header === target) setOpen(family);
      else if (event.key === 'ArrowLeft') { if (header === target) setOpen(null); else header?.focus(); }
      event.preventDefault();
      return;
    }
    const next = navTarget(event.key, current < 0 ? -1 : current, rows.length);
    if (next === null) return;
    event.preventDefault();
    rows[current < 0 && event.key === 'ArrowUp' ? 0 : next]?.focus();
  };

  const radio = (s: Scheme, variant: boolean) => {
    const on = s.id === value, { dots, note } = look(s);
    return <div key={s.id} className="scheme-item">
      <button type="button" role="radio" aria-checked={on} className={`scheme-row${variant ? ' scheme-variant' : ''}`} data-testid={`appearance-${s.id}`}
        data-scheme-nav="" data-family={variant ? s.family : undefined} onClick={() => pick(s.id)}>
        {on && <motion.span layoutId="scheme-selected" className="scheme-selected" transition={glide} />}
        {dotsOf(dots)}
        <span className="scheme-name">{s.name}</span>
        <span className="scheme-note">{note}</span>
      </button>
      <AnimatePresence initial={false} custom={{ keyboard, reduced }}>{s.id === 'custom' && on && <motion.div key="custom" className="scheme-custom" initial={hidden} animate={{ opacity: 1, height: 'auto' }} exit="closed" variants={variants} transition={expand}>
        {SEEDS.map(([key, label, role]) => <div key={key} className="scheme-seed">
          <ColorSwatch id={`scheme-${key}`} label={`Custom ${label.toLowerCase()} colour`} value={custom[role]} onPick={hex => patch({ [key]: hex })} />
          <span>{label}</span><code>{custom[role]}</code>
        </div>)}
        <button className="link" type="button" onClick={() => patch({ customAccent: '', customBackground: '', customText: '' })}>Reset</button>
      </motion.div>}</AnimatePresence>
    </div>;
  };

  const group = (f: Family) => {
    const active = f.schemes.find(s => s.id === value), isOpen = open === f.id;
    const shown = active ?? f.schemes[0]!;
    return <div key={f.id} className="scheme-item scheme-family" data-open={isOpen}>
      <button type="button" className="scheme-row scheme-family-row" aria-expanded={isOpen} aria-controls={`scheme-family-${f.id}`}
        data-testid={`scheme-family-${f.id}`} data-scheme-nav="" data-family={f.id} data-family-header={f.id} data-active={!!active}
        onClick={() => setOpen(isOpen ? null : f.id)}>
        {active && !isOpen && <motion.span layoutId="scheme-selected" className="scheme-selected" transition={glide} />}
        {dotsOf(look(shown).dots)}
        <span className="scheme-name">{f.name}</span>
        <span className="scheme-note">{active ? shown.name : `${f.schemes.length} ${f.id === 'monolith' ? 'schemes' : 'flavours'}`}</span>
        <svg className="scheme-chevron" viewBox="0 0 12 12" aria-hidden="true"><path d="M4 2.5 7.5 6 4 9.5" /></svg>
      </button>
      <AnimatePresence initial={false} custom={{ keyboard, reduced }}>
        {isOpen && <motion.div key="variants" id={`scheme-family-${f.id}`} className="scheme-variants" role="group" aria-label={`${f.name} variants`}
          initial={hidden} animate={{ opacity: 1, height: 'auto' }} exit="closed" variants={variants} transition={expand}>
          {f.schemes.map(s => radio(s, true))}
        </motion.div>}
      </AnimatePresence>
    </div>;
  };

  let community = false;
  return <div className="scheme-list" role="radiogroup" aria-label="Colour scheme" ref={list} onKeyDown={onKeyDown}>
    {families.map(f => {
      const label = f.group === 'community' && !community ? (community = true, <div key="community-label" className="scheme-group-label" role="presentation">Community</div>) : null;
      return [label, f.schemes.length > 1 ? group(f) : radio(f.schemes[0]!, false)];
    })}
  </div>;
}
