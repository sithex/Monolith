"""Fix-pass tests (habby-FIXES-metrics.md): heartbeat (M1), background-review fork bucket (M2), compression
rotation (M3), unknown cost (minor 2), subagent row input (minor 1), pending-child adoption (minor 6), wait
without session (minor 7), preview privacy (QA m3), pruning (minor 9), start failure (minor 11), write
backoff (minor 12), prompt history (QA m5)."""
from __future__ import annotations

import json
import os
import queue
import time

from _common import Feed, load_plugin, rmtree, tmpdir

SID = "20261020_000000_main01"
SID2 = "20261020_000500_cont01"
CHILD = "20261020_000001_child1"
COST = lambda m, p, b, i, o, cr, cw: round((i * 4 + o * 20 + cr * 0.2 + cw * 5) / 1e6, 6)  # noqa: E731


def _feed(out, **kw):
    mod = load_plugin()
    kw.setdefault("cost_fn", COST)
    kw.setdefault("pid", 4242)
    return mod, Feed(mod, out, **kw)


def _api(f, sid=SID, turn="turn_1", tools=1, finish="tool_use", model="claude-opus-5-5", prompt=90112):
    f.hook("post_api_request", session_id=sid, turn_id=turn, model=model, provider="anthropic",
           base_url="https://api.anthropic.com", platform="tui", api_call_count=1, api_duration=2.5,
           started_at=f.t, first_chunk_at=f.t + 0.6, finish_reason=finish,
           usage={"input_tokens": 12, "output_tokens": 345, "cache_read_tokens": 88000, "cache_write_tokens": 2100,
                  "prompt_tokens": prompt}, context_length=1_000_000, assistant_tool_call_count=tools)


def _user_turn(f, sid=SID, turn="turn_1"):
    f.hook("pre_llm_call", session_id=sid, turn_id=turn, model="claude-opus-5-5", platform="tui", user_message="x")
    f.hook("pre_api_request", session_id=sid, turn_id=turn, model="claude-opus-5-5", platform="tui")
    _api(f, sid, turn)


def _snap(out, pid=4242):
    return json.loads((out / f"snapshot-{pid}.json").read_text(encoding="utf-8"))


# ---- M1 heartbeat ---------------------------------------------------------------------------------
def test_heartbeat_restamps_while_busy_and_blocks_when_idle():
    out = tmpdir("hb-")
    try:
        mod, f = _feed(out)
        _user_turn(f)
        f.hook("on_human_input_request", kind="approval", request_id="r1", session_id=SID)
        clock = [f.t]
        f.m.write_snapshot(clock[0])
        t0 = clock[0]
        q = queue.SimpleQueue()
        assert 29 <= mod._next_timeout(f.m, t0) <= 30  # busy: wake up for the heartbeat
        stamps = []
        for dt in (30, 60, 90, 120, 130):
            clock[0] = t0 + dt
            q.put(("noop", clock[0], {}))  # unknown event kind: wakes the step without blocking, changes nothing
            mod._writer_step(f.m, q, clock=lambda: clock[0])
            stamps.append(round(mod._parse_iso(_snap(out)["written_at"]) - t0))
        assert stamps == [30, 60, 90, 120, 120], stamps  # every 30 s, never older than 30 s at t+130
        s = _snap(out)["sessions"][SID]
        assert s["status"] == "waiting" and s["waiting"]["kind"] == "approval"
        # idle -> no heartbeat, the writer blocks forever (zero CPU)
        f.hook("on_human_input_resolved", kind="approval", request_id="r1", session_id=SID, outcome="once")
        f.hook("on_session_end", session_id=SID, completed=True, failed=False, interrupted=False)
        f.m.write_snapshot(clock[0])
        assert mod._next_timeout(f.m, clock[0] + 500) is None
    finally:
        rmtree(out)


# ---- M2 background-review fork ----------------------------------------------------------------------
def test_background_fork_goes_to_background_bucket():
    out = tmpdir("fork-")
    try:
        mod, f = _feed(out)
        _user_turn(f)
        f.hook("pre_tool_call", session_id=SID, tool_name="terminal", tool_call_id="c1", turn_id="turn_1", args={"command": "ls"})
        f.hook("post_tool_call", session_id=SID, tool_name="terminal", tool_call_id="c1", turn_id="turn_1", duration_ms=5, status="ok")
        _api(f, tools=0, finish="stop")
        f.hook("on_session_end", session_id=SID, completed=True, failed=False, interrupted=False)
        before = f.m.snapshot()["sessions"][SID]
        # the review fork: same session_id, its own turn id, no pre_llm_call, no on_session_end
        fork = SID + ":review:abcd1234"
        f.hook("pre_api_request", session_id=SID, turn_id=fork, model="claude-opus-5-5", platform="tui")
        _api(f, turn=fork, tools=1)
        f.hook("pre_tool_call", session_id=SID, tool_name="memory", tool_call_id="m1", turn_id=fork, args={"action": "add"})
        f.hook("post_tool_call", session_id=SID, tool_name="memory", tool_call_id="m1", turn_id=fork, duration_ms=2, status="ok")
        f.hook("api_request_error", session_id=SID, turn_id=fork, error_type="X", status_code=500)
        _api(f, turn=fork, tools=0, finish="stop")
        s = f.m.snapshot()["sessions"][SID]
        assert s["status"] == "idle", s["status"]
        assert s["turn"] == before["turn"] and s["totals"] == before["totals"] and s["steps"] == before["steps"]
        b = s["background"]
        assert b["calls"] == 2 and b["tool_calls"] == 1 and b["output"] == 690 and b["cost_usd"] > 0
        # the next real user turn is foreground again
        _user_turn(f, turn="turn_2")
        s = f.m.snapshot()["sessions"][SID]
        assert s["status"] == "tool" and s["turn"]["calls"] == 1 and s["totals"]["calls"] == before["totals"]["calls"] + 1
    finally:
        rmtree(out)


def test_minimum_fallback_final_answer_is_idle():
    """No pre_llm_call ever seen (plugin loaded mid-turn / older Hermes): a final stop answer = idle."""
    out = tmpdir("fallback-")
    try:
        mod, f = _feed(out)
        f.hook("pre_api_request", session_id=SID, turn_id="t", model="m")
        _api(f, turn="t", tools=0, finish="stop")
        assert f.m.snapshot()["sessions"][SID]["status"] == "idle"
        f.hook("pre_api_request", session_id=SID, turn_id="t", model="m")
        _api(f, turn="t", tools=0, finish="length")
        assert f.m.snapshot()["sessions"][SID]["status"] == "thinking"
    finally:
        rmtree(out)


# ---- M3 compression rotation ------------------------------------------------------------------------
def test_compression_rotation_migrates_state():
    out = tmpdir("rot-")
    try:
        mod, f = _feed(out)
        _user_turn(f)
        f.hook("subagent_start", parent_session_id=SID, child_session_id=CHILD, child_subagent_id="sa-0",
               child_role="leaf", child_goal="audit")
        f.hook("pre_llm_call", session_id=CHILD, turn_id="ct1", model="claude-fable-5-1", platform="subagent")
        _api(f, sid=CHILD, turn="ct1")
        f.hook("pre_auxiliary_call", aux_task="compression", session_id=SID)
        f.hook("post_auxiliary_call", aux_task="compression", session_id=SID, error_type=None)
        # rotation: new session id, SAME turn id
        f.hook("pre_api_request", session_id=SID2, turn_id="turn_1", model="claude-opus-5-5", platform="tui")
        _api(f, sid=SID2, turn="turn_1")
        snap = f.m.snapshot()["sessions"]
        new, old = snap[SID2], snap[SID]
        assert new["continued_from"] == SID and old["continued_to"] == SID2
        assert new["totals"]["calls"] == 2 and new["turn"]["calls"] == 2 and new["compressions"] == 1
        assert [r["session_id"] for r in new["subagents"]] == [CHILD] and old["subagents"] == []
        assert snap[CHILD]["parent_session_id"] == SID2 and old["status"] == "idle"
        # late events with the old id are routed to the continuation
        f.hook("pre_tool_call", session_id=SID, tool_name="terminal", tool_call_id="x", turn_id="turn_1", args={})
        assert f.m.snapshot()["sessions"][SID2]["current_tool"]["name"] == "terminal"
        # the child's spend still lands in the (re-pointed) index row
        _api(f, sid=CHILD, turn="ct1")
        assert f.m.snapshot()["sessions"][SID2]["subagents"][0]["output"] == 690
        # a second rotation chains (old -> mid -> new)
        f.hook("pre_api_request", session_id="third", turn_id="turn_1", model="m")
        assert f.m.snapshot()["sessions"]["third"]["continued_from"] == SID2
        f.hook("on_session_end", session_id=SID, completed=True, failed=False, interrupted=False)
        assert f.m.snapshot()["sessions"]["third"]["status"] == "idle"
    finally:
        rmtree(out)


# ---- minors ---------------------------------------------------------------------------------------
def test_unknown_cost_is_null_not_zero():
    out = tmpdir("cost-")
    try:
        mod, f = _feed(out, cost_fn=lambda *a: None)
        _user_turn(f)
        s = f.m.snapshot()["sessions"][SID]
        assert s["turn"]["cost_usd"] is None and s["totals"]["cost_usd"] is None
    finally:
        rmtree(out)


def test_pending_child_needs_subagent_platform_and_expires():
    out = tmpdir("pend-")
    try:
        mod, f = _feed(out)
        _user_turn(f)
        f.hook("subagent_start", parent_session_id=SID, child_session_id=None, child_subagent_id="sa-1", child_goal="g")
        f.hook("pre_tool_call", session_id="unrelated", tool_name="t", tool_call_id="u", args={})  # platform None
        f.hook("on_session_start", session_id="gateway_sess", model="m", platform="telegram")
        snap = f.m.snapshot()["sessions"]
        assert snap["unrelated"]["parent_session_id"] is None and snap["gateway_sess"]["parent_session_id"] is None
        f.t += 31  # pending entry expires after 30 s
        f.hook("on_session_start", session_id="late", model="m", platform="subagent")
        assert f.m.snapshot()["sessions"]["late"]["parent_session_id"] is None
        f.hook("subagent_start", parent_session_id=SID, child_session_id=None, child_subagent_id="sa-2", child_goal="g")
        f.hook("on_session_start", session_id="kid", model="m", platform="subagent")
        assert f.m.snapshot()["sessions"]["kid"]["parent_session_id"] == SID
    finally:
        rmtree(out)


def test_wait_or_aux_without_session_changes_no_status():
    out = tmpdir("nosid-")
    try:
        mod, f = _feed(out)
        _user_turn(f)
        f.hook("on_human_input_request", kind="approval", request_id="r", session_id="")
        f.hook("pre_auxiliary_call", aux_task="compression", session_id=None)
        s = f.m.snapshot()["sessions"][SID]
        assert s["status"] == "tool" and s["waiting"] is None
        assert [e["event"] for e in f.m.events][-2:] == ["wait_start", "compaction_start"]
    finally:
        rmtree(out)


def test_preview_privacy_rules():
    out = tmpdir("prev-")
    try:
        mod, f = _feed(out)
        _user_turn(f)
        cases = [
            ({"path": "C:\\Users\\Nick\\private\\taxes-2026.xlsx"}, "taxes-2026.xlsx"),
            ({"file_path": "/fixture/secret dir/notes.md"}, "notes.md"),
            ({"query": "how to hide a body", "url": "https://x.example/private"}, ""),
            ({"command": "mysql -uroot -pS3cretPw db"}, "mysql -uroot -p*** db"),
            ({"command": "net user bob Hunter22 /add"}, "net user bob *** /add"),
            ({"command": "mkdir -p build && echo password hunter2"}, "mkdir -p build && echo password ***"),
        ]
        got = []
        for i, (args, _) in enumerate(cases):
            f.hook("pre_tool_call", session_id=SID, tool_name="t", tool_call_id=f"p{i}", turn_id="turn_1", args=args)
            got.append(f.m.snapshot()["sessions"][SID]["current_tool"]["preview"])
        assert got == [c[1] for c in cases], got
        f.hook("subagent_start", parent_session_id=SID, child_session_id="c9", child_subagent_id="s",
               child_goal="Rotate the password Tr0ub4dor for admin")
        g = f.m.snapshot()["sessions"]["c9"]["goal"]
        assert "Tr0ub4dor" not in g, g
        mod.PREVIEW_ON = False
        f.hook("pre_tool_call", session_id=SID, tool_name="t", tool_call_id="off", turn_id="turn_1", args={"command": "ls"})
        f.hook("subagent_start", parent_session_id=SID, child_session_id="c10", child_subagent_id="s2", child_goal="g")
        snap = f.m.snapshot()["sessions"]
        assert snap[SID]["current_tool"]["preview"] == "" and snap["c10"]["goal"] == ""
    finally:
        rmtree(out)


def test_prune_dead_pids_and_old_files():
    out = tmpdir("prune-")
    try:
        mod = load_plugin()
        now = time.time()
        for n in ("snapshot-1.json", "events-1.jsonl", "events-1.1.jsonl", "snapshot-2.json", "snapshot-3.json",
                  "snapshot-4.json", "snapshot-9.json", "plan.json", "events.jsonl"):
            (out / n).write_text("{}", encoding="utf-8")
        old = now - 2 * 3600
        os.utime(out / "snapshot-3.json", (old, old))
        alive = {1: False, 2: True, 3: None, 4: None}
        removed = sorted(mod.prune_files(out, own_pid=9, now=now, alive_fn=lambda p: alive.get(p)))
        assert removed == ["events-1.1.jsonl", "events-1.jsonl", "snapshot-1.json", "snapshot-3.json"], removed
        assert sorted(p.name for p in out.iterdir()) == ["events.jsonl", "plan.json", "snapshot-2.json",
                                                         "snapshot-4.json", "snapshot-9.json"]
        assert mod._pid_alive(os.getpid()) is True and mod._pid_alive(0) is None
    finally:
        rmtree(out)


def test_start_failure_disables_queue():
    mod = load_plugin()

    def boom():
        raise OSError("no home")
    mod._out_dir = boom
    mod._h_pre_tool_call(session_id=SID, tool_name="t", tool_call_id="x", args={})
    mod._h_post_tool_call(session_id=SID, tool_name="t", tool_call_id="x")
    assert mod._disabled is True and mod._started is False and mod._Q.qsize() == 0


def test_write_failure_backoff_and_closing():
    out = tmpdir("wfail-")
    try:
        mod, f = _feed(out)
        _user_turn(f)
        f.m.dirty = True
        f.m.last_snapshot = 100.0
        f.m.write_failures = 2
        assert abs(mod._next_timeout(f.m, 100.0) - 0.25) < 1e-9
        f.m.write_failures = 5
        assert abs(mod._next_timeout(f.m, 100.0) - 5.0) < 1e-9
        mod._closing = True  # after atexit cleanup the writer never re-creates the file
        f.m.write_failures = 0
        mod._writer_step(f.m, queue.SimpleQueue(), clock=lambda: 200.0)
        assert not (out / "snapshot-4242.json").exists()
    finally:
        rmtree(out)


def test_prompt_history_last_10():
    out = tmpdir("ph-")
    try:
        mod, f = _feed(out)
        f.hook("pre_llm_call", session_id=SID, turn_id="t", model="m")
        for i in range(12):
            _api(f, turn="t", prompt=100_000 + i * 1000)
        ph = f.m.snapshot()["sessions"][SID]["prompt_history"]
        assert ph == [100_000 + i * 1000 for i in range(2, 12)], ph
    finally:
        rmtree(out)


def test_linux_pid_alive_dead_pid():
    """Linux port: /proc-based liveness; a reaped child's pid reads as dead, never signalled."""
    import subprocess
    mod = load_plugin()
    p = subprocess.Popen(["true"])
    p.wait()
    assert mod._pid_alive(p.pid) is False
    assert mod._pid_alive(1) is True and mod._pid_alive(-5) is None


def test_api_request_error_current_payload():
    """Hermes 0.22.6 api_request_error: error={"type","message"}, no top-level error_type; message never kept."""
    out = tmpdir("aerr-")
    try:
        mod, f = _feed(out)
        f.hook("pre_llm_call", session_id=SID, turn_id="t", model="m")
        f.hook("api_request_error", session_id=SID, turn_id="t", status_code=529, retryable=True,
               error={"type": "OverloadedError", "message": "ERROR-MSG-SENTINEL"}, reason="overloaded")
        ev = [e for e in f.m.events if e["event"] == "api_error"]
        assert ev and ev[-1]["error_type"] == "OverloadedError" and ev[-1]["status_code"] == 529, ev
        assert "ERROR-MSG-SENTINEL" not in json.dumps(f.m.events)
        assert f.m.snapshot()["sessions"][SID]["status"] == "error"
    finally:
        rmtree(out)
