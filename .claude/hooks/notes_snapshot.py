"""Notes snapshot (14 Nov 2026): appends a state snapshot to the session notes file before compaction.

PreCompact hook (manual and auto). Appends under "Snapshots": trigger, time, context tokens, git log -1 and
git status --short (60 lines max). Covers files changed by Bash, sed or other sessions, which the ledger misses.
PreCompact stdout becomes instructions for the compaction summary, so it prints one line naming the notes file.
Main session only. Spec: README.md"""
import os

import notes_common as nc


def main():
    data = nc.read_payload()
    if not data or nc.is_subagent(data):
        return
    project = nc.project_dir(data)
    notes = nc.notes_path(data)
    trigger = data.get("trigger") or "unknown"
    tokens = nc.last_context_tokens(data.get("transcript_path"))
    last = nc.git(project, "log", "-1", "--oneline") or "(no commit)"
    entry = [
        "### %s, %s, %s tokens" % (nc.stamp(), trigger, format(tokens, ",")),
        "",
        "Last commit: `%s`" % last,
        "",
        "```",
        nc.git_status(project, 60),
        "```",
    ]
    nc.update_notes(notes, nc.SNAPSHOTS_HEADING, entry)
    rel = os.path.relpath(notes, project)
    print(
        "The session notes file %s holds this task's asks, decisions, current step, next step and changed files. "
        "Name that file in the summary." % rel
    )


if __name__ == "__main__":
    nc.run_safely(main)
