//! Terminal tests: the nine island-rust tests (adapted to the typed API and credit flow) plus
//! the new credit-flow / attach / sink tests. Only private scripts, printf and the bash fixture
//! are spawned; never the installed Hermes or a model.

use super::*;
use crate::error::ErrorCode;
use crate::services::scratch;
use std::sync::{mpsc, Once};

fn test_env() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        std::env::set_var("HERMES_ISLAND_TEST", "1");
        // Make the scrub contract real: these must NOT reach the child.
        std::env::set_var("KITTY_WINDOW_ID", "7");
        std::env::set_var("HERMES_SESSION_ID", "leak");
        std::env::set_var("NODE_OPTIONS", "--max-old-space-size=1");
    });
}

fn config(credit_limit: usize) -> TerminalConfig {
    test_env();
    let mut cfg = TerminalConfig::new(std::env::current_dir().unwrap(), std::env::temp_dir(), true);
    cfg.credit_limit = credit_limit;
    cfg
}

fn fixture() -> Terminal {
    Terminal::new(config(DEFAULT_CREDIT_LIMIT))
}

fn collecting() -> (Arc<dyn TerminalSink>, mpsc::Receiver<TerminalEvent>) {
    collecting_events(false)
}

fn collecting_events(
    include_snapshot: bool,
) -> (Arc<dyn TerminalSink>, mpsc::Receiver<TerminalEvent>) {
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    let sink: Arc<dyn TerminalSink> = Arc::new(move |event| {
        if !include_snapshot && matches!(event, TerminalEvent::Snapshot(_)) {
            return true;
        }
        tx.lock().map(|tx| tx.send(event).is_ok()).unwrap_or(false)
    });
    (sink, rx)
}

fn until(t: &Terminal, predicate: impl Fn(&TerminalSnapshot) -> bool) -> TerminalSnapshot {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let s = t.start().unwrap();
        if predicate(&s) {
            return s;
        }
        assert!(
            Instant::now() < deadline,
            "timed out: {} bytes, running={}",
            s.data.len(),
            s.running
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn contains(needle: &'static str) -> impl Fn(&TerminalSnapshot) -> bool {
    move |s| s.data.contains(needle)
}

fn data_bytes(events: &[TerminalEvent]) -> usize {
    events
        .iter()
        .map(|e| match e {
            TerminalEvent::Data { data, .. } => data.len(),
            TerminalEvent::Exit { .. } | TerminalEvent::Snapshot(_) => 0,
        })
        .sum()
}

fn last_seq(events: &[TerminalEvent]) -> Option<u32> {
    events.iter().rev().find_map(|e| match e {
        TerminalEvent::Data { seq, .. } => Some(*seq),
        TerminalEvent::Exit { .. } | TerminalEvent::Snapshot(_) => None,
    })
}

/// Collect everything currently queued, then wait until nothing arrives for `quiet`.
fn settle(rx: &mpsc::Receiver<TerminalEvent>, quiet: Duration) -> Vec<TerminalEvent> {
    let mut events = Vec::new();
    while let Ok(event) = rx.recv_timeout(quiet) {
        events.push(event);
    }
    events
}

const FLOOD: &str =
    "exec /usr/bin/python3 -c 'import os; b=b\"x\"*16384; exec(\"while True: os.write(1,b)\")'\r";

#[test]
fn resumed_bounded_consumer_preserves_every_packet_and_replay_sequence() {
    let limit = 32 * 1024;
    let t = Terminal::new(config(limit));
    let (sink, rx) = collecting();
    let attached = t.attach(sink).unwrap();
    t.input("exec /usr/bin/python3 -c 'import sys; sys.stdout.write(chr(233)*50000+\"RESUMED_DONE\"); sys.exit(3)'\r").unwrap();
    thread::sleep(Duration::from_millis(400));
    // The consumer has acknowledged nothing: the producer must be stalled at the credit limit.
    let stalled = t.snapshot();
    let queued = settle(&rx, Duration::from_millis(100));
    assert!(
        data_bytes(&queued) < limit + 16 * 1024 + 8,
        "unbounded queue: {}",
        data_bytes(&queued)
    );
    assert!(!queued
        .iter()
        .any(|e| matches!(e, TerminalEvent::Exit { .. })));
    assert_eq!(Some(stalled.seq), last_seq(&queued).or(Some(attached.seq)));
    // Resume: consume + acknowledge every packet; sequence numbers must be gap-free.
    let mut text = String::new();
    let mut seq = attached.seq;
    let mut pending: std::collections::VecDeque<_> = queued.into();
    let code = loop {
        let event = match pending.pop_front() {
            Some(event) => event,
            None => rx.recv_timeout(Duration::from_secs(3)).unwrap(),
        };
        match event {
            TerminalEvent::Snapshot(_) => panic!("snapshot already consumed at attach"),
            TerminalEvent::Exit { code, signal } => {
                assert_eq!(signal, None);
                break code;
            }
            TerminalEvent::Data { seq: got, data } => {
                seq = seq.wrapping_add(1);
                assert_eq!(got, seq);
                text.push_str(&data);
                t.ack(got);
            }
        }
    };
    assert_eq!(code, Some(3));
    let snapshot = until(&t, |s| !s.running);
    assert_eq!(snapshot.seq, seq);
    assert!(snapshot.data.ends_with("RESUMED_DONE"));
    assert!(text.ends_with("RESUMED_DONE"));
    assert_eq!(text.matches('é').count(), 50000);
    assert_eq!(snapshot.data.matches('é').count(), 50000);
    assert_eq!(
        snapshot.data,
        attached.data + &text,
        "exact replay/delivery equality"
    );
}

#[test]
fn stalled_consumer_backpressures_output_without_locking_state_or_drop() {
    let limit = 64 * 1024;
    let t = Terminal::new(config(limit));
    let (sink, rx) = collecting();
    t.attach(sink).unwrap();
    t.input(FLOOD).unwrap();
    thread::sleep(Duration::from_millis(300));
    let start = Instant::now();
    let s = t.snapshot();
    t.resize(90, 25).unwrap();
    drop(t);
    assert!(start.elapsed() < Duration::from_millis(700));
    // Nothing is ever acknowledged, so both the replay and the delivered bytes are bounded by
    // the credit limit plus one read.
    let bound = limit + 16 * 1024 + 64;
    assert!(
        s.data.len() <= bound,
        "unbounded output: {} bytes",
        s.data.len()
    );
    let delivered = settle(&rx, Duration::from_millis(50));
    assert!(
        data_bytes(&delivered) <= bound,
        "unbounded queue: {}",
        data_bytes(&delivered)
    );
}

#[test]
fn unread_huge_paste_returns_backpressure_without_blocking_controls() {
    let t = fixture();
    t.start().unwrap();
    t.input("stty -echo -icanon; printf '\\nREADY_%s\\n' INPUT; exec sleep 120\r")
        .unwrap();
    until(&t, contains("READY_INPUT"));
    let start = Instant::now();
    let error = t.input(&"x".repeat(65536)).unwrap_err();
    assert_eq!(error.code, ErrorCode::Busy);
    assert!(
        error.message.contains("backpressure") && error.message.contains("bytes accepted"),
        "{}",
        error.message
    );
    t.resize(90, 25).unwrap();
    drop(t);
    assert!(start.elapsed() < Duration::from_millis(700));
}

#[test]
fn reparented_setsid_slave_cannot_delay_exit_or_drop() {
    let t = fixture();
    t.start().unwrap();
    t.input("exec /usr/bin/python3 -c 'import os,time,signal; p=os.fork(); os.setsid() if p==0 else None; signal.signal(signal.SIGHUP,signal.SIG_IGN); print(\"\\nESCAPED=\"+str(os.getpid()),flush=True) if p==0 else None; time.sleep(120 if p==0 else 0.2)'\r").unwrap();
    let s = until(&t, |s| {
        s.data.lines().any(|l| {
            l.strip_prefix("ESCAPED=")
                .is_some_and(|v| v.trim().parse::<u32>().is_ok())
        })
    });
    let escaped: u32 = s
        .data
        .lines()
        .find_map(|l| l.strip_prefix("ESCAPED=")?.trim().parse().ok())
        .unwrap();
    let identity = process(escaped).unwrap();
    // A watchdog makes a blocking join fail rather than wedging the harness.
    let watchdog = thread::spawn(move || {
        thread::sleep(Duration::from_millis(1200));
        kill_owned((identity.pid, identity.birth));
    });
    let start = Instant::now();
    until(&t, |s| !s.running);
    drop(t);
    let elapsed = start.elapsed();
    watchdog.join().unwrap();
    assert!(
        elapsed < Duration::from_millis(700),
        "escaped slave delayed shutdown: {elapsed:?}"
    );
}

#[test]
fn bounded_utf8_replay_and_split_scalars_over_real_pty() {
    let t = fixture();
    let (sink, rx) = collecting();
    let attached = t.attach(sink).unwrap();
    t.input("printf '\\303'; sleep 0.03; printf '\\251'; printf 'SPLIT_%s\\n' OK\r")
        .unwrap();
    let s = until(&t, contains("SPLIT_OK"));
    assert!(s.data.contains("éSPLIT_OK"));
    assert!(!s.data.contains('\u{fffd}'));
    t.input("/usr/bin/python3 -c \"import sys; sys.stdout.write('é'*600000)\"; printf '\\nREPLAY_%s\\n' DONE\r").unwrap();
    // 1.2 MB of output exceeds the 1 MiB credit window: the consumer must acknowledge.
    let mut events = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut text = String::new();
    while !text.contains("REPLAY_DONE") {
        assert!(Instant::now() < deadline, "no REPLAY_DONE");
        if let Ok(event) = rx.recv_timeout(Duration::from_millis(50)) {
            if let TerminalEvent::Data { seq, data } = &event {
                text.push_str(data);
                if text.len() > 4_000_000 {
                    text.drain(..2_000_000);
                }
                t.ack(*seq);
            }
            events.push(event);
        }
    }
    // Whatever trails REPLAY_DONE (the next prompt) must be consumed before comparing with the
    // final snapshot.
    thread::sleep(Duration::from_millis(200));
    while let Ok(event) = rx.try_recv() {
        if let TerminalEvent::Data { seq, .. } = &event {
            t.ack(*seq);
        }
        events.push(event);
    }
    let s = until(&t, contains("REPLAY_DONE"));
    assert!(s.data.len() <= REPLAY_LIMIT);
    assert!(s.data.len() >= REPLAY_LIMIT - 3);
    assert!(!s.data.contains('\u{fffd}'));
    assert!(events
        .iter()
        .any(|e| matches!(e, TerminalEvent::Data { seq, .. } if *seq == s.seq)));
    let mut expected = attached.seq;
    for event in &events {
        if let TerminalEvent::Data { seq, data } = event {
            expected = expected.wrapping_add(1);
            assert_eq!(*seq, expected);
            assert!(data.len() <= MAX_CHUNK);
        }
    }
    let mut delivered = attached.data;
    for event in &events {
        if let TerminalEvent::Data { data, .. } = event {
            delivered.push_str(data);
        }
    }
    let mut cut = delivered.len().saturating_sub(REPLAY_LIMIT);
    while !delivered.is_char_boundary(cut) {
        cut += 1;
    }
    assert_eq!(
        s.data,
        delivered[cut..],
        "exact bounded replay equals delivered suffix"
    );
}

#[test]
fn drop_kills_own_background_tree_not_other_terminal() {
    let other = fixture();
    other.start().unwrap();
    let t = fixture();
    let leader = t.start().unwrap().pid.unwrap();
    t.input("sleep 120 & printf 'CHILD=%s\\n' \"$!\"\r")
        .unwrap();
    let s = until(&t, |s| {
        s.data.lines().any(|l| {
            l.strip_prefix("CHILD=")
                .is_some_and(|v| v.trim().parse::<u32>().is_ok())
        })
    });
    let child: u32 = s
        .data
        .lines()
        .find_map(|l| l.strip_prefix("CHILD=")?.trim().parse().ok())
        .unwrap();
    assert!(process(child).is_some());
    drop(t);
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let live = |pid| {
            fs::read_to_string(format!("/proc/{pid}/stat"))
                .ok()
                .is_some_and(|s| !s.rsplit_once(") ").unwrap().1.starts_with('Z'))
        };
        if !live(leader) && !live(child) {
            break;
        }
        assert!(Instant::now() < deadline, "owned PTY tree survived Drop");
        thread::sleep(Duration::from_millis(10));
    }
    other.input("printf 'OTHER_%s\\n' ALIVE\r").unwrap();
    until(&other, contains("OTHER_ALIVE"));
}

#[test]
fn validation_and_environment_contract() {
    let t = fixture();
    assert_eq!(t.pid(), None);
    let no_terminal = t.input("x").unwrap_err();
    assert_eq!(
        (no_terminal.code, no_terminal.message.as_str()),
        (ErrorCode::Unavailable, "No terminal")
    );
    assert_eq!(t.restart().unwrap_err().message, "No terminal");
    t.resize(0, 0).unwrap(); // optional resize is a no-op before first start
    t.start().unwrap();
    for bad in [
        "x".repeat(INPUT_LIMIT_BYTES + 1),
        "😀".repeat(INPUT_LIMIT_BYTES / 4 + 1),
    ] {
        let error = t.input(&bad).unwrap_err();
        assert_eq!(
            (error.code, error.message.as_str()),
            (ErrorCode::Invalid, "Invalid terminal input")
        );
    }
    for (cols, rows) in [(1, 30), (501, 30), (90, 201), (90, 1), (0, 0)] {
        let error = t.resize(cols, rows).unwrap_err();
        assert_eq!(
            (error.code, error.message.as_str()),
            (ErrorCode::Invalid, "Invalid terminal dimensions"),
            "{cols}x{rows}"
        );
    }
    t.resize(100, 30).unwrap();
    t.input("printf 'ENV_%s|%s|%s|%s|%s|%s\\n' OK \"$TERM\" \"$COLORTERM\" \"${NODE_OPTIONS-unset}\" \"${KITTY_WINDOW_ID-unset}\" \"${HERMES_SESSION_ID-unset}\"; stty size\r").unwrap();
    let s = until(&t, |s| {
        s.data
            .contains("ENV_OK|xterm-256color|truecolor|unset|unset|unset")
            && s.data.contains("30 100")
    });
    assert!(s.data.contains("30 100"));
    // The scrub policy itself is covered by pty::tests::scrub_policy_and_incremental_utf8.
}

#[test]
fn bash_io_resize_snapshot_and_exit_restart_contract() {
    let t = fixture();
    let (sink, rx) = collecting();
    let first = t.attach(sink).unwrap();
    assert!(first.running);
    assert_eq!(first.exit_code, None);
    assert!(serde_json::to_value(&first)
        .unwrap()
        .get("exitCode")
        .is_none());
    assert_eq!(t.pid(), first.pid);
    assert_eq!(
        t.restart().unwrap().pid,
        first.pid,
        "restart of a running terminal is a no-op"
    );
    t.resize(91, 27).unwrap();
    t.input("printf 'ISLAND_%s\\n' ALIVE; stty size\r").unwrap();
    let attached = until(&t, contains("27 91"));
    assert_eq!(attached.pid, first.pid);
    assert!(attached.data.contains("ISLAND_ALIVE"));
    let mut seq = first.seq;
    let mut count = 0;
    for event in settle(&rx, Duration::from_millis(100)) {
        let TerminalEvent::Data { seq: got, data } = event else {
            panic!("unexpected exit before `exit`");
        };
        seq = seq.wrapping_add(1);
        assert_eq!(got, seq);
        assert!(!data.is_empty());
        t.ack(got);
        count += 1;
    }
    assert!(count > 0);
    t.input("exit 7\r").unwrap();
    let exited = until(&t, |s| !s.running);
    assert_eq!(exited.exit_code, Some(7));
    assert_eq!(exited.pid, first.pid);
    let error = t.input("x").unwrap_err();
    assert_eq!(error.message, "Terminal is not running");
    let events = settle(&rx, Duration::from_millis(100));
    assert_eq!(
        events.last().unwrap(),
        &TerminalEvent::Exit {
            code: Some(7),
            signal: None
        }
    );
    let second = t.restart().unwrap();
    assert_ne!(second.pid, first.pid);
    assert!(second.running);
    assert_eq!(second.data, "");
    assert_eq!(second.seq, 0);
    assert_eq!(second.exit_code, None);
    // The attached client survives a restart and sees the new run's chunks from seq 1.
    match rx.recv_timeout(Duration::from_secs(3)).unwrap() {
        TerminalEvent::Data { seq, .. } => assert_eq!(seq, 1),
        other => panic!("expected data from the new run, got {other:?}"),
    }
}

// ---- configurable launch --------------------------------------------------------------

fn custom_terminal(
    dir: &std::path::Path,
    command: &str,
) -> (Terminal, Arc<crate::services::settings::SettingsService>) {
    test_env();
    let settings = Arc::new(crate::services::settings::SettingsService::new(
        dir.join("data"),
        crate::services::autostart::Autostart::with_default_launcher(
            dir.to_path_buf(),
            dir.join("app"),
            true,
        ),
    ));
    set_command(&settings, command);
    let cfg = TerminalConfig::new(dir.to_path_buf(), dir.to_path_buf(), false);
    (Terminal::with_settings(cfg, settings.clone()), settings)
}

fn set_command(settings: &crate::services::settings::SettingsService, command: &str) {
    settings
        .patch(&serde_json::from_value(serde_json::json!({"terminalCommand":command})).unwrap())
        .unwrap();
}

#[test]
fn custom_command_real_pty_preserves_quoted_argv_and_literal_metacharacters() {
    let dir = scratch("custom-argv");
    let marker = dir.join("MUST_NOT_EXIST");
    let literal = format!("$(touch {})", marker.display());
    let command = format!(
        "/usr/bin/printf '<%s>' 'two words' '' --TUI '{}' '$HOME' '~' ';' '|' '>' '*'",
        literal
    );
    let (t, _) = custom_terminal(&dir, &command);
    t.start().unwrap();
    let s = until(&t, |s| !s.running);
    assert_eq!(s.exit_code, Some(0));
    assert_eq!(
        s.data,
        format!("<two words><><--TUI><{literal}><$HOME><~><;><|><>><*>")
    );
    assert!(!marker.exists());
    drop(t);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn custom_command_change_preserves_live_pty_and_next_restart_reads_latest_settings() {
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch("custom-live-settings");
    let script = dir.join("private script");
    fs::write(&script, "#!/bin/bash\nprintf 'FIRST|%s|%s|%s|%s' \"$TERM_PROGRAM\" \"$TERM\" \"${HERMES_SESSION_ID-unset}\" \"$1\"\nread -r ignored\n").unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
    let (t, settings) = custom_terminal(&dir, &format!("'{}' --CLI", script.display()));
    let first = t.start().unwrap();
    until(
        &t,
        contains("FIRST|hermes-island|xterm-256color|unset|--CLI"),
    );
    set_command(&settings, "/usr/bin/printf SECOND");
    assert_eq!(t.start().unwrap().pid, first.pid);
    assert_eq!(t.restart().unwrap().pid, first.pid);
    assert!(t.snapshot().running);
    t.input("done\r").unwrap();
    until(&t, |s| !s.running);
    let next = t.restart().unwrap();
    assert_ne!(next.pid, first.pid);
    let s = until(&t, |s| !s.running);
    assert_eq!(s.data, "SECOND");
    assert_eq!(s.exit_code, Some(0));
    drop(t);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn custom_command_missing_executable_is_recoverable_after_correction() {
    let dir = scratch("custom-missing");
    let (t, settings) = custom_terminal(&dir, "ripple-missing-executable-unique --TUI");
    let error = t.start().unwrap_err();
    assert_eq!(error.code, ErrorCode::Unavailable);
    assert!(error.message.contains("ripple-missing-executable-unique"));
    assert!(t.pid().is_none());
    set_command(&settings, "/usr/bin/printf RECOVERED");
    t.start().unwrap();
    until(&t, |s| !s.running);
    set_command(&settings, "ripple-missing-executable-unique");
    assert!(t.restart().is_err());
    set_command(&settings, "/usr/bin/printf RESTART_RECOVERED");
    t.restart().unwrap();
    assert_eq!(until(&t, |s| !s.running).data, "RESTART_RECOVERED");
    drop(t);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn custom_command_resolution_uses_path_then_safe_fallbacks_without_expansion() {
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch("custom-resolution");
    let path_bin = dir.join("systemd-bin");
    let local_bin = dir.join(".local/bin");
    let launcher_bin = dir.join(".hermes/hermes-agent/.hermes/bin");
    for bin in [&path_bin, &local_bin, &launcher_bin] {
        fs::create_dir_all(bin).unwrap();
        for name in ["hermes", "hermes-local"] {
            fs::write(bin.join(name), "#!/bin/bash\nprintf PRIVATE_ONLY\n").unwrap();
            fs::set_permissions(bin.join(name), fs::Permissions::from_mode(0o700)).unwrap();
        }
    }
    let path = std::env::join_paths([&path_bin]).unwrap();
    for input in [
        "hermes",
        "hermes --tui",
        "hermes --cli",
        "hermes-local",
        "hermes --TUI",
    ] {
        let argv = crate::contract::settings::parse_terminal_command(input).unwrap();
        assert_eq!(
            pty::resolve_custom_program(&argv[0], &dir, &dir, Some(&path)).unwrap(),
            path_bin.join(&argv[0])
        );
        if input.ends_with("--TUI") {
            assert_eq!(argv[1], "--TUI");
        }
    }
    assert_eq!(
        pty::resolve_custom_program("hermes-local", &dir, &dir, None).unwrap(),
        local_bin.join("hermes-local")
    );
    fs::remove_file(local_bin.join("hermes-local")).unwrap();
    assert_eq!(
        pty::resolve_custom_program("hermes-local", &dir, &dir, None).unwrap(),
        launcher_bin.join("hermes-local")
    );
    assert_eq!(
        pty::resolve_custom_program("./systemd-bin/hermes", &dir, &dir, None).unwrap(),
        dir.join("./systemd-bin/hermes")
    );
    assert!(pty::resolve_custom_program("~/.local/bin/hermes", &dir, &dir, Some(&path)).is_err());
    assert!(
        pty::resolve_custom_program("$HOME/.local/bin/hermes", &dir, &dir, Some(&path)).is_err()
    );
    fs::set_permissions(path_bin.join("hermes"), fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(
        pty::resolve_custom_program("hermes", &dir, &dir, Some(&path)).unwrap(),
        local_bin.join("hermes")
    );
    assert!(
        pty::resolve_custom_program(path_bin.to_str().unwrap(), &dir, &dir, Some(&path)).is_err()
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn custom_command_empty_reset_preserves_original_launch_exactly_and_fixture_ignores_settings() {
    let dir = scratch("custom-default");
    let launcher = crate::services::hermes_launcher(&dir);
    fs::create_dir_all(launcher.parent().unwrap()).unwrap();
    fs::write(launcher, "exec /usr/bin/python3 ignored\n").unwrap();
    let script = dir.join("Hermes/Dropdown/custom_hermes.py");
    fs::create_dir_all(script.parent().unwrap()).unwrap();
    // Deliberately not valid Python: we only inspect the command, never execute it.
    fs::write(&script, "NEVER EXECUTE HERMES").unwrap();
    let (t, settings) = custom_terminal(&dir, "");
    for reset in ["", "   "] {
        set_command(&settings, reset);
        let command = t.command().unwrap();
        assert_eq!(
            command.get_argv(),
            &vec![
                std::ffi::OsString::from("/usr/bin/python3"),
                "-I".into(),
                script.clone().into_os_string(),
                "chat".into(),
                "--tui".into()
            ]
        );
        assert_eq!(
            command.get_env("TERM_PROGRAM"),
            Some(std::ffi::OsStr::new("hermes-island"))
        );
    }
    set_command(&settings, "missing-command-never-run");
    let mut cfg = config(DEFAULT_CREDIT_LIMIT);
    cfg.root = dir.clone();
    cfg.home = dir.clone();
    let fixture = Terminal::with_settings(cfg, settings);
    assert_eq!(
        fixture.command().unwrap().get_argv(),
        &vec![
            std::ffi::OsString::from("/bin/bash"),
            "--noprofile".into(),
            "--norc".into()
        ]
    );
    fixture.start().unwrap();
    fixture.input("printf FIXTURE_%s OK\r").unwrap();
    until(&fixture, contains("FIXTURE_OK"));
    drop(fixture);
    drop(t);
    fs::remove_dir_all(dir).unwrap();
}

// ---- new tests ------------------------------------------------------------------------

#[test]
fn credit_stalls_at_the_limit_without_acks_and_resumes_after_ack() {
    let limit = 16 * 1024;
    let t = Terminal::new(config(limit));
    let (sink, rx) = collecting();
    t.attach(sink).unwrap();
    t.input(FLOOD).unwrap();
    thread::sleep(Duration::from_millis(300));
    let mut events = settle(&rx, Duration::from_millis(150));
    let stalled_bytes = data_bytes(&events);
    assert!(
        stalled_bytes >= limit,
        "never reached the limit: {stalled_bytes}"
    );
    assert!(
        stalled_bytes <= limit + 16 * 1024 + 8,
        "overshot the limit: {stalled_bytes}"
    );
    // Still no acks -> still nothing new.
    thread::sleep(Duration::from_millis(200));
    assert!(
        rx.try_recv().is_err(),
        "producer must stay stalled without acks"
    );
    // Ahead / stale acknowledgements are ignored: still stalled.
    let newest = last_seq(&events).unwrap();
    t.ack(newest.wrapping_add(1000));
    t.ack(0);
    thread::sleep(Duration::from_millis(200));
    assert!(
        rx.try_recv().is_err(),
        "ahead/unknown acks must not grant credit"
    );
    // A real ack resumes the flow.
    t.ack(newest);
    let resumed = rx.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(matches!(resumed, TerminalEvent::Data { seq, .. } if seq == newest.wrapping_add(1)));
    events.push(resumed);
    let metrics = t.flow_metrics();
    assert!(metrics.paused_reads >= 1 && metrics.resumed_reads >= 1);
    assert!(metrics.peak_unacked_bytes <= limit + MAX_CHUNK);
    assert!(metrics.read_bytes >= data_bytes(&events));
    eprintln!(
        "real PTY flow metrics: {}",
        serde_json::to_string(&metrics).unwrap()
    );
    drop(t);
}

#[test]
fn reattach_resets_accounting_and_hands_over_without_gap_or_duplicate() {
    let limit = 64 * 1024;
    let t = Terminal::new(config(limit));
    let (first_sink, first_rx) = collecting();
    t.attach(first_sink).unwrap();
    t.input(FLOOD).unwrap();
    thread::sleep(Duration::from_millis(250));
    let before = settle(&first_rx, Duration::from_millis(100));
    assert!(data_bytes(&before) >= limit, "first client never stalled");
    // Many re-attaches while output is stalled/flowing: the new client must receive exactly
    // snapshot.seq + 1 first (accounting reset lets the stalled reader continue), then a
    // contiguous run, and the previous client must receive nothing newer.
    let mut previous = first_rx;
    for round in 0..25 {
        let (sink, rx) = collecting();
        let snapshot = t.attach(sink).unwrap();
        assert!(snapshot.running);
        let in_flight = lock(&t.shared).flow.in_flight_bytes();
        assert!(
            in_flight >= snapshot.data.len(),
            "snapshot must consume credit"
        );
        assert!(
            in_flight <= snapshot.data.len() + MAX_CHUNK,
            "at most the already-started read may follow a full-credit snapshot"
        );
        t.ack(snapshot.seq);
        let mut expected = snapshot.seq.wrapping_add(1);
        for _ in 0..3 {
            match rx.recv_timeout(Duration::from_secs(3)).unwrap() {
                TerminalEvent::Data { seq, .. } => {
                    assert_eq!(
                        seq, expected,
                        "round {round}: gap or duplicate after attach"
                    );
                    expected = expected.wrapping_add(1);
                }
                other => panic!("unexpected {other:?}"),
            }
        }
        // Anything the previous client still has queued was appended BEFORE the swap, so it is
        // contained in `snapshot` (seq <= snapshot.seq): never a chunk the new client also gets.
        for stale in previous.try_iter() {
            if let TerminalEvent::Data { seq, .. } = stale {
                assert!(
                    flow::seq_le(seq, snapshot.seq),
                    "round {round}: old sink got newer chunk {seq}"
                );
            }
        }
        previous = rx;
        t.ack(expected.wrapping_sub(1));
    }
}

#[test]
fn exit_is_delivered_despite_exhausted_credit() {
    let limit = 1024;
    let t = Terminal::new(config(limit));
    let (sink, rx) = collecting();
    t.attach(sink).unwrap();
    t.input("exec /usr/bin/python3 -c 'import sys; sys.stdout.write(\"y\"*6000); sys.stdout.flush(); sys.exit(5)'\r").unwrap();
    // Never acknowledge anything.
    let mut got_exit = None;
    let mut bytes = 0;
    let deadline = Instant::now() + Duration::from_secs(5);
    while got_exit.is_none() {
        assert!(
            Instant::now() < deadline,
            "Exit was held back by credit ({bytes} data bytes seen)"
        );
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(TerminalEvent::Exit { code, signal }) => got_exit = Some((code, signal)),
            Ok(TerminalEvent::Data { data, .. }) => bytes += data.len(),
            Ok(TerminalEvent::Snapshot(_)) => panic!("snapshot already consumed"),
            Err(_) => {}
        }
    }
    assert_eq!(got_exit, Some((Some(5), None)));
    // After exit the reader drains what the kernel still holds (bounded) despite the limit.
    assert!(bytes >= 6000, "output after exit was not drained: {bytes}");
    // The credit really was exhausted (nothing was ever acknowledged) when Exit was delivered.
    let in_flight = lock(&t.shared).flow.in_flight_bytes();
    assert!(in_flight >= limit, "credit was not exhausted: {in_flight}");
    assert!(!t.snapshot().running);
}

#[test]
fn dead_sink_is_detached_and_output_keeps_flowing_into_the_replay_buffer() {
    let t = Terminal::new(config(8 * 1024));
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = calls.clone();
    let sink: Arc<dyn TerminalSink> = Arc::new(move |_event| {
        counter.fetch_add(1, Ordering::SeqCst);
        false // webview is gone
    });
    t.attach(sink).unwrap();
    t.input("printf 'ONE_%s\\n' A; sleep 0.1; printf 'TWO_%s\\n' B\r")
        .unwrap();
    let s = until(&t, contains("TWO_B"));
    assert!(s.data.contains("ONE_A"));
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "a dead sink must be detached after one failure"
    );
}

#[test]
fn seq_wraps_and_replay_is_trimmed_on_char_boundaries() {
    let flow = Arc::new(Flow::new(1024));
    let mut state = State::new(flow.clone());
    state.seq = u32::MAX;
    let (seq, sink) = state.append("é");
    assert_eq!(seq, 0, "u32 wrap");
    assert!(sink.is_none());
    assert_eq!(
        flow.in_flight_bytes(),
        0,
        "nothing attached, nothing in flight"
    );
    let sink: Arc<dyn TerminalSink> = Arc::new(|_event: TerminalEvent| true);
    state.sink = Some(sink);
    let (seq, sink) = state.append(&"é".repeat(REPLAY_LIMIT));
    assert_eq!(seq, 1);
    assert!(sink.is_some());
    assert_eq!(flow.in_flight_bytes(), REPLAY_LIMIT * 2);
    assert!(state.data.len() <= REPLAY_LIMIT);
    assert!(state.data.is_char_boundary(0) && !state.data.contains('\u{fffd}'));
}

#[test]
fn utf8_input_exact_256_kib_boundary_is_accepted_and_plus_one_rejected() {
    struct Writer(Arc<Mutex<Vec<u8>>>);
    impl Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    // Use the real input method with an infallible writer: kernel PTY capacity must not turn
    // an accepted boundary into Busy and hide a wrong UTF-8/UTF-16 validation bound.
    for exact in [
        "x".repeat(INPUT_LIMIT_BYTES),
        "😀".repeat(INPUT_LIMIT_BYTES / 4),
    ] {
        let t = fixture();
        let bytes = Arc::new(Mutex::new(Vec::new()));
        {
            let mut state = lock(&t.shared);
            state.started = true;
            state.running = true;
        }
        t.run().unwrap().writer = Some(Box::new(Writer(bytes.clone())));
        assert_eq!(exact.len(), INPUT_LIMIT_BYTES);
        t.input(&exact).unwrap();
        assert_eq!(*bytes.lock().unwrap(), exact.as_bytes());
        let error = t.input(&(exact + "x")).unwrap_err();
        assert_eq!(error.code, ErrorCode::Invalid);
        assert_eq!(
            bytes.lock().unwrap().len(),
            INPUT_LIMIT_BYTES,
            "oversize wrote bytes"
        );
    }
}

#[test]
fn snapshot_is_first_on_each_attach_replaces_sink_and_counts_replay_credit() {
    let t = fixture();
    let (sink, old) = collecting_events(true);
    let first = t.attach(sink).unwrap();
    assert_eq!(
        old.recv_timeout(Duration::from_secs(3)).unwrap(),
        TerminalEvent::Snapshot(first.clone())
    );
    t.ack(first.seq);
    t.input("printf 'ATTACH_%s\\n' READY\r").unwrap();
    until(&t, contains("ATTACH_READY"));
    let (sink, new) = collecting_events(true);
    let attached = t.attach(sink).unwrap();
    assert_eq!(
        new.recv_timeout(Duration::from_secs(3)).unwrap(),
        TerminalEvent::Snapshot(attached.clone())
    );
    assert_eq!(first.pid, attached.pid);
    assert!(lock(&t.shared).flow.in_flight_bytes() >= attached.data.len());
    t.ack(attached.seq);
    t.input("printf 'NEW_%s\\n' SINK\r").unwrap();
    let mut text = attached.data.clone();
    let mut seq = attached.seq;
    while !text.contains("NEW_SINK") {
        match new.recv_timeout(Duration::from_secs(3)).unwrap() {
            TerminalEvent::Data { seq: got, data } => {
                seq = seq.wrapping_add(1);
                assert_eq!(got, seq);
                text.push_str(&data);
                t.ack(got);
            }
            other => panic!("unexpected {other:?}"),
        }
    }
    for event in old.try_iter() {
        if let TerminalEvent::Data { seq, .. } = event {
            assert!(flow::seq_le(seq, attached.seq));
        }
    }
    let s = t.snapshot();
    while seq != s.seq {
        if let TerminalEvent::Data { seq: got, data } =
            new.recv_timeout(Duration::from_secs(3)).unwrap()
        {
            seq = seq.wrapping_add(1);
            assert_eq!(got, seq);
            text.push_str(&data);
            t.ack(got);
        }
    }
    assert_eq!(s.data, text);
    let metrics = t.flow_metrics();
    assert!(metrics.read_bytes > 0 && metrics.peak_unacked_bytes >= attached.data.len());
}

#[test]
fn fixture_shell_refuses_to_run_without_the_test_env_gate() {
    // Defense in depth: `command()` re-checks HERMES_ISLAND_TEST through this function.
    for bad in [None, Some(""), Some("0"), Some("true"), Some("yes")] {
        assert_eq!(
            fixture_allowed(bad).unwrap_err().code,
            ErrorCode::Unsupported,
            "{bad:?}"
        );
    }
    assert!(fixture_allowed(Some("1")).is_ok());
}

#[test]
fn real_hermes_resolution_needs_a_launcher_and_is_never_spawned_here() {
    test_env();
    let home = scratch("no-hermes");
    let mut cfg = TerminalConfig::new(home.clone(), home.clone(), false);
    cfg.credit_limit = 1024;
    // No wrapper and no `hermes` on this PATH: the real one must never be found or spawned.
    cfg.search_path = Some(home.clone().into_os_string());
    let t = Terminal::new(cfg);
    let error = t.start().unwrap_err();
    assert_eq!(error.code, ErrorCode::Unavailable);
    assert!(t.pid().is_none());
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn empty_command_uses_private_wrapper_only_when_present_else_plain_hermes() {
    use std::os::unix::fs::PermissionsExt;
    let home = scratch("default-command");
    let bin = home.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let hermes = bin.join("hermes");
    fs::write(&hermes, "#!/bin/sh\n# fixture only; never executed\n").unwrap();
    fs::set_permissions(&hermes, fs::Permissions::from_mode(0o755)).unwrap();
    let path = std::ffi::OsString::from(&bin);
    // A fresh clone: no ~/Hermes/Dropdown/custom_hermes.py -> plain `hermes chat --tui`.
    let c = default_hermes_command(&home, &home, Some(&path)).unwrap();
    assert_eq!(
        c.get_argv(),
        &vec![hermes.clone().into_os_string(), "chat".into(), "--tui".into()]
    );
    // Neither the wrapper nor hermes on PATH: a clear, actionable error.
    let empty = home.join("empty");
    fs::create_dir_all(&empty).unwrap();
    let err = default_hermes_command(&empty, &empty, Some(empty.as_os_str())).unwrap_err();
    assert!(err.to_string().contains("Settings"), "{err}");
    // The author's machine: launcher + wrapper present -> unchanged behaviour.
    let launcher = home.join(".hermes/hermes-agent/.hermes/bin/hermes");
    let script = home.join("Hermes/Dropdown/custom_hermes.py");
    fs::create_dir_all(launcher.parent().unwrap()).unwrap();
    fs::create_dir_all(script.parent().unwrap()).unwrap();
    fs::write(&script, "# fixture only; never executed\n").unwrap();
    fs::write(&launcher, "#!/bin/sh\nexec /usr/bin/python3\n").unwrap();
    let c = default_hermes_command(&home, &home, Some(&path)).unwrap();
    assert_eq!(c.get_argv()[0], std::ffi::OsString::from("/usr/bin/python3"));
    assert_eq!(c.get_argv()[2], script.into_os_string());
    fs::remove_dir_all(home).unwrap();
}
