import { useMemo, type KeyboardEvent } from 'react';
import { AnimatePresence, motion } from 'framer-motion';
import { useReducedMotionAll } from './components/motion-preference';
import type { Appearance, Settings, SettingsPatch } from '../ipc/client';
import { customPalette, gridTarget, schemeFamilies, schemeGrids, schemeLabel, type Scheme } from './tokens/schemes';
import { ColorSwatch } from './components/ColorField';
import { useAlbumPreview } from './album-scheme';
import { DUR, tween } from './motion-profile';

type SeedKey = 'customAccent' | 'customBackground' | 'customText';
const SEEDS: [SeedKey, string, 'accent' | 'background' | 'text'][] = [
  ['customAccent', 'Accent', 'accent'], ['customBackground', 'Background', 'background'], ['customText', 'Text', 'text']];
const NAV = '[data-scheme-nav]';

/**
 * Colour scheme picker: a swatch grid, one tile per scheme (Nov 16), the selected tile ringed in
 * accent. Every scheme is one click away with its name on the tile; variants carry their family
 * ("Gruvbox Dark"). One radiogroup with a single Tab stop: arrows move through the grid, Enter
 * or Space picks. The Custom seed colours open under the grid while Custom is selected.
 */
export function SchemePicker({ value, onPick, settings, patch }: {
  value: Appearance; onPick(id: Appearance): void; settings: Settings; patch(value: SettingsPatch): void;
}) {
  const custom = customPalette(settings), album = useAlbumPreview();
  const families = useMemo(() => schemeFamilies(), []);
  const grids = useMemo(() => schemeGrids(families), [families]);
  const reduced = useReducedMotionAll() || settings.reduceMotion === true;
  const keyboard = typeof document !== 'undefined' && document.documentElement.dataset.navVia === 'key';
  const fade = { ...tween(DUR.base), duration: Math.min(.3, tween(DUR.base).duration) };
  const expand = keyboard || reduced ? { duration: 0 } : fade;
  // The seed editor pushes the rows below, so height animates (rare, user-initiated) to avoid a jump.
  const hidden = { opacity: 0, height: 0 };
  const variants = { closed: ({ keyboard }: { keyboard: boolean; reduced: boolean }) => ({
    opacity: 0, height: 0, transition: keyboard ? { duration: 0 } : fade,
  }) };

  const look = (s: Scheme): { dots: readonly string[]; note: string } => s.id === 'custom' ? { dots: [custom.surface, custom.accent, custom.text], note: `accent ${custom.accent}` }
    : s.id === 'album' ? { dots: album ? [album.surface, album.accent, album.text] : s.dots, note: album ? `accent ${album.accent} · from artwork` : 'no artwork · follows Hermes skin' }
    : { dots: s.dots, note: s.note };
  const dotsOf = (dots: readonly string[]) => <span className="scheme-dots" aria-hidden="true">{dots.map((c, i) => <i key={i} style={{ background: c }} />)}</span>;
  const pick = (id: Appearance) => { if (id !== value) onPick(id); };
  // Roving tabindex: the selected swatch is the group's one Tab stop (the first, if none matches).
  const all = grids.flatMap(grid => grid.schemes), stop = all.some(s => s.id === value) ? value : all[0]?.id;

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const group = event.currentTarget, swatches = [...group.querySelectorAll<HTMLElement>(NAV)];
    const tracks = [...group.querySelectorAll<HTMLElement>('.scheme-grid')];
    const columns = tracks[0] ? getComputedStyle(tracks[0]).gridTemplateColumns.split(' ').filter(Boolean).length : 1;
    const next = gridTarget(event.key, swatches.indexOf(document.activeElement as HTMLElement), tracks.map(track => track.querySelectorAll(NAV).length), columns);
    if (next === null) return;
    event.preventDefault();
    swatches[next]?.focus();
  };

  const swatch = (s: Scheme) => {
    const on = s.id === value, { dots, note } = look(s), name = schemeLabel(s, families);
    return <button key={s.id} type="button" role="radio" aria-checked={on} aria-label={name} data-tooltip={`${name} · ${note}`} tabIndex={s.id === stop ? 0 : -1}
      className="scheme-swatch" data-testid={`appearance-${s.id}`} data-scheme-nav="" onClick={() => pick(s.id)}>
      {dotsOf(dots)}
      <span className="scheme-name">{name}</span>
    </button>;
  };

  return <div className="scheme-picker">
    <div className="scheme-list" role="radiogroup" aria-label="Colour scheme" onKeyDown={onKeyDown}>
      {grids.map(grid => [
        grid.id === 'community' && <div key="community-label" className="scheme-group-label" role="presentation">Community</div>,
        <div key={grid.id} className="scheme-grid" role="presentation" data-scheme-grid={grid.id}>{grid.schemes.map(swatch)}</div>,
      ])}
    </div>
    <AnimatePresence initial={false} custom={{ keyboard, reduced }}>{value === 'custom' && <motion.div key="custom" className="scheme-custom" initial={hidden} animate={{ opacity: 1, height: 'auto' }} exit="closed" variants={variants} transition={expand}>
      {SEEDS.map(([key, label, role]) => <div key={key} className="scheme-seed">
        <ColorSwatch id={`scheme-${key}`} label={`Custom ${label.toLowerCase()} colour`} value={custom[role]} onPick={hex => patch({ [key]: hex })} />
        <span>{label}</span><code>{custom[role]}</code>
      </div>)}
      <button className="link" type="button" onClick={() => patch({ customAccent: '', customBackground: '', customText: '' })}>Reset</button>
    </motion.div>}</AnimatePresence>
  </div>;
}
