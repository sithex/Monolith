# Claude Code usage

Read-only metrics for Claude Code CLI, VS Code and Desktop, inside AI Agents → Live and Usage. The existing AI Agents wrapper, tab memory and Hermes cards are preserved. This feature is adapted from the GPLv3-compatible fork's `5448e1b` implementation.

## Data and privacy

- Directory: nonempty `CLAUDE_CONFIG_DIR`, otherwise `<home>/.claude`. Test instances must ignore the environment and use their scratch home.
- Registry: `sessions/<pid>.json`, newest 200. Rust emits only whitelisted identifiers, paths, entrypoint/status/kind, identity and timestamp metadata. Arbitrary registry fields and session titles are omitted, not merely hidden by React.
- Transcripts: `projects/*/*.jsonl` and `projects/*/*/subagents/*.jsonl`, newest 5000 by mtime, key as tie-breaker. No recursive walk outside these depths.
- Credentials and key files are never opened. No network, CLI execution, sign-in, writes, session resume or process-control action.
- Rust reduces assistant messages to token usage, identifiers, model, timestamps, stop reason, tool **name**, text UTF-16 character count, thinking-signature character count, and serialized tool-input length. User content is reduced to meta/interrupt flags. Message text, thinking text, signatures, tool inputs and tool results never cross IPC.
- Tool names are restricted to 64 ASCII alphanumeric/underscore/dot/colon/hyphen characters; invalid names become `tool`.
- Paths, model names and session identifiers remain metadata; this is not an anonymization feature. React renders text, never HTML.
- Stable symlinks/nonregular files and symlink directory roots are refused. Final opens use read-only `O_NOFOLLOW | O_NONBLOCK`, and transcript metadata is rechecked after opening. This is not an adversarial filesystem sandbox against concurrent swaps of ancestor directories.

`serialized_result_never_contains_secret_fixture_content` injects a sentinel into text, thinking, signatures, tool inputs/results, user/system content and unapproved registry fields, serializes the **complete** service response, and asserts the sentinel is absent. It runs in the integrated Rust library suite.

## Incremental reading

The renderer supplies `{key, ino, off}` cursors; keys only select listed files, never arbitrary paths. New inode, shrink or invalid/fractional offset resets the cursor. Reads use 1 MiB chunks and retain at most an 8 MiB line. Oversized completed lines are discarded and counted. A partial final line is replayed on a later poll, not consumed or reported as `more` by itself.

Default budget is 16 MiB per request, explicit budgets capped at 64 MiB. The reader charges actual chunk reads, with at most an additional 8 MiB plus one newline byte to finish/discard a line; resumed offsets also check their preceding byte. A huge line yields bounded discard checkpoints until its newline, never parsing a tail as JSON. A bounded metadata-only cache avoids rereading unchanged unfinished EOF tails, so they cannot starve later files. Inode, size, offset or mtime changes invalidate the cache. No transcript bytes persist in Rust after a poll. Untouched files remain in the listing with zero facts. `more` requests continue after 50 ms only while visible. No Rust watcher or background actor is started.

Cold scans catch up from offset zero. Usage and Live retain one shared in-memory feed. Polling uses the current retained visible poller and one shared IPC flight; cancellation is checked after page-flight waits and again before accepting results. Live is 1 s busy / 5 s idle, Usage 60 s. Hiding or switching pages prevents subsequent reads.

## Usage semantics

- Streaming blocks are replaced by `(message.id || uuid, requestId)` within the 4096-entry recent window. Main/subagent overlap claims are scoped to the parent session.
- Output without a final stop reason may be estimated as `round(.44 * text/tool chars + .21 * signature chars)` when above recorded output. Final usage replaces the estimate. `~` marks estimated output; `+` marks partial pricing; wholly unpriced calls display `—`.
- Prices come from the current Hermes model table, including one-hour cache writes and fast mode. Dollar figures are list-price API equivalents, **not subscription expenditure**.
- Context comes only from the latest main-thread call; compaction clears it. Subagents contribute totals, not parent context. Default context is 1M except inferred older/Haiku 200k variants.
- Registry PID liveness and optional `/proc` start identity reject dead/reused processes. Daemons are excluded. Missing identity is unverified, not fabricated. Recent unregistered transcripts can provide estimated fallback; known dead registry sessions are not resurrected by fallback.
- Waiting rows precede working, then recent idle rows; eight rows maximum, idle after 30 min hidden. Unknown status stays unknown. Busy with ten minutes of inactivity becomes uncertain idle, not false permission waiting.
- History offers All / Hermes / Desktop / VS Code / CLI for available sources. Claude spending is bucketed by call day, Hermes by session start day. Top Claude sessions have `copy:null`; only Hermes rows retain resume-copy controls.

## Verification

The Rust fixtures cover listing, resume/append/partial tails, inode/shrink resets, budgets and oversized lines, PID identity, symlink refusal, timestamp/length parity and whole-response privacy. Frontend tests cover streaming replacement, dedupe, estimates, source/history aggregation, empty/error rendering and real-hook visibility/cancellation/catch-up. Integration tests render the card/history lead alongside missing Hermes data. Commands use generated bindings and the normal ACL/handler registration path.

GUI/compositor testing and live deployment are separate controller gates; this port deliberately does not restart the app or install the plasmoid.
