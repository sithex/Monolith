// Centenaria AI Agents tab, Live page: Habby's metrics dock model ported to the webview
// (source: Habby app/lib/habby/metrics-model.ts; contract: docs/agent-live.md). Kept close to verbatim
// so Habby's tests port over (tests/phase7b-agent-live-model.test.mjs). Differences from Habby:
//   - no fs: LogTailer is replaced by LogAccumulator, fed by AgentLiveRaw.log (Rust keeps the file
//     cursor and does the rotation-safe reads, services/agent_live.rs)
//   - the dock geometry (dockRect, Electron/Windows only) is dropped
//   - the bottom section ports metrics-reader.ts readSessionAndPlugin / readBgProcs over the raw
//     AgentLiveRaw data and metrics.ts collect() as AgentLiveFeed (the poll loop's per-tick logic)
// Habby's original header:
// Habby metrics dock: pure logic (no Electron), unit-tested in habby-tests/metrics.test.cjs.
//   - agent.log line parser + rotation-safe tailer (SPIKE-metrics C9)
//   - live-session resolution (Habby process tree x runtime/active_sessions.json)
//   - snapshot/plan normalisation (Hermes plugin files, read defensively: any missing field = null)
//   - view model for the dock renderer (Live + History), alerts (X1), pace, dock geometry (C7)
// Provenance of every number: 'official' (Anthropic plan %), 'exact' (Hermes counters), 'estimate'.
export type Prov = 'official' | 'exact' | 'estimate'

// ---------------------------------------------------------------------------------------------
// prices (API list price per million tokens: input, output, cache read, cache write). Opus 5.5 = Hermes
// agent/usage_pricing.py official_docs_snapshot (_OPUS_5_5); Fable 5.1 = the models.dev row
// (anthropic/claude-fable-5-1), which is what Hermes prices it from. Used only for per-call / turn / savings
// ESTIMATES. Session totals come from state.db estimated_cost_usd. Fast-mode calls are priced at the normal
// rate here (Hermes's hook usage drops raw_usage, review minor 5).
// `cacheWrite` = the 5-minute cache write (1.25x input); `cacheWrite1h` = the 1-hour cache write (2x input,
// Anthropic pricing page), which Claude Code uses for its main thread (spike S2: ephemeral_1h_input_tokens).
// Sonnet / Haiku rows = Hermes usage_pricing.py official_docs_snapshot (Sonnet 5's $2/$10 intro ended
// 2026-08-31, so it is priced like Sonnet 4.6). `fast` = fast-mode multiplier (Hermes
// _ANTHROPIC_FAST_MODE_PRICING: Opus 5.5 = 2x). A model with no row has no price: callers show "—".
export interface Price { input: number, output: number, cacheRead: number, cacheWrite: number, cacheWrite1h: number, fast?: number }
/**
 * Exact model match (review m6: never guess a price for a future model): the family-version id, optionally
 * prefixed (`claude-`, `anthropic/claude-`) and optionally followed by a date stamp (`-20251001`) and a
 * bracket tag (`[1m]`), and nothing else. `claude-sonnet-5-1` or `claude-opus-5-50` match no row -> no price.
 */
function exactModel (...ids: string[]): RegExp {
    return new RegExp(`(?:^|[^a-z0-9])(?:${ids.join('|')})(?:-\\d{8})?(?:\\[[a-z0-9]+\\])?$`, 'i')
}
export const PRICES: { match: RegExp, price: Price }[] = [
    { match: exactModel('opus-5-5'), price: { input: 4, output: 20, cacheRead: 0.2, cacheWrite: 5, cacheWrite1h: 8, fast: 2 } },
    { match: exactModel('fable-5-1'), price: { input: 10, output: 50, cacheRead: 0.25, cacheWrite: 12.5, cacheWrite1h: 20 } },
    { match: exactModel('sonnet-5', 'sonnet-4-6', 'sonnet-4-5', 'sonnet-4'), price: { input: 3, output: 15, cacheRead: 0.3, cacheWrite: 3.75, cacheWrite1h: 6 } },
    { match: exactModel('haiku-4-5'), price: { input: 1, output: 5, cacheRead: 0.1, cacheWrite: 1.25, cacheWrite1h: 2 } },
]

export function priceFor (model: string | null | undefined): Price | null {
    if (!model) {
        return null
    }
    return PRICES.find(p => p.match.test(model))?.price ?? null
}

/**
 * API-equivalent $ of one call; `input` = UNCACHED input (Anthropic semantics). `cacheWrite` is priced as a
 * 5-minute write, the optional `cacheWrite1h` as a 1-hour write; `fast` applies the fast-mode multiplier.
 */
export function costOf (model: string | null | undefined, t: { input: number, output: number, cacheRead: number, cacheWrite: number, cacheWrite1h?: number }, fast = false): number | null {
    const p = priceFor(model)
    if (!p) {
        return null
    }
    const base = (t.input * p.input + t.output * p.output + t.cacheRead * p.cacheRead + t.cacheWrite * p.cacheWrite + (t.cacheWrite1h ?? 0) * p.cacheWrite1h) / 1e6
    return fast && p.fast ? base * p.fast : base
}

// ---------------------------------------------------------------------------------------------
// small helpers

export function isObj (x: any): x is Record<string, any> {
    return !!x && typeof x === 'object' && !Array.isArray(x)
}

export function numOr (x: any, fallback: number | null = null): number | null {
    return typeof x === 'number' && isFinite(x) ? x : fallback
}

export function strOr (x: any, fallback: string | null = null): string | null {
    return typeof x === 'string' ? x : fallback
}

/** epoch seconds, epoch ms or ISO string -> epoch ms; null when unparseable */
export function toMs (x: any): number | null {
    if (typeof x === 'number' && isFinite(x)) {
        return x < 1e11 ? Math.round(x * 1000) : Math.round(x)
    }
    if (typeof x === 'string' && x.trim()) {
        if (/^\d+(\.\d+)?$/.test(x.trim())) {
            return toMs(parseFloat(x))
        }
        const t = Date.parse(x)
        return isNaN(t) ? null : t
    }
    return null
}

export function median (xs: number[]): number | null {
    if (!xs.length) {
        return null
    }
    const s = [...xs].sort((a, b) => a - b)
    const m = Math.floor(s.length / 2)
    return s.length % 2 ? s[m]! : (s[m - 1]! + s[m]!) / 2
}

export function truncate (s: string | null | undefined, n: number): string | null {
    if (typeof s !== 'string') {
        return null
    }
    // strip control characters (the renderer uses textContent, this only keeps rows on one line)
    // eslint-disable-next-line no-control-regex
    const clean = s.replace(/[\u0000-\u001f\u007f]+/g, ' ').trim()
    return clean.length > n ? clean.slice(0, n - 1) + '\u2026' : clean
}

// ---------------------------------------------------------------------------------------------
// agent.log (Hermes agent/turn_usage.py:209-213 + :96, conversation_loop "Turn ended", tool_executor)

export interface LogCall {
    sid: string
    n: number
    model: string
    provider: string
    /** full prompt = input + cache_read + cache_write (context-fill proxy); null for usage=unavailable */
    prompt: number | null
    output: number | null
    latencyS: number | null
    cacheRead: number | null
    cacheWrite: number | null
    hitPct: number | null
    id: string | null
    upstream: string | null
    at: number
}

export type LogEvent =
    | { kind: 'call', call: LogCall }
    | { kind: 'turnEnd', sid: string, at: number, reason: string }
    | { kind: 'tool', sid: string, at: number, tool: string, status: 'ok' | 'error', ms: number }
    | { kind: 'error', sid: string, at: number, what: 'rate-limited' | 'overloaded' | 'api-error' }
    | { kind: 'ctx', sid: string | null, at: number, contextLength: number }

const TS_RX = /^(\d{4})-(\d{2})-(\d{2}) (\d{2}):(\d{2}):(\d{2}),(\d{3}) /
const CALL_RX = /\[(?<sid>[^\]\s]+)\] agent\.conversation_loop: API call #(?<n>\d+): model=(?<model>\S+) provider=(?<prov>\S+) in=(?<in>\d+) out=(?<out>\d+) total=(?<tot>\d+) latency=(?<lat>[\d.]+)s(?: cache=(?<cr>\d+)\/(?<pt>\d+) \((?<pct>\d+)%\))?(?: write=(?<cw>\d+))?(?: id=(?<id>\S+))?(?: upstream=(?<up>\S+))?/
const CALL_UNAVAIL_RX = /\[(?<sid>[^\]\s]+)\] agent\.conversation_loop: API call #(?<n>\d+): model=(?<model>\S+) provider=(?<prov>\S+) in=\? out=\?/
const TURN_END_RX = /\[(?<sid>[^\]\s]+)\] agent\.conversation_loop: Turn ended: reason=(?<reason>[^\s(]+)/
const TOOL_RX = /\[(?<sid>[^\]\s]+)\] agent\.tool_executor: [Tt]ool (?<tool>[\w.:-]+) (?<st>completed|failed|returned error) \((?<s>[\d.]+)s/
const ERR_RX = /^\S+ \S+ (?:ERROR|WARNING) \[(?<sid>[^\]\s]+)\] agent\.conversation_loop: (?:API call failed|Retrying API call)/
const CTX_RX = /(?:\[(?<sid>[^\]\s]+)\] )?agent\.context_compressor: Context compressor initialized: .*?context_length=(?<len>\d+)/

/** "2026-10-20 19:32:14,028" (local time) -> epoch ms */
export function parseLogTime (line: string): number | null {
    const m = TS_RX.exec(line)
    if (!m) {
        return null
    }
    return new Date(+m[1]!, +m[2]! - 1, +m[3]!, +m[4]!, +m[5]!, +m[6]!, +m[7]!).getTime()
}

export function parseLogLine (line: string): LogEvent | null {
    if (line.length > 20000) {
        return null
    }
    const at = parseLogTime(line)
    if (at === null) {
        return null
    }
    if (line.includes('API call #')) {
        const m = CALL_RX.exec(line)
        if ((m as any)?.groups) {
            const g = (m as any).groups
            return {
                kind: 'call',
                call: {
                    sid: g.sid, n: +g.n, model: g.model, provider: g.prov,
                    prompt: +g.in, output: +g.out, latencyS: +g.lat,
                    cacheRead: g.cr !== undefined ? +g.cr : null,
                    cacheWrite: g.cw !== undefined ? +g.cw : null,
                    hitPct: g.pct !== undefined ? +g.pct : null,
                    id: g.id ?? null, upstream: g.up ?? null, at,
                },
            }
        }
        const u = CALL_UNAVAIL_RX.exec(line)
        if ((u as any)?.groups) {
            const g = (u as any).groups
            return {
                kind: 'call',
                call: { sid: g.sid, n: +g.n, model: g.model, provider: g.prov, prompt: null, output: null, latencyS: null, cacheRead: null, cacheWrite: null, hitPct: null, id: null, upstream: null, at },
            }
        }
    }
    if (line.includes('Turn ended:')) {
        const m = TURN_END_RX.exec(line)
        if ((m as any)?.groups) {
            return { kind: 'turnEnd', sid: (m as any).groups.sid, at, reason: (m as any).groups.reason }
        }
    }
    if (line.includes('agent.tool_executor:')) {
        const m = TOOL_RX.exec(line)
        if ((m as any)?.groups) {
            return { kind: 'tool', sid: (m as any).groups.sid, at, tool: (m as any).groups.tool, status: (m as any).groups.st === 'completed' ? 'ok' : 'error', ms: Math.round(parseFloat((m as any).groups.s) * 1000) }
        }
    }
    if (line.includes('context_length=')) {
        const m = CTX_RX.exec(line)
        if ((m as any)?.groups) {
            return { kind: 'ctx', sid: (m as any).groups.sid ?? null, at, contextLength: +(m as any).groups.len }
        }
    }
    const e = ERR_RX.exec(line)
    if (e) {
        const what = /429|rate.?limit/i.test(line) ? 'rate-limited' : /overloaded|529/i.test(line) ? 'overloaded' : 'api-error'
        return { kind: 'error', sid: (e as any).groups.sid, at, what }
    }
    return null
}

export interface SessionLog {
    calls: LogCall[]
    /** call index (into calls) of the first call after the last "Turn ended" */
    turnStart: number
    lastTurnEndAt: number | null
    turns: { endAt: number, calls: number, cost: number | null }[]
    steps: { tool: string, status: 'ok' | 'error' | 'running', ms: number, at: number }[]
    lastError: { at: number, what: string } | null
    contextLength: number | null
    lastAt: number
}

/** Per-session aggregation of parsed log events (bounded memory). */
export class LogState {
    sessions = new Map<string, SessionLog>()
    lastError: { at: number, what: string } | null = null
    lastContextLength: number | null = null
    lines = 0

    private get (sid: string): SessionLog {
        let s = this.sessions.get(sid)
        if (!s) {
            s = { calls: [], turnStart: 0, lastTurnEndAt: null, turns: [], steps: [], lastError: null, contextLength: null, lastAt: 0 }
            this.sessions.set(sid, s)
            if (this.sessions.size > 200) {
                // drop the least recently active session
                let oldest: string | null = null
                let t = Infinity
                for (const [k, v] of this.sessions) {
                    if (v.lastAt < t) {
                        t = v.lastAt
                        oldest = k
                    }
                }
                if (oldest !== null) {
                    this.sessions.delete(oldest)
                }
            }
        }
        return s
    }

    apply (ev: LogEvent): void {
        this.lines++
        switch (ev.kind) {
            case 'call': {
                const s = this.get(ev.call.sid)
                s.calls.push(ev.call)
                s.lastAt = ev.call.at
                if (s.calls.length > 120) {
                    const drop = s.calls.length - 120
                    s.calls.splice(0, drop)
                    s.turnStart = Math.max(0, s.turnStart - drop)
                }
                break
            }
            case 'turnEnd': {
                const s = this.get(ev.sid)
                const turnCalls = s.calls.slice(s.turnStart)
                let cost: number | null = 0
                for (const c of turnCalls) {
                    const k = callCost(c)
                    cost = k === null || cost === null ? null : cost + k
                }
                s.turns.push({ endAt: ev.at, calls: turnCalls.length, cost })
                if (s.turns.length > 50) {
                    s.turns.shift()
                }
                s.turnStart = s.calls.length
                s.lastTurnEndAt = ev.at
                s.lastAt = ev.at
                break
            }
            case 'tool': {
                const s = this.get(ev.sid)
                s.steps.push({ tool: ev.tool, status: ev.status, ms: ev.ms, at: ev.at })
                if (s.steps.length > 5) {
                    s.steps.shift()
                }
                s.lastAt = ev.at
                break
            }
            case 'error': {
                // per session only: the dock alerts on the live session's own retry lines (review minor 14)
                this.lastError = { at: ev.at, what: ev.what }
                this.get(ev.sid).lastError = { at: ev.at, what: ev.what }
                break
            }
            case 'ctx': {
                this.lastContextLength = ev.contextLength
                if (ev.sid) {
                    this.get(ev.sid).contextLength = ev.contextLength
                }
                break
            }
        }
    }
}

export function callCost (c: LogCall): number | null {
    if (c.prompt === null || c.output === null) {
        return null
    }
    const cr = c.cacheRead ?? 0
    const cw = c.cacheWrite ?? 0
    return costOf(c.model, { input: Math.max(0, c.prompt - cr - cw), output: c.output, cacheRead: cr, cacheWrite: cw })
}

export interface LogChunk { cursor: string | null, lines: string[], rotated: boolean, error: string | null }
export interface AccumulatorStats { reads: number, rotations: number, lines: number, lastLines: number, error: string | null }

/**
 * Centenaria replacement for Habby's LogTailer (rotation-safe agent.log tailer, SPIKE-metrics C9). The webview
 * has no fs: Rust (agent_live_raw, services/agent_live.rs) does Habby's open -> read [cursor, size) -> close,
 * the inode-change / shrink rotation check, the drain of agent.log.1, the first-read window (last 512 KiB,
 * partial first line dropped) and returns only COMPLETE lines plus an opaque cursor. This side keeps the cursor
 * for the next request and feeds each line through parseLogLine -> LogState.apply, like Habby's LogReader.
 * `rotated` lines are already in order (old file's rest first), so nothing is flushed or reset here; the
 * LogState keeps every session across a rotation (Habby: same).
 */
export class LogAccumulator {
    state = new LogState()
    cursor: string | null = null
    stats: AccumulatorStats = { reads: 0, rotations: 0, lines: 0, lastLines: 0, error: null }

    constructor (private onLine?: (line: string) => void) { }

    /** apply one AgentLiveRaw.log; returns the log error (null = ok), the LogReader.tick() contract */
    feed (chunk: LogChunk | null | undefined): string | null {
        if (!isObj(chunk)) {
            this.stats.error = 'agent.log n/a'
            return this.stats.error
        }
        this.stats.reads++
        this.stats.error = strOr(chunk.error)
        if (chunk.rotated === true) {
            this.stats.rotations++
        }
        if (typeof chunk.cursor === 'string') {
            this.cursor = chunk.cursor
        }
        const lines = Array.isArray(chunk.lines) ? chunk.lines : []
        this.stats.lastLines = lines.length
        for (const raw of lines) {
            if (typeof raw !== 'string') {
                continue
            }
            const line = raw.endsWith('\r') ? raw.slice(0, -1) : raw
            this.stats.lines++
            this.onLine?.(line)
            const ev = parseLogLine(line)
            if (ev) {
                this.state.apply(ev)
            }
        }
        return this.stats.error
    }
}

// ---------------------------------------------------------------------------------------------
// live session resolution

/**
 * chatId / platform: active_sessions metadata. Gateway-hosted sessions (surface 'gateway:local', which is
 * how the island's TUI runs) carry a routing key as session_id ('agent:main:local:dm:<chat_id>'); the
 * state.db row and the plugin snapshot are keyed by chat_id. See canonicalSessionId.
 */
export interface ActiveSession { pid: number, sessionId: string, surface: string | null, startedAt: number | null, chatId?: string | null, platform?: string | null }
export interface ProcLite { pid: number, ppid: number, exe: string }

export function parseActiveSessions (raw: any): ActiveSession[] {
    const entries = Array.isArray(raw?.entries) ? raw.entries : Array.isArray(raw) ? raw : []
    const out: ActiveSession[] = []
    for (const e of entries) {
        if (!isObj(e) || typeof e.session_id !== 'string' || typeof e.pid !== 'number') {
            continue
        }
        const md = isObj(e.metadata) ? e.metadata : null
        const chatId = strOr(md?.chat_id)
        out.push({
            pid: e.pid, sessionId: e.session_id, surface: strOr(e.surface), startedAt: toMs(e.started_at),
            ...(chatId ? { chatId, platform: strOr(md?.platform) } : {}),
        })
    }
    return out
}

/** All descendant pids of rootPid (parent-pid walk; ppid reuse is harmless here: read-only matching). */
export function descendants (procs: ProcLite[], rootPid: number): Set<number> {
    const kids = new Map<number, number[]>()
    for (const p of procs) {
        if (p.pid === p.ppid) {
            continue
        }
        const list = kids.get(p.ppid) ?? []
        list.push(p.pid)
        kids.set(p.ppid, list)
    }
    const out = new Set<number>()
    const stack = [rootPid]
    while (stack.length) {
        const pid = stack.pop()!
        for (const k of kids.get(pid) ?? []) {
            if (!out.has(k)) {
                out.add(k)
                stack.push(k)
            }
        }
    }
    return out
}

export type MatchedBy = 'habby-child' | 'gateway-local' | 'newest-live' | 'unverified' | 'db-newest' | 'snapshot-newest' | 'none'

/**
 * Which Hermes session belongs to this Habby: an active_sessions entry whose pid is a descendant of the
 * Habby main process (the TUI gateway python under hermes.exe under the PTY). Fallback: the newest
 * entry whose pid still runs (surface tui preferred). procs = null (no koffi): newest entry, labelled
 * 'unverified' (its pid may be dead).
 */
export function resolveSession (entries: ActiveSession[], procs: ProcLite[] | null, habbyPid: number): { entry: ActiveSession | null, matchedBy: MatchedBy } {
    const byNewest = (a: ActiveSession, b: ActiveSession) => (b.startedAt ?? 0) - (a.startedAt ?? 0)
    const tuiFirst = (a: ActiveSession, b: ActiveSession) => (a.surface === 'tui' ? 0 : 1) - (b.surface === 'tui' ? 0 : 1) || byNewest(a, b)
    if (procs) {
        const mine = descendants(procs, habbyPid)
        const own = entries.filter(e => mine.has(e.pid)).sort(tuiFirst)
        if (own.length) {
            return { entry: own[0]!, matchedBy: 'habby-child' }
        }
        const alive = new Set(procs.map(p => p.pid))
        const live = entries.filter(e => alive.has(e.pid)).sort(tuiFirst)
        return live.length ? { entry: live[0]!, matchedBy: 'newest-live' } : { entry: null, matchedBy: 'none' }
    }
    const all = [...entries].sort(tuiFirst)
    return all.length ? { entry: all[0]!, matchedBy: 'unverified' } : { entry: null, matchedBy: 'none' }
}

// ---------------------------------------------------------------------------------------------
// plugin files (contract with Coder B, hermes-plugin/habby-metrics). Every field optional.

export type LiveStatus = 'idle' | 'thinking' | 'tool' | 'waiting' | 'compacting' | 'error'
const STATUSES: LiveStatus[] = ['idle', 'thinking', 'tool', 'waiting', 'compacting', 'error']

export interface SnapSession {
    status: LiveStatus | null
    model: string | null
    turn: { startedAt: number | null, calls: number | null, input: number | null, output: number | null, cacheRead: number | null, cacheWrite: number | null, cost: number | null } | null
    currentTool: { name: string, preview: string | null, startedAt: number | null } | null
    steps: { tool: string, status: 'ok' | 'running' | 'error', ms: number | null }[]
    lastCall: { input: number | null, output: number | null, cacheRead: number | null, cacheWrite: number | null, prompt: number | null, ttfbMs: number | null, latencyMs: number | null, at: number | null } | null
    context: { used: number | null, max: number | null, pct: number | null, estimated: boolean, breakdown: { label: string, tokens: number }[] | null } | null
    waiting: { kind: string, since: number | null } | null
    subagents: { id: string, goal: string | null, role: string | null, model: string | null, status: string | null, startedAt: number | null, ms: number | null, tools: number | null, input: number | null, output: number | null, cacheRead: number | null, cacheWrite: number | null, cost: number | null }[]
    compressions: number | null
    /** R10 agent tree: child sessions carry their parent + role/goal */
    parentSessionId: string | null
    role: string | null
    goal: string | null
    startedAt: number | null
    toolsByName: { name: string, n: number }[]
    totals: { calls: number | null, input: number | null, output: number | null, cacheRead: number | null, cacheWrite: number | null, cost: number | null, tools: number | null } | null
    turns: number | null
    /** background-review / btw forks sharing this session id (plugin >= 1.1; never in turn/totals) */
    background: { calls: number | null, cost: number | null, tools: number | null } | null
    /** prompt tokens of the last <= 10 calls (plugin >= 1.1): one source for "calls left" */
    promptHistory: number[]
    /** context-compression rotation (plugin >= 1.1) */
    continuedTo: string | null
    continuedFrom: string | null
}

export interface Snapshot { pid: number | null, writtenAt: number | null, sessions: Map<string, SnapSession> }

function normBreakdown (b: any): { label: string, tokens: number }[] | null {
    const out: { label: string, tokens: number }[] = []
    if (Array.isArray(b)) {
        for (const x of b) {
            const label = strOr(x?.label) ?? strOr(x?.name) ?? strOr(x?.category)
            const tokens = numOr(x?.tokens) ?? numOr(x?.value)
            if (label && tokens !== null) {
                out.push({ label: truncate(label, 32)!, tokens })
            }
        }
    } else if (isObj(b)) {
        for (const [k, v] of Object.entries(b)) {
            const tokens = numOr(v) ?? numOr((v as any)?.tokens)
            if (tokens !== null) {
                out.push({ label: truncate(k.replace(/_/g, ' '), 32)!, tokens })
            }
        }
    }
    return out.length ? out.slice(0, 12) : null
}

export function normSnapSession (s: any): SnapSession {
    const t = isObj(s.turn) ? s.turn : null
    const ct = isObj(s.current_tool) ? s.current_tool : null
    const lc = isObj(s.last_call) ? s.last_call : null
    const cx = isObj(s.context) ? s.context : null
    const w = isObj(s.waiting) ? s.waiting : null
    return {
        status: STATUSES.includes(s.status) ? s.status : null,
        model: strOr(s.model),
        turn: t ? { startedAt: toMs(t.started_at), calls: numOr(t.calls), input: numOr(t.input), output: numOr(t.output), cacheRead: numOr(t.cache_read), cacheWrite: numOr(t.cache_write), cost: numOr(t.cost_usd) } : null,
        currentTool: ct && typeof ct.name === 'string' ? { name: truncate(ct.name, 48)!, preview: truncate(ct.preview, 60), startedAt: toMs(ct.started_at) } : null,
        steps: (Array.isArray(s.steps) ? s.steps : []).filter((x: any) => isObj(x) && typeof x.tool === 'string').slice(-5).map((x: any) => ({
            tool: truncate(x.tool, 32)!, status: x.status === 'error' ? 'error' : x.status === 'running' ? 'running' : 'ok', ms: numOr(x.ms),
        })),
        lastCall: lc ? { input: numOr(lc.in), output: numOr(lc.out), cacheRead: numOr(lc.cache_read), cacheWrite: numOr(lc.cache_write), prompt: numOr(lc.prompt), ttfbMs: numOr(lc.ttfb_ms), latencyMs: numOr(lc.latency_ms), at: toMs(lc.at) } : null,
        context: cx ? { used: numOr(cx.used), max: numOr(cx.max), pct: numOr(cx.pct), estimated: !!cx.estimated, breakdown: normBreakdown(cx.breakdown) } : null,
        waiting: w ? { kind: strOr(w.kind) ?? 'input', since: toMs(w.since) } : null,
        subagents: (Array.isArray(s.subagents) ? s.subagents : []).filter(isObj).slice(0, 30).map((a: any, i: number) => ({
            id: strOr(a.session_id) ?? strOr(a.id) ?? String(i), goal: truncate(a.goal, 80), role: truncate(a.role, 24), model: strOr(a.model), status: strOr(a.status),
            startedAt: toMs(a.started_at), ms: numOr(a.ms), tools: numOr(a.tools), input: numOr(a.input), output: numOr(a.output),
            cacheRead: numOr(a.cache_read), cacheWrite: numOr(a.cache_write), cost: numOr(a.cost_usd),
        })),
        compressions: numOr(s.compressions),
        parentSessionId: strOr(s.parent_session_id) ?? strOr(s.parent),
        role: truncate(strOr(s.role), 24),
        goal: truncate(strOr(s.goal), 80),
        startedAt: toMs(s.started_at ?? s.first_seen),
        toolsByName: normToolsByName(s.tools_by_name ?? s.totals?.tools_by_name),
        totals: normTotals(isObj(s.session) ? s.session : isObj(s.totals) ? s.totals : null),
        turns: numOr(s.turns) ?? numOr(s.turn_count),
        background: isObj(s.background) ? { calls: numOr(s.background.calls), cost: numOr(s.background.cost_usd), tools: numOr(s.background.tool_calls) } : null,
        promptHistory: (Array.isArray(s.prompt_history) ? s.prompt_history : []).filter((x: any) => numOr(x) !== null).slice(-10),
        continuedTo: strOr(s.continued_to),
        continuedFrom: strOr(s.continued_from),
    }
}

function normToolsByName (x: any): { name: string, n: number }[] {
    const out: { name: string, n: number }[] = []
    if (isObj(x)) {
        for (const [k, v] of Object.entries(x)) {
            const n = numOr(v) ?? numOr((v as any)?.count) ?? numOr((v as any)?.n)
            if (n !== null) {
                out.push({ name: truncate(k, 32)!, n })
            }
        }
    } else if (Array.isArray(x)) {
        for (const v of x) {
            const name = strOr(v?.name) ?? strOr(v?.tool)
            const n = numOr(v?.count) ?? numOr(v?.n)
            if (name && n !== null) {
                out.push({ name: truncate(name, 32)!, n })
            }
        }
    }
    return out.sort((a, b) => b.n - a.n)
}

function normTotals (t: any): SnapSession['totals'] {
    if (!t) {
        return null
    }
    return {
        calls: numOr(t.calls) ?? numOr(t.api_calls), input: numOr(t.input), output: numOr(t.output),
        cacheRead: numOr(t.cache_read), cacheWrite: numOr(t.cache_write), cost: numOr(t.cost_usd), tools: numOr(t.tools) ?? numOr(t.tool_calls),
    }
}

export function parseSnapshot (raw: any): Snapshot | null {
    if (!isObj(raw)) {
        return null
    }
    const sessions = new Map<string, SnapSession>()
    if (isObj(raw.sessions)) {
        for (const [id, s] of Object.entries(raw.sessions)) {
            if (isObj(s)) {
                sessions.set(id, normSnapSession(s))
            }
        }
    }
    return { pid: numOr(raw.pid), writtenAt: toMs(raw.written_at), sessions }
}

/** The snapshot entry for a session id, following compression continuations (old id -> new id, bounded). */
export function followContinuation (snap: Snapshot, sid: string): SnapSession | null {
    let s = snap.sessions.get(sid) ?? null
    for (let i = 0; s?.continuedTo && i < 20; i++) {
        const next = snap.sessions.get(s.continuedTo)
        if (!next) {
            break
        }
        s = next
    }
    return s
}

export interface PlanWindow { label: string, pct: number, resetAt: number | null, scope: string | null }
export interface Plan { fetchedAt: number | null, windows: PlanWindow[], unavailable: string | null, details: string[] }

/** plan data older than this is "stale": no alerts, badge says stale (review M5) */
export const PLAN_STALE_MS = 15 * 60e3

export function parsePlan (raw: any): Plan | null {
    if (!isObj(raw)) {
        return null
    }
    const windows: PlanWindow[] = []
    for (const w of Array.isArray(raw.windows) ? raw.windows : []) {
        const pct = numOr(w?.used_percent)
        if (!isObj(w) || pct === null) {
            continue
        }
        windows.push({ label: truncate(strOr(w.label) ?? strOr(w.scope) ?? 'window', 32)!, pct, resetAt: toMs(w.reset_at), scope: strOr(w.scope) })
    }
    // Hermes's extra lines, e.g. "Extra usage: 12.40 / 50.00 USD" (QA m4)
    const details = (Array.isArray(raw.details) ? raw.details : []).filter((x: any) => typeof x === 'string' && x.trim()).slice(0, 4).map((x: string) => truncate(x, 80)!)
    return { fetchedAt: toMs(raw.fetched_at), windows: windows.slice(0, 6), unavailable: truncate(strOr(raw.unavailable_reason), 80), details }
}

/** Window length from Hermes's label/scope: 5 h for the "session" window, 7 d for weekly ones. */
export function windowLengthMs (w: PlanWindow): number | null {
    const s = `${w.label} ${w.scope ?? ''}`.toLowerCase()
    if (/five|5.?h|session/.test(s)) {
        return 5 * 3600e3
    }
    if (/week|seven|7.?d/.test(s)) {
        return 7 * 24 * 3600e3
    }
    return null
}

/**
 * Linear pace: at the burn rate measured when the % was fetched, when would the window reach 100 %?
 * The rate and the projection both start at `fetchedAt` (the % is as of then, review M5); null = can't tell.
 */
export function planPace (w: PlanWindow, now: number, fetchedAt: number | null = null): { onPace: boolean, hitAt: number | null } | null {
    const len = windowLengthMs(w)
    if (!len || w.resetAt === null || w.pct <= 0 || w.resetAt <= now) {
        return null
    }
    const ref = fetchedAt !== null && fetchedAt <= now ? fetchedAt : now
    const elapsed = len - (w.resetAt - ref)
    if (elapsed <= 60e3 || elapsed > len) {
        return null
    }
    const rate = w.pct / elapsed
    if (w.pct >= 100) {
        return { onPace: false, hitAt: ref }
    }
    const hitAt = ref + (100 - w.pct) / rate
    return hitAt >= w.resetAt ? { onPace: true, hitAt: null } : { onPace: false, hitAt: Math.round(hitAt) }
}

/** Where an even burn would be now: the elapsed fraction of the window (0..1), drawn as the bar's pace marker. */
export function planMarker (w: PlanWindow, now: number): number | null {
    const len = windowLengthMs(w)
    if (!len || w.resetAt === null || w.resetAt <= now) {
        return null
    }
    const f = (len - (w.resetAt - now)) / len
    return f >= 0 && f <= 1 ? f : null
}

/** Plan data the dock may trust for alerts: windows not yet reset, fetched in the last 15 min. */
export function livePlan (plan: Plan | null, now: number): Plan | null {
    if (!plan || plan.fetchedAt === null || now - plan.fetchedAt > PLAN_STALE_MS) {
        return null
    }
    return { ...plan, windows: plan.windows.filter(w => w.resetAt === null || w.resetAt > now) }
}

// ---------------------------------------------------------------------------------------------
// state.db rows (shape produced by metrics-reader.ts)

export interface DbSession {
    id: string, title: string | null, model: string | null, calls: number, input: number, output: number,
    cacheRead: number, cacheWrite: number, cost: number | null, tools: number, startedAt: number | null,
    endedAt: number | null, lastActivityAt: number | null, lastSeen: number | null,
}
export interface DbData {
    /** the live session; with compaction continuations this is the chain SUM (id = newest segment) */
    live: DbSession | null
    /** compression continuation chain, root first, newest (= live.id) last (review M3) */
    chain?: string[]
    children: DbSession[]
    tasks: { task: string, model: string | null, calls: number, cost: number | null }[]
    delegations: { id: string, state: string | null, dispatchedAt: number | null, completedAt: number | null, goals: string[] }[]
    newestTopLevel: string | null
    /** tool calls by name for the live session (messages role=tool), top 5 */
    toolsByName: { name: string, n: number }[]
    /** user messages in the live session = turns */
    turns: number | null
}

export interface HistoryRaw {
    /** `calls` / `unpriced` / `estOut` (X4, optional): calls, calls without a price, calls with estimated output */
    days: { d: string, sessions: number, tokens: number, cost: number, subCost: number, calls?: number, unpriced?: number, estOut?: number }[]
    models: { d: string, model: string, cost: number, tokens: number }[]
    /** grouped by compression-chain root (subagents and continuations included) */
    top: { id: string, title: string | null, cost: number, segments?: number, src?: string | null, copy?: number | null, calls?: number, unpriced?: number, estOut?: number }[]
    savings: number | null
}

// ---------------------------------------------------------------------------------------------
// view model

export interface ViewOptions {
    now: number
    cacheTtlSec: number
    contextMaxDefault: number
    page: 'live' | 'history'
    pluginStaleSec: number
}

export interface Alert { level: 'warn' | 'crit', text: string }

export function planAlerts (plan: Plan | null): Alert[] {
    const out: Alert[] = []
    for (const w of plan?.windows ?? []) {
        const lvl = w.pct >= 95 ? 95 : w.pct >= 90 ? 90 : w.pct >= 80 ? 80 : 0
        if (lvl) {
            out.push({ level: lvl >= 90 ? 'crit' : 'warn', text: `${w.label}: ${Math.round(w.pct)} % of plan used (\u2265 ${lvl} %)` })
        }
    }
    return out
}

export function buildAlerts (a: { plan: Plan | null, contextPct: number | null, turnCost: number | null, priorTurnCosts: number[], status: LiveStatus | null, lastError: { at: number, what: string } | null, now: number }): Alert[] {
    const out = planAlerts(a.plan)
    if (a.contextPct !== null && a.contextPct >= 85) {
        out.push({ level: a.contextPct >= 95 ? 'crit' : 'warn', text: `Context ${Math.round(a.contextPct)} % full: compaction soon` })
    }
    const med = median(a.priorTurnCosts.filter(x => x > 0))
    if (a.turnCost !== null && med !== null && a.priorTurnCosts.length >= 3 && a.turnCost > 5 * med) {
        out.push({ level: 'warn', text: `Cost spike: this turn \u2248 $${a.turnCost.toFixed(2)} (> 5\u00d7 median $${med.toFixed(2)})` })
    }
    if (a.status === 'error') {
        out.push({ level: 'crit', text: 'Hermes reports an error state' })
    }
    if (a.lastError && a.now - a.lastError.at < 5 * 60e3) {
        const what = a.lastError.what === 'rate-limited' ? 'Rate limited' : a.lastError.what === 'overloaded' ? 'API overloaded' : 'API error'
        out.push({ level: a.lastError.what === 'api-error' ? 'warn' : 'crit', text: `${what} (${Math.round((a.now - a.lastError.at) / 1000)} s ago, retrying)` })
    }
    return out
}

export function modelLabel (model: string | null): string | null {
    if (!model) {
        return null
    }
    // major [-minor] (1-2 digits each; a date stamp is not a minor version): claude-sonnet-5 -> Sonnet 5,
    // claude-haiku-4-5-20251001 -> Haiku 4.5, claude-opus-6 -> Opus 6 (QA m2)
    const m = /claude-(opus|fable|sonnet|haiku)-(\d{1,2})(?:-(\d{1,2}))?(?!\d)/i.exec(model)
    if (m) {
        return `${m[1]![0]!.toUpperCase()}${m[1]!.slice(1).toLowerCase()} ${m[2]}${m[3] !== undefined ? '.' + m[3] : ''}`
    }
    return truncate(model, 24)
}

export function modelFamily (model: string | null): string {
    const m = /(opus|fable|sonnet|haiku)/i.exec(model ?? '')
    return m ? m[1]!.toLowerCase() : 'other'
}

/** "≈ N calls left": average prompt growth per call over the current session's recent calls. */
export function callsLeft (prompts: number[], max: number | null): number | null {
    if (!max || prompts.length < 3) {
        return null
    }
    const recent = prompts.slice(-10)
    const growth = (recent[recent.length - 1]! - recent[0]!) / (recent.length - 1)
    if (growth <= 0) {
        return null
    }
    return Math.max(0, Math.floor((max - recent[recent.length - 1]!) / growth))
}

/** a background process Hermes started (runtime processes.json), owned by a session */
export interface BgProc { sessionId: string, name: string, startedAt: number | null, alive: boolean | null }

export interface ViewInput {
    sessionId: string | null
    matchedBy: MatchedBy
    snapshot: Snapshot | null
    snapshotError: string | null
    /** is the snapshot's writer process alive (Habby's process list)? null = no process list (review M1) */
    snapshotPidAlive?: boolean | null
    /** state.db was busy/locked this tick: `db` is the last good read (review M4) */
    dbBusy?: boolean
    bgProcs?: BgProc[] | null
    plan: Plan | null
    planError: string | null
    db: DbData | null
    dbError: string | null
    log: LogState | null
    logError: string | null
    history: HistoryRaw | null
    blocks: { resetAt: number, peak: number }[]
}

function sumTokens (s: { input: number, output: number, cacheRead: number, cacheWrite: number }): number {
    return s.input + s.output + s.cacheRead + s.cacheWrite
}

/** Plain JSON for the dock (sent over IPC, rendered with textContent only). */
export function buildView (inp: ViewInput, o: ViewOptions): Record<string, any> {
    const now = o.now
    const sid = inp.sessionId
    // M1: a snapshot is fresh while its writer process is alive (the plugin heartbeats every 30 s while
    // busy); the age rule applies only when there is no process list
    const snapFresh = !!inp.snapshot && (inp.snapshotPidAlive === true ? true : inp.snapshotPidAlive === false ? false
        : inp.snapshot.writtenAt !== null && now - inp.snapshot.writtenAt < o.pluginStaleSec * 1000)
    const snap = sid && inp.snapshot ? followContinuation(inp.snapshot, sid) : null
    const ls = sid && inp.log ? inp.log.sessions.get(sid) ?? null : null
    const db = inp.db
    const live = db?.live ?? null
    const lastCall = ls?.calls.length ? ls.calls[ls.calls.length - 1] : null

    // ---- status / header
    let status: LiveStatus | null = null
    let statusProv: Prov = 'exact'
    if (snap?.status && snapFresh) {
        status = snap.status
    } else if (snap?.status && snap.status === 'idle') {
        status = 'idle'
    } else if (ls) {
        statusProv = 'estimate'
        const turnOpen = ls.calls.length > ls.turnStart
        const recent = now - ls.lastAt < 120e3
        status = turnOpen && recent ? 'thinking' : 'idle'
    }
    const model = snap?.model ?? lastCall?.model ?? live?.model ?? null
    const children = db?.children ?? []
    const turnStartedAt = snap?.turn?.startedAt ?? (ls && ls.calls.length > ls.turnStart ? ls.calls[ls.turnStart]!.at - (ls.calls[ls.turnStart]!.latencyS ?? 0) * 1000 : null)
    const header = {
        model: modelLabel(model), family: modelFamily(model),
        team: modelFamily(model) === 'fable' && (children.length > 0 || (snap?.subagents.length ?? 0) > 0),
        title: truncate(live?.title ?? null, 60), status, statusProv,
        turnStartedAt: status && status !== 'idle' ? turnStartedAt : null,
    }

    // ---- activity (#4)
    let activity: Record<string, any>
    if (snap && snapFresh) {
        const runningSubs = snap.subagents.filter(a => a.status === 'running').length
        let text = 'Idle'
        if (snap.waiting) {
            text = snap.waiting.kind === 'approval' ? 'Waiting for your approval' : snap.waiting.kind === 'clarify' ? 'Waiting for your answer' : snap.waiting.kind === 'sudo' ? 'Waiting for a sudo password' : 'Waiting for you'
        } else if (snap.currentTool) {
            text = `Running ${snap.currentTool.name}${snap.currentTool.preview ? ': ' + snap.currentTool.preview : ''}`
        } else if (runningSubs) {
            text = `${runningSubs} subagent${runningSubs > 1 ? 's' : ''} working`
        } else if (snap.status === 'thinking') {
            text = 'Thinking'
        } else if (snap.status === 'compacting') {
            text = 'Compacting context'
        } else if (snap.status === 'error') {
            text = 'Error'
        }
        activity = {
            text, since: snap.waiting?.since ?? snap.currentTool?.startedAt ?? null, prov: 'exact' as Prov,
            steps: snap.steps.map(s => ({ tool: s.tool, status: s.status, ms: s.ms })),
        }
    } else if (ls) {
        activity = {
            text: status === 'idle' ? 'Idle' : 'Working (from agent.log)', since: null, prov: 'estimate' as Prov,
            steps: ls.steps.map(s => ({ tool: s.tool, status: s.status, ms: s.ms })),
        }
    } else {
        activity = { text: null, since: null, prov: 'estimate' as Prov, steps: [] }
    }

    // ---- context
    let ctxUsed: number | null = null
    let ctxMax: number | null = null
    let ctxProv: Prov = 'exact'
    let breakdown: { label: string, tokens: number }[] | null = null
    if (snap?.context && snap.context.used !== null) {
        ctxUsed = snap.context.used
        ctxMax = snap.context.max
        ctxProv = snap.context.estimated ? 'estimate' : 'exact'
        breakdown = snap.context.breakdown
    } else if (lastCall?.prompt != null) {
        ctxUsed = lastCall.prompt
        ctxProv = 'estimate'
    }
    if (ctxUsed !== null && !ctxMax) {
        // same-session compressor value only (another session's 200k aux model must not leak in, minor 13)
        ctxMax = ls?.contextLength ?? o.contextMaxDefault
    }
    if (!breakdown && snap?.lastCall && snap.lastCall.prompt !== null) {
        // tokens by category of the last call's prompt (snapshot): cache read / cache write / uncached
        const cr = snap.lastCall.cacheRead ?? 0
        const cw = snap.lastCall.cacheWrite ?? 0
        const unc = snap.lastCall.input ?? Math.max(0, snap.lastCall.prompt - cr - cw)
        breakdown = [{ label: 'cached (read)', tokens: cr }, { label: 'cache write', tokens: cw }, { label: 'uncached input', tokens: unc }].filter(x => x.tokens > 0)
        if (!breakdown.length) {
            breakdown = null
        }
    }
    const ctxPct = ctxUsed !== null && ctxMax ? Math.min(100, ctxUsed / ctxMax * 100) : null
    // one source for "calls left" (QA m5): the snapshot's prompt history while it is fresh, else agent.log
    const prompts = snap && snapFresh && snap.context
        ? snap.promptHistory
        : (ls?.calls ?? []).filter(c => c.prompt !== null).map(c => c.prompt as number)
    const context = {
        used: ctxUsed, max: ctxMax, pct: ctxPct, prov: ctxProv,
        compressions: snap?.compressions ?? null,
        callsLeft: callsLeft(prompts, ctxMax), breakdown,
    }

    // ---- cache heartbeat
    const lastAt = snap?.lastCall?.at ?? lastCall?.at ?? live?.lastSeen ?? null
    const priceModel = priceFor(model)
    const sessHit = live && (live.input + live.cacheRead + live.cacheWrite) > 0 ? live.cacheRead / (live.input + live.cacheRead + live.cacheWrite) * 100 : null
    const lastHit = lastCall?.hitPct ?? (snap?.lastCall && snap.lastCall.prompt ? (snap.lastCall.cacheRead ?? 0) / snap.lastCall.prompt * 100 : null)
    const cache = {
        lastCallAt: lastAt, ttlSec: o.cacheTtlSec,
        expiresAt: lastAt !== null ? lastAt + o.cacheTtlSec * 1000 : null,
        hitSession: sessHit, hitLast: lastHit,
        recost: priceModel && ctxUsed !== null ? ctxUsed * (priceModel.cacheWrite - priceModel.cacheRead) / 1e6 : null,
    }

    // ---- plan
    const plan = inp.plan
    const planStale = !!plan && (plan.fetchedAt === null || now - plan.fetchedAt > PLAN_STALE_MS)
    const planView = plan ? {
        fetchedAt: plan.fetchedAt, unavailable: plan.unavailable, stale: planStale, details: plan.details ?? [],
        windows: plan.windows.map(w => {
            // M5: a window whose reset time has passed shows greyed "reset — waiting for refresh"
            const expired = w.resetAt !== null && w.resetAt <= now
            return {
                label: w.label, pct: w.pct, resetAt: w.resetAt, expired,
                pace: expired || planStale ? null : planPace(w, now, plan.fetchedAt),
                marker: expired ? null : planMarker(w, now),
            }
        }),
    } : null

    // ---- this turn
    let turn: Record<string, any> | null = null
    const turnCalls = ls ? ls.calls.slice(ls.turnStart) : []
    const recentCalls = ls ? ls.calls.slice(-20) : []
    const latencies = recentCalls.filter(c => c.latencyS !== null).map(c => c.latencyS as number)
    const tpsList = recentCalls.filter(c => c.latencyS && c.output !== null).map(c => (c.output as number) / (c.latencyS as number))
    if (snap?.turn && (snapFresh || snap.status === 'idle')) {
        const t = snap.turn
        // "input" = prompt tokens (uncached + cache read + cache write) on both paths (review minor 3)
        const prompt = t.input !== null ? t.input + (t.cacheRead ?? 0) + (t.cacheWrite ?? 0) : null
        turn = {
            calls: t.calls, input: prompt, output: t.output, cacheRead: t.cacheRead, cacheWrite: t.cacheWrite,
            cost: t.cost ?? (t.input !== null && t.output !== null ? costOf(model, { input: t.input, output: t.output, cacheRead: t.cacheRead ?? 0, cacheWrite: t.cacheWrite ?? 0 }) : null),
            prov: 'exact' as Prov,
        }
    } else if (ls) {
        const tc = turnCalls
        let cost: number | null = 0
        let output = 0
        let prompt = 0
        for (const c of tc) {
            const k = callCost(c)
            cost = k === null || cost === null ? null : cost + k
            output += c.output ?? 0
            prompt += c.prompt ?? 0
        }
        turn = { calls: tc.length, input: prompt, output, cacheRead: null, cacheWrite: null, cost: tc.length ? cost : null, prov: 'estimate' as Prov }
    }
    if (turn) {
        // generation speed: output / (latency - TTFB) when the plugin measured TTFB, else output / latency
        // from agent.log, which includes the time to first byte (review minor 4)
        const lc = snap?.lastCall
        const gen = lc && lc.output !== null && lc.latencyMs !== null && lc.ttfbMs !== null && lc.latencyMs - lc.ttfbMs > 50
            ? lc.output / ((lc.latencyMs - lc.ttfbMs) / 1000) : null
        turn.tps = gen ?? (tpsList.length ? tpsList[tpsList.length - 1] : null)
        turn.tpsGen = gen !== null
        turn.latencies = latencies
        turn.ttfbMs = lc?.ttfbMs ?? null
    }

    // ---- session totals
    let session: Record<string, any> | null = null
    if (live) {
        const subCost = children.reduce((a, c) => a + (c.cost ?? 0), 0)
        const p = priceFor(live.model)
        // background/aux spend grouped by task (one line per task, not per model: QA nit 4)
        const byTask = new Map<string, { task: string, cost: number | null, calls: number }>()
        for (const t of (db?.tasks ?? []).filter(x => x.task)) {
            const g = byTask.get(t.task) ?? { task: t.task, cost: 0, calls: 0 }
            g.cost = g.cost === null || t.cost === null ? (g.cost ?? 0) + (t.cost ?? 0) : g.cost + t.cost
            g.calls += t.calls
            byTask.set(t.task, g)
        }
        const bg = [...byTask.values()]
        session = {
            cost: live.cost, subCost,
            tokens: { uncached: live.input, cacheWrite: live.cacheWrite, cacheRead: live.cacheRead, output: live.output },
            total: sumTokens(live),
            savings: p ? live.cacheRead * (p.input - p.cacheRead) / 1e6 : null,
            tools: live.tools, calls: live.calls,
            background: bg.length ? { cost: bg.reduce((a, t) => a + (t.cost ?? 0), 0), tasks: bg.map(t => ({ task: truncate(t.task, 24), cost: t.cost, calls: t.calls })) } : null,
            continuations: Math.max(0, (db?.chain?.length ?? 1) - 1),
        }
    }

    // ---- R10 agent tree: main agent row + one row per subagent (plugin snapshot first, state.db fallback)
    const sessTok = (x: { input: number | null, output: number | null, cacheRead: number | null, cacheWrite: number | null } | null) =>
        x ? { input: x.input, output: x.output, cacheRead: x.cacheRead, cacheWrite: x.cacheWrite } : null
    const spark = (l: SessionLog | null | undefined) => (l?.calls ?? []).slice(-20).map(c => c.output ?? 0)
    const logStatus = (l: SessionLog | null | undefined): LiveStatus | null => {
        if (!l) {
            return null
        }
        return l.calls.length > l.turnStart && now - l.lastAt < 120e3 ? 'thinking' : 'idle'
    }
    const snapTotals = snap?.totals ?? null
    const mainTokens = live
        ? { input: live.input, output: live.output, cacheRead: live.cacheRead, cacheWrite: live.cacheWrite }
        : sessTok(snapTotals)
    const mainRow = sid ? {
        id: sid, kind: 'main',
        role: snap?.role ?? (header.team ? 'administrator' : 'main agent'),
        goal: truncate(live?.title ?? null, 80),
        model: modelLabel(model), family: modelFamily(model),
        status, prov: statusProv,
        tool: snap && snapFresh && snap.currentTool ? { name: snap.currentTool.name, preview: snap.currentTool.preview, since: snap.currentTool.startedAt } : null,
        waiting: snap && snapFresh && snap.waiting ? snap.waiting : null,
        steps: activity.steps,
        turn: turn ? { input: turn.input, output: turn.output, cacheRead: turn.cacheRead, cacheWrite: turn.cacheWrite, cost: turn.cost, prov: turn.prov } : null,
        tokens: mainTokens,
        cost: live?.cost ?? snapTotals?.cost ?? null,
        calls: live?.calls ?? snapTotals?.calls ?? null,
        tools: live?.tools ?? snapTotals?.tools ?? null,
        ms: live?.startedAt != null ? now - live.startedAt : (snap?.startedAt != null ? now - snap.startedAt : null),
        ctxPct, spark: spark(ls),
        procs: procsFor(sid, db?.chain),
        background: snap?.background && snap.background.calls ? snap.background : null,
    } : null

    function procsFor (id: string, also?: string[]): { name: string, startedAt: number | null, alive: boolean | null }[] {
        const set = new Set([id, ...(also ?? [])])
        return (inp.bgProcs ?? []).filter(p => set.has(p.sessionId) && p.alive !== false).slice(0, 6)
            .map(p => ({ name: p.name, startedAt: p.startedAt, alive: p.alive }))
    }
    const ids: string[] = []
    const addId = (id: string | null | undefined) => {
        if (id && id !== sid && !ids.includes(id)) {
            ids.push(id)
        }
    }
    const parentIds = new Set([sid, ...(db?.chain ?? []), snap?.continuedFrom].filter(Boolean))
    if (inp.snapshot && sid) {
        for (const [id, s] of inp.snapshot.sessions) {
            if (s.parentSessionId && parentIds.has(s.parentSessionId) && !s.continuedTo) {
                addId(id)
            }
        }
    }
    for (const a of snap?.subagents ?? []) {
        addId(a.id)
    }
    for (const c of children) {
        addId(c.id)
    }
    let earlier = 0
    const subRows: Record<string, any>[] = []
    ids.forEach(id => {
        const sc = inp.snapshot?.sessions.get(id) ?? null
        const sa = snap?.subagents.find(x => x.id === id) ?? null
        const dc = children.find(x => x.id === id) ?? null
        const lg = inp.log?.sessions.get(id) ?? null
        let st: string | null = null
        let prov: Prov = 'exact'
        if (sc?.status && snapFresh) {
            st = sc.status
        } else if (sa?.status && snapFresh) {
            st = sa.status
        } else if (dc) {
            prov = 'estimate'
            st = dc.endedAt === null && dc.lastSeen !== null && now - dc.lastSeen < 90e3 ? 'running' : (logStatus(lg) === 'thinking' ? 'running' : 'done')
        } else if (sa?.status) {
            st = sa.status
        } else {
            prov = 'estimate'
            st = logStatus(lg) === 'thinking' ? 'running' : 'done'
        }
        const active = st !== null && !['done', 'idle', 'completed', 'ok', 'error', 'failed', 'interrupted'].includes(st)
        const startedAt = sc?.startedAt ?? sa?.startedAt ?? dc?.startedAt ?? null
        const endAt = active ? null : (sa?.startedAt != null && sa.ms != null ? sa.startedAt + sa.ms : dc?.endedAt ?? dc?.lastSeen ?? lg?.lastAt ?? null)
        if (!active && endAt !== null && now - endAt > 60e3) {
            earlier++
            return
        }
        const tok = sessTok(sc?.totals ?? null) ?? (dc ? { input: dc.input, output: dc.output, cacheRead: dc.cacheRead, cacheWrite: dc.cacheWrite } : null) ??
            (sa ? { input: sa.input, output: sa.output, cacheRead: sa.cacheRead, cacheWrite: sa.cacheWrite } : null)
        const role = sc?.role ?? sa?.role ?? null
        const smodel = sc?.model ?? sa?.model ?? dc?.model ?? (lg?.calls.length ? lg.calls[lg.calls.length - 1]!.model : null)
        subRows.push({
            id, kind: 'sub',
            // Hermes's internal "leaf" role means nothing to the reader (QA nit 6); the goal says what it does.
            // No delegation-goal fallback by index: it attached the wrong goal (review minor 15).
            role: role && role !== 'leaf' ? role : 'subagent',
            goal: sc?.goal || sa?.goal || truncate(dc?.title ?? null, 80),
            model: modelLabel(smodel), family: modelFamily(smodel),
            status: st, prov, active, fading: !active,
            tool: sc && snapFresh && sc.currentTool ? { name: sc.currentTool.name, preview: sc.currentTool.preview, since: sc.currentTool.startedAt } : null,
            waiting: sc && snapFresh && sc.waiting ? sc.waiting : null,
            procs: procsFor(id),
            steps: sc?.steps.length ? sc.steps : (lg?.steps ?? []).map(s => ({ tool: s.tool, status: s.status, ms: s.ms })),
            tokens: tok,
            cost: sc?.totals?.cost ?? dc?.cost ?? sa?.cost ?? null,
            calls: sc?.totals?.calls ?? dc?.calls ?? (lg ? lg.calls.length : null),
            tools: sc?.totals?.tools ?? dc?.tools ?? sa?.tools ?? null,
            toolsByName: (sc?.toolsByName ?? []).slice(0, 5),
            ms: sa?.ms ?? (startedAt !== null ? (endAt ?? now) - startedAt : null),
            ctxPct: sc?.context?.pct ?? (sc?.context?.used != null && sc.context.max ? sc.context.used / sc.context.max * 100 : null),
            spark: spark(lg),
        })
    })
    const agents = { main: mainRow, subs: subRows.slice(0, 16), earlier }

    // ---- totals strip: all agents of this session
    const tokSum = (t: any) => t ? (t.input ?? 0) + (t.output ?? 0) + (t.cacheRead ?? 0) + (t.cacheWrite ?? 0) : 0
    const allChildrenTok = children.reduce((acc, c) => acc + sumTokens(c), 0)
    const totals = {
        tokens: (mainTokens ? tokSum(mainTokens) : 0) + (children.length ? allChildrenTok : subRows.reduce((acc, r) => acc + tokSum(r.tokens), 0)),
        cost: live?.cost != null || children.length
            ? (live?.cost ?? 0) + children.reduce((acc, c) => acc + (c.cost ?? 0), 0)
            : (mainRow?.cost != null ? mainRow.cost + subRows.reduce((acc, r) => acc + (r.cost ?? 0), 0) : null),
        calls: (mainRow?.calls ?? 0) + (children.length ? children.reduce((acc, c) => acc + c.calls, 0) : subRows.reduce((acc, r) => acc + (r.calls ?? 0), 0)),
        active: (status && status !== 'idle' ? 1 : 0) + subRows.filter(r => r.active).length,
        agents: 1 + Math.max(ids.length, children.length),
    }

    // ---- comprehensive datapoints (main session)
    const sessCalls = ls?.calls ?? []
    const lat = sessCalls.filter(c => c.latencyS !== null).map(c => c.latencyS as number)
    const stats = {
        calls: live?.calls ?? snapTotals?.calls ?? (sessCalls.length || null),
        avgLatencyS: lat.length ? lat.reduce((x, y) => x + y, 0) / lat.length : null,
        ttfbMs: snap?.lastCall?.ttfbMs ?? null,
        tps: turn?.tps ?? null,
        hitPct: sessHit,
        compressions: snap?.compressions ?? null,
        toolsByName: (snap?.toolsByName.length ? snap.toolsByName : db?.toolsByName ?? []).slice(0, 5),
        toolsProv: 'exact',
        background: session?.background ?? null,
        turns: snap?.turns ?? db?.turns ?? (ls ? ls.turns.length : null),
        elapsedMs: mainRow?.ms ?? null,
    }

    // ---- alerts (X1)
    const priorTurnCosts = (ls?.turns ?? []).map(t => t.cost).filter((x): x is number => x !== null)
    const alerts = buildAlerts({
        // M5: no plan alerts from data older than 15 min or from windows that already reset
        plan: livePlan(plan, now), contextPct: ctxPct, turnCost: turn?.cost ?? null, priorTurnCosts,
        status, lastError: ls?.lastError ?? null, now,
    })

    // ---- history
    let history: Record<string, any> | null = null
    if (o.page === 'history' && inp.history) {
        history = buildHistory(inp.history, inp.blocks, now)
    }

    return {
        v: 1, at: now, page: o.page,
        who: { id: sid, short: sid ? sid.slice(-6) : null, matchedBy: inp.matchedBy },
        sources: {
            plugin: inp.snapshot ? (snapFresh ? 'ok' : 'stale') : (inp.snapshotError ?? 'n/a'),
            plan: inp.plan ? 'ok' : (inp.planError ?? 'n/a'),
            db: inp.dbBusy ? 'db busy' : inp.db ? 'ok' : (inp.dbError ?? 'n/a'),
            log: inp.log && !inp.logError ? 'ok' : (inp.logError ?? 'n/a'),
        },
        header, activity, context, cache, plan: planView, turn, session,
        agents, totals, stats,
        alerts, history,
    }
}

function localDay (t: number): string {
    const d = new Date(t)
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
}

/** The local calendar date `daysAgo` days before `now` (calendar stepping: no DST skips/duplicates, QA nit 7). */
export function dayBack (now: number, daysAgo: number): string {
    const d = new Date(now)
    return localDay(new Date(d.getFullYear(), d.getMonth(), d.getDate() - daysAgo, 12).getTime())
}

export function buildHistory (h: HistoryRaw, blocks: { resetAt: number, peak: number }[], now: number): Record<string, any> {
    const byDay = new Map(h.days.map(d => [d.d, d]))
    const today = dayBack(now, 0)
    const yesterday = dayBack(now, 1)
    const sumRange = (fromDaysAgo: number, toDaysAgo: number) => {
        let cost = 0, tokens = 0, sessions = 0, calls = 0, unpriced = 0, estOut = 0
        for (let i = fromDaysAgo; i < toDaysAgo; i++) {
            const d = byDay.get(dayBack(now, i))
            if (d) {
                cost += d.cost
                tokens += d.tokens
                sessions += d.sessions
                calls += d.calls ?? 0
                unpriced += d.unpriced ?? 0
                estOut += d.estOut ?? 0
            }
        }
        return { cost, tokens, sessions, calls, unpriced, estOut }
    }
    // 12-week heatmap: one column per calendar week (Monday first), one row per weekday, ending today
    const weekday = (new Date(now).getDay() + 6) % 7 // Mon = 0
    const heat: { d: string, cost: number, col: number, row: number }[] = []
    const cells = 11 * 7 + weekday + 1
    for (let k = 0; k < cells; k++) {
        const d = dayBack(now, cells - 1 - k)
        heat.push({ d, cost: byDay.get(d)?.cost ?? 0, col: Math.floor(k / 7), row: k % 7 })
    }
    // per-model stacked bars, last 14 days (Sonnet / Haiku: Claude Code sessions, QA n7)
    const families = ['opus', 'fable', 'sonnet', 'haiku', 'other']
    const models: { d: string, by: Record<string, number> }[] = []
    for (let i = 13; i >= 0; i--) {
        const d = dayBack(now, i)
        const by: Record<string, number> = { opus: 0, fable: 0, sonnet: 0, haiku: 0, other: 0 }
        for (const m of h.models) {
            if (m.d === d) {
                const f = modelFamily(m.model)
                by[families.includes(f) ? f : 'other']! += m.cost
            }
        }
        models.push({ d, by })
    }
    const week = sumRange(0, 7)
    const weekSub = [...Array(7).keys()].reduce((a, i) => a + (byDay.get(dayBack(now, i))?.subCost ?? 0), 0)
    return {
        today: { ...sumRange(0, 1), d: today },
        yesterday: { ...sumRange(1, 2), d: yesterday },
        week, lastWeek: sumRange(7, 14),
        heat, models,
        blocks: blocks.slice(-12),
        top: h.top.slice(0, 5).map((t, i) => ({
            title: truncate(t.title ?? t.id, 48), cost: t.cost, segments: t.segments ?? 1,
            // X4: source label (Hermes / VS Code / CLI / Desktop) and the index into the Hermes top list for
            // click-to-copy (null = not copyable); a plain Hermes list keeps its own index
            src: t.src ?? null, copy: t.copy === undefined ? i : t.copy,
            // X4: calls / unpriced calls / calls with estimated output (Claude Code rows; Hermes rows: 0)
            calls: t.calls ?? 0, unpriced: t.unpriced ?? 0, estOut: t.estOut ?? 0,
        })),
        savings: h.savings,
        subShare: week.cost > 0 ? weekSub / week.cost * 100 : null,
    }
}

/** Remember the peak five-hour % per window (keyed by its reset time) for the History page. */
export function updateBlocks (blocks: { resetAt: number, peak: number }[], plan: Plan | null): { blocks: { resetAt: number, peak: number }[], changed: boolean } {
    let changed = false
    const out = [...blocks]
    for (const w of plan?.windows ?? []) {
        if (windowLengthMs(w) !== 5 * 3600e3 || w.resetAt === null) {
            continue
        }
        // reset times jitter by a few seconds between fetches: same window if within 10 min
        const b = out.find(x => Math.abs(x.resetAt - w.resetAt!) < 10 * 60e3)
        if (b) {
            if (w.pct > b.peak) {
                b.peak = w.pct
                changed = true
            }
        } else {
            out.push({ resetAt: w.resetAt, peak: w.pct })
            changed = true
        }
    }
    out.sort((a, b) => a.resetAt - b.resetAt)
    return { blocks: out.slice(-48), changed }
}

// ---------------------------------------------------------------------------------------------
// Centenaria: AgentLiveRaw (docs/agent-live.md) -> ViewInput. Ports metrics-reader.ts readSessionAndPlugin /
// readBgProcs over the raw data Rust returns, and metrics.ts collect() + the tick cadence as AgentLiveFeed.

export type LivePage = 'live' | 'history'
export interface LiveBlock { resetAt: number, peak: number }

/**
 * Which Hermes session belongs to this island. Habby's "Habby pid" is the island's own Hermes child here
 * (AgentLiveRaw.terminalPid): that pid itself or any descendant (the TUI gateway) is "this island's" session
 * (matchedBy 'habby-child', kept for the ported tests). Otherwise Habby's resolveSession with the island pid
 * (children of the island = other terminals it spawned), then newest live / unverified.
 */
export function resolveIslandSession (entries: ActiveSession[], procs: ProcLite[] | null, terminalPid: number | null, islandPid: number): { entry: ActiveSession | null, matchedBy: MatchedBy } {
    if (terminalPid !== null) {
        const mine = procs ? descendants(procs, terminalPid) : new Set<number>()
        mine.add(terminalPid)
        const own = entries.filter(e => mine.has(e.pid))
            .sort((a, b) => (a.surface === 'tui' ? 0 : 1) - (b.surface === 'tui' ? 0 : 1) || (b.startedAt ?? 0) - (a.startedAt ?? 0))
        if (own[0]) {
            return { entry: own[0], matchedBy: 'habby-child' }
        }
        if (procs) {
            // The island's Hermes TUI usually runs its agent inside the Hermes gateway (no pid link to the
            // terminal): the newest live 'gateway:local' session is the best guess for "this island's".
            const alive = new Set(procs.map(p => p.pid))
            const local = entries.filter(e => e.surface === 'gateway:local' && alive.has(e.pid))
                .sort((a, b) => (b.startedAt ?? 0) - (a.startedAt ?? 0))
            if (local[0]) {
                return { entry: local[0], matchedBy: 'gateway-local' }
            }
        }
    }
    return resolveSession(entries, procs, islandPid)
}

/**
 * The id state.db and the plugin snapshot use for an active_sessions entry: session_id, unless only its
 * metadata.chat_id is known to a snapshot, or it is a local gateway session (routing key ends in chat_id).
 */
export function canonicalSessionId (entry: ActiveSession, snaps: (Snapshot | null)[]): string {
    const chat = entry.chatId
    if (!chat || chat === entry.sessionId) {
        return entry.sessionId
    }
    if (snaps.some(s => s?.sessions.has(entry.sessionId))) {
        return entry.sessionId
    }
    if (snaps.some(s => s?.sessions.has(chat))) {
        return chat
    }
    return entry.platform === 'local' && entry.sessionId.endsWith(':' + chat) ? chat : entry.sessionId
}

/**
 * No active_sessions match: the most recently updated top-level session (not a subagent) in the newest
 * snapshot files of live Hermes processes. Habby's db-newest, from plugin data instead of state.db.
 */
export function newestSnapshotSession (files: RawSnapshotFile[], alive: Set<number> | null): { sessionId: string, pid: number } | null {
    let best: { sessionId: string, pid: number, at: number } | null = null
    const live = files.filter(f => alive ? alive.has(f.pid) : f.alive).sort((a, b) => (b.mtimeMs ?? 0) - (a.mtimeMs ?? 0)).slice(0, 32)
    for (const f of live) {
        const sessions = isObj(f.data) && isObj((f.data as any).sessions) ? (f.data as any).sessions as Record<string, any> : null
        for (const [id, s] of Object.entries(sessions ?? {})) {
            if (!isObj(s) || s.platform === 'subagent' || strOr(s.parent_session_id) || strOr(s.continued_to)) {
                continue
            }
            const at = toMs(s.updated_at) ?? toMs(s.first_seen) ?? 0
            if (!best || at > best.at) {
                best = { sessionId: id, pid: f.pid, at }
            }
        }
    }
    return best && { sessionId: best.sessionId, pid: best.pid }
}

export interface RawSnapshotFile { pid: number, alive: boolean, mtimeMs: number, data: unknown }
/** the parts of AgentLiveRaw that readSessionAndPlugin reads (structural, so tests pass plain objects) */
export interface RawSessionInput {
    islandPid: number
    terminalPid: number | null
    procs: ProcLite[] | null
    activeSessions: unknown
    activeSessionsError?: string | null
    snapshots: RawSnapshotFile[]
    snapshotError: string | null
    plan: unknown
    planError: string | null
}

export interface ReadResult {
    sessionId: string | null
    matchedBy: MatchedBy
    entry: ActiveSession | null
    snapshot: Snapshot | null
    /** pid of the snapshot-<pid>.json used (Habby: snapshotFile) */
    snapshotPid: number | null
    snapshotError: string | null
    /** the snapshot writer's pid is alive (null: no process list) */
    snapshotPidAlive: boolean | null
    plan: Plan | null
    planError: string | null
}

/**
 * Session resolution + plugin files (Habby metrics-reader.ts readSessionAndPlugin) over AgentLiveRaw. Rust
 * already listed and parsed every snapshot-<pid>.json (newest mtime first); the choice is made here.
 * Habby's db-newest fallback is not available (agent_live_raw returns state.db rows only for a requested
 * session id); instead, no active_sessions entry = the newest top-level session in a live snapshot
 * ('snapshot-newest'), and the feed re-requests state.db for it.
 */
export function readSessionAndPlugin (raw: RawSessionInput): ReadResult {
    const entries = parseActiveSessions(raw.activeSessions)
    const procs = Array.isArray(raw.procs) ? raw.procs : null
    const resolved = resolveIslandSession(entries, procs, typeof raw.terminalPid === 'number' ? raw.terminalPid : null, raw.islandPid)
    const entry = resolved.entry
    let matchedBy = resolved.matchedBy
    const alive = procs ? new Set(procs.map(p => p.pid)) : null
    const files = (Array.isArray(raw.snapshots) ? raw.snapshots : []).filter(f => isObj(f) && typeof f.pid === 'number')
    let sessionId: string | null = null
    if (entry) {
        const known = files.filter(f => !alive || alive.has(f.pid)).slice(0, 32).map(f => parseSnapshot(f.data))
        sessionId = canonicalSessionId(entry, known)
    } else {
        const newest = newestSnapshotSession(files, alive)
        if (newest) {
            sessionId = newest.sessionId
            matchedBy = 'snapshot-newest'
        }
    }
    let snapshot: Snapshot | null = null
    let snapshotPid: number | null = null
    if (entry) {
        const f = files.find(x => x.pid === entry.pid)
        snapshot = f ? parseSnapshot(f.data) : null
        snapshotPid = snapshot && f ? f.pid : null
    }
    if (!snapshot && sessionId) {
        // fallback: the newest snapshot files (by mtime) of LIVE processes that know this session id (minor 9)
        let best: Snapshot | null = null
        const live = files.filter(f => !alive || alive.has(f.pid)).sort((a, b) => (b.mtimeMs ?? 0) - (a.mtimeMs ?? 0)).slice(0, 32)
        for (const f of live) {
            const s = parseSnapshot(f.data)
            if (s && s.sessions.has(sessionId) && (!best || (s.writtenAt ?? 0) > (best.writtenAt ?? 0))) {
                best = s
                snapshotPid = f.pid
            }
        }
        snapshot = best
    }
    let snapshotError = snapshot ? null : strOr(raw.snapshotError) ?? (files.length ? 'n/a' : 'not installed')
    if (snapshotError === 'not found' || snapshotError === 'plugin not installed') {
        snapshotError = 'not installed'
    }
    const pid = snapshot?.pid ?? snapshotPid
    const snapshotPidAlive = snapshot && alive && pid ? alive.has(pid) : null
    const plan = parsePlan(raw.plan)
    const pe = strOr(raw.planError)
    return {
        sessionId, matchedBy, entry, snapshot, snapshotPid, snapshotError, snapshotPidAlive,
        plan, planError: plan ? null : (pe === null || pe === 'not found' ? 'not installed' : pe),
    }
}

/** Hermes's background-process checkpoint (processes.json): who started what, still running (QA m4). */
export function parseBgProcs (data: unknown, procs: ProcLite[] | null): BgProc[] {
    if (!Array.isArray(data)) {
        return []
    }
    const alive = procs ? new Set(procs.map(p => p.pid)) : null
    const out: BgProc[] = []
    for (const e of data.slice(0, 200)) {
        if (!e || typeof e !== 'object' || typeof e.parent_session_id !== 'string' || !e.parent_session_id) {
            continue
        }
        // only the program name (first token, basename, no extension): never arguments or paths
        const m = /^\s*(?:"([^"]*)"|'([^']*)'|(\S+))/.exec(String(e.command ?? ''))
        const first = (m && (m[1] ?? m[2] ?? m[3])) || ''
        const name = truncate((first.split(/[\\/]/).pop() ?? '').replace(/\.(exe|cmd|bat|ps1)$/i, '') || 'process', 24)!
        const host = e.pid_scope === undefined || e.pid_scope === null || e.pid_scope === 'host'
        out.push({
            sessionId: e.parent_session_id, name, startedAt: toMs(e.started_at),
            alive: alive && host && typeof e.pid === 'number' ? alive.has(e.pid) : null,
        })
    }
    return out
}

/** Habby config defaults (config.ts): refreshMs 1000, idleRefreshMs 5000, cacheTtlSec 300, contextMax 1M, pluginStaleSec 120 */
export const LIVE_REFRESH_MS = 1000
export const IDLE_REFRESH_MS = 5000
export const USAGE_REFRESH_MS = 60e3
export const VIEW_DEFAULTS = { cacheTtlSec: 300, contextMaxDefault: 1000000, pluginStaleSec: 120 }

/** Habby metrics.ts tick(): fast cadence while a turn runs, a call landed < 10 s ago or a subagent is active */
export function isActiveView (view: Record<string, any> | null | undefined, now: number): boolean {
    const st = view?.header?.status
    const lastCallAt = view?.cache?.lastCallAt
    return (!!st && st !== 'idle') || (typeof lastCallAt === 'number' && now - lastCallAt < 10e3) || (view?.agents?.subs ?? []).some((s: any) => s.active)
}

export function nextDelay (page: LivePage, active: boolean): number {
    return page === 'history' ? USAGE_REFRESH_MS : active ? LIVE_REFRESH_MS : IDLE_REFRESH_MS
}

export interface FeedRequest { sessionId: string | null, logCursor: string | null, history: boolean }
/** structural AgentLiveRaw (agent-live-ipc.ts has the full contract type) */
export interface FeedRaw extends RawSessionInput {
    now: number
    processes: unknown
    log: LogChunk
    db: DbData | null
    dbError: string | null
    dbBusy: boolean
    history: HistoryRaw | null
    historyError: string | null
}
export interface FeedDeps {
    raw: (req: FeedRequest) => Promise<FeedRaw>
    blocks: (op: { type: 'get' } | { type: 'set', blocks: LiveBlock[] }) => Promise<{ blocks: LiveBlock[] }>
}
export interface FeedResult { view: Record<string, any>, active: boolean, history: HistoryRaw | null, requests: number }

/**
 * Habby metrics.ts collect(), one call per poll tick: log accumulation, session resolution, M4 last-good
 * state.db rows while the db is busy, 5-hour block bookkeeping (agent_live_blocks instead of
 * habby-metrics-blocks.json), background processes, history kept between Usage polls, buildView.
 * The caller owns the timer (only while the tab is shown) and never runs two ticks at once.
 */
export class AgentLiveFeed {
    log = new LogAccumulator()
    sessionId: string | null = null
    history: HistoryRaw | null = null
    historyError: string | null = null
    blocks: LiveBlock[] | null = null
    blocksError: string | null = null
    private lastDb: DbData | null = null
    private bgProcs: BgProc[] | null = null

    constructor (private deps: FeedDeps, private opts = VIEW_DEFAULTS) { }

    private async request (sessionId: string | null, page: LivePage): Promise<FeedRaw> {
        const raw = await this.deps.raw({ sessionId, logCursor: this.log.cursor, history: page === 'history' })
        this.log.feed(raw.log)
        return raw
    }

    private async loadBlocks (): Promise<void> {
        try {
            const r = await this.deps.blocks({ type: 'get' })
            this.blocks = (Array.isArray(r?.blocks) ? r.blocks : []).filter(b => typeof b?.resetAt === 'number' && typeof b?.peak === 'number')
            this.blocksError = null
        } catch (err) {
            this.blocks = []
            this.blocksError = String(err)
        }
    }

    async tick (page: LivePage, current: () => boolean = () => true): Promise<FeedResult> {
        const check = () => { if (!current()) throw new Error('Agent Live read cancelled') }
        check()
        if (this.blocks === null) {
            await this.loadBlocks()
            check()
        }
        let asked = this.sessionId
        let raw = await this.request(asked, page)
        check()
        let rr = readSessionAndPlugin(raw)
        let requests = 1
        if (rr.sessionId !== asked && rr.sessionId !== null) {
            // the state.db part belongs to the session we asked for: re-request once (docs/agent-live.md)
            asked = rr.sessionId
            raw = await this.request(asked, page)
            check()
            rr = readSessionAndPlugin(raw)
            requests++
        }
        if (rr.sessionId !== this.sessionId) {
            this.lastDb = null
        }
        this.sessionId = rr.sessionId
        const same = rr.sessionId !== null && rr.sessionId === asked
        let db: DbData | null = same && isObj(raw.db) ? raw.db : null
        let dbError: string | null = same ? (db ? null : strOr(raw.dbError) ?? 'n/a') : (rr.sessionId ? 'session changed' : 'no live session')
        let dbBusy = false
        if (same && !db && raw.dbBusy === true && this.lastDb) {
            // M4: keep the last good view, say so in the footer
            dbBusy = true
            db = this.lastDb
            dbError = null
        } else if (db) {
            this.lastDb = db
        }
        if (page === 'history') {
            if (isObj(raw.history)) {
                this.history = raw.history
            }
            this.historyError = raw.history ? null : strOr(raw.historyError) ?? (raw.dbBusy ? 'db busy' : null)
        }
        if (page === 'live') {
            this.bgProcs = parseBgProcs(raw.processes, Array.isArray(raw.procs) ? raw.procs : null)
        }
        const ub = updateBlocks(this.blocks ?? [], rr.plan)
        if (ub.changed) {
            // Await persistence: no detached write may race a later poll or hide.
            check()
            try {
                const saved = await this.deps.blocks({ type: 'set', blocks: ub.blocks })
                this.blocks = saved.blocks
                this.blocksError = null
            } catch (err) {
                this.blocksError = String(err)
            }
            check()
        }
        const now = numOr(raw.now) ?? Date.now()
        const view = buildView({
            sessionId: rr.sessionId, matchedBy: rr.matchedBy,
            snapshot: rr.snapshot, snapshotError: rr.snapshotError, snapshotPidAlive: rr.snapshotPidAlive,
            plan: rr.plan, planError: rr.planError,
            db, dbError, dbBusy,
            log: this.log.state, logError: this.log.stats.error,
            history: this.history, blocks: this.blocks ?? [],
            bgProcs: this.bgProcs,
        }, { now, ...this.opts, page })
        if (view.history === null && page === 'history') {
            view.historyError = this.historyError ?? 'loading'
        }
        view.sources.sessions = strOr(raw.activeSessionsError) ?? 'ok'
        view.hermesPid = rr.entry?.pid ?? null
        return { view, active: isActiveView(view, now), history: this.history, requests }
    }
}
