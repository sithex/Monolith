"""habby-metrics: Hermes observer hooks -> files for Centenaria's AI Agents tab (Live page).

Linux port of the Habby (Windows) plugin; the name and the ``cache/habby-metrics`` file contract are
kept as-is (Centenaria docs/agent-live.md). Design (habby-SPIKE-metrics.md C1-C5,
DESIGN-metrics-panel.md F3/§8, habby-FIXES-metrics.md):

* Every hook body is O(1): it copies a few scalar fields out of the payload and does one
  ``SimpleQueue.put``. ``pre_tool_call`` is fail-closed in Hermes (a slow or raising callback
  blocks the tool), so every hook is wrapped in ``try/except`` and returns ``None``.
* One daemon writer thread (started lazily on the first hook, so short ``hermes ...`` CLI
  commands stay idle) applies the events to in-memory state, appends ``events-<pid>.jsonl`` and
  writes ``snapshot-<pid>.json`` atomically (``os.replace``) at most 4 times a second. While any
  session is busy (not idle/error) it re-stamps the snapshot every 30 s (heartbeat), so a long
  approval wait or a long tool call never looks stale.
* Background-review / ``/btw`` forks share the parent's session_id but never fire ``pre_llm_call``
  (Hermes skips it for ``_persist_disabled`` agents): their calls go to a separate ``background``
  bucket and never touch the session's status, turn or totals.
* Context-compression rotation (new session id, same turn_id) migrates the session's state to the
  new id (``continued_from``); the old id keeps a stub with ``continued_to``.
* One daemon plan thread mirrors official plan windows to ``plan.json`` from Hermes's in-memory
  ``account_usage_cache`` only. Monolith disables network fetching, including when an inherited
  ``HABBY_METRICS_PLAN=fetch`` is present. ``<dir>/plan.off`` stops mirroring on the next poll.
* Nothing that could carry message text ever reaches disk: assistant messages, responses, tool
  args/results, prompts, user messages and approval commands are never copied out of the hook
  payload. The only free text kept is a <= 60-char tool preview and a <= 80-char subagent goal
  (snapshot only, never in the events file), pattern-redacted; ``HABBY_METRICS_PREVIEW=0`` drops
  both.

The file contract is documented in README.md.
"""
from __future__ import annotations

import atexit
import json
import os
import queue
import re
import threading
import time
from collections import deque
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Callable, Optional

__version__ = "1.1.0"

# ---- tunables (env overrides are for tests/harnesses only) -----------------------------------
SNAPSHOT_MIN_INTERVAL_S = 0.25            # <= 4 Hz
HEARTBEAT_S = float(os.environ.get("HABBY_METRICS_HEARTBEAT_S", "30"))
EVENTS_MAX_BYTES = int(os.environ.get("HABBY_METRICS_EVENTS_MAX_BYTES", str(5 * 1024 * 1024)))
PLAN_INTERVAL_S = float(os.environ.get("HABBY_METRICS_PLAN_INTERVAL_S", "300"))
PLAN_CACHE_POLL_S = 60.0
PLAN_MAX_BACKOFF_S = 3600.0
PLAN_LOCK_STALE_S = 120.0
PRUNE_INTERVAL_S = 600.0
PRUNE_UNKNOWN_PID_AGE_S = 3600.0
STALE_SNAPSHOT_DAYS = 7
WRITE_FAIL_BACKOFF_AFTER = 5
WRITE_FAIL_BACKOFF_S = 5.0
PENDING_CHILD_TTL_S = 30.0
MAX_STEPS = 5
MAX_SESSIONS = 64
MAX_SUBAGENT_ROWS = 32
MAX_CHAIN_HOPS = 20
PREVIEW_CHARS = 60
GOAL_CHARS = 80
RAW_TEXT_CAP = 2000                        # raw text cut in the hook before redaction in the writer
PREVIEW_ON = os.environ.get("HABBY_METRICS_PREVIEW", "1") != "0"
# first present key wins. query/url are never previewed; path keys keep the basename only.
_PREVIEW_KEYS = ("command", "path", "file_path", "pattern", "goal", "name", "action")
_PATH_KEYS = ("path", "file_path")
_WAIT_KINDS = ("approval", "clarify", "sudo")
_BUSY = ("thinking", "tool", "waiting", "compacting")

# extra redaction around Hermes's agent.redact (QA m3): mysql -p<pw>, "password X", net user <u> <pw>
_EXTRA_REDACT = (
    (re.compile(r"(?i)\bnet\s+user\s+(\S+)\s+(?!/)(?!\*\*\*)\S+"), r"net user \1 ***"),
    (re.compile(r"(?i)(--?(?:password|passwd|pass|pwd|token|secret|api-?key)[= ])(?!\*\*\*)\S+"), r"\1***"),
    (re.compile(r"(?i)\b(password|passwd|pwd|secret)(\s*[=:]\s*|\s+)(?!\*\*\*)\S+"), r"\1\2***"),
    (re.compile(r"(?<![\w-])-p(?=\S)(?!\*\*\*)\S+"), "-p***"),
)


def _iso(ts: Optional[float]) -> Optional[str]:
    if ts is None:
        return None
    try:
        return datetime.fromtimestamp(float(ts), timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z")
    except (TypeError, ValueError, OverflowError, OSError):
        return None


def _parse_iso(s: Any) -> Optional[float]:
    if not isinstance(s, str) or not s:
        return None
    try:
        return datetime.fromisoformat(s.replace("Z", "+00:00")).timestamp()
    except ValueError:
        return None


def _int(v: Any) -> int:
    try:
        return int(v or 0)
    except (TypeError, ValueError):
        return 0


def _extra_redact(s: str) -> str:
    for rx, rep in _EXTRA_REDACT:
        s = rx.sub(rep, s)
    return s


def _redact(text: str, limit: int) -> str:
    """Redact, flatten to one line and cut to ``limit`` chars. Never returns raw text on failure."""
    try:
        from agent.redact import redact_sensitive_text
        s = _extra_redact(redact_sensitive_text(_extra_redact(str(text or "")), force=True))
    except Exception:
        return ""
    s = " ".join(s.split())
    return (s[: limit - 1] + "\u2026") if len(s) > limit else s


def _default_cost(model: Any, provider: Any, base_url: Any, inp: int, out: int, cr: int, cw: int) -> Optional[float]:
    """API-list-price estimate via Hermes's own pricing (``agent.usage_pricing``); None if unknown."""
    try:
        from agent.usage_pricing import CanonicalUsage, estimate_usage_cost
        r = estimate_usage_cost(str(model or ""), CanonicalUsage(input_tokens=inp, output_tokens=out,
                                                               cache_read_tokens=cr, cache_write_tokens=cw),
                                provider=provider or None, base_url=base_url or None)
        amt = getattr(r, "amount_usd", None)
        return None if amt is None else float(amt)
    except Exception:
        return None


def _pid_alive(pid: int) -> Optional[bool]:
    """True/False when it can be told, None when unknown. Linux: ``/proc/<pid>``, else signal 0
    (existence check only; never delivers a signal)."""
    if not isinstance(pid, int) or pid <= 0:
        return None
    try:
        if os.path.isdir("/proc/self"):
            return os.path.exists(f"/proc/{pid}")
    except OSError:
        pass
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        return True  # exists, owned by someone else
    except Exception:
        return None


def _atomic_write_json(path: Path, data: Any) -> None:
    """Write a sibling temp file, then ``os.replace`` (atomic on POSIX: readers see old or new, never half)."""
    tmp = path.with_name(f"{path.name}.{os.getpid()}.{threading.get_ident()}.tmp")
    try:
        with open(tmp, "w", encoding="utf-8") as f:
            json.dump(data, f, separators=(",", ":"), default=str)
        os.replace(tmp, path)
    except BaseException:
        try:
            os.unlink(tmp)
        except OSError:
            pass
        raise


def _try_lock(path: Path, stale_s: float, now: float) -> Optional[int]:
    try:
        if path.exists() and now - path.stat().st_mtime > stale_s:
            path.unlink()
    except OSError:
        pass
    try:
        fd = os.open(str(path), os.O_CREAT | os.O_EXCL | os.O_WRONLY)
    except OSError:
        return None
    try:
        os.write(fd, str(os.getpid()).encode())
    except OSError:
        pass
    return fd


def _unlock(path: Path, fd: Optional[int]) -> None:
    if fd is None:
        return
    try:
        os.close(fd)
    except OSError:
        pass
    try:
        path.unlink()
    except OSError:
        pass


_FILE_PID_RX = re.compile(r"^(?:snapshot|events)-(\d+)(?:\.\d+)?\.jsonl?$")


def prune_files(out: Path, own_pid: int, now: Optional[float] = None,
                alive_fn: Callable[[int], Optional[bool]] = _pid_alive) -> list:
    """Delete snapshot-<pid>.json / events-<pid>[.1].jsonl of processes that are gone (pid dead; liveness
    unknown: mtime older than 1 h; anything older than 7 days). Returns the removed names."""
    now = time.time() if now is None else now
    removed = []
    try:
        entries = list(Path(out).iterdir())
    except OSError:
        return removed
    for p in entries:
        m = _FILE_PID_RX.match(p.name)
        if not m:
            continue
        pid = int(m.group(1))
        if pid == own_pid:
            continue
        try:
            age = now - p.stat().st_mtime
        except OSError:
            continue
        alive = alive_fn(pid)
        if age > STALE_SNAPSHOT_DAYS * 86400 or alive is False or (alive is None and age > PRUNE_UNKNOWN_PID_AGE_S):
            try:
                p.unlink()
                removed.append(p.name)
            except OSError:
                pass
    return removed


# =============================================================================================
# State (writer thread only)
# =============================================================================================
def _new_bucket() -> dict:
    return {"calls": 0, "input": 0, "output": 0, "cache_read": 0, "cache_write": 0, "cost_usd": 0.0, "_unpriced": 0}


def _new_totals() -> dict:
    b = _new_bucket()
    b.update({"tool_calls": 0, "tools_by_name": {}})
    return b


def _new_session(sid: str, ts: float) -> dict:
    return {
        "status": "idle", "model": None, "platform": None,
        "parent_session_id": None, "role": None, "goal": None, "subagent_id": None,
        "turn": None, "current_tool": None, "steps": [], "last_call": None, "context": None,
        "waiting": None, "subagents": [], "compressions": 0, "totals": _new_totals(),
        "background": None, "prompt_history": [], "continued_from": None, "continued_to": None,
        "first_seen": _iso(ts), "updated_at": _iso(ts),
        # private (stripped from the snapshot)
        "_ts": ts, "_turn_id": None, "_running": {}, "_fg_turns": deque(maxlen=16), "_llm_seen": False,
        "_bg_calls": set(),
    }


def _add_cost(bucket: dict, cost: Optional[float]) -> None:
    if cost is None:
        bucket["_unpriced"] = bucket.get("_unpriced", 0) + 1
    else:
        bucket["cost_usd"] = round((bucket.get("cost_usd") or 0.0) + cost, 6)


def _pub_bucket(b: Optional[dict]) -> Optional[dict]:
    """Public copy: private keys stripped; cost_usd = null when any call in the bucket was unpriced."""
    if b is None:
        return None
    out = {k: v for k, v in b.items() if not k.startswith("_")}
    if b.get("_unpriced"):
        out["cost_usd"] = None
    if "tools_by_name" in out:
        out["tools_by_name"] = dict(out["tools_by_name"])
    return out


class Metrics:
    """All bookkeeping. Not thread-safe by itself: only the writer thread (or a test) calls it."""

    def __init__(self, out_dir: Path, *, pid: Optional[int] = None,
                 cost_fn: Callable[..., Optional[float]] = _default_cost,
                 redact_fn: Callable[[str, int], str] = _redact,
                 clock: Callable[[], float] = time.time) -> None:
        self.out = Path(out_dir)
        self.pid = os.getpid() if pid is None else pid
        self.cost_fn, self.redact_fn, self.clock = cost_fn, redact_fn, clock
        self.sessions: dict[str, dict] = {}
        self.child_parent: dict[str, str] = {}           # child session id -> parent session id
        self.pending_children: deque = deque()            # (parent_sid, row, ts) whose child sid was unknown
        self.anthropic_base_urls: dict[str, float] = {}   # base_url -> last seen (writer thread only)
        self.base_urls_snapshot: tuple = ()               # immutable copy read by the plan thread
        self.events: list[dict] = []
        self.dirty = False
        self.last_snapshot = 0.0
        self.write_failures = 0
        self.snapshot_path = self.out / f"snapshot-{self.pid}.json"
        self.events_path = self.out / f"events-{self.pid}.jsonl"

    # ---- helpers -----------------------------------------------------------------------------
    def _resolve(self, sid: str) -> str:
        """Follow compression continuations (old sid -> new sid), bounded."""
        for _ in range(MAX_CHAIN_HOPS):
            s = self.sessions.get(sid)
            nxt = s.get("continued_to") if s else None
            if not nxt or nxt == sid:
                return sid
            sid = nxt
        return sid

    def _sess(self, sid: Any, ts: float, platform: Any = None, turn_id: Any = None) -> Optional[dict]:
        sid = str(sid or "")
        if not sid:
            return None
        sid = self._resolve(sid)
        s = self.sessions.get(sid)
        if s is None and turn_id:
            s = self._maybe_continue(sid, str(turn_id), ts)
        if s is None:
            s = self.sessions[sid] = _new_session(sid, ts)
            if platform:
                s["platform"] = platform
            self._prune()
        elif platform and not s["platform"]:
            s["platform"] = platform
        if s["platform"] == "subagent" and sid not in self.child_parent:
            self._adopt_pending_child(sid, s, ts)
        s["_ts"] = ts
        s["updated_at"] = _iso(ts)
        return s

    def _maybe_continue(self, sid: str, tid: str, ts: float) -> Optional[dict]:
        """M3: a new session id carrying the open turn of another session = context-compression rotation."""
        for old_sid, o in list(self.sessions.items()):
            if o.get("continued_to"):
                continue
            if o["_turn_id"] == tid or tid in o["_fg_turns"]:
                return self._continue(old_sid, sid, ts)
        return None

    def _continue(self, old_sid: str, sid: str, ts: float) -> dict:
        s = self.sessions.pop(old_sid)
        s["continued_from"] = old_sid
        s["continued_to"] = None
        self.sessions[sid] = s
        stub = _new_session(old_sid, ts)
        for k in ("model", "platform", "parent_session_id", "role", "goal", "subagent_id", "first_seen"):
            stub[k] = s[k]
        stub["continued_to"] = sid
        self.sessions[old_sid] = stub
        for child, parent in list(self.child_parent.items()):
            if parent == old_sid:
                self.child_parent[child] = sid
                c = self.sessions.get(child)
                if c is not None and c["parent_session_id"] == old_sid:
                    c["parent_session_id"] = sid
        if old_sid in self.child_parent:  # a subagent's own rotation: re-point the parent's index row
            parent_sid = self.child_parent.pop(old_sid)
            self.child_parent[sid] = parent_sid
            for row in (self.sessions.get(parent_sid) or {}).get("subagents", []):
                if row.get("session_id") == old_sid:
                    row["session_id"] = sid
        self._event(ts, sid, "session_continued", continued_from=old_sid)
        return s

    def _adopt_pending_child(self, sid: str, s: dict, ts: float) -> None:
        """A subagent whose child_session_id was None at subagent_start: map it to the first new session
        that explicitly says platform 'subagent', within 30 s (minor 6)."""
        while self.pending_children and ts - self.pending_children[0][2] > PENDING_CHILD_TTL_S:
            self.pending_children.popleft()
        if not self.pending_children or sid in self.child_parent or s.get("platform") != "subagent":
            return
        parent_sid, row, _ = self.pending_children.popleft()
        self._link_child(parent_sid, sid, row)

    def _link_child(self, parent_sid: str, child_sid: str, row: dict) -> None:
        row["session_id"] = child_sid
        self.child_parent[child_sid] = parent_sid
        c = self.sessions.get(child_sid)
        if c is None:
            c = self.sessions[child_sid] = _new_session(child_sid, self.clock())
        c["parent_session_id"] = parent_sid
        c["role"] = row.get("role")
        c["goal"] = row.get("goal")
        c["subagent_id"] = row.get("id")
        c["platform"] = c.get("platform") or "subagent"

    def _row_for_child(self, child_sid: str) -> Optional[dict]:
        parent = self.sessions.get(self.child_parent.get(child_sid, ""))
        if not parent:
            return None
        for row in reversed(parent["subagents"]):
            if row.get("session_id") == child_sid:
                return row
        return None

    def _prune(self) -> None:
        if len(self.sessions) <= MAX_SESSIONS:
            return
        idle = sorted((s["_ts"], sid) for sid, s in self.sessions.items() if s["status"] in ("idle", "error"))
        for _, sid in idle[: len(self.sessions) - MAX_SESSIONS]:
            self.sessions.pop(sid, None)

    def _event(self, ts: float, sid: Any, event: str, **fields: Any) -> None:
        rec = {"ts": _iso(ts), "pid": self.pid, "session_id": str(sid or "") or None, "event": event}
        rec.update({k: v for k, v in fields.items() if v is not None})
        self.events.append(rec)

    def _resume_status(self, s: dict) -> None:
        if s["waiting"]:
            s["status"] = "waiting"
        elif s["_running"]:
            s["status"] = "tool"
        elif s["turn"] is not None and s["turn"].get("ended_at") is None:
            s["status"] = "thinking"
        else:
            s["status"] = "idle"

    def _start_turn(self, s: dict, ts: float, turn_id: Any, sid: Any = None, model: Any = None) -> None:
        s["turn"] = {"started_at": _iso(ts), "ended_at": None, **_new_bucket()}
        s["_turn_id"] = turn_id or None
        s["steps"] = []
        s["_running"] = {}
        s["current_tool"] = None
        self._event(ts, sid, "turn_start", model=model or s.get("model"))

    @staticmethod
    def _is_background(s: dict, tid: Any) -> bool:
        """M2: a turn id this session never announced through pre_llm_call (forks never fire it)."""
        return bool(tid) and s["_llm_seen"] and tid not in s["_fg_turns"]

    def _bg(self, s: dict) -> dict:
        if s["background"] is None:
            s["background"] = {**_new_bucket(), "tool_calls": 0}
        return s["background"]

    def any_busy(self) -> bool:
        return any(s["status"] in _BUSY for s in self.sessions.values())

    # ---- event application --------------------------------------------------------------------
    def apply(self, kind: str, ts: float, d: dict) -> None:
        fn = getattr(self, "_on_" + kind, None)
        if fn is None:
            return
        fn(ts, d)
        self.dirty = True

    def _on_session_start(self, ts: float, d: dict) -> None:
        s = self._sess(d.get("session_id"), ts, d.get("platform"))
        if s is None:
            return
        s["model"] = d.get("model") or s["model"]
        if d.get("platform"):
            s["platform"] = d["platform"]
            sid = self._resolve(str(d.get("session_id")))
            if s["platform"] == "subagent" and sid not in self.child_parent:
                self._adopt_pending_child(sid, s, ts)
        self._event(ts, d.get("session_id"), "session_start", model=s["model"], platform=s["platform"])

    def _on_llm(self, ts: float, d: dict) -> None:
        """pre_llm_call: a real (persisted) turn starts. Background-review forks never fire it."""
        tid = d.get("turn_id") or None
        s = self._sess(d.get("session_id"), ts, d.get("platform"), tid)
        if s is None:
            return
        s["_llm_seen"] = True
        if tid:
            if tid not in s["_fg_turns"]:
                s["_fg_turns"].append(tid)
            if s["turn"] is None or s["turn"].get("ended_at") is not None or (s["_turn_id"] and tid != s["_turn_id"]):
                self._start_turn(s, ts, tid, d.get("session_id"), d.get("model"))
            elif not s["_turn_id"]:
                s["_turn_id"] = tid
        s["model"] = d.get("model") or s["model"]
        if not s["waiting"]:
            s["status"] = "thinking"

    def _on_pre_api(self, ts: float, d: dict) -> None:
        tid = d.get("turn_id") or None
        s = self._sess(d.get("session_id"), ts, d.get("platform"), tid)
        if s is None or self._is_background(s, tid):
            return
        turn = s["turn"]
        if turn is None or turn.get("ended_at") is not None or (tid and s["_turn_id"] and tid != s["_turn_id"]):
            self._start_turn(s, ts, tid, d.get("session_id"), d.get("model"))
        elif tid and not s["_turn_id"]:
            s["_turn_id"] = tid
        s["model"] = d.get("model") or s["model"]
        if not s["waiting"]:
            s["status"] = "thinking"
        approx = _int(d.get("approx"))
        if approx and (s["context"] is None or s["context"].get("estimated")):
            mx = (s["context"] or {}).get("max")
            s["context"] = {"used": approx, "max": mx, "pct": round(100.0 * approx / mx, 1) if mx else None,
                            "estimated": True}

    def _on_api(self, ts: float, d: dict) -> None:
        sid = d.get("session_id")
        tid = d.get("turn_id") or None
        s = self._sess(sid, ts, d.get("platform"), tid)
        if s is None:
            return
        inp, out, cr, cw = _int(d.get("in")), _int(d.get("out")), _int(d.get("cr")), _int(d.get("cw"))
        prompt = _int(d.get("prompt")) or (inp + cr + cw)
        model, provider, base_url = d.get("model"), d.get("provider"), d.get("base_url")
        if str(provider or "").lower() == "anthropic":
            self.anthropic_base_urls[str(base_url or "")] = ts
            self.base_urls_snapshot = tuple(sorted(self.anthropic_base_urls, key=self.anthropic_base_urls.get,
                                                   reverse=True))
        cost = self.cost_fn(model, provider, base_url, inp, out, cr, cw)
        n_tools = _int(d.get("tool_calls"))
        if self._is_background(s, tid):
            b = self._bg(s)
            b["calls"] += 1
            b["input"] += inp
            b["output"] += out
            b["cache_read"] += cr
            b["cache_write"] += cw
            _add_cost(b, cost)
            self._event(ts, sid, "api_call", background=True, model=model, input=inp, output=out, cache_read=cr,
                        cache_write=cw, cost_usd=cost)
            return
        if s["turn"] is None or s["turn"].get("ended_at") is not None:
            self._start_turn(s, ts, tid, sid, d.get("model"))
        t, tot = s["turn"], s["totals"]
        for bucket in (t, tot):
            bucket["calls"] += 1
            bucket["input"] += inp
            bucket["output"] += out
            bucket["cache_read"] += cr
            bucket["cache_write"] += cw
            _add_cost(bucket, cost)
        started, first, dur = d.get("started"), d.get("first_chunk"), d.get("dur")
        ttfb = None
        try:
            if first and started:
                ttfb = int(round((float(first) - float(started)) * 1000))
        except (TypeError, ValueError):
            ttfb = None
        try:
            latency = int(round(float(dur) * 1000)) if dur is not None else None
        except (TypeError, ValueError):
            latency = None
        finish = d.get("finish") if isinstance(d.get("finish"), str) else None
        s["last_call"] = {"in": inp, "out": out, "cache_read": cr, "cache_write": cw, "prompt": prompt,
                          "ttfb_ms": ttfb, "latency_ms": latency, "at": _iso(ts), "cost_usd": cost,
                          "finish_reason": finish}
        s["prompt_history"] = (s["prompt_history"] + [prompt])[-10:]
        ctx_max = _int(d.get("ctx_len")) or None
        s["context"] = {"used": prompt, "max": ctx_max,
                        "pct": round(100.0 * prompt / ctx_max, 1) if ctx_max else None, "estimated": False}
        s["model"] = model or s["model"]
        if not s["waiting"]:
            if n_tools:
                s["status"] = "tool"
            elif finish in ("stop", "end_turn") and not s["_running"]:
                s["status"] = "idle"  # final answer; on_session_end closes the turn (M2 minimum fallback)
            else:
                s["status"] = "thinking"
        row = self._row_for_child(self._resolve(str(sid or "")))
        if row is not None:
            row["input"] += inp
            row["output"] += out
            row["cache_read"] = row.get("cache_read", 0) + cr
            row["cache_write"] = row.get("cache_write", 0) + cw
            row["model"] = s["model"]
            _add_cost(row, cost)
        self._event(ts, sid, "api_call", model=s["model"], n=_int(d.get("n")) or None, input=inp, output=out,
                    cache_read=cr, cache_write=cw, prompt=prompt, ttfb_ms=ttfb, latency_ms=latency,
                    tool_calls=n_tools, cost_usd=cost)

    def _on_api_error(self, ts: float, d: dict) -> None:
        tid = d.get("turn_id") or None
        s = self._sess(d.get("session_id"), ts, None, tid)
        if s is None:
            return
        bg = self._is_background(s, tid)
        if not bg:
            s["status"] = "error"
        status_code = d.get("status_code")
        self._event(ts, d.get("session_id"), "api_error", background=True if bg else None,
                    error_type=str(d.get("error_type"))[:64] if d.get("error_type") else None,
                    status_code=status_code if isinstance(status_code, int) else None,
                    retryable=d.get("retryable") if isinstance(d.get("retryable"), bool) else None)

    def _on_tool_start(self, ts: float, d: dict) -> None:
        sid = d.get("session_id")
        tid = d.get("turn_id") or None
        s = self._sess(sid, ts, None, tid)
        if s is None:
            return
        name = str(d.get("tool") or "?")[:64]
        call = d.get("call") or f"anon-{ts}"
        if self._is_background(s, tid):
            s["_bg_calls"].add(call)
            self._bg(s)["tool_calls"] += 1
            self._event(ts, sid, "tool_start", background=True, tool=name)
            return
        preview = self.redact_fn(d.get("raw") or "", PREVIEW_CHARS) if d.get("raw") else ""
        if s["turn"] is None or s["turn"].get("ended_at") is not None:
            self._start_turn(s, ts, tid, sid, d.get("model"))
        step = {"tool": name, "status": "running", "ms": None, "_id": call}
        s["_running"][call] = {"name": name, "preview": preview, "started_at": _iso(ts), "_step": step}
        s["steps"] = (s["steps"] + [step])[-MAX_STEPS:]
        s["current_tool"] = {"name": name, "preview": preview, "started_at": _iso(ts)}
        tot = s["totals"]
        tot["tool_calls"] += 1
        tot["tools_by_name"][name] = tot["tools_by_name"].get(name, 0) + 1
        if not s["waiting"]:
            s["status"] = "tool"
        row = self._row_for_child(self._resolve(str(sid or "")))
        if row is not None:
            row["tools"] += 1
        self._event(ts, sid, "tool_start", tool=name)

    def _on_tool_end(self, ts: float, d: dict) -> None:
        sid = d.get("session_id")
        tid = d.get("turn_id") or None
        s = self._sess(sid, ts, None, tid)
        if s is None:
            return
        call = d.get("call")
        if (call and call in s["_bg_calls"]) or self._is_background(s, tid):
            s["_bg_calls"].discard(call)
            return
        name = str(d.get("tool") or "?")[:64]
        ok = str(d.get("status") or "ok").lower() not in ("error", "failed", "blocked")
        ms = d.get("ms")
        ms = int(ms) if isinstance(ms, (int, float)) else None
        run = s["_running"].pop(call, None) if call else None
        if run is None:  # unmatched: oldest running step of the same tool
            for k, v in list(s["_running"].items()):
                if v["name"] == name:
                    run = s["_running"].pop(k)
                    break
        if run is not None:
            step = run["_step"]
            step["status"], step["ms"] = ("ok" if ok else "error"), ms
            if step not in s["steps"]:
                s["steps"] = (s["steps"] + [step])[-MAX_STEPS:]
        else:
            s["steps"] = (s["steps"] + [{"tool": name, "status": "ok" if ok else "error", "ms": ms}])[-MAX_STEPS:]
        if s["_running"]:
            last = list(s["_running"].values())[-1]
            s["current_tool"] = {"name": last["name"], "preview": last["preview"], "started_at": last["started_at"]}
        else:
            s["current_tool"] = None
        self._resume_status(s)
        self._event(ts, sid, "tool_end", tool=name, status="ok" if ok else "error", ms=ms)

    def _on_wait_start(self, ts: float, d: dict) -> None:
        kind = d.get("kind") if d.get("kind") in _WAIT_KINDS else "approval"
        s = self._sess(d.get("session_id"), ts)  # no session id: event only, never guess a session (minor 7)
        if s is not None:
            s["waiting"] = {"kind": kind, "since": _iso(ts), "_req": d.get("req")}
            s["status"] = "waiting"
        self._event(ts, d.get("session_id"), "wait_start", kind=kind)

    def _on_wait_end(self, ts: float, d: dict) -> None:
        kind = d.get("kind") if d.get("kind") in _WAIT_KINDS else "approval"
        s = self._sess(d.get("session_id"), ts)
        if s is None or not s["waiting"]:
            s = next((x for x in self.sessions.values() if x["waiting"] and x["waiting"].get("_req") == d.get("req")), s)
        if s is not None and s["waiting"]:
            s["waiting"] = None
            self._resume_status(s)
        outcome = d.get("outcome")
        self._event(ts, d.get("session_id"), "wait_end", kind=kind,
                    outcome=str(outcome)[:32] if isinstance(outcome, str) else None)

    def _on_approval(self, ts: float, d: dict) -> None:
        # Status comes from on_human_input_* (C2); these only feed the events log.
        self._event(ts, d.get("session_id"), "approval_request", surface=_short(d.get("surface")),
                    pattern=_short(d.get("pattern")))

    def _on_approval_done(self, ts: float, d: dict) -> None:
        self._event(ts, d.get("session_id"), "approval_response", surface=_short(d.get("surface")),
                    choice=_short(d.get("choice")))

    def _on_aux_start(self, ts: float, d: dict) -> None:
        if not _is_compression(d.get("task")):
            return
        s = self._sess(d.get("session_id"), ts)
        if s is not None:
            s["status"] = "compacting"
        self._event(ts, d.get("session_id"), "compaction_start")

    def _on_aux_end(self, ts: float, d: dict) -> None:
        if not _is_compression(d.get("task")):
            return
        s = self._sess(d.get("session_id"), ts)
        if s is not None:
            if not d.get("error"):
                s["compressions"] += 1
            if s["status"] == "compacting":
                self._resume_status(s)
        self._event(ts, d.get("session_id"), "compaction_end", ok=not d.get("error"))

    def _on_sub_start(self, ts: float, d: dict) -> None:
        parent_sid = self._resolve(str(d.get("parent") or ""))
        p = self._sess(parent_sid, ts)
        if p is None:
            return
        goal = self.redact_fn(d.get("goal") or "", GOAL_CHARS) if PREVIEW_ON and d.get("goal") else ""
        row = {"id": str(d.get("sub") or d.get("child") or f"sa-{len(p['subagents'])}"),
               "session_id": None, "goal": goal, "role": _short(d.get("role")), "model": None, "status": "running",
               "started_at": _iso(ts), "ms": None, "tools": 0, "input": 0, "output": 0, "cache_read": 0,
               "cache_write": 0, "cost_usd": 0.0, "_unpriced": 0}
        p["subagents"] = (p["subagents"] + [row])[-MAX_SUBAGENT_ROWS:]
        child = str(d.get("child") or "")
        if child:
            self._link_child(parent_sid, child, row)
        else:
            self.pending_children.append((parent_sid, row, ts))
        self._event(ts, parent_sid, "subagent_start", child_session_id=child or None, role=row["role"])

    def _on_sub_stop(self, ts: float, d: dict) -> None:
        parent_sid = self._resolve(str(d.get("parent") or ""))
        child = self._resolve(str(d.get("child"))) if d.get("child") else ""
        p = self._sess(parent_sid, ts)
        if p is None:
            return
        row = None
        for r in reversed(p["subagents"]):
            if (child and r.get("session_id") == child) or (not child and r["status"] == "running"):
                row = r
                break
        status = _short(d.get("status")) or "completed"
        ms = d.get("ms") if isinstance(d.get("ms"), (int, float)) else None
        if row is not None:
            row["status"], row["ms"] = status, ms
            if isinstance(d.get("tools"), int):
                row["tools"] = max(row["tools"], d["tools"])
        c = self.sessions.get(child) if child else None
        if c is not None:
            c["status"] = "error" if status == "failed" else "idle"
            c["current_tool"], c["_running"], c["waiting"] = None, {}, None
            if c["turn"] is not None and c["turn"].get("ended_at") is None:
                c["turn"]["ended_at"] = _iso(ts)
        self._event(ts, parent_sid, "subagent_stop", child_session_id=child or None, status=status, ms=ms,
                    tools=d.get("tools") if isinstance(d.get("tools"), int) else None)

    def _on_turn_end(self, ts: float, d: dict) -> None:
        s = self._sess(d.get("session_id"), ts)
        if s is None:
            return
        if s["turn"] is not None and s["turn"].get("ended_at") is None:
            s["turn"]["ended_at"] = _iso(ts)
        for run in s["_running"].values():
            if run["_step"]["status"] == "running":
                run["_step"]["status"] = "error" if d.get("interrupted") else "ok"
        s["_running"], s["current_tool"], s["waiting"] = {}, None, None
        s["status"] = "error" if d.get("failed") else "idle"
        self._event(ts, d.get("session_id"), "turn_end", completed=_bool(d.get("completed")),
                    interrupted=_bool(d.get("interrupted")), failed=_bool(d.get("failed")),
                    calls=(s["turn"] or {}).get("calls"))

    # ---- output -----------------------------------------------------------------------------
    def snapshot(self, now: Optional[float] = None) -> dict:
        now = self.clock() if now is None else now
        out: dict[str, dict] = {}
        for sid, s in self.sessions.items():
            pub = {k: v for k, v in s.items() if not k.startswith("_")}
            pub["steps"] = [{k: v for k, v in st.items() if not k.startswith("_")} for st in s["steps"]]
            pub["waiting"] = ({k: v for k, v in s["waiting"].items() if not k.startswith("_")}
                              if s["waiting"] else None)
            pub["turn"] = _pub_bucket(s["turn"])
            pub["totals"] = _pub_bucket(s["totals"])
            pub["background"] = _pub_bucket(s["background"])
            pub["subagents"] = [_pub_bucket(r) for r in s["subagents"]]
            pub["prompt_history"] = list(s["prompt_history"])
            out[sid] = pub
        return {"v": 1, "pid": self.pid, "written_at": _iso(now), "plugin_version": __version__, "sessions": out}

    def write_snapshot(self, now: Optional[float] = None) -> None:
        now = self.clock() if now is None else now
        self.out.mkdir(parents=True, exist_ok=True)
        _atomic_write_json(self.snapshot_path, self.snapshot(now))
        self.last_snapshot = now
        self.dirty = False

    def flush_events(self) -> None:
        if not self.events:
            return
        batch, self.events = self.events, []
        data = "".join(json.dumps(e, separators=(",", ":"), default=str) + "\n" for e in batch).encode("utf-8")
        self.out.mkdir(parents=True, exist_ok=True)
        self._maybe_rotate()
        fd = os.open(str(self.events_path), os.O_WRONLY | os.O_CREAT | os.O_APPEND)
        try:
            os.write(fd, data)
        finally:
            os.close(fd)

    def _maybe_rotate(self) -> None:
        """Per-pid file, one writer: rotate to events-<pid>.1.jsonl at EVENTS_MAX_BYTES (no shared lock)."""
        try:
            if self.events_path.stat().st_size < EVENTS_MAX_BYTES:
                return
            os.replace(self.events_path, self.out / f"events-{self.pid}.1.jsonl")
        except OSError:
            pass


def _short(v: Any, n: int = 48) -> Optional[str]:
    return str(v)[:n] if isinstance(v, str) and v else None


def _bool(v: Any) -> Optional[bool]:
    return v if isinstance(v, bool) else None


def _is_compression(task: Any) -> bool:
    return isinstance(task, str) and task.lower().startswith(("compress", "compact"))


# =============================================================================================
# Plan windows (official 5h / weekly %)
# =============================================================================================
def _default_token() -> Optional[str]:
    """The Claude Code OAuth access token ONLY while it is currently valid (read-only: never refreshes)."""
    try:
        from agent.anthropic_credentials import is_claude_code_token_valid, read_claude_code_credentials
        creds = read_claude_code_credentials()
        if not creds or not is_claude_code_token_valid(creds):
            return None
        tok = creds.get("accessToken")
        return tok if isinstance(tok, str) and tok else None
    except Exception:
        return None


def _default_fetch(base_url: Optional[str], token: str):
    # Monolith's port is strictly local, even with an inherited FETCH env.
    # The upstream poller tests can still inject a fake fetch_fn.
    raise RuntimeError("Monolith metrics are cache-only; network fetching is disabled")


def _default_cached(base_urls: list):
    """Freshest non-stale snapshot already in Hermes's in-memory cache (no network)."""
    from agent.account_usage_cache import cached_account_usage, snapshot_is_stale
    best = None
    for bu in list(base_urls) + [None]:
        try:
            snap = cached_account_usage("anthropic", base_url=bu or None)
        except Exception:
            snap = None
        if snap is None or not getattr(snap, "windows", None) or snapshot_is_stale(snap):
            continue
        if best is None or snap.fetched_at > best.fetched_at:
            best = snap
    return best


def _windows(snap) -> list:
    out = []
    for w in getattr(snap, "windows", ()) or ():
        ra = getattr(w, "reset_at", None)
        up = getattr(w, "used_percent", None)
        out.append({"label": str(getattr(w, "label", "") or ""),
                    "used_percent": None if up is None else round(float(up), 1),
                    "reset_at": _iso(ra.timestamp()) if hasattr(ra, "timestamp") else None,
                    "scope": str(getattr(w, "scope", "") or "account")})
    return out


class PlanPoller:
    """mode 'cache' (default): mirror Hermes's in-memory account_usage_cache only, never the network.
    mode 'fetch' remains only for injected fake-fetch unit tests; Monolith's production
    hooks always select cache mode and the default fetch function rejects network access.
    ``plan.off`` in the output dir pauses everything."""

    def __init__(self, out_dir: Path, base_urls: Callable[[], Any], *, mode: str = "cache",
                 fetch_fn: Callable = _default_fetch, cached_fn: Callable = _default_cached,
                 token_fn: Callable[[], Optional[str]] = _default_token,
                 clock: Callable[[], float] = time.time, interval_s: float = PLAN_INTERVAL_S,
                 max_backoff_s: float = PLAN_MAX_BACKOFF_S, lock_stale_s: float = PLAN_LOCK_STALE_S) -> None:
        self.out = Path(out_dir)
        self.path = self.out / "plan.json"
        self.lock = self.out / "plan.lock"
        self.off = self.out / "plan.off"
        self.mode = "fetch" if mode == "fetch" else "cache"
        self.base_urls, self.fetch_fn, self.cached_fn, self.token_fn, self.clock = \
            base_urls, fetch_fn, cached_fn, token_fn, clock
        self.interval, self.max_backoff, self.lock_stale = interval_s, max_backoff_s, lock_stale_s
        self.fetches = 0  # network fetches made by this process (tests)

    def read(self) -> Optional[dict]:
        try:
            with open(self.path, encoding="utf-8") as f:
                rec = json.load(f)
            return rec if isinstance(rec, dict) else None
        except (OSError, ValueError):
            return None

    def _due(self, rec: Optional[dict], now: float) -> float:
        """Seconds until the next attempt is allowed (<= 0: due now)."""
        if not rec:
            return 0.0
        nxt = _parse_iso(rec.get("next_attempt_at"))
        if nxt is None:
            last = max(_parse_iso(rec.get("fetched_at")) or 0.0, _parse_iso(rec.get("attempted_at")) or 0.0)
            nxt = last + self.interval
        return nxt - now

    def _write(self, rec: dict) -> None:
        self.out.mkdir(parents=True, exist_ok=True)
        _atomic_write_json(self.path, rec)

    def tick(self) -> float:
        """One poll step; returns how long to sleep before the next one."""
        if self.off.exists():  # plan.off: consumer opted out of plan windows (M8)
            return PLAN_CACHE_POLL_S
        now = self.clock()
        rec = self.read()
        urls = [u for u in self.base_urls()]
        # 1) Hermes already holds a fresher snapshot (the TUI's session.usage refreshes it): no network.
        try:
            snap = self.cached_fn(urls) if urls else None
        except Exception:
            snap = None
        if snap is not None:
            ft = snap.fetched_at.timestamp() if hasattr(snap.fetched_at, "timestamp") else None
            if ft and now - ft < self.interval and ft > (_parse_iso((rec or {}).get("fetched_at")) or 0.0) + 1:
                self._write({"fetched_at": _iso(ft), "windows": _windows(snap), "details": [str(x) for x in snap.details or ()],
                             "unavailable_reason": None, "source": "hermes-cache", "attempted_at": _iso(now),
                             "next_attempt_at": _iso(max(now, ft + self.interval)), "failures": 0})
                return PLAN_CACHE_POLL_S if self.mode == "cache" else max(30.0, ft + self.interval - now)
        if self.mode != "fetch":  # M7: cache-only by default
            return PLAN_CACHE_POLL_S
        wait = self._due(rec, now)
        if wait > 0:
            return min(max(wait, 5.0), 300.0)
        if not urls:  # no Anthropic call seen in this process yet: nothing to ask for
            return 60.0
        try:
            token = self.token_fn()
        except Exception:
            token = None
        if not token:  # expired / no Claude Code login: never trigger a refresh; try again later
            return self.interval
        fd = _try_lock(self.lock, self.lock_stale, now)
        if fd is None:
            return 30.0
        try:
            rec = self.read()  # another process may have fetched while we waited for the lock
            if self._due(rec, now) > 0:
                return min(max(self._due(rec, now), 5.0), 300.0)
            failures = _int((rec or {}).get("failures"))
            try:
                self.fetches += 1
                snap = self.fetch_fn(urls[0] or None, token)
            except Exception:
                snap = None
            now = self.clock()
            if snap is not None and getattr(snap, "windows", None) and not getattr(snap, "unavailable_reason", None):
                self._write({"fetched_at": _iso(now), "windows": _windows(snap),
                             "details": [str(x) for x in (getattr(snap, "details", ()) or ())],
                             "unavailable_reason": None, "source": "fetch", "attempted_at": _iso(now),
                             "next_attempt_at": _iso(now + self.interval), "failures": 0})
                return self.interval
            # None also covers HTTP 429: fetch_account_usage swallows every error (C5).
            failures += 1
            backoff = min(self.interval * (2 ** (failures - 1)), self.max_backoff)
            reason = getattr(snap, "unavailable_reason", None) if snap is not None else None
            prev = rec or {}
            self._write({"fetched_at": prev.get("fetched_at"), "windows": prev.get("windows") or [],
                         "details": prev.get("details") or [],
                         "unavailable_reason": str(reason)[:200] if reason else "fetch failed",
                         "source": prev.get("source"), "attempted_at": _iso(now),
                         "next_attempt_at": _iso(now + backoff), "failures": failures})
            return backoff
        finally:
            _unlock(self.lock, fd)


# =============================================================================================
# Runtime: queue, lazy threads, hooks
# =============================================================================================
_Q: "queue.SimpleQueue[tuple[str, float, dict]]" = queue.SimpleQueue()
_started = False
_disabled = False
_closing = False
_start_lock = threading.Lock()
_metrics: Optional[Metrics] = None


def _out_dir() -> Path:
    from hermes_constants import get_hermes_home
    return Path(get_hermes_home()) / "cache" / "habby-metrics"


def _write_backoff(m: Metrics) -> float:
    return WRITE_FAIL_BACKOFF_S if m.write_failures >= WRITE_FAIL_BACKOFF_AFTER else SNAPSHOT_MIN_INTERVAL_S


def _next_timeout(m: Metrics, now: float) -> Optional[float]:
    """How long the writer may block on the queue: None = forever (idle, zero CPU)."""
    if m.dirty:
        return max(0.0, m.last_snapshot + _write_backoff(m) - now)
    if m.any_busy():
        return max(0.0, m.last_snapshot + HEARTBEAT_S - now)  # M1 heartbeat while something is active
    return None


def _writer_step(m: Metrics, q: "queue.SimpleQueue", clock: Callable[[], float] = time.time) -> None:
    """One writer iteration (factored out of the loop for tests)."""
    timeout = _next_timeout(m, clock())
    try:
        item = q.get() if timeout is None else (q.get(timeout=timeout) if timeout > 0 else q.get_nowait())
    except queue.Empty:
        item = None
    batch = [item] if item is not None else []
    while len(batch) < 5000:
        try:
            batch.append(q.get_nowait())
        except queue.Empty:
            break
    for kind, ts, d in batch:
        try:
            m.apply(kind, ts, d)
        except Exception:
            pass
    try:
        m.flush_events()
    except Exception:
        m.events = []
    now = clock()
    if not m.dirty and m.any_busy() and now - m.last_snapshot >= HEARTBEAT_S:
        m.dirty = True  # heartbeat: re-stamp written_at
    if m.dirty and not _closing and now - m.last_snapshot >= _write_backoff(m):
        try:
            m.write_snapshot(now)
            m.write_failures = 0
        except Exception:
            m.write_failures += 1
            m.last_snapshot = now  # stay dirty: retried after 250 ms; after 5 failures every 5 s (minor 12)


def _writer_loop(m: Metrics) -> None:
    last_prune = 0.0
    try:
        m.out.mkdir(parents=True, exist_ok=True)
    except Exception:
        pass
    while not _closing:
        if time.time() - last_prune > PRUNE_INTERVAL_S:
            last_prune = time.time()
            try:
                prune_files(m.out, m.pid)
            except Exception:
                pass
        _writer_step(m, _Q)


def _plan_loop(p: PlanPoller) -> None:
    while not _closing:
        try:
            delay = p.tick()
        except Exception:
            delay = 300.0
        time.sleep(max(1.0, float(delay)))


def _cleanup_snapshot() -> None:
    global _closing
    _closing = True  # the writer must not re-create the file after this (minor 9)
    try:
        if _metrics is not None:
            _metrics.snapshot_path.unlink()
    except Exception:
        pass


def _start() -> None:
    global _started, _disabled, _metrics
    with _start_lock:
        if _started or _disabled:
            return
        try:
            m = Metrics(_out_dir())
            threading.Thread(target=_writer_loop, args=(m,), name="habby-metrics-writer", daemon=True).start()
            _metrics = m
            mode = os.environ.get("HABBY_METRICS_PLAN", "cache").strip().lower()
            if mode not in ("0", "off", "false"):
                poller = PlanPoller(m.out, lambda: list(m.base_urls_snapshot), mode="cache")
                threading.Thread(target=_plan_loop, args=(poller,), name="habby-metrics-plan", daemon=True).start()
            atexit.register(_cleanup_snapshot)
            _started = True
        except Exception:
            _disabled = True  # no writer: _put becomes a no-op instead of queueing forever (minor 11)


def _put(kind: str, d: dict) -> None:
    if _disabled:
        return
    if not _started:
        _start()
        if _disabled:
            return
    _Q.put((kind, time.time(), d))


def _raw_preview(args: Any) -> str:
    if not PREVIEW_ON or not isinstance(args, dict):
        return ""
    for k in _PREVIEW_KEYS:
        v = args.get(k)
        if isinstance(v, str) and v:
            if k in _PATH_KEYS:
                v = re.split(r"[\\/]", v.rstrip("\\/"))[-1]
            return v[:RAW_TEXT_CAP]
    return ""


# Every hook: copy scalars, one queue put, swallow everything, return None.
def _h_pre_llm_call(**kw: Any) -> None:
    try:
        _put("llm", {"session_id": kw.get("session_id"), "turn_id": kw.get("turn_id"), "model": kw.get("model"),
                     "platform": kw.get("platform")})
    except Exception:
        pass
    return None  # never inject context


def _h_pre_api_request(**kw: Any) -> None:
    try:
        _put("pre_api", {"session_id": kw.get("session_id"), "turn_id": kw.get("turn_id"), "model": kw.get("model"),
                         "platform": kw.get("platform"), "approx": kw.get("approx_input_tokens")})
    except Exception:
        pass


def _h_post_api_request(**kw: Any) -> None:
    try:
        u = kw.get("usage")
        if not isinstance(u, dict):
            u = {}
        _put("api", {"session_id": kw.get("session_id"), "turn_id": kw.get("turn_id"), "model": kw.get("model"),
                     "provider": kw.get("provider"), "base_url": kw.get("base_url"), "platform": kw.get("platform"),
                     "n": kw.get("api_call_count"), "dur": kw.get("api_duration"), "started": kw.get("started_at"),
                     "first_chunk": kw.get("first_chunk_at"), "finish": kw.get("finish_reason"),
                     "ctx_len": kw.get("context_length"), "in": u.get("input_tokens"), "out": u.get("output_tokens"),
                     "cr": u.get("cache_read_tokens"), "cw": u.get("cache_write_tokens"),
                     "prompt": u.get("prompt_tokens"), "tool_calls": kw.get("assistant_tool_call_count")})
    except Exception:
        pass


def _h_api_request_error(**kw: Any) -> None:
    try:
        # Hermes >= 0.22.6 sends error={"type", "message"} (no top-level error_type); only the type is kept.
        err = kw.get("error")
        etype = kw.get("error_type") or (err.get("type") if isinstance(err, dict) else None)
        _put("api_error", {"session_id": kw.get("session_id"), "turn_id": kw.get("turn_id"),
                           "error_type": etype, "status_code": kw.get("status_code"),
                           "retryable": kw.get("retryable")})
    except Exception:
        pass


def _h_pre_tool_call(**kw: Any) -> None:
    try:
        _put("tool_start", {"session_id": kw.get("session_id"), "tool": kw.get("tool_name"),
                            "call": kw.get("tool_call_id"), "turn_id": kw.get("turn_id"),
                            "raw": _raw_preview(kw.get("args"))})
    except Exception:
        pass
    return None  # never block, never modify args


def _h_post_tool_call(**kw: Any) -> None:
    try:
        _put("tool_end", {"session_id": kw.get("session_id"), "tool": kw.get("tool_name"), "turn_id": kw.get("turn_id"),
                          "call": kw.get("tool_call_id"), "ms": kw.get("duration_ms"), "status": kw.get("status")})
    except Exception:
        pass


def _h_subagent_start(**kw: Any) -> None:
    try:
        g = kw.get("child_goal")
        _put("sub_start", {"parent": kw.get("parent_session_id"), "child": kw.get("child_session_id"),
                           "sub": kw.get("child_subagent_id"), "role": kw.get("child_role"),
                           "goal": g[:RAW_TEXT_CAP] if isinstance(g, str) else ""})
    except Exception:
        pass


def _h_subagent_stop(**kw: Any) -> None:
    try:
        h = kw.get("tool_call_history")
        _put("sub_stop", {"parent": kw.get("parent_session_id"), "child": kw.get("child_session_id"),
                          "status": kw.get("child_status"), "ms": kw.get("duration_ms"),
                          "tools": len(h) if isinstance(h, (list, tuple)) else None})
    except Exception:
        pass


def _h_pre_approval_request(**kw: Any) -> None:
    try:
        _put("approval", {"session_id": kw.get("session_id"), "surface": kw.get("surface"),
                          "pattern": kw.get("pattern_key")})
    except Exception:
        pass


def _h_post_approval_response(**kw: Any) -> None:
    try:
        _put("approval_done", {"session_id": kw.get("session_id"), "surface": kw.get("surface"),
                               "choice": kw.get("choice")})
    except Exception:
        pass


def _h_on_human_input_request(**kw: Any) -> None:
    try:
        _put("wait_start", {"session_id": kw.get("session_id"), "kind": kw.get("kind"), "req": kw.get("request_id")})
    except Exception:
        pass


def _h_on_human_input_resolved(**kw: Any) -> None:
    try:
        _put("wait_end", {"session_id": kw.get("session_id"), "kind": kw.get("kind"), "req": kw.get("request_id"),
                          "outcome": kw.get("outcome")})
    except Exception:
        pass


def _h_on_session_start(**kw: Any) -> None:
    try:
        _put("session_start", {"session_id": kw.get("session_id"), "model": kw.get("model"),
                               "platform": kw.get("platform")})
    except Exception:
        pass


def _h_on_session_end(**kw: Any) -> None:
    try:
        _put("turn_end", {"session_id": kw.get("session_id"), "completed": kw.get("completed"),
                          "interrupted": kw.get("interrupted"), "failed": kw.get("failed")})
    except Exception:
        pass


def _h_pre_auxiliary_call(**kw: Any) -> None:
    try:
        t = kw.get("aux_task")
        if _is_compression(t):
            _put("aux_start", {"session_id": kw.get("session_id"), "task": t})
    except Exception:
        pass


def _h_post_auxiliary_call(**kw: Any) -> None:
    try:
        t = kw.get("aux_task")
        if _is_compression(t):
            _put("aux_end", {"session_id": kw.get("session_id"), "task": t, "error": bool(kw.get("error_type"))})
    except Exception:
        pass


HOOKS = (
    ("pre_llm_call", _h_pre_llm_call),
    ("pre_api_request", _h_pre_api_request), ("post_api_request", _h_post_api_request),
    ("api_request_error", _h_api_request_error), ("pre_tool_call", _h_pre_tool_call),
    ("post_tool_call", _h_post_tool_call), ("subagent_start", _h_subagent_start),
    ("subagent_stop", _h_subagent_stop), ("pre_approval_request", _h_pre_approval_request),
    ("post_approval_response", _h_post_approval_response), ("on_human_input_request", _h_on_human_input_request),
    ("on_human_input_resolved", _h_on_human_input_resolved), ("on_session_start", _h_on_session_start),
    ("on_session_end", _h_on_session_end), ("pre_auxiliary_call", _h_pre_auxiliary_call),
    ("post_auxiliary_call", _h_post_auxiliary_call),
)


def register(ctx) -> None:
    """Register observer hooks only. Threads start lazily on the first hook."""
    for name, fn in HOOKS:
        try:
            ctx.register_hook(name, fn)
        except Exception:
            pass
