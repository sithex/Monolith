"""Notes resume (14 Nov 2026): after a compaction, injects the session notes file and the resume order.

Registered on SessionStart (matcher "compact") and PostCompact. Only SessionStart with source "compact" can add
context (hookSpecificOutput.additionalContext); PostCompact has no output channel to the model, so on PostCompact
this hook does nothing. Injects the resume order (spec §4.4), the notes file (30k characters max: the agent's
sections plus the newest snapshot) and a fresh git status. Once per compaction: a second call within 60 s is
skipped. Main session only. Spec: README.md"""
import json
import os
import time

import notes_common as nc

CAP = 30000
DEDUPE_SECONDS = 60
ORDER = (
    "A compaction just happened. Your notes file is below and is more accurate than the summary. Resume from "
    "\"Next\". Before editing any file listed under \"Files changed\" or in the git status, read it again: your memory "
    "of its contents is gone. Do not redo steps listed under \"Done\". If \"Next\" is empty or unclear, ask "
    "before acting."
)


def trim_notes(text, cap=CAP):
    """Keep the head sections and the newest snapshot; drop older snapshots, then cut the head if still too long."""
    key = nc.SNAPSHOTS_HEADING.split(" (")[0]
    idx = text.find("\n" + key)
    if idx < 0:
        head, snaps = text, ""
    else:
        head, snaps = text[:idx + 1], text[idx + 1:]
    tail = ""
    if snaps:
        lines = snaps.splitlines()
        starts = [i for i, l in enumerate(lines) if l.startswith("### ")]
        tail = lines[0] + "\n\n"
        if starts:
            if len(starts) > 1:
                tail += "(%d older snapshots omitted)\n\n" % (len(starts) - 1)
            tail += "\n".join(lines[starts[-1]:]).rstrip() + "\n"
    if len(head) + len(tail) > cap:
        room = max(0, cap - len(tail) - 60)
        head = head[:room] + "\n\n... (notes cut at %d characters)\n\n" % cap
    return head + tail


def main():
    data = nc.read_payload()
    if not data or nc.is_subagent(data):
        return
    event = data.get("hook_event_name") or "SessionStart"
    if event != "SessionStart" or data.get("source") != "compact":
        return
    state = nc.load_state(data)
    if time.time() - float(state.get("resumed_at", 0)) < DEDUPE_SECONDS:
        return
    state["resumed_at"] = time.time()
    nc.save_state(data, state)
    project = nc.project_dir(data)
    notes = nc.notes_path(data)
    rel = os.path.relpath(notes, project)
    text = nc.read_notes(notes)
    status = nc.git_status(project, 60)
    if text is None:
        body = (
            "A compaction just happened. No session notes file exists at %s, so the summary is all you have. "
            "Before editing any file in the git status below, read it again. If the next step is unclear, ask "
            "the user before acting.\n\ngit status --short:\n%s" % (rel, status)
        )
    else:
        body = "%s\n\nNotes file: %s\n\n%s\n\ngit status --short (now):\n%s" % (ORDER, rel, trim_notes(text), status)
    print(json.dumps({"hookSpecificOutput": {"hookEventName": "SessionStart", "additionalContext": body}}))


if __name__ == "__main__":
    nc.run_safely(main)
