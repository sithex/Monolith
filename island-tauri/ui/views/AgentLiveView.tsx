// One shared feed retains the log cursor across Live / Usage switches. Both IPC and the
// display clock use Nick's retained visible poller; hidden pages never start new work.
import { useCallback, useEffect, useRef, useState } from 'react';
import { AgentLiveFeed, nextDelay, type FeedResult, type LiveBlock, type LivePage } from './agent-live-model';
import { agentLiveBlocks, agentLiveRaw, copyText } from './agent-live-ipc';
import { createVisiblePoller } from './visible-poller';
import { AgentLivePage } from '../island/components/AgentLive';
import { AgentLiveUsageCards } from '../island/components/AgentLiveUsage';
import { ClaudeCodeLive, useClaudeHistory } from './ClaudeCodeView';
import '../island/ai-stats.css';
import '../island/agent-live.css';

let feed: AgentLiveFeed | null = null;
let inFlight: Promise<unknown> | null = null;
export const liveFeed = () => feed ??= new AgentLiveFeed({ raw: agentLiveRaw, blocks: agentLiveBlocks });

async function tickOnce(page: LivePage, current: () => boolean): Promise<FeedResult> {
  // A page switch can queue behind a previous page's read; cancellation must also
  // be checked after that read drains, before acquiring the shared feed.
  while (inFlight) await inFlight.catch(() => undefined);
  if (!current()) throw new Error('Agent Live read cancelled');
  const p = liveFeed().tick(page, current);
  inFlight = p;
  try { return await p; } finally { if (inFlight === p) inFlight = null; }
}

function useAgentLive(active: boolean, page: LivePage) {
  const [result, setResult] = useState<FeedResult | null>(null), [error, setError] = useState('');
  const [now, setNow] = useState(Date.now);
  const lifetime = useRef({ visible: false, epoch: 0 });
  const cadence = useRef(false);
  const [poller] = useState(() => createVisiblePoller(() => {
    const epoch = lifetime.current.epoch;
    cadence.current = false;
    return tickOnce(page, () => lifetime.current.visible && lifetime.current.epoch === epoch);
  }, r => {
    cadence.current = r.active;
    setResult(r); setError(''); setNow(Date.now());
  }, err => setError(String(err)),
  callback => window.setTimeout(callback, nextDelay(page, cadence.current)),
  timer => window.clearTimeout(timer as number)));
  const [clock] = useState(() => createVisiblePoller(async () => Date.now(), setNow, () => {},
    callback => window.setTimeout(callback, 1000), timer => window.clearTimeout(timer as number)));
  useEffect(() => {
    const sync = () => {
      const visible = active && document.visibilityState !== 'hidden';
      if (visible !== lifetime.current.visible) {
        lifetime.current.visible = visible; lifetime.current.epoch++;
      }
      poller.setVisible(visible);
      clock.setVisible(visible && page === 'live');
    };
    sync();
    document.addEventListener('visibilitychange', sync);
    return () => {
      document.removeEventListener('visibilitychange', sync);
      lifetime.current.visible = false; lifetime.current.epoch++;
      poller.setVisible(false); clock.setVisible(false);
    };
  }, [active, page, poller, clock]);
  useEffect(() => () => { poller.dispose(); clock.dispose(); }, [poller, clock]);
  return { result, error, now };
}

function useCopy() {
  const [note, setNote] = useState('');
  const timer = useRef(0), generation = useRef(0), mounted = useRef(false);
  const copy = useCallback((id: string) => {
    const request = ++generation.current;
    void copyText(id).then(ok => {
      if (!mounted.current || request !== generation.current) return;
      setNote(ok ? 'session id copied' : 'copy failed');
      window.clearTimeout(timer.current);
      timer.current = window.setTimeout(() => setNote(''), 2500);
    });
  }, []);
  useEffect(() => { mounted.current = true; return () => {
    mounted.current = false; generation.current++; window.clearTimeout(timer.current);
  }; }, []);
  return { note, copy };
}

export function AgentLiveView({ active }: { active: boolean }) {
  const { result, error, now } = useAgentLive(active, 'live'), { note, copy } = useCopy();
  return <AgentLivePage view={result?.view ?? null} now={now} onCopy={copy} note={note} error={error} extra={<ClaudeCodeLive active={active} now={now}/>}/>;
}

const NO_BLOCKS: LiveBlock[] = [];
export function AgentLiveUsage({ active }: { active: boolean }) {
  const { result, error, now } = useAgentLive(active, 'history'), { note, copy } = useCopy();
  const view = result?.view;
  const ids = (result?.history?.top ?? []).map(t => typeof t?.id === 'string' ? t.id : null);
  const cc = useClaudeHistory(active, result?.history ?? null, view?.history ?? null, result ? liveFeed().blocks ?? NO_BLOCKS : NO_BLOCKS);
  return <AgentLiveUsageCards history={cc.history} lead={cc.lead} ids={ids} now={now} onCopy={copy} note={note}
    error={error || view?.historyError || ''} pluginMissing={view?.sources?.plugin === 'not installed'}
    blocks={result ? liveFeed().blocks ?? [] : []} loaded={!!result}/>;
}
