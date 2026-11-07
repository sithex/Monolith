// One shared cursor feed; retained visible poller matches the newer AgentLiveView.
// Cancellation is checked after waiting for the previous page and again before accepting IPC.
import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { ClaudeCodeFeed, HISTORY_SOURCES, MORE_DELAY_MS, buildHistoryVariants, type ClaudeLive, type ClaudeProgress, type ClaudeSessionSum, type HistorySource } from './claude-code-model';
import { nextDelay, type HistoryRaw, type LiveBlock, type LivePage } from './agent-live-model';
import { claudeCodeRaw } from './claude-code-ipc';
import { createVisiblePoller } from './visible-poller';
import { ClaudeCodeCard, HistorySourceChips, HistoryTotalsCard } from '../island/components/ClaudeCodeCard';
import '../island/claude-code.css';
let feed: ClaudeCodeFeed | null = null;
let inFlight: Promise<unknown> | null = null;
export const claudeFeed = () => feed ??= new ClaudeCodeFeed();
export interface ClaudeSnap { live: ClaudeLive; sessions: ClaudeSessionSum[] | null; progress: ClaudeProgress; at: number; more: boolean }
function snapOf(page: LivePage): ClaudeSnap {
  const f = claudeFeed();
  return { live: f.liveView(), sessions: page === 'history' ? f.sessions() : null, progress: f.progress(), at: f.now || Date.now(), more: f.more };
}
async function tickOnce(page: LivePage, current: () => boolean): Promise<ClaudeSnap> {
  while (inFlight) await inFlight.catch(() => undefined);
  if (!current()) throw new Error('Claude Code read cancelled');
  const f = claudeFeed();
  const p = claudeCodeRaw(f.request());
  inFlight = p;
  try {
    const raw = await p;
    if (!current()) throw new Error('Claude Code read cancelled');
    f.ingest(raw);
    return snapOf(page);
  } finally { if (inFlight === p) inFlight = null; }
}
export function useClaudeCode(active: boolean, page: LivePage) {
  const [snap, setSnap] = useState<ClaudeSnap | null>(() => feed?.loaded ? snapOf(page) : null);
  const [error, setError] = useState('');
  const lifetime = useRef({ visible: false, epoch: 0 });
  const delay = useRef(nextDelay(page, false));
  const [poller] = useState(() => createVisiblePoller(() => {
    const epoch = lifetime.current.epoch;
    delay.current = nextDelay(page, false);
    return tickOnce(page, () => lifetime.current.visible && lifetime.current.epoch === epoch);
  }, s => {
    setSnap(s); setError('');
    delay.current = s.more ? MORE_DELAY_MS : nextDelay(page, claudeFeed().busy(s.live));
  }, err => setError(String(err)),
  callback => window.setTimeout(callback, delay.current), timer => window.clearTimeout(timer as number)));
  useEffect(() => {
    const sync = () => {
      const visible = active && document.visibilityState !== 'hidden';
      if (visible !== lifetime.current.visible) { lifetime.current.visible = visible; lifetime.current.epoch++; }
      poller.setVisible(visible);
    };
    sync(); document.addEventListener('visibilitychange', sync);
    return () => {
      document.removeEventListener('visibilitychange', sync);
      lifetime.current.visible = false; lifetime.current.epoch++; poller.setVisible(false);
    };
  }, [active, page, poller]);
  useEffect(() => () => poller.dispose(), [poller]);
  return { snap, error };
}
export function ClaudeCodeLive({ active, now }: { active: boolean; now: number }) {
  const { snap, error } = useClaudeCode(active, 'live');
  return <ClaudeCodeCard live={snap?.live ?? null} now={now} error={error}/>;
}
export function useClaudeHistory(active: boolean, hermesRaw: HistoryRaw | null, hermesView: Record<string, any> | null, blocks: LiveBlock[]): { history: Record<string, any> | null; lead: ReactNode; claudeSrc: boolean } {
  const { snap, error } = useClaudeCode(active, 'history');
  const [src, setSrc] = useState<HistorySource>('all');
  const variants = useMemo(() => snap?.sessions?.length ? buildHistoryVariants(hermesRaw, snap.sessions, blocks, snap.at) : null, [snap, hermesRaw, blocks]);
  const sources = variants ? HISTORY_SOURCES.filter(([k]) => k === 'all' || variants[k]) : [];
  const cur: HistorySource = sources.some(([k]) => k === src) ? src : 'all';
  const history = !variants ? hermesView : cur === 'hermes' ? (variants.hermes ?? hermesView) : cur === 'all' && !variants.all ? hermesView : variants[cur];
  const claudeSrc = !!variants && cur !== 'hermes';
  const lead = <>
    {variants && <HistorySourceChips sources={sources} value={cur} onChange={k => setSrc(k as HistorySource)}/>}
    <HistoryTotalsCard history={history} claudeSrc={claudeSrc} progress={snap?.more || snap?.progress.readErrors ? snap?.progress : null}/>
    {error && <p className="ai-footnote">Claude Code history unavailable: {error}</p>}
  </>;
  return { history, lead, claudeSrc };
}
