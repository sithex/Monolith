import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from 'react';
import { AnimatePresence, motion, useReducedMotion } from 'framer-motion';
import { useSettings, runtimeStore } from '../../stores';
import { useIslandState, dispatch } from '../../ipc/island-state';
import { changePopup, listenPopups, usePopups } from '../../ipc/notifications';
import type { Popup, PopupOp } from '../../ipc/client';
import { nativePill } from '../geometry';
import './notification-cards.css';

const surfacesChanged = () => window.dispatchEvent(new Event('island-surfaces-changed'));
export function NotificationCard({ popup, redact = false, perform, changed = surfacesChanged }: {
  popup: Popup; redact?: boolean; perform(op: PopupOp): void; changed?(): void;
}) {
  const n = popup.notification;
  const [hovered, setHovered] = useState(false), [focused, setFocused] = useState(false);
  const held = useRef(false), focusedNow = useRef(false);
  focusedNow.current = focused;
  const pause = (value: boolean) => {
    if (held.current !== value) { held.current = value; perform({ type: 'pause', id: n.id, paused: value }); }
  };
  useEffect(() => {
    const left = () => { setHovered(false); pause(focusedNow.current); };
    addEventListener('island-pointer-left', left);
    return () => { removeEventListener('island-pointer-left', left); if (held.current) perform({ type: 'pause', id: n.id, paused: false }); };
  }, [n.id]);
  const open = () => perform({ type: 'invoke', id: n.id, key: 'default' });
  const actions = n.actions.filter(a => a.key !== 'default' && a.key !== 'inline-reply');
  const critical = n.urgency === 'critical';
  return <motion.article className="notification-card" data-island-extension="notification" data-critical={critical}
    data-testid="notification-toast" role={critical ? 'alert' : 'status'} onUpdate={changed}
    onPointerEnter={() => { setHovered(true); pause(true); }} onPointerLeave={() => { setHovered(false); pause(focused); }}
    onFocusCapture={() => { setFocused(true); pause(true); }} onBlurCapture={e => {
      if (!e.currentTarget.contains(e.relatedTarget)) { setFocused(false); pause(hovered); }
    }}>
    <div className="notification-card-head">
      {n.iconPng?.startsWith('data:image/png;base64,') ? <img className="notification-icon" src={n.iconPng} alt="" onError={e => { e.currentTarget.hidden = true; }} />
        : <span className="notification-icon notification-monogram" aria-hidden="true">{n.appName.slice(0, 1).toLocaleUpperCase()}</span>}
      <span className="notification-app">{n.appName}</span><span className="notification-priority">{critical ? 'Critical' : 'Now'}</span>
      <button className="notification-close" aria-label={`Dismiss ${n.title}`} onClick={() => perform({ type: 'dismiss', id: n.id })}>×</button>
    </div>
    <button className="notification-content" onClick={open} aria-label={`Open ${n.appName}: ${n.title}`}>
      <strong>{n.title}</strong>{!redact && n.body && <span>{n.body}</span>}
    </button>
    {n.canInvokeActions && (actions.length > 0 || n.actions.some(a => a.key === 'inline-reply')) && <div className="notification-actions">
      {actions.slice(0, 4).map(a => <button key={a.key} onClick={() => perform({ type: 'invoke', id: n.id, key: a.key })}>{a.label}</button>)}
      {n.actions.some(a => a.key === 'inline-reply') && <button title="Plasma does not expose inline reply to other notification clients" onClick={open}>Reply in app ↗</button>}
    </div>}
    {!critical && popup.remainingMs !== null && <div className="notification-timer" aria-hidden="true"><span key={`${popup.remainingMs}:${popup.paused}`}
      style={{ '--remaining': popup.remainingMs / popup.ttlMs, animationDuration: `${popup.remainingMs}ms`, animationPlayState: hovered || focused || popup.paused ? 'paused' : 'running' } as CSSProperties} /></div>}
  </motion.article>;
}
export function NotificationCards({ fail }: { fail(error: unknown): void }) {
  const settings = useSettings(), state = useIslandState(), snapshot = usePopups();
  const reduced = useReducedMotion() === true || settings.reduceMotion;
  const [, resized] = useState(0);
  const [fixture, setFixture] = useState<Popup[] | null>(null);
  useEffect(() => {
    if (!runtimeStore.getSnapshot().testHooks) return;
    Object.defineProperty(window, '__islandNotificationsTest', { configurable: true, value: (rows: Popup[] | null) => setFixture(rows) });
    return () => { delete (window as unknown as Record<string, unknown>).__islandNotificationsTest; };
  }, []);
  useEffect(() => { void listenPopups().catch(fail); }, []);
  useEffect(() => { const resize = () => resized(n => n + 1); addEventListener('resize', resize); addEventListener('island-native-pill', resize); return () => { removeEventListener('resize', resize); removeEventListener('island-native-pill', resize); }; }, []);
  useLayoutEffect(() => { surfacesChanged(); }, [snapshot, fixture, state.mode]);
  const pill = nativePill();
  const width = Math.min(390, innerWidth - 24), left = Math.max(12, Math.min(innerWidth - width - 12, (pill ? pill.x + pill.width / 2 : innerWidth / 2) - width / 2));
  const shown = settings.notificationsEnabled && !state.dnd && !state.gameMode ? (fixture ?? snapshot.items) : [];
  const duration = reduced ? 0 : .18 / Math.max(.25, (settings.animationSpeed ?? 100) / 100);
  const perform = (op: PopupOp) => {
    const popup = 'id' in op ? snapshot.items.find(p => p.notification.id === op.id) : null;
    if (op.type === 'invoke' && popup && popup.notification.source !== 'system') {
      void dispatch({ type: 'selectView', view: popup?.notification.appName === 'Agents' ? 'agents' : 'notifications' }).then(() => changePopup({ type: 'dismiss', id: op.id })).catch(fail);
    } else void changePopup(op).catch(fail);
  };
  const visibleCount = Math.max(1, Math.min(3, Math.floor((innerHeight - (pill ? pill.y + pill.height : 30) - 60) / 205)));
  const hiddenIds = JSON.stringify(snapshot.items.slice(visibleCount).map(p => p.notification.id));
  useEffect(() => {
    const ids = JSON.parse(hiddenIds) as string[];
    for (const id of ids) void changePopup({ type: 'pause', id, paused: true }).catch(fail);
    return () => { for (const id of ids) void changePopup({ type: 'pause', id, paused: false }).catch(fail); };
  }, [hiddenIds]);
  return <section className="notification-stack" aria-label="Notification popups" style={{ left, top: (pill ? pill.y + pill.height : 30) + 12, width, maxHeight: innerHeight - (pill?.y ?? 0) - 60 }}>
    <AnimatePresence onExitComplete={surfacesChanged}>
      {shown.slice(0, visibleCount).map(p => <motion.div key={p.notification.id} initial={{ opacity: 0, y: reduced ? 0 : -12 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: reduced ? 0 : -8 }} transition={{ duration }} onUpdate={surfacesChanged}>
        <NotificationCard popup={p} redact={settings.redactBodies} perform={perform} />
      </motion.div>)}
    </AnimatePresence>
    {shown.length > visibleCount && <button className="notification-overflow" data-island-extension="notification" onClick={() => { void dispatch({ type: 'selectView', view: 'notifications' }).catch(fail); }}>+{shown.length - visibleCount} more · Open history</button>}
  </section>;
}
