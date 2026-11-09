import { useEffect, useRef, useState } from 'react';

/** One delay per rail visit, not per icon crossed on the way to a section. */
export function useRailTooltip() {
  const [shown, setShown] = useState<string | null>(null);
  const warm = useRef(false), timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const leave = () => { if (timer.current !== null) clearTimeout(timer.current); timer.current = null; setShown(null); };
  const reset = () => { leave(); warm.current = false; };
  const enter = (id: string) => {
    leave();
    if (typeof matchMedia !== 'function' || !matchMedia('(hover: hover) and (pointer: fine)').matches) return;
    if (warm.current) setShown(id);
    else timer.current = setTimeout(() => { warm.current = true; setShown(id); timer.current = null; }, 300);
  };
  useEffect(() => {
    if (typeof window !== 'undefined') window.addEventListener('island-pointer-left', reset);
    return () => { if (timer.current !== null) clearTimeout(timer.current); if (typeof window !== 'undefined') window.removeEventListener('island-pointer-left', reset); };
  }, []);
  return { shown, enter, leave, reset };
}
