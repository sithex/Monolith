//! Test-only control socket (Phase 2 acceptance tooling).
//!
//! Enabled ONLY when the process runs with `--test-instance` AND `HERMES_ISLAND_TEST=1`;
//! otherwise no socket is created and `test_probe_reply` answers `unsupported`.
//!
//! `<data_dir>/test-control.sock` (mode 0600, same-uid peers only via `SO_PEERCRED`),
//! line-delimited JSON, ≤ 64 KiB per line, one client at a time, read/write timeouts:
//!
//! * `{"op":"ping"}`                       → `{"ok":true,"pong":true}`
//! * `{"op":"terminal"}`                   → `{"ok":true,"pid":…,"running":…,"seq":…}` (no data)
//! * `{"op":"eval","js":"<expr>","timeoutMs":N}` → evaluates the JS *expression* in the island
//!   webview, awaits it, and the page answers through `test_probe_reply`;
//!   → `{"ok":true,"value":…}` or `{"ok":false,"error":…}` (also on timeout).
//!
//! Real pointer input is never synthesised here: the GUI tests click through EIS and only use
//! this socket to *read* DOM state or call `window.island.*`.

use crate::{cli::Cli, error::IslandError};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{BufRead, Read, Write},
    os::{
        fd::AsRawFd,
        unix::net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc, Arc, Mutex,
    },
    thread::JoinHandle,
    time::Duration,
};

pub const SOCKET_NAME: &str = "test-control.sock";
pub const MAX_LINE: usize = 64 * 1024;
const CLIENT_IDLE: Duration = Duration::from_secs(30);
const DEFAULT_EVAL_MS: u64 = 5000;

/// The only place the gate is decided.
pub fn hooks_enabled(cli: &Cli, env_value: Option<&str>) -> bool {
    cli.test_instance && env_value == Some("1")
}

/// What the hooks need from the running app (a fake in unit tests).
pub trait ProbeHost: Send + Sync + 'static {
    /// Run a script in the island webview (fire and forget).
    fn eval(&self, script: &str) -> Result<(), IslandError>;
    /// `{pid, running, seq}` — from Rust, never terminal data.
    fn terminal(&self) -> Value;
}

type Reply = Result<String, String>;

struct Inner {
    pending: Mutex<HashMap<u32, mpsc::Sender<Reply>>>,
    next_id: AtomicU32,
}

pub struct TestHooks {
    inner: Arc<Inner>,
    stop: Arc<AtomicBool>,
    worker: Mutex<Option<JoinHandle<()>>>,
    socket: PathBuf,
}

impl TestHooks {
    /// Start the hooks if (and only if) the gate is open. Creates nothing otherwise.
    pub fn maybe_start(
        enabled: bool,
        data_dir: &Path,
        host: Arc<dyn ProbeHost>,
    ) -> Result<Option<Self>, IslandError> {
        if !enabled {
            return Ok(None);
        }
        Self::start(data_dir, host).map(Some)
    }

    fn start(data_dir: &Path, host: Arc<dyn ProbeHost>) -> Result<Self, IslandError> {
        let socket = data_dir.join(SOCKET_NAME);
        // Our own test data dir: a stale socket from a crashed run may be replaced.
        if let Err(error) = std::fs::remove_file(&socket) {
            if error.kind() != std::io::ErrorKind::NotFound {
                return Err(IslandError::io(format!(
                    "cannot clear stale test socket: {error}"
                )));
            }
        }
        let listener = UnixListener::bind(&socket)
            .map_err(|e| IslandError::io(format!("cannot bind test socket: {e}")))?;
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| IslandError::io(format!("cannot restrict test socket: {e}")))?;
        }
        listener
            .set_nonblocking(true)
            .map_err(|e| IslandError::io(e.to_string()))?;
        let inner = Arc::new(Inner {
            pending: Mutex::new(HashMap::new()),
            next_id: AtomicU32::new(1),
        });
        let stop = Arc::new(AtomicBool::new(false));
        let worker = {
            let inner = inner.clone();
            let stop = stop.clone();
            std::thread::spawn(move || accept_loop(&listener, &inner, host.as_ref(), &stop))
        };
        Ok(Self {
            inner,
            stop,
            worker: Mutex::new(Some(worker)),
            socket,
        })
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket
    }

    /// Called by the `test_probe_reply` command with the page's answer.
    pub fn reply(&self, id: u32, ok: bool, value_json: String) -> Result<(), IslandError> {
        let sender = self
            .inner
            .pending
            .lock()
            .map_err(|_| IslandError::internal("probe table is poisoned"))?
            .remove(&id)
            .ok_or_else(|| IslandError::invalid("no pending probe with that id"))?;
        let message = if ok { Ok(value_json) } else { Err(value_json) };
        // The waiter may have timed out in the meantime; that is not an error here.
        if sender.send(message).is_err() {
            tracing::debug!(id, "probe reply after the waiter gave up");
        }
        Ok(())
    }

    /// Stop the listener, join it, and remove the socket (idempotent).
    pub fn shutdown(&self) {
        self.stop.store(true, Ordering::Release);
        let worker = self.worker.lock().ok().and_then(|mut w| w.take());
        if let Some(worker) = worker {
            if worker.join().is_err() {
                tracing::warn!("test-control thread panicked");
            }
        }
        if let Err(error) = std::fs::remove_file(&self.socket) {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(%error, "cannot remove test socket");
            }
        }
    }
}

impl Drop for TestHooks {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn peer_is_same_user(stream: &UnixStream) -> bool {
    let mut cred = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: cred/len are valid for the call; the fd is a live socket.
    let rc = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut cred as *mut libc::ucred).cast(),
            &mut len,
        )
    };
    // SAFETY: geteuid has no preconditions.
    rc == 0 && cred.uid == unsafe { libc::geteuid() }
}

fn accept_loop(listener: &UnixListener, inner: &Inner, host: &dyn ProbeHost, stop: &AtomicBool) {
    while !stop.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((stream, _)) => {
                if !peer_is_same_user(&stream) {
                    tracing::warn!("test-control: rejected a peer with a different uid");
                    continue;
                }
                serve(stream, inner, host, stop);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => {
                tracing::warn!(%error, "test-control accept failed");
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

fn serve(stream: UnixStream, inner: &Inner, host: &dyn ProbeHost, stop: &AtomicBool) {
    if stream.set_nonblocking(false).is_err()
        || stream.set_read_timeout(Some(CLIENT_IDLE)).is_err()
        || stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .is_err()
    {
        return;
    }
    let Ok(mut writer) = stream.try_clone() else {
        return;
    };
    let mut reader = std::io::BufReader::new(stream);
    while !stop.load(Ordering::Acquire) {
        let mut line = Vec::new();
        let read = reader
            .by_ref()
            .take(MAX_LINE as u64 + 1)
            .read_until(b'\n', &mut line);
        match read {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        let complete = line.last() == Some(&b'\n');
        let reply = if !complete && line.len() > MAX_LINE {
            // Cannot resynchronise after an oversize line: answer and hang up.
            if write_line(&mut writer, &json!({"ok": false, "error": "line too long"})).is_err() {
                tracing::debug!("test-control: client left before the error reply");
            }
            return;
        } else if !complete {
            return; // EOF in the middle of a line
        } else {
            handle(&line, inner, host, stop)
        };
        if write_line(&mut writer, &reply).is_err() {
            return;
        }
    }
}

fn write_line(writer: &mut UnixStream, value: &Value) -> std::io::Result<()> {
    let mut bytes = serde_json::to_vec(value).unwrap_or_else(|_| b"{\"ok\":false}".to_vec());
    bytes.push(b'\n');
    writer.write_all(&bytes)
}

fn fail(message: impl Into<String>) -> Value {
    json!({"ok": false, "error": message.into()})
}

fn handle(line: &[u8], inner: &Inner, host: &dyn ProbeHost, stop: &AtomicBool) -> Value {
    let Ok(request) = serde_json::from_slice::<Value>(line) else {
        return fail("invalid JSON");
    };
    match request["op"].as_str() {
        Some("ping") => json!({"ok": true, "pong": true}),
        Some("terminal") => {
            let mut value = host.terminal();
            value["ok"] = json!(true);
            value
        }
        Some("eval") => {
            let Some(js) = request["js"].as_str() else {
                return fail("eval needs a string `js`");
            };
            let timeout = request["timeoutMs"]
                .as_u64()
                .unwrap_or(DEFAULT_EVAL_MS)
                .clamp(50, 60_000);
            eval(inner, host, stop, js, Duration::from_millis(timeout))
        }
        Some(other) => fail(format!("unknown op `{other}`")),
        None => fail("missing `op`"),
    }
}

/// The wrapper the page runs; the newlines keep a trailing `//` comment in `js` harmless.
pub fn eval_script(id: u32, js: &str) -> String {
    format!(
        "(async()=>{{try{{const v=await (\n{js}\n);window.__TAURI_INTERNALS__.invoke('test_probe_reply',{{id:{id},ok:true,valueJson:JSON.stringify(v===undefined?null:v)}})}}catch(e){{window.__TAURI_INTERNALS__.invoke('test_probe_reply',{{id:{id},ok:false,valueJson:JSON.stringify(String(e&&e.message||e))}})}}}})()"
    )
}

fn eval(
    inner: &Inner,
    host: &dyn ProbeHost,
    stop: &AtomicBool,
    js: &str,
    timeout: Duration,
) -> Value {
    let id = inner.next_id.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = mpsc::channel();
    match inner.pending.lock() {
        Ok(mut pending) => {
            pending.insert(id, tx);
        }
        Err(_) => return fail("probe table is poisoned"),
    }
    let forget = || {
        if let Ok(mut pending) = inner.pending.lock() {
            pending.remove(&id);
        }
    };
    if let Err(error) = host.eval(&eval_script(id, js)) {
        forget();
        return fail(error.message);
    }
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if stop.load(Ordering::Acquire) {
            forget();
            return fail("shutting down");
        }
        let slice = deadline
            .saturating_duration_since(std::time::Instant::now())
            .min(Duration::from_millis(100));
        match rx.recv_timeout(slice) {
            Ok(Ok(json)) => {
                return match serde_json::from_str::<Value>(&json) {
                    Ok(value) => json!({"ok": true, "value": value}),
                    Err(_) => fail("page replied with invalid JSON"),
                }
            }
            Ok(Err(json)) => {
                let message = serde_json::from_str::<Value>(&json).unwrap_or(Value::String(json));
                return json!({"ok": false, "error": message});
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if std::time::Instant::now() >= deadline {
                    forget();
                    return fail("eval timed out (syntax error, or the page never replied)");
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                forget();
                return fail("probe channel closed");
            }
        }
    }
}

/// Production host: the island webview of a running app.
pub struct AppProbeHost {
    pub app: tauri::AppHandle,
    pub terminal: Arc<crate::services::terminal::Terminal>,
}

impl ProbeHost for AppProbeHost {
    fn eval(&self, script: &str) -> Result<(), IslandError> {
        use tauri::Manager;
        self.app
            .get_webview_window(crate::window::ISLAND_LABEL)
            .ok_or_else(|| IslandError::unavailable("island window is not open"))?
            .eval(script)
            .map_err(|e| IslandError::internal(format!("eval failed: {e}")))
    }

    fn terminal(&self) -> Value {
        let s = self.terminal.snapshot();
        let flow = self.terminal.flow_metrics();
        json!({"pid": s.pid, "running": s.running, "seq": s.seq,
            "peakUnackedBytes": flow.peak_unacked_bytes,
            "pausedReads": flow.paused_reads, "resumedReads": flow.resumed_reads,
            "readBytes": flow.read_bytes})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::scratch;
    use std::io::BufReader;

    struct FakeHost {
        hooks: Mutex<Option<Arc<TestHooks>>>,
        scripts: Mutex<Vec<String>>,
        /// Reply instantly with this (ok, valueJson); `None` = never reply.
        answer: Mutex<Option<(bool, String)>>,
    }

    impl FakeHost {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                hooks: Mutex::new(None),
                scripts: Mutex::new(Vec::new()),
                answer: Mutex::new(None),
            })
        }
    }

    impl ProbeHost for FakeHost {
        fn eval(&self, script: &str) -> Result<(), IslandError> {
            self.scripts.lock().unwrap().push(script.to_owned());
            let id: u32 = script
                .split("id:")
                .nth(1)
                .and_then(|s| s.split(',').next())
                .and_then(|s| s.parse().ok())
                .unwrap();
            let answer = self.answer.lock().unwrap().clone();
            let hooks = self.hooks.lock().unwrap().clone();
            if let (Some((ok, value)), Some(hooks)) = (answer, hooks) {
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(20));
                    hooks.reply(id, ok, value).unwrap();
                });
            }
            Ok(())
        }
        fn terminal(&self) -> Value {
            json!({"pid": 4242, "running": true, "seq": 9})
        }
    }

    fn start(dir: &Path, host: &Arc<FakeHost>) -> Arc<TestHooks> {
        let hooks = Arc::new(
            TestHooks::maybe_start(true, dir, host.clone())
                .unwrap()
                .unwrap(),
        );
        *host.hooks.lock().unwrap() = Some(hooks.clone());
        hooks
    }

    fn talk(socket: &Path, lines: &[&str]) -> Vec<Value> {
        let stream = UnixStream::connect(socket).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut writer = stream.try_clone().unwrap();
        let mut reader = BufReader::new(stream);
        lines
            .iter()
            .map(|line| {
                writer.write_all(line.as_bytes()).unwrap();
                writer.write_all(b"\n").unwrap();
                let mut reply = String::new();
                reader.read_line(&mut reply).unwrap();
                serde_json::from_str(&reply).unwrap()
            })
            .collect()
    }

    #[test]
    fn gate_requires_test_instance_and_the_env_value() {
        let test = |on| Cli {
            test_instance: on,
            ..Cli::default()
        };
        assert!(hooks_enabled(&test(true), Some("1")));
        for (cli, env) in [
            (test(true), None),
            (test(true), Some("")),
            (test(true), Some("0")),
            (test(true), Some("true")),
            (test(false), Some("1")),
            (test(false), None),
        ] {
            assert!(!hooks_enabled(&cli, env), "{:?} {env:?}", cli.test_instance);
        }
    }

    #[test]
    fn disabled_hooks_create_no_socket_and_nothing_listens() {
        let dir = scratch("hooks-off");
        let host = FakeHost::new();
        let hooks = TestHooks::maybe_start(false, &dir, host).unwrap();
        assert!(hooks.is_none());
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            0,
            "no file may be created"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn socket_is_private_answers_ping_and_terminal_and_is_removed_on_exit() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("hooks-on");
        let host = FakeHost::new();
        let hooks = start(&dir, &host);
        let socket = hooks.socket_path().to_path_buf();
        assert_eq!(socket, dir.join("test-control.sock"));
        assert_eq!(
            std::fs::metadata(&socket).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let replies = talk(
            &socket,
            &[
                r#"{"op":"ping"}"#,
                r#"{"op":"terminal"}"#,
                r#"{"op":"nope"}"#,
                "not json",
                r#"{"foo":1}"#,
                r#"{"op":"eval"}"#,
            ],
        );
        assert_eq!(replies[0], json!({"ok": true, "pong": true}));
        assert_eq!(
            replies[1],
            json!({"ok": true, "pid": 4242, "running": true, "seq": 9})
        );
        assert!(replies[1].get("data").is_none());
        for bad in &replies[2..] {
            assert_eq!(bad["ok"], false);
        }
        hooks.shutdown();
        assert!(!socket.exists());
        hooks.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn eval_round_trips_values_and_errors_and_times_out() {
        let dir = scratch("hooks-eval");
        let host = FakeHost::new();
        let hooks = start(&dir, &host);
        let socket = hooks.socket_path().to_path_buf();
        *host.answer.lock().unwrap() = Some((true, r#"{"a":[1,2]}"#.into()));
        let ok = &talk(
            &socket,
            &[r#"{"op":"eval","js":"document.title","timeoutMs":3000}"#],
        )[0];
        assert_eq!(*ok, json!({"ok": true, "value": {"a": [1, 2]}}));
        let script = host.scripts.lock().unwrap().last().cloned().unwrap();
        assert!(script.contains("document.title"));
        assert!(script.contains("test_probe_reply"));
        assert!(script.contains("valueJson:JSON.stringify(v===undefined?null:v)"));
        *host.answer.lock().unwrap() = Some((false, r#""boom""#.into()));
        let err = &talk(&socket, &[r#"{"op":"eval","js":"x()","timeoutMs":3000}"#])[0];
        assert_eq!(*err, json!({"ok": false, "error": "boom"}));
        *host.answer.lock().unwrap() = None;
        let timeout = &talk(&socket, &[r#"{"op":"eval","js":"1","timeoutMs":100}"#])[0];
        assert_eq!(timeout["ok"], false);
        assert!(timeout["error"].as_str().unwrap().contains("timed out"));
        assert!(
            hooks.inner.pending.lock().unwrap().is_empty(),
            "timed-out probe must be forgotten"
        );
        // A late reply for a forgotten probe is a clean error, not a panic.
        assert!(hooks.reply(9999, true, "1".into()).is_err());
        hooks.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn oversize_lines_are_refused_and_other_clients_wait_their_turn() {
        let dir = scratch("hooks-limits");
        let host = FakeHost::new();
        let hooks = start(&dir, &host);
        let socket = hooks.socket_path().to_path_buf();
        let mut big = UnixStream::connect(&socket).unwrap();
        big.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        big.write_all(&vec![b'x'; MAX_LINE + 10]).unwrap();
        let mut reply = String::new();
        BufReader::new(big.try_clone().unwrap())
            .read_line(&mut reply)
            .unwrap();
        assert!(reply.contains("line too long"), "{reply}");
        // The server hung up on that client and serves the next one.
        let ok = &talk(&socket, &[r#"{"op":"ping"}"#])[0];
        assert_eq!(ok["pong"], true);
        // Exactly MAX_LINE bytes (including the newline) are accepted.
        let mut line = String::from(r#"{"op":"ping","pad":""#);
        while line.len() < MAX_LINE - 3 {
            line.push('a');
        }
        line.push_str("\"}");
        assert!(line.len() < MAX_LINE);
        assert_eq!(talk(&socket, &[&line])[0]["pong"], true);
        hooks.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn eval_script_survives_trailing_line_comments() {
        let script = eval_script(7, "1+1 // comment");
        assert!(script.contains("\n1+1 // comment\n)"));
        assert!(script.contains("id:7,ok:true"));
    }
}
