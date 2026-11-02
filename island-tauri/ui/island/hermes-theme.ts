import { useEffect, useRef } from 'react';
import { hermesThemeSync, type Palette } from '../ipc/client';
import type { SchemeRoles } from './tokens/schemes';

const ROLES = ['background', 'surface', 'border', 'dim', 'text', 'primary', 'accent', 'error'] as const;
/** The 8 roles as a skin palette, or null when any role is not a plain #rrggbb colour. */
export function themePayload(roles: SchemeRoles): Palette | null {
  const out = { name: 'ripple-island' } as Palette;
  for (const role of ROLES) {
    const value = roles[role];
    if (typeof value !== 'string' || !/^#[0-9a-f]{6}$/i.test(value)) return null;
    out[role] = value.toLowerCase();
  }
  return out;
}

/**
 * Optional Hermes theme sync (Nov 2). While enabled, the island scheme (`roles`) is written
 * as the `ripple-island` Hermes skin, debounced so a dragged colour picker writes once;
 * `roles === null` (the "Hermes skin" scheme) and turning sync off restore the skin that was
 * active before. Nothing runs at startup while sync is off, and identical requests are skipped.
 */
export function useHermesThemeSync(enabled: boolean, roles: SchemeRoles | null, fail: (error: unknown) => void, delayMs = 600) {
  const sent = useRef<string | null>(null), wasEnabled = useRef(false);
  const payload = enabled && roles ? themePayload(roles) : null;
  const key = enabled ? JSON.stringify(payload) : 'off';
  useEffect(() => {
    const turningOff = !enabled && wasEnabled.current;
    wasEnabled.current = enabled;
    if (!enabled && !turningOff) return;
    if (key === sent.current) return;
    const timer = setTimeout(() => {
      sent.current = key;
      hermesThemeSync(payload).catch(error => { sent.current = null; fail(error); });
    }, turningOff ? 0 : delayMs);
    return () => clearTimeout(timer);
    // `key` encodes enabled + payload.
  }, [key]);
}
