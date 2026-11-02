import { useId, useRef, useState, type ReactNode } from 'react';
import type { IslandIntents } from '../models';
import { Icon } from './Icon';
export const UI_SPRING = { stiffness: 350, damping: 32, mass: 0.8 };
export function IslandShell({ title, children, intents, home = false, menuItems = [] }: {
  title: string; children: ReactNode; intents: IslandIntents; home?: boolean;
  menuItems?: { label: string; action(): void }[];
}) {
  const id = useId();
  const [menu, setMenu] = useState(false);
  const more = useRef<HTMLButtonElement>(null);
  return <section className="island-shell" aria-labelledby={id} onKeyDown={e => {
    if (e.key === 'Escape' && menu) { e.stopPropagation(); setMenu(false); more.current?.focus(); }
  }}>
    <header className="title-row">
      <button className="icon-button" aria-label={home ? 'Open apps' : 'Back to Hermes'} onClick={() => intents.navigate(home ? 'apps' : 'terminal')}><Icon name={home ? 'grid' : 'back'} /></button>
      <h2 id={id}>{title}</h2>
      <span className="title-spacer" />
      <div className="menu-anchor">
        <button ref={more} className="icon-button" aria-label={`${title} options`} aria-expanded={menu} aria-controls={`${id}-options`} onClick={() => setMenu(!menu)}><Icon name="more" /></button>
        {menu && <div className="popover" id={`${id}-options`} aria-label={`${title} options list`}>
          {menuItems.map(item => <button key={item.label} onClick={() => { item.action(); setMenu(false); more.current?.focus(); }}>{item.label}</button>)}
          <button onClick={() => { intents.navigate(home ? 'apps' : 'terminal'); setMenu(false); }}>Go to {home ? 'apps' : 'Hermes'}</button>
        </div>}
      </div>
    </header>
    <div className="shell-content">{children}</div>
  </section>;
}
export function TerminalHome({ children }: { children?: ReactNode }) {
  return <div className="terminal-home" aria-label="Hermes terminal placeholder">
    {children ?? <><div className="terminal-mark">H</div><h3>Your agent. At home.</h3><p>This is the terminal slot.</p><p className="muted">Phase 7 mounts the retained xterm session here.</p><div className="terminal-prompt"><span>hermes ›</span> <span className="muted">What shall we build?</span><span className="caret" /></div></>}
  </div>;
}
