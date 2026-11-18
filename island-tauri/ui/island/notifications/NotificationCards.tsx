import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from 'react';
import { AnimatePresence, motion, useReducedMotion } from 'framer-motion';
import { useSettings } from '../../stores';
import { useIslandState, dispatch } from '../../ipc/island-state';
import { changePopup, listenPopups, usePopups } from '../../ipc/notifications';
import { terminalCopy, type Popup, type PopupOp } from '../../ipc/client';
import './notification-cards.css';
import { MAX_STACK, setMeasured, useShownPopups } from './notification-hover';
export { useNotificationHover } from './notification-hover';

const surfacesChanged = () => window.dispatchEvent(new Event('island-surfaces-changed'));

/** The hex a successful colour pick posted ("Copied #rrggbb" from the Colour picker), else null.
 *  Mirrors `popups::sticky` in Rust: those cards never time out. */
export function pickedColour(n: Popup['notification']): string | null {
  if (n.source !== 'alert' || n.appName !== 'Colour picker') return null;
  const hex = /^Copied (#[0-9a-f]{6})$/i.exec(n.title)?.[1];
  return hex ? hex.toLowerCase() : null;
}
// Same event as Icon's pingIcon (inlined: the node card tests load this module standalone).
const pingIcon = (name: string) => window.dispatchEvent(new CustomEvent('island-icon-ping', { detail: name }));
const rgbOf = (hex: string) => { const v = parseInt(hex.slice(1), 16); return `rgb(${v >> 16 & 255}, ${v >> 8 & 255}, ${v & 255})`; };

/** A picked colour: the swatch, its hex and rgb, and Copy. Stays until clicked; a click
 *  anywhere on it (or Copy) dismisses it — it never opens another page. */
type Mute = (app: string, id: string) => void;

function MuteButton({ notification: n, mute }: { notification: Popup['notification']; mute?: Mute }) {
  return mute && <button type="button" className="notification-mute" aria-label={`Mute ${n.appName}`} title={`Mute ${n.appName}`}
    onClick={e => { e.stopPropagation(); mute(n.appName, n.id); }}>Mute</button>;
}

function ColourCard({ hex, popup, perform, inNotch, mute }: { hex: string; popup: Popup; perform(op: PopupOp): void; inNotch: boolean; mute?: Mute }) {
  const n = popup.notification;
  const [copied, setCopied] = useState<string | null>(null);
  const timer = useRef<number | null>(null);
  useEffect(() => { pingIcon('picker'); return () => { if (timer.current !== null) clearTimeout(timer.current); }; }, []);
  const dismiss = () => perform({ type: 'dismiss', id: n.id });
  const copy = (text: string) => {
    if (timer.current !== null) clearTimeout(timer.current);
    terminalCopy(text).then(() => { setCopied(text); timer.current = window.setTimeout(dismiss, 650); })
      .catch(() => setCopied('failed'));
  };
  const rgb = rgbOf(hex);
  return <motion.article className="notification-card colour-card" data-island-extension={inNotch ? undefined : 'notification'} data-in-notch={inNotch}
    data-testid="notification-toast" data-colour={hex} role="status" aria-label={`Picked colour ${hex}`} onUpdate={surfacesChanged} onClick={dismiss}>
    <div className="notification-card-head">
      <span className="notification-icon colour-card-dot" aria-hidden="true" style={{ background: hex }} />
      <span className="notification-app">{n.appName}</span><span className="notification-priority">Click to close</span>
      <MuteButton notification={n} mute={mute} />
      <button className="notification-close" aria-label="Dismiss picked colour" onClick={e => { e.stopPropagation(); dismiss(); }}>
        <svg viewBox="0 0 12 12" aria-hidden="true"><path d="M2 2l8 8M10 2l-8 8" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" /></svg>
      </button>
    </div>
    <div className="colour-card-body">
      {/* The swatch springs up out of a drop, like ink landing. */}
      <motion.span className="colour-card-swatch" style={{ background: hex }} aria-hidden="true"
        initial={{ scale: .45, opacity: 0, borderRadius: 28 }} animate={{ scale: 1, opacity: 1, borderRadius: 16 }}
        transition={{ type: 'spring', stiffness: 420, damping: 22, opacity: { duration: .12 } }} />
      <div className="colour-card-values">
        <strong className="colour-card-hex">{hex.toUpperCase()}</strong>
        <span className="colour-card-rgb">{rgb}</span>
      </div>
      <div className="colour-card-copy" onClick={e => e.stopPropagation()}>
        <button type="button" data-copied={copied === hex} onClick={() => copy(hex)}>{copied === hex ? 'Copied' : 'Copy hex'}</button>
        <button type="button" data-copied={copied === rgb} onClick={() => copy(rgb)}>{copied === rgb ? 'Copied' : 'Copy rgb'}</button>
      </div>
    </div>
    {copied === 'failed' && <p className="colour-card-error" role="alert">Couldn't copy. The colour was copied when it was picked.</p>}
  </motion.article>;
}
export function NotificationCard({ popup, redact = false, perform, changed = surfacesChanged, inNotch = false, mute }: {
  popup: Popup; redact?: boolean; perform(op: PopupOp): void; changed?(): void; mute?: Mute;
  /** Drawn inside the island surface: no separate input region, no floating card chrome. */
  inNotch?: boolean;
}) {
  const hex = pickedColour(popup.notification);
  if (hex) return <ColourCard hex={hex} popup={popup} perform={perform} inNotch={inNotch} mute={mute} />;
  return <PlainCard popup={popup} redact={redact} perform={perform} changed={changed} inNotch={inNotch} mute={mute} />;
}
function PlainCard({ popup, redact, perform, changed, inNotch, mute }: { popup: Popup; redact: boolean; perform(op: PopupOp): void; changed(): void; inNotch: boolean; mute?: Mute }) {
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
      <MuteButton notification={n} mute={mute} />
      <button className="notification-close" aria-label={`Dismiss ${n.title}`} onClick={() => perform({ type: 'dismiss', id: n.id })}>
        <svg viewBox="0 0 12 12" aria-hidden="true"><path d="M2 2l8 8M10 2l-8 8" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" /></svg>
      </button>
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
export function NotchNotifications({ fail, placement, mute, confirmation, undo }: {
  fail(error: unknown): void; placement: 'hover' | 'open'; mute: Mute; confirmation?: string; undo(): void;
}) {
  const settings = useSettings(), snapshot = usePopups(), shown = useShownPopups();
  // This module is also loaded standalone by the node card tests.
  const reduced = useReducedMotion() === true || settings.reduceMotion || (typeof document !== 'undefined' && document.documentElement.dataset.reduceMotion === 'true')
    || (typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches);
  const duration = .15 / Math.min(2, Math.max(.5, (settings.animationSpeed ?? 100) / 100));
  const transition = { duration, ease: [.23, 1, .32, 1] as const };
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
  if (!shown.length && !confirmation) return null;
  const more = shown.length - visibleCount;
  return <section ref={stack} className="notch-notifications" data-placement={placement} aria-label="Notifications">
    {confirmation && <div className="notification-muted" role="status"><span>Muted {confirmation}</span><button type="button" onClick={undo}>Undo</button></div>}
    <AnimatePresence initial={false} onExitComplete={surfacesChanged}>
      {shown.slice(0, visibleCount).map(p => <motion.div key={p.notification.id} layout={!reduced}
        initial={{ opacity: 0, y: reduced ? 0 : -8, scale: reduced ? 1 : .97 }} animate={{ opacity: 1, y: 0, scale: 1 }}
        exit={{ opacity: 0, y: reduced ? 0 : -8, scale: reduced ? 1 : .97 }} transition={transition}>
        <NotificationCard popup={p} redact={settings.redactBodies} perform={perform} inNotch mute={mute} />
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
  // A new card on top rings every bell icon (side button, launcher tile, history header).
  // Only a card that wasn't there before rings it; dismissing the top card doesn't.
  const top = snapshot.items[0]?.notification.id ?? null, seen = useRef<Set<string> | null>(null);
  useEffect(() => {
    const ids = new Set(snapshot.items.map(p => p.notification.id));
    if (seen.current && top && !seen.current.has(top) && pickedColour(snapshot.items[0]!.notification) === null) pingIcon('bell');
    seen.current = ids;
  }, [snapshot]);
  useLayoutEffect(() => { surfacesChanged(); }, [snapshot, state.mode]);
  return null;
}
