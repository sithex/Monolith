export type SearchItem = { id: string; label: string; group: string; detail?: string; keywords?: string; run(): void | Promise<unknown> };
const norm = (s: string) => s.toLowerCase().normalize('NFKD').replace(/\p{M}/gu, '');
/** Bounded subsequence search; exact/prefix and word starts outrank scattered letters. */
export function fuzzyScore(text: string, query: string): number | null {
  const hay = norm(text).slice(0, 1200), words = norm(query).trim().slice(0, 120).split(/\s+/).filter(Boolean);
  if (!words.length) return 0;
  let score = 0;
  for (const word of words) {
    const exact = hay.indexOf(word);
    if (exact >= 0) { score += exact === 0 ? 0 : /[\s/·-]/.test(hay[exact - 1]!) ? 3 : 8; continue; }
    let pos = -1, gaps = 0;
    for (const c of word) { const next = hay.indexOf(c, pos + 1); if (next < 0) return null; gaps += next - pos - 1; pos = next; }
    score += 30 + gaps;
  }
  return score;
}
export function searchCommands(items: readonly SearchItem[], query: string, limit = 12): SearchItem[] {
  return items.map((item, index) => {
    const score = fuzzyScore(`${item.label} ${item.group} ${item.detail ?? ''} ${item.keywords ?? ''}`, query);
    return { item, index, score: score === null ? null : score - (norm(query).trim() && norm(item.label) === norm(query).trim() ? 10 : 0) };
  })
    .filter((r): r is typeof r & { score: number } => r.score !== null)
    .sort((a, b) => a.score - b.score || a.index - b.index).slice(0, Math.max(0, Math.min(24, limit))).map(r => r.item);
}
export function nextSelection(index: number, key: string, count: number): number {
  if (!count) return 0;
  if (key === 'Home') return 0;
  if (key === 'End') return count - 1;
  return (index + (key === 'ArrowUp' ? -1 : 1) + count) % count;
}
