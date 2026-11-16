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

export function InstantTooltip() {
  const [target, setTarget] = useState<HTMLButtonElement | null>(null);
  const [position, setPosition] = useState({ left: 0, top: 0, maxWidth: 0 });
  const tip = useRef<HTMLDivElement>(null);
  useEffect(() => bindInstantTooltips(document, window, setTarget), []);
  useLayoutEffect(() => {
    if (!target || !tip.current) return;
    // Paint within the existing island rectangle. Never tag as data-island-extension:
    // tooltips neither grow the input region nor intercept a pointer on the desktop.
    const island = document.querySelector('.island')?.getBoundingClientRect();
    if (!island) return;
    const button = target.getBoundingClientRect(), inset = 12;
    tip.current.style.maxWidth = `${island.width - 2 * inset}px`;
    const box = tip.current.getBoundingClientRect(), width = Math.min(box.width, island.width - 2 * inset);
    setPosition({
      left: Math.max(island.left + inset, Math.min(island.right - width - inset, button.left + (button.width - width) / 2)),
      top: Math.max(island.top + inset, Math.min(island.bottom - box.height - inset, button.bottom + 4)),
      maxWidth: island.width - 2 * inset,
    });
  }, [target]);
  return target && <div ref={tip} id={ID} role="tooltip" className="instant-tooltip" style={{ ...position, maxWidth: position.maxWidth || undefined }}>
    {target.dataset.tooltip || target.getAttribute('aria-label')}
  </div>;
}
