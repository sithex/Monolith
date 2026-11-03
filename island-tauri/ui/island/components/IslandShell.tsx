import { useId, type ReactNode } from 'react';
import { AnimatePresence, motion, useReducedMotion } from 'framer-motion';
import type { IslandIntents } from '../models';
import { Icon } from './Icon';
export const UI_SPRING = { stiffness: 350, damping: 32, mass: 0.8 };
/**
 * Page chrome: back button + title. Back returns to the page you came from
 * (Island keeps the history); on the Hermes home it shows the terminal glyph.
 */
export function IslandShell({ title, children, intents, home = false, back, actions }: {
  title: string; children: ReactNode; intents: IslandIntents; home?: boolean;
  back?: { label: string; go(): void }; actions?: ReactNode;
}) {
  const id = useId(), reduced = useReducedMotion();
  return <section className="island-shell" aria-labelledby={id}>
    <header className="title-row">
      <button className="icon-button" aria-label={home ? 'Hermes home' : back ? `Back to ${back.label}` : 'Back to Hermes'} title={home ? undefined : back ? `Back to ${back.label}` : 'Back'}
        onClick={() => { if (home) return; if (back) back.go(); else intents.navigate('terminal'); }}><Icon name={home ? 'terminal' : 'back'} /></button>
      <h2 id={id} className="title-slot"><AnimatePresence mode="popLayout" initial={false}>
        <motion.span key={title} initial={reduced ? false : { opacity: 0, y: 6, filter: 'blur(4px)' }} animate={{ opacity: 1, y: 0, filter: 'blur(0px)' }}
          exit={reduced ? { opacity: 0, transition: { duration: 0 } } : { opacity: 0, y: -6, filter: 'blur(4px)' }} transition={{ duration: .2, ease: [.22, 1, .36, 1] }}>{title}</motion.span>
      </AnimatePresence></h2>
      <span className="title-spacer" />
      {actions}
    </header>
    <div className="shell-content">{children}</div>
  </section>;
}
export function TerminalHome({ children }: { children?: ReactNode }) {
  return <div className="terminal-home" aria-label="Hermes terminal placeholder">
    {children ?? <><div className="terminal-mark">H</div><h3>Your agent. At home.</h3><p>This is the terminal slot.</p><p className="muted">Phase 7 mounts the retained xterm session here.</p><div className="terminal-prompt"><span>hermes ›</span> <span className="muted">What shall we build?</span><span className="caret" /></div></>}
  </div>;
}
