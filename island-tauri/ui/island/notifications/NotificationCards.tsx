import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from 'react';
import { AnimatePresence, motion, useReducedMotion } from 'framer-motion';
import { useSettings } from '../../stores';
import { useIslandState, dispatch } from '../../ipc/island-state';
import { changePopup, listenPopups, usePopups } from '../../ipc/notifications';
import type { Popup, PopupOp } from '../../ipc/client';
import './notification-cards.css';
import { MAX_STACK, setMeasured, useShownPopups } from './notification-hover';
export { useNotificationHover } from './notification-hover';

const surfacesChanged = () => window.dispatchEvent(new Event('island-surfaces-changed'));
export function NotificationCard({ popup, redact = false, perform, changed = surfacesChanged, inNotch = false }: {
  popup: Popup; redact?: boolean; perform(op: PopupOp): void; changed?(): void;
  /** Drawn inside the island surface: no separate input region, no floating card chrome. */
  inNotch?: boolean;
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
  return <motion.article className="notification-card" data-island-extension={inNotch ? undefined : 'notification'} data-in-notch={inNotch} data-critical={critical}
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
/** Popup cards as part of the notch. Hover: the notch grows into the stack (rendered in the
 *  hover preview slot). Open island: a banner pinned under the topline, inside the surface.
 *  Never a separate floating window. Mounted once; listens for popup snapshots. */
export function NotchNotifications({ fail, placement }: { fail(error: unknown): void; placement: 'hover' | 'open' }) {
  const settings = useSettings(), snapshot = usePopups(), shown = useShownPopups();
  const reduced = useReducedMotion() === true || settings.reduceMotion;
  const duration = reduced ? 0 : .18 / Math.max(.25, (settings.animationSpeed ?? 100) / 100);
  const perform = (op: PopupOp) => {
    const popup = 'id' in op ? snapshot.items.find(p => p.notification.id === op.id) : null;
    if (op.type === 'invoke' && popup && popup.notification.source !== 'system') {
      void dispatch({ type: 'selectView', view: popup?.notification.appName === 'Agents' ? 'agents' : 'notifications' }).then(() => changePopup({ type: 'dismiss', id: op.id })).catch(fail);
    } else void changePopup(op).catch(fail);
  };
  const visibleCount = placement === 'open' ? 1 : MAX_STACK;
  const hiddenIds = JSON.stringify(snapshot.items.slice(visibleCount).map(p => p.notification.id));
  useEffect(() => {
    const ids = JSON.parse(hiddenIds) as string[];
    for (const id of ids) void changePopup({ type: 'pause', id, paused: true }).catch(fail);
    return () => { for (const id of ids) void changePopup({ type: 'pause', id, paused: false }).catch(fail); };
  }, [hiddenIds]);
  const stack = useRef<HTMLElement | null>(null);
  useLayoutEffect(() => {
    if (placement !== 'hover' || !stack.current) return;
    const el = stack.current, measure = () => setMeasured(Math.ceil(el.scrollHeight));
    const ro = new ResizeObserver(measure); ro.observe(el); measure();
    return () => ro.disconnect();
  }, [placement]);
  const more = shown.length - visibleCount;
  return <section ref={stack} className="notch-notifications" data-placement={placement} aria-label="Notifications">
    <AnimatePresence initial={false} onExitComplete={surfacesChanged}>
      {shown.slice(0, visibleCount).map(p => <motion.div key={p.notification.id} layout={!reduced}
        initial={{ opacity: 0, y: reduced ? 0 : -8 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, transition: { duration: duration * .7 } }} transition={{ duration }}>
        <NotificationCard popup={p} redact={settings.redactBodies} perform={perform} inNotch />
      </motion.div>)}
    </AnimatePresence>
    {more > 0 && <button className="notification-overflow" onClick={() => { void dispatch({ type: 'selectView', view: 'notifications' }).catch(fail); }}>+{more} more · Open history</button>}
  </section>;
}

/** Subscribes to popup snapshots once and keeps the surface geometry in step. Renders nothing:
 *  the cards themselves live inside the island surface (NotchNotifications). */
export function NotificationCards({ fail }: { fail(error: unknown): void }) {
  const state = useIslandState(), snapshot = usePopups();
  useEffect(() => { void listenPopups().catch(fail); }, []);
  useLayoutEffect(() => { surfacesChanged(); }, [snapshot, state.mode]);
  return null;
}
