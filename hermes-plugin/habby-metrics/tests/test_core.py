"""Unit tests: hook payload -> snapshot shape, child sessions (R10), statuses, redaction, privacy,
rotation, atomic snapshots, hooks never raise. Plain functions (pytest-compatible names); run with
tests/run_tests.py under the Hermes python."""
from __future__ import annotations

import json
import os
import threading
import time
from pathlib import Path

from _common import Feed, load_plugin, rmtree, tmpdir

SID = "20261020_000000_main01"
CHILD = "20261020_000001_child1"
SECRET_KEY = "sk-ant-api03-" + "A1b2C3d4E5f6G7h8I9j0" * 4          # looks like a real Anthropic key
SENTINELS = ("ASSISTANT-TEXT-SENTINEL", "RESPONSE-SENTINEL", "TOOL-RESULT-SENTINEL", "USER-MSG-SENTINEL",
             "SYSPROMPT-SENTINEL", "HUMAN-PROMPT-SENTINEL", "APPROVAL-CMD-SENTINEL", "FILE-CONTENT-SENTINEL",
             "CHILD-SUMMARY-SENTINEL", "ERROR-BODY-SENTINEL", SECRET_KEY)


def _turn(f: Feed, sid=SID, turn="turn_1", model="claude-opus-5-5"):
    f.hook("on_session_start", session_id=sid, model=model, platform="tui")
    f.hook("pre_api_request", session_id=sid, turn_id=turn, model=model, platform="tui", approx_input_tokens=50_000,
           user_message="USER-MSG-SENTINEL", conversation_history=["USER-MSG-SENTINEL"],
           request_messages=["USER-MSG-SENTINEL"], system_prompt="SYSPROMPT-SENTINEL")
    f.hook("post_api_request", session_id=sid, turn_id=turn, model=model, provider="anthropic",
           base_url="https://api.anthropic.com", platform="tui", api_call_count=1, api_duration=2.5,
           started_at=f.t, ended_at=f.t + 2.5, first_chunk_at=f.t + 0.6, finish_reason="tool_use",
           usage={"input_tokens": 12, "output_tokens": 345, "cache_read_tokens": 88000, "cache_write_tokens": 2100,
                  "prompt_tokens": 90112, "total_tokens": 90457},
           context_length=1_000_000, assistant_message="ASSISTANT-TEXT-SENTINEL",
           response={"content": "RESPONSE-SENTINEL"}, assistant_tool_call_count=1)


def _feed(out, **kw):
    mod = load_plugin()
    kw.setdefault("cost_fn", lambda m, p, b, i, o, cr, cw: round((i * 4 + o * 20 + cr * 0.2 + cw * 5) / 1e6, 6))
    kw.setdefault("pid", 4242)
    return mod, Feed(mod, out, **kw)


def test_snapshot_shape():
    out = tmpdir("shape-")
    try:
        mod, f = _feed(out)
        _turn(f)
        f.hook("pre_tool_call", session_id=SID, tool_name="terminal", tool_call_id="c1", turn_id="turn_1",
               args={"command": "git status --short"})
        f.flush()
        snap = json.loads((out / "snapshot-4242.json").read_text(encoding="utf-8"))
        assert set(snap) >= {"pid", "written_at", "sessions"} and snap["pid"] == 4242
        assert snap["written_at"].endswith("Z")
        s = snap["sessions"][SID]
        need = {"status", "model", "turn", "current_tool", "steps", "last_call", "context", "waiting", "subagents",
                "compressions", "totals", "parent_session_id", "role", "goal"}
        assert need <= set(s), need - set(s)
        assert not [k for k in s if k.startswith("_")]
        assert s["status"] == "tool" and s["model"] == "claude-opus-5-5"
        assert set(s["turn"]) >= {"started_at", "calls", "input", "output", "cache_read", "cache_write", "cost_usd"}
        assert s["turn"]["calls"] == 1 and s["turn"]["output"] == 345 and s["turn"]["cache_read"] == 88000
        assert abs(s["turn"]["cost_usd"] - (12 * 4 + 345 * 20 + 88000 * 0.2 + 2100 * 5) / 1e6) < 1e-6
        assert s["current_tool"]["name"] == "terminal" and s["current_tool"]["preview"] == "git status --short"
        assert s["current_tool"]["started_at"].endswith("Z")
        assert s["steps"] == [{"tool": "terminal", "status": "running", "ms": None}]
        lc = s["last_call"]
        assert set(lc) >= {"in", "out", "cache_read", "cache_write", "prompt", "ttfb_ms", "latency_ms", "at"}
        assert lc["prompt"] == 90112 and lc["ttfb_ms"] == 600 and lc["latency_ms"] == 2500
        assert s["context"] == {"used": 90112, "max": 1_000_000, "pct": 9.0, "estimated": False}
        assert s["waiting"] is None and s["subagents"] == [] and s["compressions"] == 0
        t = s["totals"]
        assert set(t) == {"calls", "input", "output", "cache_read", "cache_write", "cost_usd", "tool_calls", "tools_by_name"}
        assert t["tool_calls"] == 1 and t["tools_by_name"] == {"terminal": 1}
        f.hook("post_tool_call", session_id=SID, tool_name="terminal", tool_call_id="c1", duration_ms=840,
               status="ok", result="TOOL-RESULT-SENTINEL", args={"command": "git status --short"})
        for i in range(6):
            f.hook("pre_tool_call", session_id=SID, tool_name="read_file", tool_call_id=f"r{i}", args={"path": f"a{i}.py"})
            f.hook("post_tool_call", session_id=SID, tool_name="read_file", tool_call_id=f"r{i}", duration_ms=5,
                   status="error" if i == 5 else "ok", result="FILE-CONTENT-SENTINEL")
        f.hook("on_session_end", session_id=SID, completed=True, failed=False, interrupted=False)
        f.flush()
        s = json.loads((out / "snapshot-4242.json").read_text(encoding="utf-8"))["sessions"][SID]
        assert s["status"] == "idle" and s["current_tool"] is None and s["turn"]["ended_at"]
        assert len(s["steps"]) == 5 and s["steps"][-1] == {"tool": "read_file", "status": "error", "ms": 5}
        assert s["totals"]["tools_by_name"] == {"terminal": 1, "read_file": 6} and s["totals"]["tool_calls"] == 7
        # second turn: turn counters reset, totals keep accumulating
        _turn(f, turn="turn_2")
        f.flush()
        s = json.loads((out / "snapshot-4242.json").read_text(encoding="utf-8"))["sessions"][SID]
        assert s["turn"]["calls"] == 1 and s["totals"]["calls"] == 2 and s["steps"] == []
        assert s["totals"]["output"] == 690
    finally:
        rmtree(out)


def test_events_jsonl_shape():
    out = tmpdir("events-")
    try:
        mod, f = _feed(out)
        _turn(f)
        f.hook("pre_tool_call", session_id=SID, tool_name="terminal", tool_call_id="c1", args={"command": "ls"})
        f.hook("post_tool_call", session_id=SID, tool_name="terminal", tool_call_id="c1", duration_ms=3, status="ok")
        f.hook("on_session_end", session_id=SID, completed=True, failed=False, interrupted=False)
        f.flush()
        lines = (out / "events-4242.jsonl").read_text(encoding="utf-8").splitlines()
        evs = [json.loads(x) for x in lines]
        assert [e["event"] for e in evs] == ["session_start", "turn_start", "api_call", "tool_start", "tool_end", "turn_end"]
        for e in evs:
            assert {"ts", "pid", "session_id", "event"} <= set(e) and e["pid"] == 4242 and e["session_id"] == SID
        assert "preview" not in json.dumps(evs) and "ls" not in [v for e in evs for v in e.values()]
    finally:
        rmtree(out)


def test_child_sessions_full_state():
    """R10: every child session carries full state + parent_session_id/role/goal; parent keeps the index."""
    out = tmpdir("child-")
    try:
        mod, f = _feed(out)
        _turn(f)
        f.hook("subagent_start", parent_session_id=SID, parent_turn_id="turn_1", parent_subagent_id=None,
               child_session_id=CHILD, child_subagent_id="sa-0", child_role="leaf",
               child_goal="Audit the plugin loader and report " + SECRET_KEY + " findings " * 20)
        f.hook("on_session_start", session_id=CHILD, model="claude-fable-5-1", platform="subagent")
        _turn(f, sid=CHILD, turn="ct1", model="claude-fable-5-1")
        f.hook("pre_tool_call", session_id=CHILD, tool_name="search_files", tool_call_id="k1", args={"pattern": "def register"})
        f.flush()
        snap = json.loads((out / "snapshot-4242.json").read_text(encoding="utf-8"))
        c = snap["sessions"][CHILD]
        assert c["parent_session_id"] == SID and c["role"] == "leaf" and c["subagent_id"] == "sa-0"
        assert len(c["goal"]) <= 80 and SECRET_KEY not in c["goal"] and c["goal"].startswith("Audit the plugin")
        assert c["status"] == "tool" and c["current_tool"]["name"] == "search_files"
        assert c["model"] == "claude-fable-5-1" and c["context"]["pct"] == 9.0 and c["totals"]["calls"] == 1
        assert c["totals"]["tools_by_name"] == {"search_files": 1}
        rows = snap["sessions"][SID]["subagents"]
        assert len(rows) == 1
        r = rows[0]
        assert set(r) >= {"id", "goal", "role", "model", "status", "started_at", "ms", "tools", "input", "output", "cost_usd"}
        assert r["session_id"] == CHILD and r["status"] == "running" and r["tools"] == 1
        assert r["input"] == 12 and r["cache_read"] == 88000 and r["cache_write"] == 2100 and r["output"] == 345 and r["model"] == "claude-fable-5-1" and r["cost_usd"] > 0
        f.hook("subagent_stop", parent_session_id=SID, parent_turn_id="turn_1", child_session_id=CHILD, child_role="leaf",
               child_summary="CHILD-SUMMARY-SENTINEL", child_status="completed", tool_call_history=[{"t": 1}, {"t": 2}],
               duration_ms=4321)
        f.flush()
        snap = json.loads((out / "snapshot-4242.json").read_text(encoding="utf-8"))
        r = snap["sessions"][SID]["subagents"][0]
        assert r["status"] == "completed" and r["ms"] == 4321 and r["tools"] == 2
        assert snap["sessions"][CHILD]["status"] == "idle" and snap["sessions"][CHILD]["current_tool"] is None
        # child_session_id unknown at subagent_start -> adopted by the first new 'subagent' session
        f.hook("subagent_start", parent_session_id=SID, child_session_id=None, child_subagent_id="sa-1", child_role=None,
               child_goal="second")
        f.hook("on_session_start", session_id="late_child", model="m", platform="subagent")
        f.flush()
        snap = json.loads((out / "snapshot-4242.json").read_text(encoding="utf-8"))
        assert snap["sessions"]["late_child"]["parent_session_id"] == SID
        assert snap["sessions"][SID]["subagents"][1]["session_id"] == "late_child"
    finally:
        rmtree(out)


def test_status_waiting_compacting_error():
    out = tmpdir("status-")
    try:
        mod, f = _feed(out)
        _turn(f)
        f.hook("pre_tool_call", session_id=SID, tool_name="terminal", tool_call_id="c1", args={"command": "rm -rf x"})
        f.hook("pre_approval_request", command="APPROVAL-CMD-SENTINEL", description="d", pattern_key="recursive_delete",
               pattern_keys=["recursive_delete"], session_key="k", surface="cli")
        f.hook("on_human_input_request", kind="approval", request_id="req1", session_id=SID, session_key="k",
               platform="tui", prompt="HUMAN-PROMPT-SENTINEL")
        s = f.m.snapshot()["sessions"][SID]
        assert s["status"] == "waiting" and s["waiting"]["kind"] == "approval" and s["waiting"]["since"]
        assert set(s["waiting"]) == {"kind", "since"}
        f.hook("on_human_input_resolved", kind="approval", request_id="req1", session_id="", outcome="deny",
               prompt="HUMAN-PROMPT-SENTINEL")  # session_id missing: matched by request_id
        f.hook("post_approval_response", command="APPROVAL-CMD-SENTINEL", choice="deny", surface="cli")
        s = f.m.snapshot()["sessions"][SID]
        assert s["waiting"] is None and s["status"] == "tool"
        f.hook("post_tool_call", session_id=SID, tool_name="terminal", tool_call_id="c1", duration_ms=1, status="error")
        assert f.m.snapshot()["sessions"][SID]["status"] == "thinking"
        f.hook("on_human_input_request", kind="clarify", request_id="req2", session_id=SID, prompt="HUMAN-PROMPT-SENTINEL")
        assert f.m.snapshot()["sessions"][SID]["waiting"]["kind"] == "clarify"
        f.hook("on_human_input_resolved", kind="clarify", request_id="req2", session_id=SID, outcome="answered")
        f.hook("on_human_input_request", kind="sudo", request_id="req3", session_id=SID)
        assert f.m.snapshot()["sessions"][SID]["waiting"]["kind"] == "sudo"
        f.hook("on_human_input_resolved", kind="sudo", request_id="req3", session_id=SID, outcome="cancelled")
        f.hook("pre_auxiliary_call", aux_task="compression", session_id=SID, request_messages=["USER-MSG-SENTINEL"])
        assert f.m.snapshot()["sessions"][SID]["status"] == "compacting"
        f.hook("post_auxiliary_call", aux_task="compression", session_id=SID, error_type=None,
               response={"content": "RESPONSE-SENTINEL"})
        s = f.m.snapshot()["sessions"][SID]
        assert s["status"] == "thinking" and s["compressions"] == 1
        f.hook("pre_auxiliary_call", aux_task="title_generation", session_id=SID)  # ignored
        assert f.m.snapshot()["sessions"][SID]["status"] == "thinking"
        f.hook("api_request_error", session_id=SID, error_type="RateLimitError", status_code=429, retryable=True,
               error_message="ERROR-BODY-SENTINEL")
        assert f.m.snapshot()["sessions"][SID]["status"] == "error"
        f.hook("pre_api_request", session_id=SID, turn_id="turn_1", model="claude-opus-5-5")
        assert f.m.snapshot()["sessions"][SID]["status"] == "thinking"
        f.hook("on_session_end", session_id=SID, completed=False, failed=True, interrupted=False)
        assert f.m.snapshot()["sessions"][SID]["status"] == "error"
        f.flush()
        ev = [json.loads(x)["event"] for x in (out / "events-4242.jsonl").read_text(encoding="utf-8").splitlines()]
        for k in ("approval_request", "approval_response", "wait_start", "wait_end", "compaction_start",
                  "compaction_end", "api_error", "turn_end"):
            assert k in ev, k
    finally:
        rmtree(out)


def test_no_message_content_on_disk():
    out = tmpdir("privacy-")
    try:
        test_dir = out
        mod, f = _feed(test_dir)
        _turn(f)
        f.hook("pre_tool_call", session_id=SID, tool_name="terminal", tool_call_id="c1",
               args={"command": f"curl -H 'x-api-key: {SECRET_KEY}' https://example.com", "content": "FILE-CONTENT-SENTINEL"})
        f.hook("post_tool_call", session_id=SID, tool_name="terminal", tool_call_id="c1", duration_ms=1, status="ok",
               result="TOOL-RESULT-SENTINEL", error_message="ERROR-BODY-SENTINEL")
        f.hook("pre_approval_request", command="APPROVAL-CMD-SENTINEL", description="APPROVAL-CMD-SENTINEL",
               pattern_key="p", session_key="k", surface="cli")
        f.hook("on_human_input_request", kind="approval", request_id="r", session_id=SID, prompt="HUMAN-PROMPT-SENTINEL")
        f.hook("on_human_input_resolved", kind="approval", request_id="r", session_id=SID, prompt="HUMAN-PROMPT-SENTINEL",
               outcome="once")
        f.hook("subagent_start", parent_session_id=SID, child_session_id=CHILD, child_subagent_id="sa", child_role=None,
               child_goal="goal")
        f.hook("subagent_stop", parent_session_id=SID, child_session_id=CHILD, child_summary="CHILD-SUMMARY-SENTINEL",
               child_status="completed", tool_call_history=[], duration_ms=1)
        f.hook("api_request_error", session_id=SID, error_type="X", error_message="ERROR-BODY-SENTINEL")
        f.hook("on_session_end", session_id=SID, completed=True, failed=False, interrupted=False)
        f.flush()
        blob = "".join(p.read_text(encoding="utf-8") for p in test_dir.iterdir() if p.is_file())
        leaked = [s for s in SENTINELS if s in blob]
        assert not leaked, leaked
        snap = json.loads((test_dir / "snapshot-4242.json").read_text(encoding="utf-8"))
        assert len(snap["sessions"][SID]["steps"][0]["tool"]) <= 64
    finally:
        rmtree(out)


def test_redaction_real_hermes():
    """The preview goes through Hermes's own agent.redact (force=True) and is cut to 60 chars."""
    mod = load_plugin()
    p = mod._redact(f"export ANTHROPIC_API_KEY={SECRET_KEY} && run", 60)
    assert SECRET_KEY not in p and SECRET_KEY[:20] not in p, p
    assert len(p) <= 60
    long = mod._redact("echo " + "x" * 500, 60)
    assert len(long) == 60 and long.endswith("\u2026")
    assert mod._redact("line1\nline2\tz", 60) == "line1 line2 z"
    g = mod._redact("Investigate token ghp_" + "Z" * 36 + " in repo", 80)
    assert "ghp_" + "Z" * 36 not in g, g
    # a redaction failure never falls back to raw text
    import agent.redact as R
    orig = R.redact_sensitive_text
    R.redact_sensitive_text = lambda *a, **k: (_ for _ in ()).throw(RuntimeError("boom"))
    try:
        assert mod._redact("SECRET-RAW", 60) == ""
    finally:
        R.redact_sensitive_text = orig


def test_events_rotation_per_pid():
    """minor 10: one events file per pid, rotated by its only writer (no shared lock)."""
    out = tmpdir("rotate-")
    try:
        mod, f = _feed(out)
        mod.EVENTS_MAX_BYTES = 4000
        for i in range(400):
            f.hook("pre_tool_call", session_id=SID, tool_name="terminal", tool_call_id=f"c{i}", args={"command": "x"})
            f.hook("post_tool_call", session_id=SID, tool_name="terminal", tool_call_id=f"c{i}", duration_ms=1, status="ok")
            if i % 10 == 0:
                f.m.flush_events()
        f.m.flush_events()
        main, old = out / "events-4242.jsonl", out / "events-4242.1.jsonl"
        assert old.exists() and main.exists()
        assert old.stat().st_size >= 4000 and main.stat().st_size < 4000 + 3000
        for p in (main, old):
            for line in p.read_text(encoding="utf-8").splitlines():
                json.loads(line)
        assert not [p for p in out.iterdir() if p.name.endswith(".lock") or p.name == "events.jsonl"]
    finally:
        rmtree(out)


def test_atomic_snapshot_under_concurrent_reads():
    out = tmpdir("atomic-")
    try:
        mod, f = _feed(out)
        _turn(f)
        f.flush()
        stop = threading.Event()
        bad, reads, busy = [], [0], [0]

        def reader():
            p = out / "snapshot-4242.json"
            while not stop.is_set():
                try:
                    json.loads(p.read_text(encoding="utf-8"))
                    reads[0] += 1
                except PermissionError:
                    busy[0] += 1
                except ValueError as e:
                    bad.append(repr(e))
        th = threading.Thread(target=reader)
        th.start()
        write_errors = 0
        for i in range(300):
            f.hook("pre_tool_call", session_id=SID, tool_name="t", tool_call_id=f"c{i}", args={})
            try:
                f.m.write_snapshot(f.t)
            except OSError:
                write_errors += 1
        stop.set()
        th.join()
        assert not bad, bad[:3]
        assert reads[0] > 0
        assert not [p for p in out.iterdir() if p.name.endswith(".tmp")]
        print(f"    atomic: reads={reads[0]} sharing-violations(reader)={busy[0]} writer-retries-failed={write_errors}")
    finally:
        rmtree(out)


def _node_reader_run(gap_ms: str, write_gap_s: float):
    import shutil
    import subprocess
    node = shutil.which("node")
    if not node:
        print("    node not found: skipped")
        return None
    out = tmpdir("atomic-node-")
    mod, f = _feed(out)
    _turn(f)
    f.flush()
    p = subprocess.Popen([node, str(Path(__file__).with_name("node_reader.cjs")), str(out / "snapshot-4242.json"),
                          "3000", gap_ms], stdout=subprocess.PIPE, text=True)
    write_errors, writes, t_end = 0, 0, time.time() + 2.8
    while time.time() < t_end:
        f.hook("pre_tool_call", session_id=SID, tool_name="t", tool_call_id=f"c{writes}", args={})
        try:
            f.m.write_snapshot(f.t)
        except OSError:
            write_errors += 1
        writes += 1
        if write_gap_s:
            time.sleep(write_gap_s)
    res = json.loads(p.communicate(timeout=30)[0].strip().splitlines()[-1])
    return out, f, res, writes, write_errors


def test_atomic_snapshot_node_reader_realistic():
    """Habby-like reader (Node fs.readFileSync every 100 ms = 10x Habby's 1 s tick) vs. the 4 Hz writer:
    no torn read, no deferred write."""
    r = _node_reader_run("100", 0.25)
    if r is None:
        return
    out, f, res, writes, errs = r
    try:
        print(f"    node reader 10 Hz vs writer 4 Hz: {res} writes={writes} deferred={errs}")
        assert res["parseErrors"] == 0 and res["openErrors"] == 0 and res["reads"] > 10 and errs == 0, (res, errs)
    finally:
        rmtree(out)


def test_atomic_snapshot_node_reader():
    """Worst case: a tight-loop Node reader. No read may ever see a partial file."""
    r = _node_reader_run("0", 0.0)
    if r is None:
        return
    out, f, res, writes, write_errors = r
    try:
        print(f"    node reader (tight loop, worst case): {res} python writes={writes} "
              f"writes_deferred_after_retries={write_errors}")
        # MoveFileEx cannot replace a file another process holds open (even with FILE_SHARE_DELETE), so a
        # tight-loop reader can defer some writes; the writer stays dirty and retries on its next 250 ms tick.
        # What must never happen is a partial/torn read.
        assert res["parseErrors"] == 0 and res["reads"] > 0, res
        f.m.write_snapshot(f.t)  # reader gone -> succeeds
        assert json.loads((out / "snapshot-4242.json").read_text(encoding="utf-8"))["pid"] == 4242
        assert not [x for x in out.iterdir() if x.name.endswith(".tmp")]
    finally:
        rmtree(out)


def test_hooks_never_raise_and_return_none():
    mod = load_plugin()

    def boom(*a, **k):
        raise RuntimeError("queue broken")
    mod._put = boom
    for name, fn in mod.HOOKS:
        assert fn() is None
        assert fn(usage=object(), args=None, child_goal=123, tool_call_history=5, aux_task="compression") is None
    # garbage payloads straight into Metrics never escape apply() in the writer (it wraps apply in try/except),
    # and the hooks themselves tolerate any shape:
    mod2 = load_plugin()
    out = tmpdir("garbage-")
    try:
        f = Feed(mod2, out, pid=1)
        for name, fn in mod2.HOOKS:
            try:
                fn(session_id=SID, usage="weird", args=["list"], duration_ms="x", child_status=7, kind=None,
                   context_length="1e6", api_duration="slow", first_chunk_at="?", started_at=None)
            except Exception as e:  # the Feed routes into apply() synchronously; must still not raise
                raise AssertionError(f"{name}: {e!r}")
        f.flush()
        json.loads((out / "snapshot-1.json").read_text(encoding="utf-8"))
    finally:
        rmtree(out)


def test_session_cap_prunes_idle_only():
    out = tmpdir("cap-")
    try:
        mod, f = _feed(out)
        for i in range(mod.MAX_SESSIONS + 10):
            f.hook("on_session_start", session_id=f"s{i}", model="m", platform="cron")
        f.hook("pre_api_request", session_id="s0", turn_id="t", model="m")  # s0 busy
        for i in range(5):
            f.hook("on_session_start", session_id=f"x{i}", model="m", platform="cron")
        assert len(f.m.sessions) <= mod.MAX_SESSIONS and "s0" in f.m.sessions
    finally:
        rmtree(out)
