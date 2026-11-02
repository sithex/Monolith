/** The panel's flat crown, concave shoulders and lower corners, in local pixels.
 * Progress 0 is the live rounded shell; 1 is the native taskbar notch. No reserved
 * canvas, layout reads, or extra animation clock: sample the existing size spring.
 */
export function notchMorphPath(width: number, height: number, radius: number, progress: number, borderWidth: number): string {
  const w = Math.max(1, width), h = Math.max(1, height);
  const p = Math.max(0, Math.min(1, progress));
  const i = Math.min(w / 4, h / 4, Math.max(.5, borderWidth / 2));
  const r = Math.max(0, Math.min(radius, w / 2 - i, h / 2 - i));
  const s = Math.min(6, h / 4), nr = Math.min(10, h / 2 - 1);
  const mix = (a: number, b: number) => a + (b - a) * p;
  const k = .5522847498, x = mix(i, s), y = mix(i + r, s);
  const b = mix(r, nr), c = mix(k, 2 / 3), crown = mix(i + r, i);
  const n = (v: number) => +v.toFixed(6);
  return `M ${n(crown)} ${n(i)} H ${n(w - crown)} C ${n(mix(w - i - r + k * r, w - s))} ${n(i)} ${n(mix(w - i, w - s))} ${n(mix(i + r - k * r, s / 2))} ${n(w - x)} ${n(y)} V ${n(h - i - b)} C ${n(w - x)} ${n(h - i - b + c * b)} ${n(w - x - b + c * b)} ${n(h - i)} ${n(w - x - b)} ${n(h - i)} H ${n(x + b)} C ${n(x + b - c * b)} ${n(h - i)} ${n(x)} ${n(h - i - b + c * b)} ${n(x)} ${n(h - i - b)} V ${n(y)} C ${n(mix(i, s))} ${n(mix(i + r - k * r, s / 2))} ${n(mix(i + r - k * r, s))} ${n(i)} ${n(crown)} ${n(i)} Z`;
}
