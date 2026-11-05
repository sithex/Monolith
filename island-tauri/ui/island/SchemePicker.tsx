import { motion } from 'framer-motion';
import type { Appearance, Settings, SettingsPatch } from '../ipc/client';
import { SCHEMES, customPalette } from './tokens/schemes';
import { ColorSwatch } from './components/ColorField';
import { useAlbumPreview } from './album-scheme';
import { uiSpring } from './motion-profile';

type SeedKey = 'customAccent' | 'customBackground' | 'customText';
const SEEDS: [SeedKey, string, 'accent' | 'background' | 'text'][] = [
  ['customAccent', 'Accent', 'accent'], ['customBackground', 'Background', 'background'], ['customText', 'Text', 'text']];

/** Colour scheme list (Nick's concept: three stacked dots, name, mono note, raised selected row). */
export function SchemePicker({ value, onPick, settings, patch }: {
  value: Appearance; onPick(id: Appearance): void; settings: Settings; patch(value: SettingsPatch): void;
}) {
  const custom = customPalette(settings), album = useAlbumPreview();
  return <div className="scheme-list" role="radiogroup" aria-label="Colour scheme">
    {SCHEMES.map(s => {
      const on = s.id === value;
      const dots = s.id === 'custom' ? [custom.surface, custom.accent, custom.text]
        : s.id === 'album' && album ? [album.surface, album.accent, album.text] : s.dots;
      const note = s.id === 'custom' ? `accent ${custom.accent}` : s.id === 'album' ? (album ? `accent ${album.accent} · from artwork` : 'no artwork · follows Hermes skin') : s.note;
      return <div key={s.id} className="scheme-item">
        <button role="radio" aria-checked={on} className="scheme-row" data-testid={`appearance-${s.id}`} onClick={() => { if (!on) onPick(s.id); }}>
          {on && <motion.span layoutId="scheme-selected" className="scheme-selected" transition={uiSpring(.28, .12)} />}
          <span className="scheme-dots" aria-hidden="true">{dots.map((c, i) => <i key={i} style={{ background: c }} />)}</span>
          <span className="scheme-name">{s.name}</span>
          <span className="scheme-note">{note}</span>
        </button>
        {s.id === 'custom' && on && <motion.div className="scheme-custom" initial={{ opacity: 0, height: 0 }} animate={{ opacity: 1, height: 'auto' }}
          transition={{ duration: .22, ease: [.22, 1, .36, 1] }}>
          {SEEDS.map(([key, label, role]) => <div key={key} className="scheme-seed">
            <ColorSwatch id={`scheme-${key}`} label={`Custom ${label.toLowerCase()} colour`} value={custom[role]} onPick={hex => patch({ [key]: hex })} />
            <span>{label}</span><code>{custom[role]}</code>
          </div>)}
          <button className="link" type="button" onClick={() => patch({ customAccent: '', customBackground: '', customText: '' })}>Reset</button>
        </motion.div>}
      </div>;
    })}
  </div>;
}
