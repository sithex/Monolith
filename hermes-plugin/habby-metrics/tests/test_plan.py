"""Plan-window poller: Hermes cache first, one fetch per interval across processes (plan.json freshness +
O_EXCL plan.lock), exponential backoff on None (5 -> 60 min), only present windows. The fetch function is
always a fake: these tests never touch the network."""
from __future__ import annotations

import json
import os
import time
from datetime import datetime, timedelta, timezone

from _common import load_plugin, rmtree, tmpdir


def _snap(fetched=None, windows=(("Current session", 17.0, 3), ("Current week", 39.0, 100)), reason=None):
    from agent.account_usage import AccountUsageSnapshot, AccountUsageWindow
    now = fetched or datetime.now(timezone.utc)
    ws = tuple(AccountUsageWindow(label=l, used_percent=p, reset_at=now + timedelta(hours=h)) for l, p, h in windows)
    return AccountUsageSnapshot(provider="anthropic", source="oauth_usage_api", fetched_at=now, windows=ws,
                                details=("Extra usage: off",), unavailable_reason=reason)


class Clock:
    def __init__(self):
        self.t = time.time()

    def __call__(self):
        return self.t


def _poller(mod, out, clock, fetch, cached=lambda urls: None, urls=("https://api.anthropic.com",), mode="fetch",
            token=lambda: "valid-token"):
    return mod.PlanPoller(out, lambda: list(urls), mode=mode, fetch_fn=lambda bu, tok: fetch(bu), cached_fn=cached,
                          token_fn=token, clock=clock)


def test_fetch_then_fresh_then_refetch():
    out = tmpdir("plan-")
    try:
        mod, clock, calls = load_plugin(), Clock(), []
        p = _poller(mod, out, clock, lambda bu: calls.append(bu) or _snap())
        assert p.tick() == 300
        rec = json.loads((out / "plan.json").read_text(encoding="utf-8"))
        assert set(rec) >= {"fetched_at", "windows", "details", "unavailable_reason"}
        assert rec["unavailable_reason"] is None and rec["source"] == "fetch"
        assert [w["label"] for w in rec["windows"]] == ["Current session", "Current week"]
        assert rec["windows"][0]["used_percent"] == 17.0 and rec["windows"][0]["reset_at"].endswith("Z")
        assert set(rec["windows"][0]) == {"label", "used_percent", "reset_at", "scope"}
        assert rec["details"] == ["Extra usage: off"] and calls == ["https://api.anthropic.com"]
        clock.t += 120
        p.tick()
        assert len(calls) == 1  # fresh: no second fetch
        clock.t += 181
        p.tick()
        assert len(calls) == 2
        assert not (out / "plan.lock").exists()
    finally:
        rmtree(out)


def test_only_present_windows():
    out = tmpdir("plan-")
    try:
        mod, clock = load_plugin(), Clock()
        p = _poller(mod, out, clock, lambda bu: _snap(windows=(("Current week", 39.0, 1),)))
        p.tick()
        rec = json.loads((out / "plan.json").read_text(encoding="utf-8"))
        assert [w["label"] for w in rec["windows"]] == ["Current week"]
    finally:
        rmtree(out)


def test_backoff_on_none_5_to_60_min_keeps_last_windows():
    out = tmpdir("plan-")
    try:
        mod, clock, calls = load_plugin(), Clock(), []
        results = [_snap()] + [None] * 7 + [_snap(windows=(("Current session", 50.0, 1),))]
        p = _poller(mod, out, clock, lambda bu: calls.append(1) or results[len(calls) - 1])
        p.tick()
        delays = []
        for _ in range(7):
            clock.t += 10_000  # always past due
            delays.append(p.tick())
            rec = json.loads((out / "plan.json").read_text(encoding="utf-8"))
            assert rec["unavailable_reason"] == "fetch failed"
            assert [w["label"] for w in rec["windows"]] == ["Current session", "Current week"]  # last good kept
        assert delays == [300, 600, 1200, 2400, 3600, 3600, 3600], delays
        assert rec["failures"] == 7
        # backoff is persisted: a fresh poller (another process) also waits
        clock.t += 100
        q = _poller(mod, out, clock, lambda bu: calls.append(1) or _snap())
        n = len(calls)
        q.tick()
        assert len(calls) == n
        clock.t += 3600
        q.tick()
        rec = json.loads((out / "plan.json").read_text(encoding="utf-8"))
        assert rec["failures"] == 0 and rec["unavailable_reason"] is None
    finally:
        rmtree(out)


def test_unavailable_reason_counts_as_failure():
    out = tmpdir("plan-")
    try:
        mod, clock = load_plugin(), Clock()
        p = _poller(mod, out, clock, lambda bu: _snap(windows=(), reason="not logged in"))
        assert p.tick() == 300
        rec = json.loads((out / "plan.json").read_text(encoding="utf-8"))
        assert rec["unavailable_reason"] == "not logged in" and rec["windows"] == []
    finally:
        rmtree(out)


def test_fetch_raising_is_contained():
    out = tmpdir("plan-")
    try:
        mod, clock = load_plugin(), Clock()

        def bad(bu):
            raise RuntimeError("network down")
        p = _poller(mod, out, clock, bad)
        assert p.tick() == 300
        assert not (out / "plan.lock").exists()
    finally:
        rmtree(out)


def test_two_processes_one_fetch_per_interval():
    out = tmpdir("plan-")
    try:
        mod, clock, calls = load_plugin(), Clock(), []
        fetch = lambda bu: calls.append(1) or _snap()
        a, b = _poller(mod, out, clock, fetch), _poller(mod, out, clock, fetch)
        for _ in range(20):
            a.tick()
            b.tick()
            clock.t += 30
        # 20 rounds * 30 s = 600 s -> at most 3 fetches (t=0, ~300, ~600)
        assert 2 <= len(calls) <= 3, len(calls)
    finally:
        rmtree(out)


def test_lock_held_blocks_stale_lock_breaks():
    out = tmpdir("plan-")
    try:
        mod, clock, calls = load_plugin(), Clock(), []
        p = _poller(mod, out, clock, lambda bu: calls.append(1) or _snap())
        out.mkdir(parents=True, exist_ok=True)
        lock = out / "plan.lock"
        lock.write_text("999")
        assert p.tick() == 30 and calls == []        # fresh lock held by "another process"
        old = time.time() - 600
        os.utime(lock, (old, old))
        p.tick()
        assert calls == [1] and not lock.exists()    # stale lock (> 120 s) broken
    finally:
        rmtree(out)


def test_hermes_cache_preferred_no_network():
    out = tmpdir("plan-")
    try:
        mod, clock, calls = load_plugin(), Clock(), []
        cached = _snap(fetched=datetime.now(timezone.utc) - timedelta(seconds=30), windows=(("Current session", 61.0, 2),))
        p = _poller(mod, out, clock, lambda bu: calls.append(1) or _snap(), cached=lambda urls: cached)
        p.tick()
        rec = json.loads((out / "plan.json").read_text(encoding="utf-8"))
        assert calls == [] and rec["source"] == "hermes-cache" and rec["windows"][0]["used_percent"] == 61.0
        # the same cached snapshot is not rewritten again, and no fetch happens while it is fresh
        clock.t += 60
        p.tick()
        assert calls == []
    finally:
        rmtree(out)


def test_real_cache_reader_against_hermes_cache():
    """_default_cached reads agent.account_usage_cache (in memory, no network)."""
    mod = load_plugin()
    from agent import account_usage_cache as C
    assert mod._default_cached(["https://api.anthropic.com"]) is None
    snap = _snap()
    C.remember_account_usage("anthropic", snap, base_url="https://api.anthropic.com")
    got = mod._default_cached(["https://api.anthropic.com"])
    assert got is snap


def test_no_anthropic_seen_no_fetch():
    out = tmpdir("plan-")
    try:
        mod, clock, calls = load_plugin(), Clock(), []
        p = _poller(mod, out, clock, lambda bu: calls.append(1) or _snap(), urls=())
        assert p.tick() == 60 and calls == []
    finally:
        rmtree(out)


def test_default_mode_is_cache_only_never_fetches():
    """M7: without HABBY_METRICS_PLAN=fetch the poller only mirrors Hermes's in-memory cache."""
    out = tmpdir("plan-")
    try:
        mod, clock, calls = load_plugin(), Clock(), []
        p = mod.PlanPoller(out, lambda: ["https://api.anthropic.com"], fetch_fn=lambda bu, tok: calls.append(1),
                           cached_fn=lambda urls: None, token_fn=lambda: "valid-token", clock=clock)
        assert p.mode == "cache"
        for _ in range(5):
            assert p.tick() == mod.PLAN_CACHE_POLL_S
            clock.t += 10_000
        assert calls == [] and p.fetches == 0 and not (out / "plan.json").exists()
        cached = _snap(fetched=datetime.fromtimestamp(clock.t - 20, timezone.utc))
        p.cached_fn = lambda urls: cached
        p.tick()
        rec = json.loads((out / "plan.json").read_text(encoding="utf-8"))
        assert rec["source"] == "hermes-cache" and calls == []
    finally:
        rmtree(out)


def test_fetch_mode_needs_a_valid_token_and_passes_it():
    """M7: fetch mode never triggers a refresh: an expired/missing token = no fetch at all."""
    out = tmpdir("plan-")
    try:
        mod, clock, calls = load_plugin(), Clock(), []
        p = mod.PlanPoller(out, lambda: ["https://api.anthropic.com"], mode="fetch",
                           fetch_fn=lambda bu, tok: calls.append(tok) or _snap(), cached_fn=lambda urls: None,
                           token_fn=lambda: None, clock=clock)
        assert p.tick() == p.interval and calls == [] and not (out / "plan.lock").exists()
        p.token_fn = lambda: "valid-token"
        p.tick()
        assert calls == ["valid-token"]
        assert "valid-token" not in (out / "plan.json").read_text(encoding="utf-8")
    finally:
        rmtree(out)


def test_plan_off_file_pauses_everything():
    """M8: Habby's planUsage:false writes plan.off; the poller honours it on every poll."""
    out = tmpdir("plan-")
    try:
        mod, clock, calls = load_plugin(), Clock(), []
        cached = _snap()
        p = _poller(mod, out, clock, lambda bu: calls.append(1) or _snap(), cached=lambda urls: cached)
        out.mkdir(parents=True, exist_ok=True)
        (out / "plan.off").write_text("", encoding="utf-8")
        for _ in range(3):
            p.tick()
            clock.t += 10_000
        assert calls == [] and not (out / "plan.json").exists()
        (out / "plan.off").unlink()
        p.tick()
        assert (out / "plan.json").exists()
    finally:
        rmtree(out)


def test_default_token_reader_is_read_only():
    """_default_token only reads the Claude Code credentials; an expired token yields None (no refresh)."""
    mod = load_plugin()
    import agent.anthropic_credentials as A
    orig_read, orig_valid = A.read_claude_code_credentials, A.is_claude_code_token_valid
    try:
        A.read_claude_code_credentials = lambda: {"accessToken": "x", "refreshToken": "r", "expiresAt": 1}
        assert mod._default_token() is None
        A.read_claude_code_credentials = lambda: {"accessToken": "tok", "expiresAt": int(time.time() * 1000) + 3_600_000}
        assert mod._default_token() == "tok"
        A.read_claude_code_credentials = lambda: None
        assert mod._default_token() is None
    finally:
        A.read_claude_code_credentials, A.is_claude_code_token_valid = orig_read, orig_valid
