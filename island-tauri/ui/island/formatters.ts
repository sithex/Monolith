import type { WeatherState } from '../ipc/client';

export function formatTemperature(temp: number | null | undefined, unit: WeatherState['unit']): string {
  return typeof temp !== 'number' || !Number.isFinite(temp) ? 'Unknown temperature' : `${Math.round(temp)}°${unit === 'fahrenheit' ? 'F' : 'C'}`;
}

/** Shared with the Updates page; callers validate the elapsed seconds first. */
export function relativeAge(ageSecs: number): string {
  if (ageSecs < 60) return 'just now';
  if (ageSecs < 3600) return `${Math.floor(ageSecs / 60)} min ago`;
  if (ageSecs < 86400) return `${Math.floor(ageSecs / 3600)} hr ago`;
  return `${Math.floor(ageSecs / 86400)} days ago`;
}

/** Wire timestamps are epoch milliseconds or ISO dates (Tailscale), never epoch seconds. */
export function relativeTime(timestamp: number | string | null | undefined, now = Date.now(), unavailable = 'Age unavailable'): string {
  const at = typeof timestamp === 'string' ? (/^\d{4}-\d{2}-\d{2}T/.test(timestamp) ? Date.parse(timestamp) : NaN) : timestamp;
  if (typeof at !== 'number' || !Number.isFinite(at) || at < 0 || !Number.isFinite(now)) return unavailable;
  // A slightly future-dated file/peer must not produce a negative age after clock skew.
  return relativeAge(Math.max(0, now - at) / 1000);
}
