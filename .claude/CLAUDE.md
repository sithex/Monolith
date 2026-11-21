# Monolith: Claude Code rules

The user is Nick.

### Context budget, session notes and compaction

Sessions stay in the lean part of the context window, and a compaction must never lose track of the task.

- **Stay lean.** Heavy reading, wide searches, file dumps and image review go to subagents. The main session reads their summaries. Prefer `grep`/`head` over whole-file reads.
- **Session notes.** Every main session keeps `.claude/session-notes/<session_id>.md` (gitignored). Create it on the first turn that edits files or runs more than one command, with the sections Asks (quoted), Decisions (with why), Now, Done (with commit hashes), Next, Open questions. Update Asks when Nick changes the task; update Now, Done and Next at each step boundary. Write Next so a stranger could start from it cold. The last two sections, Files changed and Snapshots, are written by hooks; never edit them.
- **The hooks** (`.claude/hooks/`, wired in `.claude/settings.json`; subagents are skipped):
  - `notes_ledger.py` logs every file changed through Edit, Write or NotebookEdit.
  - `notes_snapshot.py` saves `git status` and the last commit just before any compaction.
  - `notes_resume.py` injects the notes file and a fresh `git status` straight after a compaction.
  - `context_guard.py` reads the real context size from the transcript. It warns at 70 % of the auto-compact point and orders a stop at 85 %, then every further 5 %.
- **On the soft warning:** finish the step you are on, bring the notes up to date, and tell Nick this is a good moment to `/compact`. **On the hard order:** start no new step; update the notes now. Auto-compact follows shortly after, so the notes must already be current.
- **After a compaction:** the notes file outranks the summary. Resume from Next. Re-read any file under Files changed or in the git status before editing it. Do not redo anything under Done. If Next is empty or unclear, ask Nick.
- **Context budget / fresh session.** When the notes file passes about 400 lines the session has outlived compaction: suggest Nick start a fresh session (not a fork) whose first instruction is to read the newest notes file.
