import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { animate, motion, useMotionValue, useReducedMotion, useTransform, type MotionValue } from 'framer-motion';
import { type LayoutModel, type IslandView } from '../ipc/client';
type LayoutButton = LayoutModel['buttons'][number];
import { useIslandState } from '../ipc/island-state';
import { useLayout, useLauncher } from '../stores/apps';
import { Icon } from './components/Icon';
import { visibleLayout, isQuickAction, sideBackFor } from './geometry';
import { actionIcon, actionLabel, quickActive, runSideAction } from './side-actions';
import { useSettings } from '../stores';
import { useApp, useMedia } from '../stores/apps';
import { sideDropPath, WITHDRAW_SECONDS } from './motion-profile';

/** Where a side button's Back goes: the page you came from (Island's back history). */
export type SideBack = { view: IslandView; label: string; go(): void };

function SideBubble({ button, diameter, gap, progress, interactive, navigate, lit, fail, back }: {
  button: LayoutButton; diameter: number; gap: number; progress: MotionValue<number>; interactive: boolean; navigate(view: IslandView): void; lit: boolean; fail(error: unknown): void; back: SideBack | null;
}) {
  const side = button.side, reach = 14 + gap + diameter / 2;
  const x = useTransform(progress, [0, 1], [side === 'left' ? reach : side === 'right' ? -reach : 0, 0], { clamp: false });
  const y = useTransform(progress, [0, 1], [side === 'bottom' ? -reach : 0, 0], { clamp: false });
  const scale = useTransform(progress, [0, 1], [.4, 1], { clamp: false });
  const opacity = useTransform(progress, [0, 1], [0, 1]);
  const visibility = useTransform(() => progress.get() > .001 ? 'visible' as const : 'hidden' as const);
  const neck = useTransform(() => sideDropPath(side, diameter, gap, progress.get()));
  const quick = isQuickAction(button.action.type);
  // A page button whose page is already showing turns into Back (same spot, same tap).
  const returning = !quick && back !== null;
  const icon = returning ? 'back' : actionIcon(button.action);
  // Hover is our own state, not CSS :hover / framer whileHover: WebKitGTK gets no DOM leave
  // when the pointer exits the island straight onto another surface, so those stuck on.
  // Rust forwards the real X11 crossing as `island-pointer-left`.
  const [hovered, setHovered] = useState(false);
  const face = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (!hovered) return;
    const clear = () => setHovered(false);
    const elsewhere = (e: Event) => { if (!face.current?.contains(e.target as Node)) clear(); };
    window.addEventListener('island-pointer-left', clear); window.addEventListener('blur', clear);
    document.addEventListener('pointerover', elsewhere, true); document.documentElement.addEventListener('pointerleave', clear);
    return () => { window.removeEventListener('island-pointer-left', clear); window.removeEventListener('blur', clear);
      document.removeEventListener('pointerover', elsewhere, true); document.documentElement.removeEventListener('pointerleave', clear); };
  }, [hovered]);
  const lifted = hovered && interactive;
  const reducedIcon = useReducedMotion();
  return <motion.div data-side-button={button.id} data-interactive={interactive} className="side-anchor" aria-hidden={!interactive}
    style={{ position: 'fixed', width: diameter, height: diameter, borderRadius: diameter / 2, visibility, pointerEvents: interactive ? 'auto' : 'none' }}>
    <svg className="side-neck" aria-hidden="true" width={diameter} height={diameter}><motion.path d={neck} style={{ opacity }} /></svg>
    <motion.div className="side-emergence" style={{ x, y, scale, opacity }}>
      <motion.button ref={face} data-testid={`side-${button.id}`} className="round-button live-side-button" tabIndex={interactive ? 0 : -1} data-quick={quick} data-lit={lit} data-hover={lifted}
        onPointerEnter={() => setHovered(true)} onPointerLeave={() => setHovered(false)}
        data-back={returning}
        aria-label={returning ? `Back to ${back!.label}` : button.label ?? (button.action.type === 'appGrid' ? 'Open apps' : quick ? actionLabel(button.action) : `Open ${actionLabel(button.action)}`)} aria-pressed={quick ? lit : undefined} aria-disabled={!interactive} title={returning ? `Back to ${back!.label}` : actionLabel(button.action)}
        animate={{ scale: lifted ? 1.04 : 1 }} whileTap={interactive ? { scale: .965, opacity: .8 } : {}}
        transition={{ type: 'spring', stiffness: 420, damping: 28 }} onClick={() => { if (!interactive) return; if (returning) back!.go(); else void runSideAction(button.action, navigate).catch(fail); }}>
        <motion.span key={icon} className="side-icon" initial={reducedIcon ? false : { opacity: 0, rotate: returning ? 90 : -90, scale: .6 }} animate={{ opacity: 1, rotate: 0, scale: 1 }}
          transition={{ type: 'spring', stiffness: 520, damping: 32 }}><Icon name={icon}/></motion.span></motion.button>
    </motion.div>
  </motion.div>;
}

export function SideControls({ navigate, fail, view, back }: { navigate(view: IslandView): void; fail(error: unknown): void; view?: IslandView; back?: SideBack | null }) {
  const layout = useLayout(), state = useIslandState(), launcher = useLauncher(), reduced = useReducedMotion();
  const settings = useSettings(), toggles = useApp('toggles'), tiles = useApp('tiles'), media = useMedia();
  const live = { dnd: settings.dnd, micMuted: toggles?.micMuted, awake: tiles?.keepAwake.on, playing: !!media?.nowPlaying?.playing };
  const model = visibleLayout(layout.layout, launcher.items.filter(i => i.enabled && i.known && i.view).map(i => i.view!));
  const progress = useMotionValue(0), generation = useRef(0);
  const [ready, setReady] = useState(false);
  const active = state.mode === 'large';
  useLayoutEffect(() => {
    const token = ++generation.current;
    setReady(false);
    if (reduced) { progress.set(active ? 1 : 0); setReady(active); window.dispatchEvent(new Event('island-surfaces-changed')); return; }
    const surface = document.querySelector<HTMLElement>('.island');
    const delay = active ? Number(surface?.dataset.sideDelay ?? 0) : 0;
    const omega = 2 * Math.PI / .38;
    // Every drawn frame re-reports the circles (coalesced to one geometry flush per frame),
    // so the KWin blur behind them grows with them. They take input once visually there,
    // not after the spring's sub-pixel tail.
    const surfaces = () => window.dispatchEvent(new Event('island-surfaces-changed'));
    const unsubscribe = progress.on('change', value => {
      surfaces();
      if (active && value >= .985 && generation.current === token) setReady(true);
    });
    const controls = animate(progress, active ? 1 : 0, active
      ? { type: 'spring', stiffness: omega ** 2, damping: 2 * .88 * omega, delay, restDelta: .001, restSpeed: .01,
          onComplete: () => { if (generation.current === token) setReady(true); } }
      : { duration: WITHDRAW_SECONDS, ease: 'easeIn' });
    return () => { generation.current++; unsubscribe(); controls.stop(); };
  }, [active, reduced, progress]);
  useLayoutEffect(() => { window.dispatchEvent(new Event('island-surfaces-changed')); }, [ready, active, layout.rev, launcher.rev]);
  return <>{model.buttons.map(button => <SideBubble key={button.id} button={button} diameter={model.buttonDiameter} gap={model.gap}
    progress={progress} interactive={active && ready} navigate={navigate} lit={quickActive(button.action, live)} fail={fail}
    back={sideBackFor(button.action, view, back)} />)}</>;
}
