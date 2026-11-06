import type { AiUsage, Registry } from '../ipc/client';
import type { Dashboard, Service, ModuleSettingsModel } from '../island/models';

const object = (v: unknown): Record<string, unknown> => v !== null && typeof v === 'object' && !Array.isArray(v) ? v as Record<string, unknown> : {};
const finite = (v: unknown): number | null => typeof v === 'number' && Number.isFinite(v) && v >= 0 ? v : null;
const text = (v: unknown): string | null => typeof v === 'string' && v.length <= 4096 ? v : null;
export type AiPeriod = 'today' | 'week' | 'month';
export type AiFamily = 'claude' | 'codex' | 'other';
export interface AiBucket { key: string; cost: number | null; tokens: number | null; sessions: number; parts: Record<AiFamily, number | null> }
export interface AiShare { name: string; family: AiFamily; cost: number | null }
export interface AiAccount { id: 'anthropic' | 'openai-codex'; name: string; family: AiFamily; plan: string | null; windows: { label: string; used: number | null; reset: string | null; pace: number | null }[] }
export interface AiDashboardData extends Dashboard {
  accounts: AiAccount[]; period: AiPeriod; cost: number | null; partial: boolean; tokens: number | null; cacheShare: number | null;
  split: Record<AiFamily, number | null>; buckets: AiBucket[]; days: AiBucket[];
  models: AiShare[]; projects: AiShare[]; totalCost: number | null; activeDays: number | null; streak: number | null;
  busiest: { date: string; cost: number | null } | null; cohort: boolean;
  running: { id: string; project: string; model: string; family: AiFamily; elapsed: number; output: number | null; cost: number | null }[];
}
const family = (v: unknown): AiFamily => v === 'claude' || v === 'codex' ? v : 'other';
const parts = (raw: unknown): Record<AiFamily, number | null> => {
  const p = object(raw); return { claude: finite(object(p.claude).costUsd), codex: finite(object(p.codex).costUsd), other: finite(object(p.other).costUsd) };
};
const list = (v: unknown) => Array.isArray(v) ? v.slice(0, 200) : [];
const bucket = (v: unknown): AiBucket => { const b = object(v); return { key: text(b.date) ?? String(b.startS), cost: finite(b.estimatedCostUsd), tokens: finite(b.totalTokens), sessions: finite(b.sessions) ?? 0, parts: parts(b.byProvider) }; };
const shares = (v: unknown): AiShare[] => list(v).map(raw => { const r = object(raw); return { name: text(r.name) ?? 'Unknown', family: family(r.family), cost: finite(r.estimatedCostUsd) }; });
export function dashboardFromUsage(usage: AiUsage | null, period: AiPeriod = 'today', now = Date.now()): Service<AiDashboardData> {
  if (!usage) return { available: false, reason: 'AI usage has not been read' };
  const account = object(usage.account), local = object(usage.local);
  const windows = Array.isArray(account.windows) ? account.windows.slice(0, 32) : [];
  const models = local.available === true && Array.isArray(local.models) ? local.models.slice(0, 200) : [];
  const detailed = local.available === true ? object(local.dashboard) : {};
  const summary = object(object(detailed.periods)[period]), activity = object(detailed.activity);
  const accountRows = Array.isArray(account.accounts) ? account.accounts.map(object) : [account];
  const accounts: AiAccount[] = (['anthropic', 'openai-codex'] as const).map(id => {
    const a = accountRows.find(a => a.provider === id) ?? {};
    return { id, name: id === 'anthropic' ? 'Claude' : 'Codex', family: id === 'anthropic' ? 'claude' : 'codex', plan: text(a.plan),
      windows: list(a.windows).slice(0, 2).map(raw => {
        const w = object(raw), label = text(w.label) ?? 'Window', reset = text(w.resets_at);
        const remaining = reset ? (Date.parse(reset) - now) / 1000 : NaN;
        // Hermes exposes the standard five-hour session and seven-day week windows.
        // Unknown window lengths deliberately have no pace marker.
        const length = /week/i.test(label) ? 604800 : /^session$/i.test(label) ? 18000 : null;
        const used = finite(w.used_percent);
        return { label: /^weekly$/i.test(label) ? 'Week' : label, used: used === null ? null : Math.min(100, used), reset,
          pace: length && Number.isFinite(remaining) ? Math.max(0, Math.min(1, 1 - remaining / length)) : null };
      }) };
  });
  const days = list(detailed.days).map(bucket), busiest = object(activity.busiestDay);
  return { available: true, value: {
    limits: windows.map((raw, i) => {
      const w = object(raw), used = finite(w.used_percent);
      return { provider: text(account.provider) ?? 'Provider unavailable', label: text(w.label) ?? `Window ${i + 1}`,
        usedPercent: used === null ? null : Math.min(100, used), resetsInS: null,
        resetText: text(w.resets_at) ? `Resets ${text(w.resets_at)}` : null };
    }),
    // This API returns a 30-day session-started cohort, NOT timestamped call rows.
    // Never relabel that cost as today, infer running from ended_at, or invent hourly zeros.
    spendTodayUsd: null, spendTodayPartial: true, active: null, hourly: [], byProject: [], heatmap: [], rowsIgnored: null,
    accounts, period, cost: finite(summary.estimatedCostUsd), partial: summary.partialCost === true,
    tokens: finite(summary.totalTokens), cacheShare: finite(summary.cacheReadShare), split: parts(summary.byProvider),
    buckets: period === 'today' ? list(detailed.hours).map(bucket) : days.slice(period === 'week' ? -7 : -30), days,
    models: shares(summary.models), projects: shares(summary.projects), totalCost: finite(activity.estimatedCostUsd),
    activeDays: finite(activity.activeDays), streak: finite(activity.streak), cohort: detailed.periodBasis === 'session_started_at',
    busiest: text(busiest.date) ? { date: text(busiest.date)!, cost: finite(busiest.costUsd) } : null,
    running: list(detailed.active).flatMap(raw => {
      const r = object(raw), last = finite(r.lastActivityAtS), start = finite(r.startedAtS);
      if (last === null || start === null || now / 1000 - last > 180 || last > now / 1000) return [];
      return [{ id: text(r.sessionId) ?? '', project: text(r.project) ?? 'Unknown project', model: text(r.model) ?? 'Unknown model',
        family: family(r.family), elapsed: Math.max(0, now / 1000 - start), output: finite(r.outputTokens), cost: finite(r.costUsd) }];
    }),
    byModel: models.map(raw => {
      const m = object(raw), input = finite(m.inputTokens), output = finite(m.outputTokens);
      return { key: `${text(m.model) ?? 'Unknown model'} · ${text(m.provider) ?? 'Unknown provider'}`,
        tokens: input === null || output === null ? null : input + output,
        costUsd: finite(m.estimatedCostUsd), rows: finite(m.sessions) ?? 0 };
    }),
  } };
}
export interface AiTopSession { title: string; project: string; model: string; family: AiFamily; source: string; subagent: boolean; cost: number | null; prompt: number | null; output: number | null; calls: number | null; duration: number | null; startedAt: number | null }
export interface AiInsightsData {
  period: AiPeriod; sessions: number; used: number; empty: number; subagents: number; priced: number; included: number; unpriced: number;
  cost: number | null; avgCost: number | null; medianCost: number | null; maxCost: number | null; costPerCall: number | null;
  calls: number | null; messages: number | null; toolCalls: number | null;
  input: number | null; output: number | null; cacheRead: number | null; cacheWrite: number | null; reasoning: number | null; prompt: number | null; cacheHit: number | null;
  avgDuration: number | null; totalDuration: number | null;
  sources: { name: string; sessions: number; cost: number | null }[]; top: AiTopSession[];
}
/** Projects `local.insights` (per-session stats, `island/ai-local-usage.py session_insights`). */
export function insightsFromUsage(usage: AiUsage | null, period: AiPeriod = 'today'): Service<AiInsightsData> {
  if (!usage) return { available: false, reason: 'AI usage has not been read' };
  const local = object(usage.local);
  if (local.available !== true) return { available: false, reason: text(local.unavailableReason) ?? 'Local usage is unavailable' };
  const insights = object(local.insights);
  if (insights.periodBasis !== 'session_started_at') return { available: false, reason: 'Session details are unavailable' };
  const p = object(object(insights.periods)[period]), n = (k: string) => finite(p[k]), count = (k: string) => finite(p[k]) ?? 0;
  return { available: true, value: {
    period, sessions: count('sessions'), used: count('usedSessions'), empty: count('emptySessions'), subagents: count('subagentSessions'),
    priced: count('pricedSessions'), included: count('includedSessions'), unpriced: count('unpricedSessions'),
    cost: n('costUsd'), avgCost: n('avgCostUsd'), medianCost: n('medianCostUsd'), maxCost: n('maxCostUsd'), costPerCall: n('costPerCallUsd'),
    calls: n('apiCalls'), messages: n('messages'), toolCalls: n('toolCalls'),
    input: n('inputTokens'), output: n('outputTokens'), cacheRead: n('cacheReadTokens'), cacheWrite: n('cacheWriteTokens'),
    reasoning: n('reasoningTokens'), prompt: n('promptTokens'), cacheHit: n('cacheHitRate') === null ? null : Math.min(1, n('cacheHitRate')!),
    avgDuration: n('avgDurationS'), totalDuration: n('totalDurationS'),
    sources: list(p.sources).slice(0, 8).map(raw => { const s = object(raw); return { name: text(s.name)?.slice(0, 40) ?? 'unknown', sessions: finite(s.sessions) ?? 0, cost: finite(s.costUsd) }; }),
    top: list(p.topSessions).slice(0, 6).map(raw => { const s = object(raw); return {
      title: text(s.title)?.slice(0, 80) ?? 'Untitled session', project: text(s.project) ?? 'Unknown project', model: text(s.model) ?? 'Unknown model',
      family: family(s.family), source: text(s.source) ?? 'unknown', subagent: s.subagent === true, cost: finite(s.estimatedCostUsd),
      prompt: finite(s.promptTokens), output: finite(s.outputTokens), calls: finite(s.apiCallCount), duration: finite(s.durationS), startedAt: finite(s.startedAtS) }; }),
  } };
}
// Knownness is a validated projection of the tagged registry config, never gallery metadata.
const kinds: Record<string, string> = { terminal:'terminal', monitor:'monitor', ai:'ai', settings:'settings', notifications:'notifications', clipboard:'clipboard', music:'music', timer:'timer', scratchpad:'scratchpad', downloads:'downloads', calendar:'calendar', brightness:'brightness', 'agent-tracker':'agentTracker', mixer:'mixer', 'quick-toggles':'quickToggles', 'update-badge':'updateBadge', 'system-tiles':'systemTiles' };
export function modulesFromRegistry(registry: Registry): ModuleSettingsModel {
  return { version: registry.version, modules: registry.modules.map(m => ({ id: m.id, label: m.id,
    order: m.order, enabled: m.enabled, locked: m.id === 'terminal' || m.id === 'settings',
    known: kinds[m.id] !== undefined && object(m.config).type === kinds[m.id] })) };
}
