import { useEffect, useLayoutEffect, useRef, useState } from 'react';

const ID = 'island-control-tooltip';
/** Delegated so icon buttons need no wrapper, focus target or geometry extension. */
export function bindInstantTooltips(doc: Document, win: Window, show: (button: HTMLButtonElement | null) => void) {
  let target: HTMLButtonElement | null = null;
  let described: string | null = null;
  const hide = () => {
    if (target) {
      if (described === null) target.removeAttribute('aria-describedby');
      else target.setAttribute('aria-describedby', described);
    }
    target = null; described = null; show(null);
  };
  const enter = (event: Event) => {
    const button = (event.target as Element | null)?.closest?.<HTMLButtonElement>('button');
    if (!button || button.disabled || button.getAttribute('aria-disabled') === 'true' || button.closest('[inert], [hidden], [aria-hidden="true"]') ||
      !(button.dataset.tooltip || (!/[\p{L}\p{N}]/u.test(button.textContent ?? '') && button.getAttribute('aria-label')))) return;
    if (target === button) return;
    hide(); target = button; described = button.getAttribute('aria-describedby');
    button.setAttribute('aria-describedby', [described, ID].filter(Boolean).join(' ')); show(button);
  };
  const leave = (event: Event) => {
    const related = (event as FocusEvent).relatedTarget as Node | null;
    if (target?.contains(related) || (event.type === 'pointerout' && doc.activeElement === target)) return;
    hide();
  };
  const key = (event: Event) => { if ((event as KeyboardEvent).key === 'Escape' && target) hide(); };
  const events: [EventTarget, string, EventListener][] = [
    [doc, 'pointerover', enter], [doc, 'focusin', enter], [doc, 'pointerout', leave], [doc, 'focusout', leave],
    [doc, 'pointerdown', hide], [doc, 'scroll', hide], [doc, 'keydown', key], [doc, 'visibilitychange', hide],
    [win, 'resize', hide], [win, 'blur', hide], [win, 'island-pointer-left', hide],
  ];
  for (const [node, type, listener] of events) node.addEventListener(type, listener, true);
  const observer = new MutationObserver(() => { if (target && (!target.isConnected || target.closest('[inert], [hidden], [aria-hidden="true"]'))) hide(); });
  observer.observe(doc.body, { childList: true, subtree: true, attributes: true, attributeFilter: ['hidden', 'inert', 'aria-hidden'] });
  return () => { observer.disconnect(); for (const [node, type, listener] of events) node.removeEventListener(type, listener, true); hide(); };
}

type Box = { left: number; top: number; right: number; bottom: number; width: number; height: number };
export type TooltipSide = 'below' | 'left' | 'right';
const GAP = 6, INSET = 8;
/** Beside its button: below (above when there is no room) or, for side buttons, outward. */
export function placeTooltip(button: Box, tip: { width: number; height: number }, bounds: Box, side: TooltipSide) {
  const maxWidth = bounds.width - 2 * INSET, width = Math.min(tip.width, maxWidth);
  const clampX = (x: number) => Math.max(bounds.left + INSET, Math.min(bounds.right - width - INSET, x));
  const clampY = (y: number) => Math.max(bounds.top + INSET, Math.min(bounds.bottom - tip.height - INSET, y));
  const centredY = clampY(button.top + (button.height - tip.height) / 2);
  if (side === 'left' && button.left - GAP - width >= bounds.left + INSET) return { left: button.left - GAP - width, top: centredY, maxWidth };
  if (side === 'right' && button.right + GAP + width <= bounds.right - INSET) return { left: button.right + GAP, top: centredY, maxWidth };
  const below = button.bottom + GAP, above = button.top - GAP - tip.height;
  const top = below + tip.height <= bounds.bottom - INSET || above < bounds.top + INSET ? clampY(below) : above;
  return { left: clampX(button.left + (button.width - width) / 2), top, maxWidth };
}

export function InstantTooltip() {
  const [target, setTarget] = useState<HTMLButtonElement | null>(null);
  const [position, setPosition] = useState({ left: 0, top: 0, maxWidth: 0 });
  const tip = useRef<HTMLDivElement>(null);
  useEffect(() => bindInstantTooltips(document, window, setTarget), []);
  useLayoutEffect(() => {
    if (!target || !tip.current) return;
    // Bound to the surface the button sits on (island body or a docked extension such as the
    // colour picker); side buttons sit outside both, so they use the window. Clamping a side
    // button's tip into the island body is what threw it away from the pointer. Never tag the
    // tip as data-island-extension: it neither grows the input region nor intercepts a pointer.
    const surface = target.closest('.island, [data-island-extension]');
    const bounds: Box = surface?.getBoundingClientRect()
      ?? { left: 0, top: 0, right: window.innerWidth, bottom: window.innerHeight, width: window.innerWidth, height: window.innerHeight };
    const sideAttr = target.closest<HTMLElement>('[data-side]')?.dataset.side;
    const side: TooltipSide = sideAttr === 'left' || sideAttr === 'right' ? sideAttr : 'below';
    tip.current.style.maxWidth = `${bounds.width - 2 * INSET}px`;
    setPosition(placeTooltip(target.getBoundingClientRect(), tip.current.getBoundingClientRect(), bounds, side));
  }, [target]);
  return target && <div ref={tip} id={ID} role="tooltip" className="instant-tooltip" style={{ ...position, maxWidth: position.maxWidth || undefined }}>
    {target.dataset.tooltip || target.getAttribute('aria-label')}
  </div>;
}
