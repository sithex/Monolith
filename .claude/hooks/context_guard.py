"""Context guard (2 Nov 2026; reworked 14 Nov 2026 for task-boundary compaction).

Runs as a PostToolUse and UserPromptSubmit hook. Reads the session transcript, takes the context size of the last
assistant turn (input + cache read + cache creation tokens) and compares it with the auto-compact point T
(CLAUDE_CODE_AUTO_COMPACT_WINDOW or CLAUDE_NOTES_CONTEXT_LIMIT, default 400,000, times CLAUDE_AUTOCOMPACT_PCT_OVERRIDE,
default 95 %). Injects, once per band per session:

- soft line at 70 % of T: finish the step, update the notes file, tell the user it is a good moment to /compact;
- hard line at 85 % of T, then every further 5 % of T: start no new step, bring the notes up to date.

After a compaction the token count drops; the stored band is reset so the soft line fires again in the next stretch.
The fresh-session handoff (CLAUDE.md "Context budget" section) is mentioned only when the notes file passes 400 lines.
Subagents are skipped. Spec: README.md"""
import json
import os

import notes_common as nc

SOFT = 70
HARD = 85
STEP = 5
LONG_NOTES = 400


def band_for(pct):
    if pct < SOFT:
        return -1
    if pct < HARD:
        return SOFT
    return HARD + int((pct - HARD) // STEP) * STEP


def notes_lines(path):
    text = nc.read_notes(path)
    return 0 if text is None else len(text.splitlines())


def main():
    data = nc.read_payload()
    if not data or nc.is_subagent(data):
        return
    used = nc.last_context_tokens(data.get("transcript_path"))
    if not used:
        return
    limit = nc.trigger_point()
    pct = 100.0 * used / limit
    band = band_for(pct)
    state = nc.load_state(data)
    seen = state.get("band", -1)
    if band < seen:  # a compaction happened: start the next stretch
        state["band"] = band
        state["tokens"] = used
        nc.save_state(data, state)
        seen = -1
    if band < 0 or band <= seen:
        return
    state["band"] = band
    state["tokens"] = used
    nc.save_state(data, state)

    project = nc.project_dir(data)
    notes = nc.notes_path(data)
    rel = os.path.relpath(notes, project)
    where = "CONTEXT GUARD: %s tokens, %d%% of the auto-compact point (%s tokens)." % (
        format(used, ","), pct, format(limit, ","))
    if band == SOFT:
        order = (
            "%s Finish the step you are on. Update Now, Done and Next in the notes file (%s). Then tell the user "
            "this is a good moment to /compact." % (where, rel)
        )
        ui = "Context at %d%% of the compact point: finishing the step, then a good moment to /compact." % pct
    else:
        order = "%s Start no new step. Bring the notes up to date now (%s)." % (where, rel)
        ui = "Context at %d%% of the compact point: notes being brought up to date before compaction." % pct
    if notes_lines(notes) > LONG_NOTES:
        order += (
            " The notes file is over %d lines, a sign this session has outlived compaction: a fresh session "
            "(see CLAUDE.md 'Context budget') is worth offering the user."
            % LONG_NOTES
        )
    print(json.dumps({
        "systemMessage": ui,
        "hookSpecificOutput": {"hookEventName": data.get("hook_event_name", "PostToolUse"), "additionalContext": order},
    }))


if __name__ == "__main__":
    nc.run_safely(main)
