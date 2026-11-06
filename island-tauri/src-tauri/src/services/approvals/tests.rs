use super::*;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};

const MS: Duration = Duration::from_millis(1);

fn event(kind: &str, payload: Value) -> String {
    json!({"jsonrpc":"2.0","method":"event","params":{"type":kind,"session_id":"s1","payload":payload}})
        .to_string()
}

fn approval_frame(prompt: &str, extra: Value) -> String {
    let mut base = json!({"prompt_id": prompt, "execution_generation": 1,
        "command": "rm -rf build", "description": "recursive delete"});
    for (k, v) in extra.as_object().cloned().unwrap_or_default() {
        base[k] = v;
    }
    event("approval.request", base)
}

fn clarify_frame(prompt: &str, choices: Value) -> String {
    event(
        "clarify.request",
        json!({"prompt_id": prompt, "execution_generation": 1,
               "question": "Which branch?", "choices": choices}),
    )
}

#[derive(Default)]
struct Keys {
    sent: Mutex<Vec<String>>,
    pid: Mutex<Option<u32>>,
    fail: AtomicBool,
}

impl Keys {
    fn with_pid(pid: u32) -> Self {
        let keys = Self::default();
        *keys.pid.lock().unwrap() = Some(pid);
        keys
    }
    fn sent(&self) -> Vec<String> {
        self.sent.lock().unwrap().clone()
    }
}

impl TerminalKeys for Keys {
    fn pid(&self) -> Option<u32> {
        *self.pid.lock().unwrap()
    }
    fn send_key(&self, key: &str) -> Result<(), IslandError> {
        if self.fail.load(Ordering::SeqCst) {
            return Err(IslandError::internal("pty closed"));
        }
        self.sent.lock().unwrap().push(key.to_owned());
        Ok(())
    }
}

#[derive(Default)]
struct Log {
    changes: Mutex<Vec<ApprovalChanged>>,
    arrived: Mutex<Vec<String>>,
    cleared: Mutex<Vec<String>>,
}

impl ApprovalSink for Log {
    fn changed(&self, change: &ApprovalChanged) {
        self.changes.lock().unwrap().push(change.clone());
    }
    fn arrived(&self, id: &str) {
        self.arrived.lock().unwrap().push(id.to_owned());
    }
    fn cleared(&self, id: &str) {
        self.cleared.lock().unwrap().push(id.to_owned());
    }
}

struct Rig {
    svc: Arc<ApprovalService>,
    run: u64,
    keys: Keys,
    log: Arc<Log>,
    enabled: Arc<AtomicBool>,
    t0: Instant,
}

/// A run with one mirroring TUI connected and the PTY child running as pid 4242.
fn rig() -> Rig {
    let enabled = Arc::new(AtomicBool::new(true));
    let flag = enabled.clone();
    let svc = ApprovalService::for_tests(Arc::new(move || flag.load(Ordering::SeqCst)));
    let log = Arc::new(Log::default());
    svc.set_sink(log.clone());
    let ticket = svc.begin_run_for_tests(1);
    let token = ticket.url.split("token=").nth(1).unwrap().to_owned();
    svc.run_started(ticket.id, Some(4242));
    assert_eq!(svc.connection_open(&token), Some(ticket.id));
    Rig {
        svc,
        run: ticket.id,
        keys: Keys::with_pid(4242),
        log,
        enabled,
        t0: Instant::now(),
    }
}

impl Rig {
    fn at(&self, ms: u64) -> Instant {
        self.t0 + MS * u32::try_from(ms).unwrap()
    }
    /// The request frame, then the TUI's `⚠` title 50 ms later (the real order).
    fn show(&self, frame: &str, ms: u64) {
        self.svc.ingest_at(self.run, frame, self.at(ms));
        self.svc.title_at(self.run, true, self.at(ms + 50));
    }
    fn prompt(&self, ms: u64) -> ApprovalPrompt {
        self.svc.snapshot_at(self.at(ms)).prompt.unwrap()
    }
    fn answer(&self, id: &str, choice: &str, ms: u64) -> Result<ApprovalChanged, IslandError> {
        self.svc.answer_at(id, choice, &self.keys, self.at(ms))
    }
}

#[test]
fn approval_is_answerable_and_types_the_choices_own_digit() {
    for (choice, digit) in [("once", "1"), ("session", "2"), ("always", "3"), ("deny", "4")] {
        let r = rig();
        r.show(&approval_frame("p1", json!({})), 0);
        let p = r.prompt(100);
        assert!(p.answerable, "{:?}", p.note);
        assert_eq!(p.text, "rm -rf build");
        assert_eq!(
            p.choices.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            ["once", "session", "always", "deny"]
        );
        r.answer(&p.id, choice, 100).unwrap();
        assert_eq!(r.keys.sent(), [digit]);
    }
}

#[test]
fn digits_follow_the_choices_the_gateway_offered() {
    let r = rig();
    r.show(&approval_frame("p1", json!({"choices": ["once", "deny"]})), 0);
    let p = r.prompt(100);
    r.answer(&p.id, "deny", 100).unwrap();
    assert_eq!(r.keys.sent(), ["2"]);
    // "always" was not offered, so it cannot be pressed at all.
    let r = rig();
    r.show(&approval_frame("p1", json!({"choices": ["once", "deny"]})), 0);
    let p = r.prompt(100);
    assert!(r.answer(&p.id, "always", 100).is_err());
    assert!(r.keys.sent().is_empty());
}

#[test]
fn clarify_choice_types_its_number() {
    let r = rig();
    r.show(&clarify_frame("c1", json!(["main", "dev", "stable"])), 0);
    let p = r.prompt(100);
    assert!(p.answerable, "{:?}", p.note);
    r.answer(&p.id, "3", 100).unwrap();
    assert_eq!(r.keys.sent(), ["3"]);
}

// ---- identity / staleness ---------------------------------------------------------------

#[test]
fn a_stale_prompt_id_never_answers_a_newer_prompt() {
    let r = rig();
    r.show(&approval_frame("p1", json!({})), 0);
    let old = r.prompt(100).id;
    // The first prompt is settled in the terminal and a second one arrives.
    r.svc.ingest_at(
        r.run,
        &event("approval.settled", json!({"prompt_id": "p1"})),
        r.at(200),
    );
    r.svc.title_at(r.run, false, r.at(210));
    r.show(&approval_frame("p2", json!({"command": "sudo make install"})), 300);
    let new = r.prompt(400);
    assert_ne!(old, new.id);
    assert_eq!(new.text, "sudo make install");
    // The click that was aimed at the old card must be refused, and nothing typed.
    let err = r.answer(&old, "once", 400).unwrap_err();
    assert!(format!("{err:?}").contains("no longer pending"), "{err:?}");
    assert!(r.keys.sent().is_empty());
    // The new id still works.
    r.answer(&new.id, "deny", 400).unwrap();
    assert_eq!(r.keys.sent(), ["4"]);
}

#[test]
fn island_prompt_ids_are_unique_across_runs() {
    let r = rig();
    r.show(&approval_frame("p1", json!({})), 0);
    let first = r.prompt(100).id;
    // Terminal restarted: same gateway prompt id in the new run, different island id.
    let ticket = r.svc.begin_run_for_tests(1);
    let token = ticket.url.split("token=").nth(1).unwrap().to_owned();
    r.svc.run_started(ticket.id, Some(4243));
    r.svc.connection_open(&token);
    r.svc.ingest_at(ticket.id, &approval_frame("p1", json!({})), r.at(200));
    r.svc.title_at(ticket.id, true, r.at(250));
    let second = r.svc.snapshot_at(r.at(300)).prompt.unwrap().id;
    assert_ne!(first, second);
    assert!(r.answer(&first, "once", 300).is_err());
    assert!(r.keys.sent().is_empty());
}

#[test]
fn a_republished_prompt_keeps_its_island_id() {
    let r = rig();
    r.show(&approval_frame("p1", json!({})), 0);
    let id = r.prompt(100).id;
    r.svc.ingest_at(r.run, &approval_frame("p1", json!({})), r.at(150));
    assert_eq!(r.prompt(200).id, id);
}

#[test]
fn answering_twice_types_once() {
    let r = rig();
    r.show(&approval_frame("p1", json!({})), 0);
    let id = r.prompt(100).id;
    let first = r.answer(&id, "once", 100).unwrap();
    assert!(first.prompt.unwrap().answered);
    assert!(r.answer(&id, "deny", 110).is_err());
    assert_eq!(r.keys.sent(), ["1"]);
}

#[test]
fn a_failed_key_write_allows_a_retry() {
    let r = rig();
    r.show(&approval_frame("p1", json!({})), 0);
    let id = r.prompt(100).id;
    r.keys.fail.store(true, Ordering::SeqCst);
    assert!(r.answer(&id, "once", 100).is_err());
    r.keys.fail.store(false, Ordering::SeqCst);
    r.answer(&id, "once", 110).unwrap();
    assert_eq!(r.keys.sent(), ["1"]);
}

#[test]
fn unknown_choice_is_rejected_without_typing() {
    let r = rig();
    r.show(&approval_frame("p1", json!({})), 0);
    let id = r.prompt(100).id;
    for bad in ["", "ONCE", "1", "yes", "once\r", "\r"] {
        assert!(r.answer(&id, bad, 100).is_err(), "{bad:?}");
    }
    assert!(r.keys.sent().is_empty());
}

// ---- the title (⚠) guard ----------------------------------------------------------------

#[test]
fn without_the_warning_title_nothing_is_answerable() {
    let r = rig();
    r.svc.ingest_at(r.run, &approval_frame("p1", json!({})), r.at(0));
    let p = r.prompt(100);
    assert!(!p.answerable);
    assert!(p.note.is_some());
    assert!(r.answer(&p.id, "once", 100).is_err());
    assert!(r.keys.sent().is_empty());
}

#[test]
fn a_title_that_predates_the_request_does_not_bind_it() {
    // A sudo/secret box turned the title earlier; a request then arrives from somewhere else.
    let r = rig();
    r.svc.title_at(r.run, true, r.at(0));
    r.svc.ingest_at(r.run, &approval_frame("p1", json!({})), r.at(5_000));
    let p = r.prompt(5_100);
    assert!(!p.answerable);
    assert!(r.answer(&p.id, "once", 5_100).is_err());
    assert!(r.keys.sent().is_empty());
}

#[test]
fn a_title_just_before_the_request_still_binds_it() {
    let r = rig();
    r.svc.title_at(r.run, true, r.at(0));
    r.svc.ingest_at(r.run, &approval_frame("p1", json!({})), r.at(100));
    let p = r.prompt(150);
    assert!(p.answerable, "{:?}", p.note);
}

#[test]
fn a_title_long_after_the_request_does_not_bind_it() {
    let r = rig();
    r.svc.ingest_at(r.run, &approval_frame("p1", json!({})), r.at(0));
    r.svc.title_at(r.run, true, r.at(10_000));
    let p = r.prompt(10_100);
    assert!(!p.answerable);
    assert!(r.answer(&p.id, "once", 10_100).is_err());
}

#[test]
fn the_title_going_away_retires_the_prompt() {
    let r = rig();
    r.show(&approval_frame("p1", json!({})), 0);
    let id = r.prompt(100).id;
    // Answered in the terminal: the title drops back before any settled frame arrives.
    r.svc.title_at(r.run, false, r.at(500));
    assert!(r.svc.snapshot_at(r.at(600)).prompt.is_none());
    assert!(r.answer(&id, "once", 600).is_err());
    assert!(r.keys.sent().is_empty());
    assert!(r.log.cleared.lock().unwrap().contains(&id));
}

#[test]
fn a_waiting_title_without_a_request_reports_unknown() {
    let r = rig();
    r.svc.title_at(r.run, true, r.at(0));
    let s = r.svc.snapshot_at(r.at(10));
    assert!(s.prompt.is_none());
    assert!(s.waiting_unknown, "sudo/secret prompts stay read-only");
    r.svc.title_at(r.run, false, r.at(20));
    assert!(!r.svc.snapshot_at(r.at(30)).waiting_unknown);
}

// ---- remaining guards -------------------------------------------------------------------

#[test]
fn disabled_setting_hides_and_refuses() {
    let r = rig();
    r.show(&approval_frame("p1", json!({})), 0);
    let id = r.prompt(100).id;
    r.enabled.store(false, Ordering::SeqCst);
    let s = r.svc.snapshot_at(r.at(200));
    assert!(s.prompt.is_none());
    assert!(!s.waiting_unknown);
    assert!(r.answer(&id, "once", 200).is_err());
    assert!(r.keys.sent().is_empty());
}

#[test]
fn a_second_mirroring_tui_makes_everything_read_only() {
    let r = rig();
    let ticket_token = {
        let inner = r.svc.lock();
        inner.run.as_ref().unwrap().token.clone()
    };
    r.show(&approval_frame("p1", json!({})), 0);
    let id = r.prompt(100).id;
    r.svc.connection_open(&ticket_token);
    assert!(!r.prompt(150).answerable);
    assert!(r.answer(&id, "once", 150).is_err());
    r.svc.connection_closed(r.run);
    assert!(r.prompt(200).answerable);
    // No connection at all: not mirrored, not answerable.
    r.svc.connection_closed(r.run);
    assert!(!r.prompt(250).answerable);
    assert!(r.keys.sent().is_empty());
}

#[test]
fn two_pending_requests_are_ambiguous() {
    let r = rig();
    r.show(&approval_frame("p1", json!({})), 0);
    r.svc.ingest_at(r.run, &approval_frame("p2", json!({})), r.at(100));
    let p = r.prompt(200);
    assert_eq!(p.more, 1);
    assert!(!p.answerable);
    assert!(r.answer(&p.id, "once", 200).is_err());
    assert!(r.keys.sent().is_empty());
}

#[test]
fn clarify_is_refused_after_the_user_typed() {
    let r = rig();
    r.svc.title_at(r.run, true, r.at(0));
    r.svc.ingest_at(r.run, &clarify_frame("c1", json!(["a", "b"])), r.at(10));
    assert!(r.prompt(20).answerable);
    // Mouse reports, focus events and terminal replies are not typing.
    for noise in ["\x1b[<0;10;5M", "\x1b[I", "\x1b[O", "\x1b[12;40R", "\x1b[?62;c", "\x1b]11;rgb:0/0/0\x07"] {
        r.svc.user_input(r.run, noise);
    }
    assert!(r.prompt(30).answerable);
    r.svc.user_input(r.run, "h");
    let p = r.prompt(40);
    assert!(!p.answerable);
    assert!(r.answer(&p.id, "1", 40).is_err());
    assert!(r.keys.sent().is_empty());
}

#[test]
fn typing_before_the_request_does_not_block_it() {
    let r = rig();
    r.svc.user_input(r.run, "hello");
    r.show(&clarify_frame("c1", json!(["a", "b"])), 10);
    assert!(r.prompt(100).answerable);
}

#[test]
fn a_replaced_pty_cannot_be_answered() {
    let r = rig();
    r.show(&approval_frame("p1", json!({})), 0);
    let id = r.prompt(100).id;
    *r.keys.pid.lock().unwrap() = Some(9999);
    assert!(r.answer(&id, "once", 100).is_err());
    *r.keys.pid.lock().unwrap() = None;
    assert!(r.answer(&id, "once", 100).is_err());
    assert!(r.keys.sent().is_empty());
}

#[test]
fn expired_requests_disappear_and_cannot_be_answered() {
    let r = rig();
    r.show(&approval_frame("p1", json!({"timeout_s": 30})), 0);
    let id = r.prompt(100).id;
    assert!(r.answer(&id, "once", 41_000).is_err());
    assert!(r.svc.snapshot_at(r.at(41_000)).prompt.is_none());
    assert!(r.keys.sent().is_empty());
}

#[test]
fn a_request_the_terminal_never_showed_is_dropped() {
    let r = rig();
    r.svc.ingest_at(r.run, &approval_frame("p1", json!({})), r.at(0));
    assert!(r.svc.snapshot_at(r.at(1_000)).prompt.is_some());
    assert!(r.svc.snapshot_at(r.at(61_000)).prompt.is_none());
}

#[test]
fn read_only_prompts_stay_visible_but_never_answer() {
    let r = rig();
    for frame in [
        approval_frame("p1", json!({"command": "x".repeat(7000)})),
        clarify_frame("p1", json!([])),
        event(
            "clarify.request",
            json!({"prompt_id": "p1", "execution_generation": 1, "question": "q",
                   "choices": ["a", "b"], "multi_select": true}),
        ),
    ] {
        let r2 = rig();
        r2.show(&frame, 0);
        let p = r2.prompt(100);
        assert!(!p.answerable, "{frame:.80}");
        assert!(p.note.is_some());
        let first = p.choices.first().map(|c| c.id.clone()).unwrap_or_else(|| "1".into());
        assert!(r2.answer(&p.id, &first, 100).is_err());
        assert!(r2.keys.sent().is_empty());
    }
    drop(r);
}

#[test]
fn a_new_run_forgets_the_old_one() {
    let r = rig();
    r.show(&approval_frame("p1", json!({})), 0);
    let id = r.prompt(100).id;
    let next = r.svc.begin_run_for_tests(1);
    assert_ne!(next.id, r.run);
    assert!(r.svc.snapshot_at(r.at(200)).prompt.is_none());
    assert!(r.answer(&id, "once", 200).is_err());
    // Frames from the old run's socket are ignored.
    r.svc.ingest_at(r.run, &approval_frame("p9", json!({})), r.at(300));
    assert!(r.svc.snapshot_at(r.at(400)).prompt.is_none());
    assert!(!r.svc.run_is_current(r.run));
}

#[test]
fn ending_the_run_clears_and_ignores_late_frames() {
    let r = rig();
    r.show(&approval_frame("p1", json!({})), 0);
    let id = r.prompt(100).id;
    r.svc.end_run(r.run);
    assert!(r.svc.snapshot_at(r.at(200)).prompt.is_none());
    assert!(r.answer(&id, "once", 200).is_err());
    assert!(r.log.cleared.lock().unwrap().contains(&id));
}

#[test]
fn settled_and_cancel_frames_remove_only_their_prompt() {
    let r = rig();
    r.show(&clarify_frame("c1", json!(["a", "b"])), 0);
    r.svc.ingest_at(r.run, &event("request.cancel", json!({"id": "other"})), r.at(100));
    assert!(r.svc.snapshot_at(r.at(110)).prompt.is_some());
    r.svc.ingest_at(r.run, &event("request.cancel", json!({"id": "c1"})), r.at(120));
    assert!(r.svc.snapshot_at(r.at(130)).prompt.is_none());
}

#[test]
fn unrelated_and_malformed_frames_change_nothing() {
    let r = rig();
    for junk in [
        "not json".to_owned(),
        event("message.delta", json!({"text": "approval. hello"})),
        "[]".to_owned(),
        event("approval.request", json!({"prompt_id": "p1"})),
        "approval.request".repeat(10_000),
    ] {
        r.svc.ingest_at(r.run, &junk, r.at(0));
    }
    let s = r.svc.snapshot_at(r.at(10));
    assert!(s.prompt.is_none());
    assert_eq!(s.rev, 0, "nothing was published");
}

#[test]
fn the_sink_hears_arrival_change_and_clear() {
    let r = rig();
    r.show(&approval_frame("p1", json!({})), 0);
    let id = r.prompt(100).id;
    assert_eq!(*r.log.arrived.lock().unwrap(), [id.clone()]);
    assert!(r.log.changes.lock().unwrap().last().unwrap().prompt.is_some());
    r.svc.ingest_at(r.run, &event("approval.settled", json!({"prompt_id": "p1"})), r.at(200));
    assert_eq!(*r.log.cleared.lock().unwrap(), [id]);
    assert!(r.log.changes.lock().unwrap().last().unwrap().prompt.is_none());
}

#[test]
fn the_notch_is_not_told_while_the_setting_is_off() {
    let r = rig();
    r.enabled.store(false, Ordering::SeqCst);
    r.show(&approval_frame("p1", json!({})), 0);
    assert!(r.log.arrived.lock().unwrap().is_empty());
}

#[test]
fn rev_only_moves_when_the_view_changes() {
    let r = rig();
    r.show(&approval_frame("p1", json!({})), 0);
    let before = r.svc.snapshot_at(r.at(100)).rev;
    assert_eq!(r.svc.snapshot_at(r.at(110)).rev, before);
    r.svc.user_input(r.run, "x");
    assert_eq!(r.svc.snapshot_at(r.at(120)).rev, before, "approval prompts ignore typing");
}

#[test]
fn only_the_current_runs_token_connects() {
    let r = rig();
    assert_eq!(r.svc.connection_open("nope"), None);
    assert_eq!(r.svc.connection_open(""), None);
    let old_token = r.svc.lock().run.as_ref().unwrap().token.clone();
    r.svc.begin_run_for_tests(1);
    assert_eq!(r.svc.connection_open(&old_token), None);
}

#[test]
fn keystroke_classifier() {
    for typed in ["a", "1", "\r", "\x1b", "\x1b[A", "\x03", "hello", "\x7f", "é"] {
        assert!(is_user_keystroke(typed), "{typed:?}");
    }
    for machine in ["", "\x1b[<0;1;1M", "\x1b[M !!", "\x1b[I", "\x1b[O", "\x1b[1;1R", "\x1b[?1;2c", "\x1b]10;?\x07", "\x1bP>|x\x1b\\"] {
        assert!(!is_user_keystroke(machine), "{machine:?}");
    }
}

#[test]
fn control_characters_in_the_command_are_neutralised() {
    let r = rig();
    r.show(
        &approval_frame("p1", json!({"command": "echo hi\u{1b}[2J\u{202e}evil\rrm"})),
        0,
    );
    let text = r.prompt(100).text;
    assert!(!text.chars().any(|c| c == '\u{1b}' || c == '\r' || c == '\u{202e}'), "{text:?}");
}

// ---- the real socket --------------------------------------------------------------------

mod socket {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpStream;

    fn mask(payload: &[u8]) -> Vec<u8> {
        let key = [1u8, 2, 3, 4];
        let mut frame = vec![0x81];
        match payload.len() {
            n if n < 126 => frame.push(0x80 | n as u8),
            n if n <= 65_535 => {
                frame.push(0x80 | 126);
                frame.extend_from_slice(&(n as u16).to_be_bytes());
            }
            n => {
                frame.push(0x80 | 127);
                frame.extend_from_slice(&(n as u64).to_be_bytes());
            }
        }
        frame.extend_from_slice(&key);
        frame.extend(payload.iter().enumerate().map(|(i, b)| b ^ key[i % 4]));
        frame
    }

    fn handshake(port: u16, target: &str, extra: &str) -> (TcpStream, String) {
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        write!(
            s,
            "GET {target} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n{extra}\r\n"
        )
        .unwrap();
        let mut reply = Vec::new();
        let mut byte = [0u8; 1];
        while !reply.ends_with(b"\r\n\r\n") {
            match s.read(&mut byte) {
                Ok(1) => reply.push(byte[0]),
                _ => break,
            }
        }
        (s, String::from_utf8_lossy(&reply).into_owned())
    }

    struct Server {
        port: u16,
        svc: Arc<ApprovalService>,
        _rt: tokio::runtime::Runtime,
    }

    fn server() -> (Server, RunTicket) {
        let svc = ApprovalService::for_tests(Arc::new(|| true));
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let s = svc.clone();
        rt.spawn(async move {
            let _ = ws::serve(listener, s).await;
        });
        let ticket = svc.begin_run_for_tests(port);
        (Server { port, svc, _rt: rt }, ticket)
    }

    fn wait_for(svc: &ApprovalService, f: impl Fn(&ApprovalChanged) -> bool) -> bool {
        for _ in 0..100 {
            if f(&svc.snapshot()) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    #[test]
    fn a_tui_connection_delivers_a_request_end_to_end() {
        let (srv, ticket) = server();
        let target = ticket.url.split("127.0.0.1:").nth(1).unwrap().split_once('/').unwrap().1;
        let (mut s, reply) = handshake(srv.port, &format!("/{target}"), "");
        assert!(reply.starts_with("HTTP/1.1 101"), "{reply}");
        assert!(reply.contains("s3pPLMBiTxaQ9kYGzzhZRbK+xOo="), "{reply}");
        s.write_all(&mask(approval_frame("p1", json!({})).as_bytes())).unwrap();
        assert!(wait_for(&srv.svc, |c| c.prompt.is_some()));
        // Large frames are skipped in-stream without breaking the connection.
        s.write_all(&mask(&vec![b'a'; 200_000])).unwrap();
        s.write_all(&mask(approval_frame("p2", json!({})).as_bytes())).unwrap();
        assert!(wait_for(&srv.svc, |c| c.prompt.as_ref().is_some_and(|p| p.more == 1)));
    }

    #[test]
    fn handshakes_without_the_token_or_with_a_browser_origin_are_refused() {
        let (srv, ticket) = server();
        let token = ticket.url.split("token=").nth(1).unwrap();
        let cases = [
            ("/sidecar?token=wrongwrongwrongwrong", ""),
            ("/sidecar", ""),
            ("/elsewhere?token=x", ""),
            (&format!("/sidecar?token={token}"), "Origin: http://evil.example\r\n"),
        ];
        for (target, extra) in cases {
            let (_s, reply) = handshake(srv.port, target, extra);
            assert!(reply.starts_with("HTTP/1.1 40"), "{target}: {reply}");
        }
        assert!(srv.svc.lock().run.as_ref().unwrap().conns == 0);
    }

    #[test]
    fn unmasked_frames_close_the_connection_and_deliver_nothing() {
        let (srv, ticket) = server();
        let target = ticket.url.split("127.0.0.1:").nth(1).unwrap().split_once('/').unwrap().1;
        let (mut s, reply) = handshake(srv.port, &format!("/{target}"), "");
        assert!(reply.starts_with("HTTP/1.1 101"));
        let body = approval_frame("p1", json!({}));
        let mut frame = vec![0x81, body.len() as u8];
        frame.extend_from_slice(body.as_bytes());
        let _ = s.write_all(&frame);
        std::thread::sleep(Duration::from_millis(300));
        assert!(srv.svc.snapshot().prompt.is_none());
    }

    #[test]
    fn closing_the_socket_releases_the_connection_slot() {
        let (srv, ticket) = server();
        let target = ticket.url.split("127.0.0.1:").nth(1).unwrap().split_once('/').unwrap().1;
        let (s, _) = handshake(srv.port, &format!("/{target}"), "");
        assert!(wait_for_conns(&srv.svc, 1));
        drop(s);
        assert!(wait_for_conns(&srv.svc, 0));
    }

    fn wait_for_conns(svc: &ApprovalService, n: usize) -> bool {
        for _ in 0..100 {
            if svc.lock().run.as_ref().is_some_and(|r| r.conns == n) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }
}
