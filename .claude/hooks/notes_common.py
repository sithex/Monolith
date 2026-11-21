"""Shared helpers for the session-notes hooks (14 Nov 2026).

Spec: README.md. Used by context_guard.py,
notes_ledger.py, notes_snapshot.py and notes_resume.py. Standard library only. Every hook runs in the main session
only and must never break it: callers wrap main() in run_safely()."""
import json
import os
import subprocess
import sys
import time

TAIL = 4 * 1024 * 1024
GIT_TIMEOUT = 5
STATE_DIR = os.environ.get("CLAUDE_NOTES_STATE_DIR") or os.path.join(
    os.path.dirname(os.path.abspath(__file__)), ".state"
)

FILES_HEADING = "## Files changed (written by hooks, do not edit)"
SNAPSHOTS_HEADING = "## Snapshots (written by hooks, do not edit)"
SKELETON = "\n".join([
    "# Session notes: (untitled)",
    "",
    "## Asks (quoted, newest last)",
    "",
    "## Decisions (what and why)",
    "",
    "## Now (the one step in progress)",
    "",
    "## Done (steps finished, with commit hashes when committed)",
    "",
    "## Next (the very next action, concrete enough to start cold)",
    "",
    "## Open questions for the user",
    "",
    FILES_HEADING,
    "",
    SNAPSHOTS_HEADING,
    "",
])


def run_safely(main):
    try:
        main()
    except Exception:  # a hook must never break the session
        pass
    sys.exit(0)


def read_payload():
    raw = sys.stdin.read()
    if not raw.strip():
        return None
    data = json.loads(raw)
    return data if isinstance(data, dict) else None


def is_subagent(data):
    return bool(data.get("agent_id") or data.get("agent_type"))


def project_dir(data):
    return os.environ.get("CLAUDE_PROJECT_DIR") or data.get("cwd") or os.getcwd()


def notes_path(data):
    sid = data.get("session_id") or "unknown"
    return os.path.join(project_dir(data), ".claude", "session-notes", "%s.md" % sid)


def now():
    return time.strftime("%H:%M")


def stamp():
    return time.strftime("%Y-%m-%d %H:%M")


# ---- transcript and trigger point ---------------------------------------------------------------------------

def last_context_tokens(path):
    """Context size of the last assistant turn: input + cache read + cache creation tokens."""
    if not path or not os.path.exists(path):
        return 0
    with open(path, "rb") as f:
        f.seek(0, os.SEEK_END)
        size = f.tell()
        f.seek(max(0, size - TAIL))
        lines = f.read().decode("utf-8", "replace").splitlines()
    for line in reversed(lines):
        try:
            d = json.loads(line)
        except ValueError:
            continue
        msg = d.get("message") if isinstance(d, dict) else None
        if d.get("type") != "assistant" or not isinstance(msg, dict) or not msg.get("usage"):
            continue
        u = msg["usage"]
        return u.get("input_tokens", 0) + u.get("cache_read_input_tokens", 0) + u.get("cache_creation_input_tokens", 0)
    return 0


def trigger_point():
    """Auto-compact point T in tokens: window x pct / 100 (window and pct from the same env Claude Code reads)."""
    window = int(os.environ.get("CLAUDE_CODE_AUTO_COMPACT_WINDOW") or os.environ.get("CLAUDE_NOTES_CONTEXT_LIMIT") or 400000)
    pct = float(os.environ.get("CLAUDE_AUTOCOMPACT_PCT_OVERRIDE") or 95)
    return max(1, int(window * pct / 100))


# ---- per-session state --------------------------------------------------------------------------------------

def _state_file(data):
    return os.path.join(STATE_DIR, "%s.json" % (data.get("session_id") or "unknown"))


def load_state(data):
    try:
        with open(_state_file(data)) as f:
            s = json.load(f)
        return s if isinstance(s, dict) else {}
    except (OSError, ValueError):
        return {}


def save_state(data, state):
    os.makedirs(STATE_DIR, exist_ok=True)
    _write(_state_file(data), json.dumps(state))


# ---- git ----------------------------------------------------------------------------------------------------

def git(project, *args):
    try:
        r = subprocess.run(
            ["git", "-C", project] + list(args), capture_output=True, text=True, timeout=GIT_TIMEOUT
        )
    except (OSError, subprocess.SubprocessError):
        return None
    return r.stdout.rstrip("\n") if r.returncode == 0 else None


def git_status(project, cap=60):
    out = git(project, "status", "--short")
    if out is None:
        return "(git status unavailable)"
    lines = out.splitlines()
    if not lines:
        return "(clean)"
    if len(lines) > cap:
        lines = lines[:cap] + ["... %d more lines" % (len(lines) - cap)]
    return "\n".join(lines)


# ---- notes file ---------------------------------------------------------------------------------------------

def _write(path, text):
    tmp = "%s.%d.tmp" % (path, os.getpid())
    with open(tmp, "w", encoding="utf-8") as f:
        f.write(text)
    os.replace(tmp, path)


def read_notes(path):
    try:
        with open(path, encoding="utf-8") as f:
            return f.read()
    except OSError:
        return None


def ensure_notes(path):
    text = read_notes(path)
    if text is None:
        os.makedirs(os.path.dirname(path), exist_ok=True)
        text = SKELETON
        _write(path, text)
    return text


def _find_heading(lines, prefix):
    for i, line in enumerate(lines):
        if line.startswith(prefix):
            return i
    return -1


def insert_under(text, heading, new_lines):
    """Append new_lines at the end of the section `heading`. Hook-owned sections stay the last two of the file:
    a missing Files heading goes before Snapshots, a missing Snapshots heading goes at the end."""
    lines = text.splitlines()
    key = heading.split(" (")[0]
    h = _find_heading(lines, key)
    if h < 0:
        snap = _find_heading(lines, SNAPSHOTS_HEADING.split(" (")[0])
        at = snap if (heading == FILES_HEADING and snap >= 0) else len(lines)
        block = [heading, ""]
        if at > 0 and lines[at - 1].strip():
            block = [""] + block
        lines[at:at] = block
        h = at + block.index(heading)
    nxt = len(lines)
    for j in range(h + 1, len(lines)):
        if lines[j].startswith("## "):
            nxt = j
            break
    body = lines[h + 1:nxt]
    while body and not body[-1].strip():
        body.pop()
    if not body:
        body = [""]
    body = body + list(new_lines)
    tail = lines[nxt:]
    out = lines[:h + 1] + body + ([""] if tail else []) + tail
    return "\n".join(out) + "\n"


def update_notes(path, heading, new_lines):
    text = ensure_notes(path)
    _write(path, insert_under(text, heading, new_lines))
