"""Notes ledger (14 Nov 2026): records every file the agent changes in its session notes file.

PostToolUse hook on Edit, Write and NotebookEdit. Adds the path once (dedupe) under "Files changed" in
.claude/session-notes/<session_id>.md, as "- path (first changed HH:MM)", repo-relative when inside the project.
Bash edits are caught by notes_snapshot.py instead. Main session only.
Spec: README.md"""
import os

import notes_common as nc


def changed_path(data):
    ti = data.get("tool_input") or {}
    if not isinstance(ti, dict):
        return None
    return ti.get("file_path") or ti.get("notebook_path")


def display_path(project, path):
    full = os.path.abspath(os.path.join(project, path))
    root = os.path.abspath(project)
    if full == root or full.startswith(root + os.sep):
        return os.path.relpath(full, root)
    return full


def main():
    data = nc.read_payload()
    if not data or nc.is_subagent(data):
        return
    if data.get("tool_name") not in ("Edit", "Write", "NotebookEdit", "MultiEdit"):
        return
    path = changed_path(data)
    if not path:
        return
    project = nc.project_dir(data)
    shown = display_path(project, path)
    notes = nc.notes_path(data)
    if os.path.abspath(path if os.path.isabs(path) else os.path.join(project, path)) == os.path.abspath(notes):
        return  # the notes file itself is not a change worth listing
    text = nc.ensure_notes(notes)
    if ("\n- %s (first changed " % shown) in ("\n" + text):
        return
    nc.update_notes(notes, nc.FILES_HEADING, ["- %s (first changed %s)" % (shown, nc.now())])


if __name__ == "__main__":
    nc.run_safely(main)
