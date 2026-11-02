import { useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { motion } from 'framer-motion';
import type { AppTile, IslandIntents } from '../models';
import type { ArrangeAction, ArrangeState } from '../arrangement';
import { Icon } from './Icon';
const query = '(prefers-reduced-motion: reduce)';
const subscribeMotion = (notify: () => void) => {
  const media = window.matchMedia(query); media.addEventListener('change', notify);
  return () => media.removeEventListener('change', notify);
};
export function useReducedMotionPreference() {
  return useSyncExternalStore(subscribeMotion, () => window.matchMedia(query).matches, () => true);
}
export function AppGrid({ apps, state, dispatch, intents }: { apps: AppTile[]; state: ArrangeState; dispatch(action: ArrangeAction): void; intents: IslandIntents }) {
  const editing = state.snapshot !== null;
  const reduced = useReducedMotionPreference();
  const [menu, setMenu] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const start = useRef({ x: 0, y: 0 });
  const held = useRef(false);
  const menuButton = useRef<HTMLButtonElement>(null);
  const cancelHold = () => { if (timer.current !== null) clearTimeout(timer.current); timer.current = null; };
  useEffect(() => cancelHold, []);
  const visible = state.items.filter(i => i.enabled);
  const hidden = state.items.filter(i => !i.enabled);
  const finish = (type: 'cancel' | 'commit') => {
    cancelHold(); dispatch({ type });
    // Return focus to a stable control if Done/Cancel (or a badge) disappears.
    menuButton.current?.focus();
  };
  return <div className="launcher" onKeyDown={e => {
    if (e.key === 'Escape') {
      e.stopPropagation();
      if (editing) finish('cancel');
      setMenu(false); menuButton.current?.focus();
    }
  }}>
    <div className="launcher-toolbar">
      <span className="muted" id="launcher-help">{editing ? 'Drag to arrange · arrows move one place' : 'Your tools, one click away'}</span>
      {editing && <><button onClick={() => finish('cancel')}>Cancel</button><button className="primary small" onClick={() => finish('commit')}>Done</button></>}
      <div className="menu-anchor"><button ref={menuButton} className="icon-button" aria-label="Launcher options" aria-expanded={menu} onClick={() => setMenu(!menu)}><Icon name="more" /></button>
        {menu && <div className="popover"><button onClick={() => { if (!editing) dispatch({ type: 'begin' }); setMenu(false); menuButton.current?.focus(); }}>Edit apps</button></div>}
      </div>
    </div>
    <div className="app-grid" aria-label="Apps" aria-describedby="launcher-help" data-editing={editing}>
      {visible.map((item, index) => {
        const app = apps.find(a => a.id === item.id);
        if (!app) return null;
        return <motion.div key={item.id} className="tile-wrap" data-tile={item.id} data-wiggling={editing && !reduced}
          animate={{ rotate: editing && !reduced ? [-1.4, 1.4, -1.4] : 0 }}
          transition={editing && !reduced ? { duration: 0.26, repeat: Infinity, ease: 'easeInOut', delay: index % 3 * 0.03 } : { duration: 0 }}
          drag={editing} dragSnapToOrigin dragMomentum={false}
          onDragEnd={(_event, info) => {
            const target = document.elementFromPoint(info.point.x - window.scrollX, info.point.y - window.scrollY)?.closest<HTMLElement>('[data-tile]');
            const to = visible.findIndex(i => i.id === target?.dataset.tile);
            if (to >= 0) dispatch({ type: 'move', id: item.id, to });
          }}>
          <button className={`app-tile tone-${app.tone}`} aria-label={`Open ${app.label}`} aria-disabled={!app.known} data-locked={item.locked}
            onPointerDown={e => {
              held.current = false;
              if (editing || e.button !== 0) return;
              cancelHold(); start.current = { x: e.clientX, y: e.clientY };
              timer.current = setTimeout(() => { held.current = true; dispatch({ type: 'begin' }); }, 500);
            }}
            onPointerMove={e => { if (Math.hypot(e.clientX - start.current.x, e.clientY - start.current.y) > 8) cancelHold(); }}
            onPointerUp={cancelHold} onPointerCancel={cancelHold} onPointerLeave={cancelHold}
            onClick={() => { if (held.current) { held.current = false; return; } if (!editing && app.known) intents.navigate(item.id); }}>
            <Icon name={app.icon} /><span>{app.label}</span>{item.locked && <span className="tile-lock">Always here</span>}
          </button>
          {editing && <div className="tile-edit-controls">
            {!item.locked && <button className="disable-badge" aria-label={`Hide ${app.label}`} onPointerDown={e => e.stopPropagation()} onClick={() => { dispatch({ type: 'disable', id: item.id }); menuButton.current?.focus(); }}>−</button>}
            <div className="move-controls" onPointerDown={e => e.stopPropagation()}>
              <button aria-label={`Move ${app.label} earlier`} disabled={index === 0} onClick={() => dispatch({ type: 'move', id: item.id, to: index - 1 })}>←</button>
              <button aria-label={`Move ${app.label} later`} disabled={index === visible.length - 1} onClick={() => dispatch({ type: 'move', id: item.id, to: index + 1 })}>→</button>
            </div>
          </div>}
        </motion.div>;
      })}
    </div>
    {!!hidden.length && <div className="hidden-apps"><span className="muted">Hidden apps</span>{hidden.map(item => {
      const app = apps.find(a => a.id === item.id);
      return <button key={item.id} disabled={!app?.known} onClick={() => {
        if (!editing) dispatch({ type: 'begin' });
        dispatch({ type: 'enable', id: item.id });
      }}>+ {app?.label ?? item.id}{!app?.known && ' · unavailable module'}</button>;
    })}</div>}
    {state.error && <p role="alert">{state.error}</p>}
    <span role="status" className="sr-only">{editing ? 'Editing apps' : 'App editing closed'}</span>
  </div>;
}
