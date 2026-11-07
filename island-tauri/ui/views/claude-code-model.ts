// Claude Code's own registry/transcript facts, ported from 5448e1b's Habby metrics-vscode model.
// No filesystem or message content here. Reuses current Hermes prices and History builder.
import { buildHistory, costOf, isObj, modelFamily, modelLabel, numOr, priceFor, strOr, toMs, truncate } from './agent-live-model'
import type { HistoryRaw, LiveBlock } from './agent-live-model'
export const RECENT_MAX = 4096
export const FALLBACK_LIVE_MS = 120e3
export const STALE_BUSY_MS = 10 * 60e3
export const TOK_PER_CHAR = 0.44
export const TOK_PER_SIG_CHAR = 0.21
export const MAX_LIVE_ROWS = 8
export const DEFAULT_CONTEXT_MAX = 1000000
export const IDLE_HIDE_MIN = 30
export const MORE_DELAY_MS = 50
export interface ClaudeCodeCursor { key: string, ino: string, off: number }
export interface ClaudeCodeRequest { cursors: ClaudeCodeCursor[], budgetBytes: number | null }
export interface ClaudeRegEntry { file: string, mtimeMs: number, alive: boolean, procStart: string | null, data: unknown }
export interface FactUsage { input: number, output: number, cacheRead: number, cacheWrite: number, cw1h: number, fast: boolean }
export type Fact =
    | { t: 'a', at: number | null, model: string, key: string, side: boolean, ep: string | null, cwd: string | null, stop: string | null, tool: string | null, chars: number, sig: number, u: FactUsage }
    | { t: 'compact', at: number | null } | { t: 'turnEnd', at: number | null } | { t: 'sys', at: number | null }
    | { t: 'user', at: number | null, meta: boolean, interrupt: boolean }
export interface ClaudeFile {
    key: string, slug: string, sessionId: string, sub: boolean, ino: string, size: number, mtimeMs: number,
    reset: boolean, from: number, next: number, facts: Fact[], skippedHuge: number, badLines: number, error: string | null,
}
export interface ClaudeCodeRaw {
    now: number, claudeDir: string | null, folderMissing: boolean, registry: ClaudeRegEntry[], registryError: string | null,
    files: ClaudeFile[], more: boolean, error: string | null,
}
export type ClaudeSrc = 'vscode' | 'cli' | 'desktop'
export const SRC_LABEL: Record<ClaudeSrc, string> = { vscode: 'VS Code', cli: 'CLI', desktop: 'Desktop' }
export function srcOf(entrypoint: string | null | undefined): ClaudeSrc {
    return entrypoint === 'claude-vscode' ? 'vscode' : entrypoint === 'claude-desktop' ? 'desktop' : 'cli'
}
export function liveSrcLabel(entrypoint: string | null, kind: string | null): string {
    const base = entrypoint === 'claude-vscode' ? 'VS Code' : entrypoint === 'cli' ? 'CLI' : entrypoint === 'claude-desktop' ? 'Desktop' : 'Claude'
    return kind === 'bg' ? base + ' (bg)' : base
}
const KNOWN_EP = new Set(['claude-vscode', 'cli', 'claude-desktop'])
export function projectSlug(cwd: string): string { return cwd.replace(/[^A-Za-z0-9]/g, '-') }
export function safeToolName(x: any): string | null {
    return typeof x !== 'string' || !x ? null : /^[\w.:-]{1,64}$/.test(x) ? x : 'tool'
}
export function safeWord(x: any): string | null { return typeof x === 'string' && /^[A-Za-z][\w .:-]{0,39}$/.test(x) ? x : null }
export function projectName(cwd: string | null | undefined): string | null {
    if (typeof cwd !== 'string' || !cwd.trim()) return null
    return truncate(cwd.replace(/[\\/]+$/, '').split(/[\\/]/).pop() || cwd, 40)
}
export function localDay(t: number): string {
    const d = new Date(t)
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
}
export function contextFor(model: string | null, configured: number): number {
    if (configured !== DEFAULT_CONTEXT_MAX || !model || /\[1m\]/i.test(model)) return configured
    return /haiku|sonnet-4(?!-6)|sonnet-3/i.test(model) ? 200000 : configured
}
// Null-prototype maps: model names and on-disk ids are untrusted keys.
const dict = <T>(): Record<string, T> => Object.create(null) as Record<string, T>
export interface Contrib { model: string, input: number, output: number, cacheRead: number, cacheWrite: number, cost: number | null, est: boolean, d?: string | null }
interface Recent { c: Contrib, chars: number, sig: number }
export interface ModelAgg { calls: number, input: number, output: number, cacheRead: number, cacheWrite: number, cost: number, unpriced: number }
export interface Agg extends ModelAgg { estOut: number, byModel: Record<string, ModelAgg> }
export interface DayAgg { calls: number, tokens: number, cost: number, unpriced: number, estOut: number, models: Record<string, { cost: number, tokens: number }> }
const modelAgg = (): ModelAgg => ({ calls: 0, input: 0, output: 0, cacheRead: 0, cacheWrite: 0, cost: 0, unpriced: 0 })
const dayAgg = (): DayAgg => ({ calls: 0, tokens: 0, cost: 0, unpriced: 0, estOut: 0, models: dict() })
export function emptyAgg(): Agg { return { ...modelAgg(), estOut: 0, byModel: dict() } }
function addTo(a: ModelAgg, c: Contrib, sign: 1 | -1): void {
    a.calls += sign; a.input += sign * c.input; a.output += sign * c.output
    a.cacheRead += sign * c.cacheRead; a.cacheWrite += sign * c.cacheWrite
    if (c.cost === null) a.unpriced += sign
    else a.cost += sign * c.cost
}
export function aggAdd(agg: Agg, c: Contrib, sign: 1 | -1): void {
    addTo(agg, c, sign)
    if (c.est) agg.estOut += sign
    const m = agg.byModel[c.model] ??= modelAgg()
    addTo(m, c, sign)
    if (m.calls <= 0) delete agg.byModel[c.model]
}
export function dayAdd(days: Record<string, DayAgg>, c: Contrib, sign: 1 | -1): void {
    const k = c.d ?? '', d = days[k] ??= dayAgg()
    const tokens = c.input + c.output + c.cacheRead + c.cacheWrite
    d.calls += sign; d.tokens += sign * tokens
    if (c.cost === null) d.unpriced += sign
    else d.cost += sign * c.cost
    if (c.est) d.estOut += sign
    const m = d.models[c.model] ??= { cost: 0, tokens: 0 }
    m.cost += sign * (c.cost ?? 0); m.tokens += sign * tokens
    if (d.calls <= 0) delete days[k]
}
export function sumAggs(aggs: Agg[]): Agg {
    const out = emptyAgg()
    for (const a of aggs) {
        out.calls += a.calls; out.input += a.input; out.output += a.output; out.cacheRead += a.cacheRead
        out.cacheWrite += a.cacheWrite; out.cost += a.cost; out.unpriced += a.unpriced; out.estOut += a.estOut
        for (const [k, m] of Object.entries(a.byModel)) {
            const t = out.byModel[k] ??= modelAgg()
            t.calls += m.calls; t.input += m.input; t.output += m.output; t.cacheRead += m.cacheRead
            t.cacheWrite += m.cacheWrite; t.cost += m.cost; t.unpriced += m.unpriced
        }
    }
    return out
}
export function contribOf(model: string, u: Partial<FactUsage>, stop: string | null, chars: number, sig: number, d: string | null = null): Contrib {
    const input = numOr(u.input, 0)!, cacheRead = numOr(u.cacheRead, 0)!, cacheWrite = numOr(u.cacheWrite, 0)!
    const cw1h = Math.min(cacheWrite, numOr(u.cw1h, 0)!), recorded = numOr(u.output, 0)!
    let output = recorded, est = false
    if (!stop) {
        const guess = Math.round(TOK_PER_CHAR * chars + TOK_PER_SIG_CHAR * sig)
        if (guess > recorded) { output = guess; est = true }
    }
    return { model, input, output, cacheRead, cacheWrite,
        cost: costOf(model, { input, output, cacheRead, cacheWrite: cacheWrite - cw1h, cacheWrite1h: cw1h }, u.fast === true), est, d }
}
export interface Ctx { used: number, model: string, at: number | null, hit: number | null }
export type LastKind = 'tool' | 'think' | 'reply' | 'user' | 'interrupt' | 'turnEnd' | 'compact'
export interface ScanStats { facts: number, skippedHuge: number, badLines: number, resets: number, crossDup: number }
export interface ScanOpts { sessionKey?: string, sessionId?: string, claims?: Map<string, string> | null }
export class TranscriptScan {
    ino: string | null = null
    offset = 0; size = 0; mtimeMs = 0; statted = false; behind = false
    agg: Agg = emptyAgg()
    days: Record<string, DayAgg> = dict()
    private recent = new Map<string, Recent>()
    keys = new Set<string>()
    readonly sessionKey: string
    readonly sessionId: string
    readonly claims: Map<string, string> | null
    firstAt: number | null = null
    lastAt: number | null = null
    entrypoint: string | null = null
    project: string | null = null
    cwd: string | null = null
    model: string | null = null
    ctx: Ctx | null = null
    tool: { name: string, at: number | null } | null = null
    last: { kind: LastKind, at: number | null } | null = null
    compactions = 0
    error: string | null = null
    stats: ScanStats = { facts: 0, skippedHuge: 0, badLines: 0, resets: 0, crossDup: 0 }
    constructor(readonly key: string, readonly isSub: boolean, o: ScanOpts = {}) {
        this.sessionKey = o.sessionKey ?? key
        this.sessionId = o.sessionId ?? (key.split('/').pop() ?? key).replace(/\.jsonl$/, '')
        this.claims = o.claims ?? null
    }
    get caughtUp(): boolean { return this.statted && !this.behind && !this.error }
    get progress(): number { return this.size > 0 ? Math.min(1, this.offset / this.size) : 1 }
    releaseClaims(): void {
        if (this.claims) for (const k of this.keys) if (this.claims.get(k) === this.key) this.claims.delete(k)
        this.keys.clear()
    }
    reset(): void {
        this.offset = 0; this.agg = emptyAgg(); this.days = dict(); this.recent.clear(); this.releaseClaims()
        this.firstAt = this.lastAt = null
        this.entrypoint = this.cwd = this.project = this.model = null
        this.ctx = this.tool = this.last = null
        this.compactions = 0; this.stats.resets++
    }
    apply(f: ClaudeFile, more = false): boolean {
        if (f.reset) { if (this.statted || this.offset > 0) this.reset() }
        else if ((this.statted && f.ino !== this.ino) || f.from !== this.offset) return false
        for (const x of Array.isArray(f.facts) ? f.facts : []) this.onFact(x)
        this.ino = f.ino; this.offset = numOr(f.next, this.offset)!; this.size = numOr(f.size, 0)!
        this.mtimeMs = numOr(f.mtimeMs, 0)!; this.error = strOr(f.error)
        this.stats.skippedHuge += numOr(f.skippedHuge, 0)!; this.stats.badLines += numOr(f.badLines, 0)!
        this.statted = true; this.behind = more && this.offset < this.size
        return true
    }
    onFact(x: Fact): void {
        if (!isObj(x)) return
        this.stats.facts++
        const at = numOr(x.at)
        if (x.t === 'a') this.assistant(x, at)
        else if (x.t === 'compact') { this.seen(at); this.ctx = null; this.compactions++; this.last = { kind: 'compact', at } }
        else if (x.t === 'turnEnd') { this.seen(at); this.last = { kind: 'turnEnd', at } }
        else if (x.t === 'sys') this.seen(at)
        else if (x.t === 'user') { this.seen(at); if (!x.meta) this.last = { kind: x.interrupt ? 'interrupt' : 'user', at } }
    }
    private seen(at: number | null): void {
        if (at === null) return
        if (this.firstAt === null || at < this.firstAt) this.firstAt = at
        if (this.lastAt === null || at > this.lastAt) this.lastAt = at
    }
    private assistant(o: Extract<Fact, { t: 'a' }>, at: number | null): void {
        const model = strOr(o.model)
        if (!model || model === '<synthetic>' || !isObj(o.u)) return
        this.seen(at)
        const chars = numOr(o.chars, 0)!, sig = numOr(o.sig, 0)!, toolName = safeToolName(o.tool), key = strOr(o.key) ?? '|'
        if (this.claims) {
            const owner = this.claims.get(key)
            if (owner !== undefined && owner !== this.key) { this.stats.crossDup++; return }
            this.claims.set(key, this.key)
        }
        this.keys.add(key)
        const prev = this.recent.get(key)
        if (prev) { aggAdd(this.agg, prev.c, -1); dayAdd(this.days, prev.c, -1); this.recent.delete(key) }
        const stop = strOr(o.stop), sumChars = (prev?.chars ?? 0) + chars, sumSig = (prev?.sig ?? 0) + sig
        const c = contribOf(model, o.u, stop, sumChars, sumSig, at !== null ? localDay(at) : prev?.c.d ?? null)
        aggAdd(this.agg, c, 1); dayAdd(this.days, c, 1)
        this.recent.set(key, { c, chars: sumChars, sig: sumSig })
        if (this.recent.size > RECENT_MAX) this.recent.delete(this.recent.keys().next().value!)
        if (!this.entrypoint && typeof o.ep === 'string') this.entrypoint = o.ep
        const side = this.isSub || o.side === true
        if (side && !this.isSub) return
        this.model = model
        if (!side) {
            if (typeof o.cwd === 'string' && o.cwd !== this.cwd) { this.cwd = o.cwd; this.project = projectName(o.cwd) }
            const used = c.input + c.cacheRead + c.cacheWrite
            this.ctx = { used, model, at, hit: used > 0 ? c.cacheRead / used * 100 : null }
        }
        if (toolName) this.tool = { name: toolName, at }
        this.last = { kind: stop === 'end_turn' || stop === 'stop_sequence' ? 'reply' : (toolName || stop === 'tool_use') ? 'tool' : 'think', at }
    }
}
export interface RegEntry {
    pid: number, sessionId: string, cwd: string | null, entrypoint: string | null, status: string | null,
    kind: string | null, startedAt: number | null, mtimeMs: number, procStart: string | null,
    updatedAt: number | null, waitingFor: string | null, name: string | null,
}
function numStr(x: any): string | null { return typeof x === 'string' ? x.trim() : typeof x === 'number' && isFinite(x) ? String(Math.round(x)) : null }
export function parseRegEntry(fileName: string, data: any, mtimeMs: number): RegEntry | null {
    const m = /^(\d{1,10})\.json$/.exec(fileName)
    if (!m || !isObj(data)) return null
    const sessionId = strOr(data.sessionId)
    if (!sessionId || !/^[\w-]{1,80}$/.test(sessionId)) return null
    const upd = [toMs(data.updatedAt), toMs(data.statusUpdatedAt)].filter((x): x is number => x !== null)
    return { pid: Number(m[1]), sessionId, cwd: strOr(data.cwd), entrypoint: strOr(data.entrypoint), status: strOr(data.status),
        kind: strOr(data.kind), startedAt: toMs(data.startedAt), mtimeMs, procStart: numStr(data.procStart),
        updatedAt: upd.length ? Math.max(...upd) : null, waitingFor: safeWord(data.waitingFor),
        // Unlike the fork, no session titles are retained; Rust excludes them too.
        name: null }
}
export function pidIdentity(e: RegEntry, now: string | null): 'ok' | 'reused' | 'unverified' {
    if (now === null || !e.procStart || !/^\d{1,20}$/.test(e.procStart) || !/^\d{1,20}$/.test(now.trim())) return 'unverified'
    return e.procStart === now.trim() ? 'ok' : 'reused'
}
export interface RegistryRead {
    entries: RegEntry[], known: string[], skipped: { dead: number, reused: number, daemon: number, invalid: number }, error: string | null,
}
export function readRegistry(files: ClaudeRegEntry[] | null | undefined, error: string | null = null): RegistryRead {
    const skipped = { dead: 0, reused: 0, daemon: 0, invalid: 0 }, entries: RegEntry[] = [], known: string[] = []
    for (const f of (Array.isArray(files) ? files : []).slice(0, 200)) {
        const e = isObj(f) && typeof f.file === 'string' ? parseRegEntry(f.file, f.data, numOr(f.mtimeMs, 0)!) : null
        if (!e) { skipped.invalid++; continue }
        known.push(e.sessionId)
        if (e.kind === 'daemon' || e.kind === 'daemon-worker') skipped.daemon++
        else if (f.alive !== true) skipped.dead++
        else if (pidIdentity(e, strOr(f.procStart)) === 'reused') skipped.reused++
        else entries.push(e)
    }
    return { entries, known, skipped, error }
}
export type VsStatus = 'working' | 'waiting' | 'idle'
export type StatusProv = 'exact' | 'estimate' | 'quiet' | 'stale' | 'unknown'
export interface ClaudeViewOptions { idleHideMin: number, contextMax: number }
export const CLAUDE_VIEW_DEFAULTS: ClaudeViewOptions = { idleHideMin: IDLE_HIDE_MIN, contextMax: DEFAULT_CONTEXT_MAX }
export interface LiveSession {
    id: string, short: string, src: string, project: string | null, name: string | null, model: string | null, modelId: string | null, family: string,
    status: VsStatus, statusProv: StatusProv, statusRaw: string | null, waitingFor: string | null, tool: string | null, since: number | null,
    ctx: { used: number, max: number, pct: number } | null, tokens: { input: number, output: number, cacheRead: number, cacheWrite: number },
    cost: number | null, costPartial: boolean, outEstimated: boolean, calls: number, hitLast: number | null,
    subagents: number, subCost: number | null, compactions: number, indexing: number | null, error: string | null,
}
export function mapStatus(s: string | null): VsStatus | null {
    return s === 'busy' || s === 'shell' || s === 'compacting' ? 'working' : s === 'waiting' ? 'waiting' : s === 'idle' ? 'idle' : null
}
export function inferStatus(last: { kind: LastKind, at: number | null } | null, activityMs: number, now: number): { status: VsStatus, prov: StatusProv } {
    if (now - activityMs > FALLBACK_LIVE_MS) return { status: 'idle', prov: 'quiet' }
    const k = last?.kind
    return { status: k === 'tool' || k === 'think' || k === 'user' ? 'working' : 'idle', prov: 'estimate' }
}
export interface StatusOut { status: VsStatus, prov: StatusProv, raw: string | null, waitingFor: string | null }
export function liveStatus(reg: RegEntry | null, main: TranscriptScan | null, activityMs: number, now: number): StatusOut {
    if (reg) {
        const mapped = mapStatus(reg.status)
        if (mapped === null) return { status: 'idle', prov: 'unknown', raw: safeWord(reg.status) ?? '?', waitingFor: null }
        const regAt = Math.max(reg.mtimeMs, reg.updatedAt ?? 0)
        if (mapped === 'working' && now - Math.max(activityMs, regAt) > STALE_BUSY_MS) return { status: 'idle', prov: 'stale', raw: null, waitingFor: null }
        return { status: mapped, prov: 'exact', raw: reg.status === 'compacting' ? 'compacting' : null, waitingFor: mapped === 'waiting' ? reg.waitingFor : null }
    }
    const s = inferStatus(main?.last ?? null, activityMs, now)
    return { ...s, raw: null, waitingFor: null }
}
export function costOrNull(a: Agg): number | null { return a.calls > 0 && a.unpriced === a.calls ? null : a.cost }
export function buildLiveSession(b: {
    sessionId: string, src: string, cwd: string | null, st?: StatusOut, name?: string | null,
    main: TranscriptScan | null, subs: TranscriptScan[], subFiles?: number, since?: number | null, contextMax: number,
    status?: VsStatus, statusProv?: StatusProv,
}): LiveSession {
    const st: StatusOut = b.st ?? { status: b.status ?? 'idle', prov: b.statusProv ?? 'estimate', raw: null, waitingFor: null }
    const subFiles = b.subFiles ?? b.subs.length
    const since = b.since !== undefined ? b.since : (Math.max(0, ...[b.main, ...b.subs].map(s => s?.mtimeMs ?? 0)) || null)
    const main = b.main, all = [main, ...b.subs].filter((x): x is TranscriptScan => !!x)
    const total = sumAggs(all.map(s => s.agg)), subAgg = sumAggs(b.subs.map(s => s.agg)), ctxUsed = main?.ctx?.used ?? null
    const ctxMax = contextFor(main?.ctx?.model ?? main?.model ?? null, b.contextMax)
    const reading = all.length < 1 + subFiles || all.some(s => !s.caughtUp && !s.error)
    const bytes = all.reduce((x, s) => x + s.size, 0), done = all.reduce((x, s) => x + Math.min(s.size, s.offset), 0)
    const tool = (st.status === 'working' || st.status === 'waiting') && main?.last?.kind === 'tool' ? main.tool?.name ?? null : null
    return { id: b.sessionId, short: b.sessionId.slice(0, 8), src: b.src, project: projectName(b.cwd) ?? main?.project ?? null, name: null,
        model: modelLabel(main?.model ?? null), modelId: main?.model ? truncate(main.model, 64) : null, family: modelFamily(main?.model ?? null),
        status: st.status, statusProv: st.prov, statusRaw: st.raw, waitingFor: st.waitingFor, tool, since,
        ctx: ctxUsed !== null ? { used: ctxUsed, max: ctxMax, pct: Math.min(100, ctxUsed / ctxMax * 100) } : null,
        tokens: { input: total.input, output: total.output, cacheRead: total.cacheRead, cacheWrite: total.cacheWrite },
        cost: costOrNull(total), costPartial: total.unpriced > 0 && total.unpriced < total.calls,
        outEstimated: total.estOut > 0, calls: total.calls, hitLast: main?.ctx?.hit ?? null,
        subagents: subFiles, subCost: subFiles ? costOrNull(subAgg) : null, compactions: main?.compactions ?? 0,
        indexing: reading ? (bytes > 0 ? Math.floor(done / bytes * 100) : 0) : null, error: main?.error ?? null }
}
export interface SessionDay extends DayAgg { subCost: number }
export interface ClaudeSessionSum {
    id: string, src: ClaudeSrc, project: string | null, startAt: number, lastAt: number, agg: Agg, sub: Agg, days: Record<string, SessionDay>,
}
type DayRow = HistoryRaw['days'][number]
export function claudeHistoryRaw(sessions: ClaudeSessionSum[], src: ClaudeSrc | null, now: number): HistoryRaw {
    const days = new Map<string, DayRow>()
    const dayRow = (d: string): DayRow => {
        let r = days.get(d)
        if (!r) { r = { d, sessions: 0, tokens: 0, cost: 0, subCost: 0, calls: 0, unpriced: 0, estOut: 0 }; days.set(d, r) }
        return r
    }
    const models = new Map<string, { d: string, model: string, cost: number, tokens: number }>()
    let savings = 0
    const top: HistoryRaw['top'] = []
    for (const s of sessions) {
        if (src && s.src !== src) continue
        dayRow(localDay(s.startAt)).sessions++
        for (const [d, x] of Object.entries(s.days)) {
            const r = dayRow(d)
            r.tokens += x.tokens; r.cost += x.cost; r.subCost += x.subCost
            r.calls = (r.calls ?? 0) + x.calls; r.unpriced = (r.unpriced ?? 0) + x.unpriced; r.estOut = (r.estOut ?? 0) + x.estOut
            for (const [model, m] of Object.entries(x.models)) {
                const k = d + '|' + model, row = models.get(k) ?? { d, model, cost: 0, tokens: 0 }
                row.cost += m.cost; row.tokens += m.tokens; models.set(k, row)
            }
        }
        for (const [model, m] of Object.entries(s.agg.byModel)) {
            const p = priceFor(model)
            if (p) savings += m.cacheRead * (p.input - p.cacheRead) / 1e6
        }
        if (s.lastAt >= now - 7 * 86400e3 && s.agg.calls > 0) top.push({ id: s.id,
            title: `${s.project ?? 'session'} · ${s.id.slice(0, 8)}`, cost: s.agg.cost, src: SRC_LABEL[s.src], copy: null,
            calls: s.agg.calls, unpriced: s.agg.unpriced, estOut: s.agg.estOut })
    }
    top.sort((a, b) => b.cost - a.cost)
    return { days: [...days.values()], models: [...models.values()], top: top.slice(0, 10), savings }
}
export function mergeHistoryRaw(parts: { raw: HistoryRaw, src: string | null }[]): HistoryRaw {
    const days = new Map<string, DayRow>(), models: HistoryRaw['models'] = [], top: HistoryRaw['top'] = []
    let savings: number | null = null
    for (const { raw, src } of parts) {
        for (const d of raw.days) {
            const t = days.get(d.d) ?? { d: d.d, sessions: 0, tokens: 0, cost: 0, subCost: 0, calls: 0, unpriced: 0, estOut: 0 }
            t.sessions += d.sessions; t.tokens += d.tokens; t.cost += d.cost; t.subCost += d.subCost
            t.calls = (t.calls ?? 0) + (d.calls ?? 0); t.unpriced = (t.unpriced ?? 0) + (d.unpriced ?? 0); t.estOut = (t.estOut ?? 0) + (d.estOut ?? 0)
            days.set(d.d, t)
        }
        models.push(...raw.models)
        raw.top.forEach((t, i) => top.push({ ...t, src: t.src ?? src, copy: t.copy === undefined ? i : t.copy }))
        if (raw.savings !== null) savings = (savings ?? 0) + raw.savings
    }
    top.sort((a, b) => b.cost - a.cost)
    return { days: [...days.values()], models, top, savings }
}
export type HistorySource = 'all' | 'hermes' | ClaudeSrc
export const HISTORY_SOURCES: [HistorySource, string][] = [['all', 'All'], ['hermes', 'Hermes'], ['desktop', 'Desktop'], ['vscode', 'VS Code'], ['cli', 'CLI']]
export function buildHistoryVariants(hermes: HistoryRaw | null, sessions: ClaudeSessionSum[] | null, blocks: LiveBlock[], now: number): Record<HistorySource, Record<string, any> | null> {
    const out: Record<HistorySource, Record<string, any> | null> = { all: null, hermes: null, vscode: null, cli: null, desktop: null }
    const parts: { raw: HistoryRaw, src: string | null }[] = []
    if (hermes) { out.hermes = buildHistory(mergeHistoryRaw([{ raw: hermes, src: 'Hermes' }]), blocks, now); parts.push({ raw: hermes, src: 'Hermes' }) }
    for (const src of ['vscode', 'cli', 'desktop'] as ClaudeSrc[]) {
        if (!(sessions ?? []).some(s => s.src === src)) continue
        const raw = claudeHistoryRaw(sessions!, src, now)
        out[src] = buildHistory(raw, blocks, now); parts.push({ raw, src: SRC_LABEL[src] })
    }
    out.all = parts.length ? buildHistory(mergeHistoryRaw(parts), blocks, now) : null
    return out
}
export interface ClaudeFeedStats {
    calls: number, files: number, indexed: number, facts: number, skippedHuge: number, badLines: number,
    readErrors: number, crossDup: number, estOutCalls: number, desyncs: number, registry: string, mode: string,
}
export interface ClaudeLive { sessions: LiveSession[], more: number, mode: string, idleHideMin: number, noFolder: boolean, unavailable: string | null, registry: string }
export interface ClaudeProgress { files: number, indexed: number, pct: number, readErrors: number }
interface LiveRef { sessionId: string, reg: RegEntry | null, main: TranscriptScan | null }
export class ClaudeCodeFeed {
    private scans = new Map<string, TranscriptScan>()
    private claims = new Map<string, Map<string, string>>()
    private registry: RegEntry[] = []
    private registryError: string | null = null
    private registered = new Set<string>()
    private gen = 0
    private histCache: { gen: number, sessions: ClaudeSessionSum[] } | null = null
    loaded = false; more = false; now = 0
    claudeDir: string | null = null
    folderMissing = false
    error: string | null = null
    hiddenRows = 0
    stats: ClaudeFeedStats = { calls: 0, files: 0, indexed: 0, facts: 0, skippedHuge: 0, badLines: 0, readErrors: 0, crossDup: 0, estOutCalls: 0, desyncs: 0, registry: 'n/a', mode: 'registry' }
    constructor(private opts: ClaudeViewOptions = CLAUDE_VIEW_DEFAULTS) {}
    request(budgetBytes: number | null = null): ClaudeCodeRequest {
        const cursors: ClaudeCodeCursor[] = []
        for (const s of this.scans.values()) if (s.statted && s.ino !== null) cursors.push({ key: s.key, ino: s.ino, off: s.offset })
        return { cursors, budgetBytes }
    }
    private claimsFor(sk: string): Map<string, string> {
        let c = this.claims.get(sk)
        if (!c) { c = new Map(); this.claims.set(sk, c) }
        return c
    }
    private dropScan(key: string): void {
        const s = this.scans.get(key)
        if (s) { s.releaseClaims(); this.scans.delete(key); this.gen++ }
    }
    ingest(raw: ClaudeCodeRaw): void {
        this.stats.calls++; this.loaded = true; this.now = numOr(raw?.now) ?? Date.now()
        this.claudeDir = strOr(raw?.claudeDir); this.folderMissing = raw?.folderMissing === true
        this.error = strOr(raw?.error); this.more = raw?.more === true
        const r = readRegistry(raw?.registry, strOr(raw?.registryError))
        this.registry = r.entries; this.registryError = r.error
        for (const id of r.known) this.registered.add(id)
        const sk = r.skipped, parts = [sk.dead && `${sk.dead} dead`, sk.reused && `${sk.reused} pid reused`, sk.daemon && `${sk.daemon} daemon`].filter(Boolean)
        this.stats.registry = r.error ?? `${r.entries.length} live` + (parts.length ? `, skipped: ${parts.join(', ')}` : '')
        const listed = new Set<string>()
        for (const f of Array.isArray(raw?.files) ? raw.files : []) {
            if (!isObj(f) || typeof f.key !== 'string' || typeof f.sessionId !== 'string') continue
            listed.add(f.key)
            let s = this.scans.get(f.key)
            if (!s) {
                const slug = strOr(f.slug) ?? f.key.split('/')[0] ?? '', sessionKey = `${slug}/${f.sessionId}.jsonl`
                s = new TranscriptScan(f.key, f.sub === true, { sessionKey, sessionId: f.sessionId, claims: this.claimsFor(sessionKey) }); this.scans.set(f.key, s)
            }
            const before = s.offset
            if (!s.apply(f, this.more)) { this.stats.desyncs++; this.dropScan(f.key); this.more = true; continue }
            if (s.offset !== before || f.reset || (Array.isArray(f.facts) && f.facts.length)) this.gen++
        }
        if (!this.error && (this.claudeDir !== null || this.folderMissing)) for (const k of [...this.scans.keys()]) if (!listed.has(k)) this.dropScan(k)
        // Release empty per-session maps as files disappear; retain known registry ids to avoid ghost fallback.
        const sessions = new Set([...this.scans.values()].map(s => s.sessionKey))
        for (const key of this.claims.keys()) if (!sessions.has(key)) this.claims.delete(key)
        let indexed = 0, facts = 0, skipped = 0, bad = 0, errors = 0, cross = 0, est = 0
        for (const s of this.scans.values()) {
            if (s.caughtUp) indexed++
            if (s.error) errors++
            facts += s.stats.facts; skipped += s.stats.skippedHuge; bad += s.stats.badLines; cross += s.stats.crossDup; est += s.agg.estOut
        }
        Object.assign(this.stats, { files: this.scans.size, indexed, facts, skippedHuge: skipped, badLines: bad, readErrors: errors, crossDup: cross, estOutCalls: est })
    }
    async tick(raw: (req: ClaudeCodeRequest) => Promise<ClaudeCodeRaw>, budgetBytes: number | null = null): Promise<boolean> {
        this.ingest(await raw(this.request(budgetBytes))); return this.more
    }
    private findMain(sessionId: string, cwd: string | null): TranscriptScan | null {
        let best: TranscriptScan | null = null
        const slug = cwd ? projectSlug(cwd) : null
        for (const s of this.scans.values()) {
            if (s.isSub || s.sessionId !== sessionId) continue
            if (slug && s.key.startsWith(slug + '/')) return s
            if (!best || s.mtimeMs > best.mtimeMs) best = s
        }
        return best
    }
    private subsOf(main: TranscriptScan): TranscriptScan[] { return [...this.scans.values()].filter(s => s.isSub && s.sessionKey === main.sessionKey) }
    liveView(now = this.now): ClaudeLive {
        const o = this.opts, idleHide = o.idleHideMin * 60e3, live: LiveRef[] = [], seen = new Set<string>()
        for (const e of this.registry) {
            if (seen.has(e.sessionId)) continue
            seen.add(e.sessionId); live.push({ sessionId: e.sessionId, reg: e, main: this.findMain(e.sessionId, e.cwd) })
        }
        const window = Math.max(idleHide, FALLBACK_LIVE_MS)
        for (const s of this.scans.values()) if (!s.isSub && now - s.mtimeMs < window && !seen.has(s.sessionId) && !this.registered.has(s.sessionId)) {
            seen.add(s.sessionId); live.push({ sessionId: s.sessionId, reg: null, main: s })
        }
        this.stats.mode = this.registryError ? 'fallback (' + this.registryError + ')' : (live.some(l => !l.reg) ? 'registry + fallback' : 'registry')
        const rows: LiveSession[] = []
        for (const l of live) {
            const main = l.main
            if (!l.reg && (!main || (main.entrypoint === null && !main.caughtUp))) continue
            const regEp = l.reg?.entrypoint ?? null, ep = regEp && KNOWN_EP.has(regEp) ? regEp : main?.entrypoint ?? regEp
            const subs = main ? this.subsOf(main) : [], activity = Math.max(0, main?.mtimeMs ?? 0, ...subs.map(s => s.mtimeMs))
            const st = liveStatus(l.reg, main, activity, now), since = activity || l.reg?.startedAt || l.reg?.mtimeMs || null
            const row = buildLiveSession({ sessionId: l.sessionId, src: liveSrcLabel(ep, l.reg?.kind ?? null), cwd: l.reg?.cwd ?? main?.cwd ?? null, st,
                main, subs, subFiles: subs.length, since, contextMax: o.contextMax })
            if (row.status === 'idle' && row.since !== null && now - row.since > idleHide) continue
            rows.push(row)
        }
        const rank = (s: VsStatus) => s === 'waiting' ? 0 : s === 'working' ? 1 : 2
        rows.sort((a, b) => rank(a.status) - rank(b.status) || (b.since ?? 0) - (a.since ?? 0))
        this.hiddenRows = Math.max(0, rows.length - MAX_LIVE_ROWS)
        return { sessions: rows.slice(0, MAX_LIVE_ROWS), more: this.hiddenRows, mode: this.stats.mode, idleHideMin: o.idleHideMin,
            noFolder: this.folderMissing, unavailable: this.error, registry: this.stats.registry }
    }
    busy(live: ClaudeLive | null): boolean { return !!live && live.sessions.some(s => s.status !== 'idle') }
    progress(): ClaudeProgress {
        let bytes = 0, read = 0
        for (const s of this.scans.values()) { bytes += s.size; read += Math.min(s.size, s.offset) }
        return { files: this.scans.size, indexed: this.stats.indexed,
            pct: !this.more ? 100 : bytes > 0 ? Math.min(99, Math.floor(read / bytes * 100)) : 0, readErrors: this.stats.readErrors }
    }
    sessions(): ClaudeSessionSum[] {
        if (this.histCache && this.histCache.gen === this.gen) return this.histCache.sessions
        const byMain = new Map<string, { main: TranscriptScan | null, subs: TranscriptScan[] }>()
        for (const s of this.scans.values()) {
            const g = byMain.get(s.sessionKey) ?? { main: null, subs: [] }
            if (s.isSub) g.subs.push(s)
            else g.main = s
            byMain.set(s.sessionKey, g)
        }
        const out: ClaudeSessionSum[] = []
        for (const g of byMain.values()) {
            const all = [g.main, ...g.subs].filter((x): x is TranscriptScan => !!x)
            const startAt = g.main?.firstAt ?? Math.min(...all.map(s => s.firstAt ?? Infinity))
            if (!isFinite(startAt)) continue
            const agg = sumAggs(all.map(s => s.agg))
            if (agg.calls <= 0) continue
            const startDay = localDay(startAt), days: Record<string, SessionDay> = dict()
            for (const s of all) for (const [k, x] of Object.entries(s.days)) {
                const d = k || startDay, t = days[d] ??= { ...dayAgg(), subCost: 0 }
                t.calls += x.calls; t.tokens += x.tokens; t.cost += x.cost; t.unpriced += x.unpriced; t.estOut += x.estOut
                if (s.isSub) t.subCost += x.cost
                for (const [model, m] of Object.entries(x.models ?? {})) {
                    const tm = t.models[model] ??= { cost: 0, tokens: 0 }; tm.cost += m.cost; tm.tokens += m.tokens
                }
            }
            const lastAt = Math.max(startAt, ...all.map(s => s.lastAt ?? 0)), first = all[0]!
            out.push({ id: first.sessionId, src: srcOf(g.main?.entrypoint ?? g.subs.find(s => s.entrypoint)?.entrypoint ?? null),
                project: g.main?.project ?? g.subs.find(s => s.project)?.project ?? null, startAt, lastAt, agg, sub: sumAggs(g.subs.map(s => s.agg)), days })
        }
        this.histCache = { gen: this.gen, sessions: out }; return out
    }
}
