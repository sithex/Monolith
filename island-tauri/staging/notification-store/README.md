# notification-store

Platform-free Rust core for SPEC §3.2 / Phase 4. Only serde (derive) and
serde_json. Unsafe forbidden. Library code has no IO, sockets, D-Bus runtime,
threads, clocks, Tauri, image codec, or dependency on neighbouring crates.

## API

- `NotificationStore::default()` or checked `new(StorePolicy)`.
- `insert(NotificationInput, now_ms)` returns `Result<Changes, Error>`.
- `list()` clones a newest-first snapshot; `len()`, `is_empty()`, `policy()`,
  `is_transient(id)` are read-only queries.
- `expire(now_ms)`, `dismiss(id)`, `clear()`, `clear_key(key)`,
  `server_closed(server_id)`, `reset_server()`, `set_policy(policy)` return
  deltas, including an empty `Changes` on a no-op. Invalid edits are atomic.
- `Changes.events`: ordered `Delta::NotificationReceived` / `NotificationClosed`.
  Emit each contained payload on its corresponding Tauri topic; the serialized
  `Delta { type, payload }` convenience envelope is NOT a second event payload.
- `Changes.toast`: caller may request a toast; DND and low urgency suppress it.
- `Changes.close_server_ids`: best-effort `CloseNotification` work for the caller
  after local dismissal/clear. Local removal never waits for a server response.
- `dbus::NotifyArgs`: caller-decoded Notify tuple with typed hints.
  `parse(server_id)` returns a validated candidate plus bounded hint metadata.
- `dbus::NotifyCorrelator`: `observe_call(sender, serial, args)`, `touch`,
  `method_return(destination, reply_serial, server_id)`, and `reset`.
- `ingest::parse_line(&[u8])`: bounded, strict operation-specific JSON request.
  `Request::apply` revalidates directly constructed requests.
- `ingest::IngestSession::new(now_ms)` / `handle(store, line, now_ms)`:
  per-connection token bucket, typed `Reply`, and deltas. `Reply::to_line()`
  serializes exactly one reply line; no request text is echoed in errors.

All supplied times must be finite and nonnegative, in one consistent caller-
chosen millisecond time domain. No real time is read. Use monotonic time for
expiry/rate decisions; if `receivedAt` must be epoch time, the integration must
choose a consistent mapping (do not mix epoch and elapsed times in this store).
The limiter tolerates rewinds without credit; the store assumes the caller
supplies sensible ordering and expires only on an explicit `expire` call.

## Chosen behavior (controller must adopt or revise explicitly)

- Ring capacity 200, newest first. Inserts/updates move to front. Only overflow
  removes the oldest. Hermes dedupe keys are literal, case-sensitive, global
  within the Hermes source; a duplicate keeps its ID and replaces the entire
  notification and expiry metadata, rather than merging absent fields.
- Local IDs are wrapping `h-N`, `s-N`, `a-N` counters with live-collision checks;
  correlated system items use authoritative server ID `d-N`. Every queue delta
  increments one wrapping u32 revision, including closes.
- D-Bus matches returned server ID and nonzero replaces_id. A differing replaced
  local identity gets a `replaced` close before the new received delta; if both
  IDs already exist, both entries are coalesced into the single returned ID.
- Drop exact own name `Hermes Island` and exact per-app deny-list matches.
  Redaction replaces a present body (even empty) with `Hidden`, before storing or
  emitting. Absent bodies remain null. Changing privacy policy also scrubs old
  bodies and removes newly denied items, returning deltas. Scrubbing is irreversible.
- DND suppresses every toast, including critical. Low urgency is always silent.
  Transient items remain readable in the in-memory ring until close/expiry;
  `is_transient` lets an opt-in persistence adapter exclude them.
- Default entry lifetime is 6000 ms; zero means resident. Unspecified lifetime
  for nontransient critical is resident; transient critical uses default TTL.
  Explicit TTL wins. Valid lifetime domain is 0..=86400000 ms. Expiry is inclusive
  (`now >= deadline`), emits `expired`, and never closes the remote server item.
  TTL here is ENTRY lifetime, not the actor's independent hover/toast timer.
- Notifications cannot invoke actions (`canInvokeActions=false`). Actions are
  retained for display only. No code here performs image encoding or opens paths.

## D-Bus adapter boundary

Serials are scoped to the original calling connection, so keys are
`(sender, serial)`; replies match `(destination, reply_serial)` and consume one
pending entry. Calls/duplicate calls/touch refresh the 256-entry LRU. Invalid
calls leave it unchanged; unmatched returns are ignored. The host MUST verify
server owner/interface/member before calling this helper. On owner change call
BOTH `correlator.reset()` and `store.reset_server()` before rearming the monitor,
so reused server IDs cannot bind to the old server's entries.

Hints: urgency byte 0..2, desktop-entry/category strings, transient bool,
image-path/image_path strings, image-data/icon_data decoded images, and
x-kde-origin-name. Hyphenated image names take precedence if both are supplied;
all supplied recognized hints are validated, including shadowed names. Unknown
hint values are ignored, while unknown string values still receive byte caps.
These are actual D-Bus map keys, NOT serde aliases.

Body tags are stripped with a conservative quote-aware scanner; b/i/u produce
aggregate semantic flags (not span ranges), br produces newline, comments and
other well-formed tags disappear, comparison signs and incomplete tags survive
as literal TEXT. Common/numeric entities decode once after stripping. The host
must render the result as text, never HTML. Metadata/formatting is returned to
the adapter separately; it is not added to the SPEC notification wire shape.

Raw image hints must already be at most 128x128, RGB/RGBA, 8-bit, consistent
alpha/channels, rowstride width*channels..=512, exact rowstride*height bytes.
Larger source images must be bounded/downscaled by the adapter before this
parse API. Caller encoding is off-thread; `iconPng` accepts PNG data URL prefix
and 64 KiB byte cap, NOT a cryptographic or codec validity check.

## Bounds and protocol

UTF-8 byte limits, inclusive: names/source/dedupe/action strings 256; title/
summary 1024; body 8192; hint/app-icon strings 1024; 32 actions; 32 hints;
iconPng 64 KiB. Oversize input is rejected, never truncated. Required strings
must not be whitespace-only; identifiers are never normalized or repaired.

Protocol version 1 operations:

    {"v":1,"op":"notify","title":"Build finished","body":"41 passed","level":"success","ttlMs":6000,"source":"hermes:cron","dedupeKey":"build-41"}
    {"v":1,"op":"progress","dedupeKey":"dl-1","title":"Downloading model","progress":0.42}
    {"v":1,"op":"clear","dedupeKey":"dl-1"}

Notify requires title; other fields are optional. Level defaults info; error
maps to critical, info/success/warning to normal. Source labels appName and
therefore participates in app filters; NotificationSource stays `hermes`.
Progress requires key/title/progress (finite 0..=1), is an upsert using the same
Hermes dedupe identity, resident below 1 and default-TTL at 1. Clear is an
idempotent Hermes-key removal. Missing clear succeeds with `{"ok":true}`;
otherwise success includes the affected id. Suppressed-by-filter notify/progress
returns `filtered`, not a fabricated successful ID. All errors have
`{"ok":false,"error":{"code":"invalid|rateLimited|filtered","message":"..."}}`.

`MAX_LINE_BYTES=16384` counts JSON payload bytes, excluding ONE final LF or CRLF.
A pre-stripped frame is also accepted; embedded physical CR/LF is rejected.
The transport MUST cap streaming accumulation before allocating the entire line,
drain/reject oversize frames safely, and send one reply per request. This
in-memory parser cannot bound the caller's prior allocation.

Token bucket: capacity 20, refill 20/s (one token each 50 ms), initial burst 20.
Every attempted frame consumes a token, including invalid/oversize requests;
rate-exceeded frames do not parse or touch the store. This is not a sliding-
window prohibition on every possible group of 20 messages. One session per
connection. Exposed caller limits: `MAX_CONNECTIONS=8`, `IDLE_TIMEOUT_MS=10000`.
Transport ownership/credentials, connection counting, idle timeout, framing,
backpressure, event delivery, tray/UI, and persistence belong to integration.

## Verification

From this directory:

    nice -n 10 cargo generate-lockfile  # first creation only
    python3 scripts/verify.py gates
    python3 scripts/verify.py mutations
    python3 scripts/verify.py gates

The script always runs cargo under nice -n 10. It snapshots the entire non-target
crate tree under target/verification/pristine, runs compiling runtime breakages,
requires each pinned named test to fail, restores/touches source in finally,
and diff-checks the complete non-target tree after EACH restoration. Raw logs
are ignored under target/verification; durable evidence is in VERIFICATION.md.
