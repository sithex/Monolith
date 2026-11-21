"""Tests for the session-notes hooks and the context guard (14 Nov 2026).

Run: python .claude/hooks/tests/test_hooks.py
Each hook is run as Claude Code runs it: a subprocess fed a JSON payload on stdin, with a temp git-initialised
CLAUDE_PROJECT_DIR, a fake transcript and a temp state dir. Standard library only."""
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
import unittest

HOOKS = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SID = "sess-test"


def script(name):
    return os.path.join(HOOKS, name)


class HookCase(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="notes-hooks-")
        self.project = os.path.join(self.tmp, "proj")
        os.makedirs(self.project)
        subprocess.run(["git", "init", "-q", self.project], check=True)
        g = ["git", "-C", self.project, "-c", "user.name=t", "-c", "user.email=t@t"]
        with open(os.path.join(self.project, "a.txt"), "w") as f:
            f.write("a\n")
        subprocess.run(g + ["add", "a.txt"], check=True)
        subprocess.run(g + ["commit", "-q", "-m", "first commit"], check=True)
        self.state = os.path.join(self.tmp, "state")
        self.transcript = os.path.join(self.tmp, "t.jsonl")
        self.set_tokens(1000)
        self.notes = os.path.join(self.project, ".claude", "session-notes", SID + ".md")

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def set_tokens(self, n):
        with open(self.transcript, "w") as f:
            f.write(json.dumps({"type": "user", "message": {"content": "hi"}}) + "\n")
            f.write(json.dumps({"type": "assistant", "message": {"usage": {
                "input_tokens": n - 100, "cache_read_input_tokens": 60, "cache_creation_input_tokens": 40}}}) + "\n")

    def run_hook(self, name, payload, raw=None):
        env = dict(os.environ)
        env.update({
            "CLAUDE_PROJECT_DIR": self.project,
            "CLAUDE_NOTES_STATE_DIR": self.state,
            "CLAUDE_CODE_AUTO_COMPACT_WINDOW": "400000",
            "CLAUDE_AUTOCOMPACT_PCT_OVERRIDE": "90",
        })
        env.pop("CLAUDE_NOTES_CONTEXT_LIMIT", None)
        if raw is None:
            base = {"session_id": SID, "transcript_path": self.transcript, "cwd": self.project}
            base.update(payload)
            raw = json.dumps(base)
        r = subprocess.run([sys.executable, script(name)], input=raw, capture_output=True, text=True, env=env,
                           timeout=20)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(r.stderr, "")
        return r.stdout

    def notes_text(self):
        with open(self.notes) as f:
            return f.read()


class LedgerTests(HookCase):
    def edit(self, path, tool="Edit", key="file_path"):
        return self.run_hook("notes_ledger.py", {
            "hook_event_name": "PostToolUse", "tool_name": tool, "tool_input": {key: path}})

    def test_dedupe_and_relative(self):
        self.assertEqual(self.edit(os.path.join(self.project, "src", "x.lua")), "")
        self.edit(os.path.join(self.project, "src", "x.lua"))
        self.edit(os.path.join(self.project, "b.txt"), tool="Write")
        self.edit(os.path.join(self.project, "n.ipynb"), tool="NotebookEdit", key="notebook_path")
        self.edit("/elsewhere/y.txt")
        t = self.notes_text()
        self.assertEqual(t.count("- src/x.lua (first changed "), 1)
        self.assertIn("- b.txt (first changed ", t)
        self.assertIn("- n.ipynb (first changed ", t)
        self.assertIn("- /elsewhere/y.txt (first changed ", t)
        self.assertTrue(t.startswith("# Session notes: (untitled)"))
        files = t.index("## Files changed")
        self.assertLess(t.index("## Open questions"), files)
        self.assertLess(files, t.index("- src/x.lua"))
        self.assertLess(t.index("- /elsewhere/y.txt"), t.index("## Snapshots"))

    def test_keeps_agent_sections(self):
        os.makedirs(os.path.dirname(self.notes))
        agent = ("# Session notes: Monolith\n\n## Asks (quoted, newest last)\n\n> fix it\n\n## Next (x)\n\n"
                 "Do the thing.\n\n## Files changed (written by hooks, do not edit)\n\n"
                 "## Snapshots (written by hooks, do not edit)\n\n### old\n")
        with open(self.notes, "w") as f:
            f.write(agent)
        self.edit(os.path.join(self.project, "c.lua"))
        t = self.notes_text()
        self.assertTrue(t.startswith("# Session notes: Monolith\n\n## Asks (quoted, newest last)\n\n> fix it\n"))
        self.assertIn("Do the thing.", t)
        self.assertLess(t.index("- c.lua"), t.index("## Snapshots"))
        self.assertIn("### old", t)

    def test_other_tool_ignored(self):
        self.run_hook("notes_ledger.py", {"hook_event_name": "PostToolUse", "tool_name": "Bash",
                                          "tool_input": {"command": "ls"}})
        self.assertFalse(os.path.exists(self.notes))


class SnapshotTests(HookCase):
    def test_appends(self):
        with open(os.path.join(self.project, "dirty.txt"), "w") as f:
            f.write("x")
        self.set_tokens(300000)
        out = self.run_hook("notes_snapshot.py", {"hook_event_name": "PreCompact", "trigger": "auto"})
        self.assertIn(".claude/session-notes/%s.md" % SID, out)
        self.run_hook("notes_snapshot.py", {"hook_event_name": "PreCompact", "trigger": "manual"})
        t = self.notes_text()
        self.assertEqual(t.count("\n### "), 2)
        self.assertIn(", auto, 300,000 tokens", t)
        self.assertIn(", manual, 300,000 tokens", t)
        self.assertIn("first commit", t)
        self.assertIn("?? dirty.txt", t)
        self.assertLess(t.index("## Files changed"), t.index("## Snapshots"))

    def test_status_capped(self):
        for i in range(70):
            open(os.path.join(self.project, "f%02d.txt" % i), "w").close()
        self.run_hook("notes_snapshot.py", {"hook_event_name": "PreCompact", "trigger": "auto"})
        t = self.notes_text()
        self.assertIn("... 10 more lines", t)
        self.assertNotIn("f69.txt", t)


class ResumeTests(HookCase):
    def resume(self, **kw):
        p = {"hook_event_name": "SessionStart", "source": "compact"}
        p.update(kw)
        return self.run_hook("notes_resume.py", p)

    def test_resume_injects_notes_and_order(self):
        self.run_hook("notes_ledger.py", {"hook_event_name": "PostToolUse", "tool_name": "Edit",
                                          "tool_input": {"file_path": os.path.join(self.project, "a.txt")}})
        for trig in ("manual", "auto"):
            self.run_hook("notes_snapshot.py", {"hook_event_name": "PreCompact", "trigger": trig})
        out = json.loads(self.resume())
        h = out["hookSpecificOutput"]
        self.assertEqual(h["hookEventName"], "SessionStart")
        ctx = h["additionalContext"]
        self.assertTrue(ctx.startswith("A compaction just happened. Your notes file is below"))
        self.assertIn("Resume from \"Next\"", ctx)
        self.assertIn("- a.txt (first changed ", ctx)
        self.assertIn("(1 older snapshots omitted)", ctx)
        self.assertIn(", auto, ", ctx)
        self.assertNotIn(", manual, ", ctx)
        self.assertIn("git status --short (now):", ctx)

    def test_idempotent(self):
        self.assertNotEqual(self.resume(), "")
        self.assertEqual(self.resume(), "")

    def test_no_notes(self):
        ctx = json.loads(self.resume())["hookSpecificOutput"]["additionalContext"]
        self.assertIn("No session notes file exists", ctx)

    def test_other_sources_and_postcompact_silent(self):
        self.assertEqual(self.resume(source="startup"), "")
        self.assertEqual(self.run_hook("notes_resume.py", {"hook_event_name": "PostCompact", "trigger": "auto",
                                                           "compact_summary": "x"}), "")
        self.assertNotEqual(self.resume(), "")  # PostCompact did not consume the once-per-compaction slot

    def test_cap(self):
        os.makedirs(os.path.dirname(self.notes))
        big = "# Session notes: big\n\n## Decisions (x)\n\n" + ("word " * 20000) + "\n"
        with open(self.notes, "w") as f:
            f.write(big)
        self.run_hook("notes_snapshot.py", {"hook_event_name": "PreCompact", "trigger": "auto"})
        ctx = json.loads(self.resume())["hookSpecificOutput"]["additionalContext"]
        self.assertIn("notes cut at 30000 characters", ctx)
        self.assertIn(", auto, ", ctx)
        self.assertLess(len(ctx), 32000)


class GuardTests(HookCase):
    # T = 400,000 x 90 % = 360,000; soft = 252,000; hard = 306,000; next band 324,000.
    def guard(self, tokens, event="PostToolUse"):
        self.set_tokens(tokens)
        return self.run_hook("context_guard.py", {"hook_event_name": event})

    def test_silent_below_soft(self):
        self.assertEqual(self.guard(1000), "")
        self.assertEqual(self.guard(251000), "")

    def test_soft_once_then_hard_then_steps(self):
        out = json.loads(self.guard(253000, "UserPromptSubmit"))
        self.assertEqual(out["hookSpecificOutput"]["hookEventName"], "UserPromptSubmit")
        ctx = out["hookSpecificOutput"]["additionalContext"]
        self.assertIn("Finish the step you are on", ctx)
        self.assertIn("/compact", ctx)
        self.assertNotIn("hand_off_to_session", ctx)
        self.assertIn("systemMessage", out)
        self.assertEqual(self.guard(260000), "")
        hard = json.loads(self.guard(307000))["hookSpecificOutput"]["additionalContext"]
        self.assertIn("Start no new step", hard)
        self.assertEqual(self.guard(310000), "")
        self.assertIn("Start no new step", self.guard(325000))

    def test_band_reset_after_compaction(self):
        self.assertNotEqual(self.guard(310000), "")
        self.assertEqual(self.guard(60000), "")
        self.assertIn("Finish the step you are on", self.guard(253000))

    def test_long_notes_mentions_fresh_session(self):
        self.assertNotIn("start_session", self.guard(253000))
        os.makedirs(os.path.dirname(self.notes))
        with open(self.notes, "w") as f:
            f.write("line\n" * 401)
        self.assertIn("a fresh session", self.guard(307000))


class RobustnessTests(HookCase):
    NAMES = ("notes_ledger.py", "notes_snapshot.py", "notes_resume.py", "context_guard.py")

    def test_subagent_skipped(self):
        self.set_tokens(330000)
        for name in self.NAMES:
            for key in ("agent_id", "agent_type"):
                out = self.run_hook(name, {key: "x", "hook_event_name": "PreCompact", "source": "compact",
                                           "tool_name": "Edit", "tool_input": {"file_path": "z.txt"}})
                self.assertEqual(out, "", name)
        self.assertFalse(os.path.exists(self.notes))

    def test_empty_and_garbage_stdin(self):
        for name in self.NAMES:
            for raw in ("", "not json {", "[1, 2]", "null"):
                self.assertEqual(self.run_hook(name, None, raw=raw), "", (name, raw))


if __name__ == "__main__":
    unittest.main(verbosity=1)
