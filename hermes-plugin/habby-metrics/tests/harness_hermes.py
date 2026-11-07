"""Integration harness (pattern of metrics-spike/s4-harness.py): load habby-metrics through Hermes's REAL plugin
manager in a SCRATCH HERMES_HOME, drive real call sites, check the files, measure hook cost.

    <hermes python> -I tests/harness_hermes.py enabled|not-enabled

Never touches the live home: inherited HERMES_* env is stripped, HERMES_HOME = scratch, plan fetch disabled,
gateway lock dir isolated. Evidence -> <scratch>/metrics-plugin/ev/harness-<mode>.json
"""
from __future__ import annotations

import json
import os
import shutil
import statistics
import sys
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from _common import EV, PLUGIN_DIR, SCRATCH, isolate_env  # noqa: E402

MODE = sys.argv[1] if len(sys.argv) > 1 else "enabled"
HOME = SCRATCH / f"harness-home-{MODE}"
shutil.rmtree(HOME, ignore_errors=True)
isolate_env(HOME)
(HOME / "plugins").mkdir(parents=True)
shutil.copytree(PLUGIN_DIR, HOME / "plugins" / "habby-metrics", ignore=shutil.ignore_patterns("tests", "__pycache__", "*.sh"))
(HOME / "config.yaml").write_text("plugins:\n  enabled:\n    - habby-metrics\n" if MODE == "enabled"
                                  else "plugins:\n  clone_timeout_seconds: 300\n", encoding="utf-8")
os.environ["HERMES_INTERACTIVE"] = "1"
import hermes_bootstrap  # noqa: E402,F401

R: dict = {"mode": MODE, "home": str(HOME), "python": sys.version.split()[0], "checks": {}}
FAILS = []


def check(name, ok, data=None):
    R["checks"][name] = {"ok": bool(ok), "data": data}
    if not ok:
        FAILS.append(name)
    print(("PASS " if ok else "FAIL ") + name, (json.dumps(data, default=str)[:240] if data is not None else ""))


def habby_threads():
    return sorted(t.name for t in threading.enumerate() if t.name.startswith("habby-metrics"))


t0 = time.perf_counter()
from hermes_cli import plugins as P  # noqa: E402
from hermes_cli.lifecycle import has_hook, invoke_hook  # noqa: E402
P.discover_plugins()
R["discover_ms"] = round((time.perf_counter() - t0) * 1000, 1)
pm = P.get_plugin_manager()
info = {k: {"enabled": v.enabled, "error": v.error, "source": v.manifest.source} for k, v in pm._plugins.items() if "habby" in k}
check("plugin_listed", "habby-metrics" in info, info)
loaded = info.get("habby-metrics", {}).get("enabled") is True
HOOKS = ("pre_llm_call", "pre_api_request", "post_api_request", "api_request_error", "pre_tool_call", "post_tool_call", "subagent_start",
         "subagent_stop", "pre_approval_request", "post_approval_response", "on_human_input_request",
         "on_human_input_resolved", "on_session_start", "on_session_end", "pre_auxiliary_call", "post_auxiliary_call")
mod = next((m for m in list(sys.modules.values())
            if getattr(m, "__file__", None) and Path(m.__file__).parent == HOME / "plugins" / "habby-metrics"), None)
if MODE != "enabled":
    check("not_loaded_without_plugins_enabled", not loaded and mod is None and not any(
        any(getattr(cb, "__module__", "").endswith("habby-metrics") for cb in pm._hooks.get(h, [])) for h in HOOKS),
        {"loaded": loaded})
    EV.mkdir(parents=True, exist_ok=True)
    (EV / f"harness-{MODE}.json").write_text(json.dumps(R, indent=1, default=str), encoding="utf-8")
    sys.exit(1 if FAILS else 0)

check("plugin_loaded", loaded and mod is not None, info)
ours = {h: sum(1 for cb in pm._hooks.get(h, []) if getattr(cb, "__module__", None) == mod.__name__) for h in HOOKS}
check("hooks_registered", all(v == 1 for v in ours.values()), ours)
check("lazy_no_threads_after_load", habby_threads() == [] and mod._started is False, habby_threads())
OUT = HOME / "cache" / "habby-metrics"
check("lazy_no_files_after_load", not OUT.exists())
SID = "harness_session_1"
SECRET = "sk-ant-api03-" + "Q9w8E7r6T5y4U3i2O1p0" * 4

# 1) real tool dispatch -> pre_tool_call + post_tool_call
import model_tools  # noqa: E402
probe = SCRATCH / "harness-probe.txt"
probe.write_text("FILE-CONTENT-SENTINEL\n", encoding="utf-8")
try:
    res = model_tools.handle_function_call("read_file", {"path": str(probe)}, task_id="t1", tool_call_id="call_1",
                                           session_id=SID, turn_id="turn_1")
    check("tool_dispatch_ran", "FILE-CONTENT-SENTINEL" in str(res))
except Exception as e:
    check("tool_dispatch_ran", False, repr(e))
check("threads_started_on_first_hook", "habby-metrics-writer" in habby_threads() and "habby-metrics-plan" not in habby_threads(),
      habby_threads())

# 2) real pre/post_api_request call sites with a fake agent (real payload builders via the mixin)
from types import SimpleNamespace  # noqa: E402
from agent.api_request_hooks import ApiRequestHooksMixin  # noqa: E402
from agent import turn_api_request as TAR, turn_response_intake as TRI  # noqa: E402


class FakeAgent(ApiRequestHooksMixin):
    session_id = SID; platform = "tui"; model = "claude-opus-5-5"; provider = "anthropic"
    _persist_disabled = False; _parent_session_id = None; _user_id = None
    base_url = "https://api.anthropic.com"; api_mode = "anthropic_messages"; _last_api_first_chunk_at = None
    tools = [{"name": "terminal"}]; max_tokens = 32000
    context_compressor = SimpleNamespace(context_length=1_000_000)


agent = FakeAgent()
now = time.time()
msgs = [{"role": "user", "content": "USER-MSG-SENTINEL"}]
from agent import turn_context as TC  # noqa: E402
try:
    # real user turn: Hermes fires pre_llm_call (skipped for _persist_disabled forks)
    TC._collect_pre_llm_call_context(agent, effective_task_id="t1", turn_id="turn_1",
                                     original_user_message="USER-MSG-SENTINEL", messages=msgs, conversation_history=[])
    check("pre_llm_call_site_ran", True)
except Exception as e:
    check("pre_llm_call_site_ran", False, repr(e))
try:
    TAR._fire_pre_api_request_hook(agent, {"messages": msgs, "system": "SYSPROMPT-SENTINEL"}, msgs, [], messages=msgs,
                                   original_user_message="USER-MSG-SENTINEL", approx_tokens=85000, total_chars=10,
                                   retry_count=0, api_call_count=1, api_request_id="r1", api_start_time=now,
                                   effective_task_id="t1", turn_id="turn_1")
    agent._last_api_first_chunk_at = now + 0.6
    resp = SimpleNamespace(model="claude-opus-5-5", id="msg_h", usage=SimpleNamespace(
        input_tokens=12, output_tokens=345, cache_read_input_tokens=88000, cache_creation_input_tokens=2100),
        content=[SimpleNamespace(type="text", text="RESPONSE-SENTINEL")])
    amsg = SimpleNamespace(content="ASSISTANT-TEXT-SENTINEL", tool_calls=[SimpleNamespace(
        id="c1", type="function", function=SimpleNamespace(name="terminal", arguments='{"command": "RESPONSE-SENTINEL"}'))])
    TRI._fire_post_api_request_hook(agent, resp, amsg, "tool_use", api_messages=msgs, api_call_count=1, api_duration=2.5,
                                    api_start_time=now, api_request_id="r1", effective_task_id="t1", turn_id="turn_1")
    check("api_callsites_ran", True)
except Exception as e:
    check("api_callsites_ran", False, repr(e))
try:  # real api_request_error call site (payload builder in ApiRequestHooksMixin)
    agent._invoke_api_request_error_hook(task_id="t1", turn_id="turn_1", api_request_id="r0", api_call_count=1,
                                         api_start_time=now, api_kwargs={"messages": msgs}, error_type="OverloadedError",
                                         error_message="ERROR-MSG-SENTINEL", status_code=529, retryable=True)
    check("api_error_callsite_ran", True)
except Exception as e:
    check("api_error_callsite_ran", False, repr(e))

# 3) real approval call sites -> on_human_input_request/resolved (+ approval observers)
try:
    from tools import approval_context, approval_prompt
    tok = approval_context._approval_session_id.set(SID)
    choice = approval_prompt.prompt_dangerous_approval(f"rm -rf C:/nonexistent --token {SECRET} APPROVAL-CMD-SENTINEL",
                                                       "harness", approval_callback=lambda *a, **k: "deny")
    check("cli_approval_prompt_ran", choice == "deny", choice)
except Exception as e:
    check("cli_approval_prompt_ran", False, repr(e))
try:
    from tools import approval_gateway_wait as AGW

    def notify(_d):
        raise RuntimeError("no gateway client in harness")
    r = AGW._await_gateway_decision("harness-key", notify, {"command": "APPROVAL-CMD-SENTINEL", "description": "d",
                                                              "pattern_key": "p", "pattern_keys": ["p"]}, surface="gateway")
    check("gateway_approval_path_ran", isinstance(r, dict) and r.get("notify_failed") is True, r)
except Exception as e:
    check("gateway_approval_path_ran", False, repr(e))

# 4) subagent hooks with the exact kwargs of tools/delegate_tool.py:330 / delegate_tool_results.py:366,
#    then the child's own hooks (child session id) through the real dispatch
CHILD = "harness_child_1"
invoke_hook("subagent_start", parent_session_id=SID, parent_turn_id="turn_1", parent_subagent_id=None,
            child_session_id=CHILD, child_subagent_id="sa-0", child_role="leaf", child_goal="Audit the loader " * 10)
invoke_hook("on_session_start", session_id=CHILD, model="claude-fable-5-1", platform="subagent")
try:
    res = model_tools.handle_function_call("read_file", {"path": str(probe)}, task_id="t2", tool_call_id="call_c1",
                                           session_id=CHILD, turn_id="cturn_1")
except Exception:
    pass
invoke_hook("subagent_stop", parent_session_id=SID, parent_turn_id="turn_1", child_session_id=CHILD, child_role="leaf",
            child_summary="CHILD-SUMMARY-SENTINEL", child_status="completed", tool_call_history=[{"tool_name": "read_file"}],
            duration_ms=4321)
invoke_hook("on_session_end", session_id=SID, completed=True, failed=False, interrupted=False)

# 5) M2: a background-review-style fork (same session_id, _persist_disabled, own turn id) through the
#    real call sites: pre_llm_call is skipped by Hermes, its calls must land in the background bucket.
class ForkAgent(FakeAgent):
    _persist_disabled = True


fork = ForkAgent()
FORK_TURN = f"{SID}:review-task:0123abcd"
try:
    TC._collect_pre_llm_call_context(fork, effective_task_id="review-task", turn_id=FORK_TURN,
                                     original_user_message="review", messages=msgs, conversation_history=msgs)
    t1 = time.time()
    TAR._fire_pre_api_request_hook(fork, {"messages": msgs}, msgs, [], messages=msgs, original_user_message="review",
                                   approx_tokens=90000, total_chars=10, retry_count=0, api_call_count=1,
                                   api_request_id="rf", api_start_time=t1, effective_task_id="review-task",
                                   turn_id=FORK_TURN)
    fork._last_api_first_chunk_at = t1 + 0.4
    fresp = SimpleNamespace(model="claude-opus-5-5", id="msg_f", usage=SimpleNamespace(
        input_tokens=5, output_tokens=40, cache_read_input_tokens=90000, cache_creation_input_tokens=0),
        content=[SimpleNamespace(type="text", text="RESPONSE-SENTINEL")])
    TRI._fire_post_api_request_hook(fork, fresp, SimpleNamespace(content="ASSISTANT-TEXT-SENTINEL", tool_calls=[
        SimpleNamespace(id="cf", type="function", function=SimpleNamespace(name="read_file", arguments="{}"))]),
        "tool_use", api_messages=msgs, api_call_count=1, api_duration=1.0, api_start_time=t1, api_request_id="rf",
        effective_task_id="review-task", turn_id=FORK_TURN)
    model_tools.handle_function_call("read_file", {"path": str(probe)}, task_id="review-task", tool_call_id="call_f1",
                                     session_id=SID, turn_id=FORK_TURN)
    TRI._fire_post_api_request_hook(fork, fresp, SimpleNamespace(content="ASSISTANT-TEXT-SENTINEL", tool_calls=[]),
                                    "stop", api_messages=msgs, api_call_count=2, api_duration=1.0, api_start_time=t1,
                                    api_request_id="rf2", effective_task_id="review-task", turn_id=FORK_TURN)
    check("fork_callsites_ran", True)
except Exception as e:
    check("fork_callsites_ran", False, repr(e))
time.sleep(1.0)

snaps = sorted(p.name for p in OUT.glob("snapshot-*.json"))
check("snapshot_written", snaps == [f"snapshot-{os.getpid()}.json"], snaps)
snap = json.loads((OUT / snaps[0]).read_text(encoding="utf-8")) if snaps else {}
s = snap.get("sessions", {}).get(SID, {})
check("context_gauge", s.get("context") == {"used": 90112, "max": 1_000_000, "pct": 9.0, "estimated": False}, s.get("context"))
check("last_call", (s.get("last_call") or {}).get("ttfb_ms") == 600 and s["last_call"]["latency_ms"] == 2500, s.get("last_call"))
check("turn_and_totals", s.get("turn", {}).get("calls") == 1 and s.get("totals", {}).get("tools_by_name") == {"read_file": 1}
      and s.get("status") == "idle" and (s.get("turn", {}).get("cost_usd") or 0) > 0, {"turn": s.get("turn"), "totals": s.get("totals")})
check("steps_from_real_dispatch", s.get("steps") == [{"tool": "read_file", "status": "ok", "ms": s["steps"][0]["ms"]}]
      if s.get("steps") else False, s.get("steps"))
c = snap.get("sessions", {}).get(CHILD, {})
check("child_session_full_state", c.get("parent_session_id") == SID and c.get("role") == "leaf"
      and c.get("totals", {}).get("tools_by_name") == {"read_file": 1} and len(c.get("goal") or "") <= 80, c)
rows = s.get("subagents") or []
check("subagent_index_row", len(rows) == 1 and rows[0]["status"] == "completed" and rows[0]["ms"] == 4321
      and rows[0]["session_id"] == CHILD and rows[0]["tools"] == 1, rows)
bg = s.get("background") or {}
check("fork_in_background_bucket", bg.get("calls") == 2 and bg.get("tool_calls") == 1 and s.get("status") == "idle"
      and (s.get("turn") or {}).get("calls") == 1 and (s.get("totals") or {}).get("calls") == 1, {"background": bg,
      "status": s.get("status"), "turn_calls": (s.get("turn") or {}).get("calls")})
ev = [json.loads(x) for x in (OUT / f"events-{os.getpid()}.jsonl").read_text(encoding="utf-8").splitlines()]
kinds = {}
for e in ev:
    kinds[e["event"]] = kinds.get(e["event"], 0) + 1
check("events_kinds", all(k in kinds for k in ("turn_start", "api_call", "tool_start", "tool_end", "wait_start", "wait_end",
                                               "approval_request", "approval_response", "subagent_start",
                                               "subagent_stop", "session_start", "turn_end")), kinds)
check("api_error_type_from_real_callsite", [(e.get("error_type"), e.get("status_code")) for e in ev
      if e["event"] == "api_error"] == [("OverloadedError", 529)], [e for e in ev if e["event"] == "api_error"])
check("wait_events_from_real_approval", [e.get("kind") for e in ev if e["event"] == "wait_start"] == ["approval", "approval"],
      [e for e in ev if e["event"].startswith(("wait", "approval"))])
blob = "".join(p.read_text(encoding="utf-8") for p in OUT.iterdir() if p.is_file())
leaks = [x for x in ("FILE-CONTENT-SENTINEL", "USER-MSG-SENTINEL", "SYSPROMPT-SENTINEL", "RESPONSE-SENTINEL",
                     "ASSISTANT-TEXT-SENTINEL", "APPROVAL-CMD-SENTINEL", "ERROR-MSG-SENTINEL", "CHILD-SUMMARY-SENTINEL", SECRET) if x in blob]
check("no_content_on_disk", not leaks, leaks)
R["snapshot_bytes"] = (OUT / snaps[0]).stat().st_size if snaps else None
R["snapshot_sample"] = snap

# 5) hook cost: our callbacks alone (what Hermes waits for beyond its own dispatch), and via invoke_hook
KW = {
    "pre_llm_call": dict(session_id=SID, task_id="t", turn_id="u", user_message="x", conversation_history=[],
                         is_first_turn=False, model="m", platform="tui", parent_session_id="", sender_id=""),
    "pre_api_request": dict(session_id=SID, turn_id="u", model="m", platform="tui", approx_input_tokens=1, user_message="x",
                            conversation_history=[], request_messages=[], system_prompt="s"),
    "post_api_request": dict(session_id=SID, turn_id="u", model="m", provider="anthropic", base_url="b", platform="tui",
                             api_call_count=1, api_duration=1.0, started_at=1.0, first_chunk_at=1.2, finish_reason="stop",
                             usage={"input_tokens": 1, "output_tokens": 2, "cache_read_tokens": 3, "cache_write_tokens": 4,
                                    "prompt_tokens": 8}, context_length=1_000_000, assistant_message=None, response={},
                             assistant_tool_call_count=0),
    "api_request_error": dict(session_id=SID, error_type="X", status_code=500, retryable=True),
    "pre_tool_call": dict(tool_name="terminal", args={"command": "git status"}, session_id=SID, task_id="t",
                          tool_call_id="p", turn_id="u"),
    "post_tool_call": dict(tool_name="terminal", args={"command": "echo"}, result="{}", session_id=SID, task_id="t",
                           tool_call_id="p", turn_id="u", api_request_id="r", duration_ms=1, status="ok",
                           error_type=None, error_message=None, middleware_trace=[]),
    "subagent_start": dict(parent_session_id=SID, parent_turn_id="u", parent_subagent_id=None, child_session_id="cc",
                           child_subagent_id="sa", child_role=None, child_goal="g" * 300),
    "subagent_stop": dict(parent_session_id=SID, parent_turn_id="u", child_session_id="cc", child_role=None,
                          child_summary="s", child_status="completed", tool_call_history=[{}], duration_ms=1),
    "pre_approval_request": dict(command="c", description="d", pattern_key="p", pattern_keys=["p"], session_key="k", surface="cli"),
    "post_approval_response": dict(command="c", description="d", pattern_key="p", session_key="k", surface="cli", choice="deny"),
    "on_human_input_request": dict(kind="approval", request_id="r", session_id=SID, session_key="k", platform="tui", prompt="p"),
    "on_human_input_resolved": dict(kind="approval", request_id="r", session_id=SID, session_key="k", platform="tui",
                                    prompt="p", outcome="deny"),
    "on_session_start": dict(session_id=SID, model="m", platform="tui"),
    "on_session_end": dict(session_id=SID, completed=True, failed=False, interrupted=False),
    "pre_auxiliary_call": dict(aux_task="compression", session_id=SID, model="m"),
    "post_auxiliary_call": dict(aux_task="compression", session_id=SID, model="m", error_type=None),
}
cb = dict(mod.HOOKS)
pc = time.perf_counter
cost = {}
N = 20000
for h in HOOKS:
    fn, kw = cb[h], KW[h]
    samples = []
    for _ in range(N):
        a = pc()
        fn(**kw)
        samples.append(pc() - a)
    samples.sort()
    bulk = []
    for _ in range(3):
        a = pc()
        for _ in range(5000):
            fn(**kw)
        bulk.append((pc() - a) / 5000)
    cost[h] = {"median_us": round(samples[N // 2] * 1e6, 2), "p99_us": round(samples[int(N * 0.99)] * 1e6, 2),
               "p999_us": round(samples[int(N * 0.999)] * 1e6, 2), "max_us": round(samples[-1] * 1e6, 1),
               "bulk_mean_us": round(min(bulk) * 1e6, 2)}
R["hook_cost_callback"] = cost
worst_p99 = max(v["p99_us"] for v in cost.values())
check("hook_cost_p99_le_50us", worst_p99 <= 50.0, {"worst_p99_us": worst_p99,
                                                    "worst_median_us": max(v["median_us"] for v in cost.values())})
disp = {}
for h in ("pre_tool_call", "post_tool_call", "post_api_request", "on_human_input_request"):
    a = pc()
    for _ in range(2000):
        invoke_hook(h, **KW[h])
    disp[h] = round((pc() - a) / 2000 * 1e6, 1)
R["invoke_hook_total_us_incl_hermes_dispatch"] = disp
time.sleep(1.5)
R["events_bytes_after_load"] = sum(p.stat().st_size for p in OUT.glob("events*.jsonl"))
R["queue_backlog_after_1_5s"] = mod._Q.qsize()
R["threads"] = habby_threads()
EV.mkdir(parents=True, exist_ok=True)
(EV / f"harness-{MODE}.json").write_text(json.dumps(R, indent=1, default=str), encoding="utf-8")
print(json.dumps({k: R[k] for k in ("discover_ms", "snapshot_bytes", "invoke_hook_total_us_incl_hermes_dispatch",
                                    "queue_backlog_after_1_5s", "threads")}, default=str))
print("hook cost (callback only, us):", json.dumps({h: (v["median_us"], v["p99_us"]) for h, v in cost.items()}))
sys.exit(1 if FAILS else 0)
