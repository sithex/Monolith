//! Bounded subprocess runner (port of island-rust `run_bounded`).
//!
//! No shell, renderer-selected executable, stderr forwarding, or unbounded pipes. The helper
//! runs in its own session; the whole process group is SIGKILLed on every exit path.

use crate::error::IslandError;
use std::{
    io::{ErrorKind, Read},
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::process::CommandExt,
    },
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

// Counts wait-loop iterations on this thread so a test can prove the loop blocks instead of
// spinning on an already-readable descriptor.
#[cfg(test)]
thread_local! {
    static WAKEUPS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn terminate(child: &mut Child, program: &str) {
    // SAFETY: setsid isolated this helper group; the leader is not reaped until after this
    // signal, so its PID/PGID cannot be reused for an unrelated group.
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    if let Err(error) = child.wait() {
        tracing::debug!(program, errno = ?error.raw_os_error(), "helper wait failed after kill");
    }
}

// Diagnostics stay in the log: IslandError messages also reach user-facing commands.
// Never log argv, environment, stderr or the executable's parent directories.
fn helper_error(
    program: &str,
    error: IslandError,
    os_error: Option<&std::io::Error>,
    status: Option<(i32, i32)>,
) -> IslandError {
    tracing::warn!(
        program,
        reason = %error.message,
        errno = ?os_error.and_then(std::io::Error::raw_os_error),
        exit_code = ?status.filter(|(code, _)| *code == libc::CLD_EXITED).map(|(_, value)| value),
        signal = ?status.filter(|(code, _)| *code != libc::CLD_EXITED).map(|(_, value)| value),
        "helper failed"
    );
    error
}

pub fn run_bounded(
    mut command: Command,
    timeout: Duration,
    max: usize,
    accept_error: bool,
) -> Result<Vec<u8>, IslandError> {
    let program = std::path::Path::new(command.get_program())
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // SAFETY: setsid is async-signal-safe and touches no Rust shared state.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }
    let mut child = command.spawn().map_err(|error| {
        helper_error(
            &program,
            IslandError::unavailable("Could not launch helper"),
            Some(&error),
            None,
        )
    })?;
    let (Some(mut output), Some(mut errors)) = (child.stdout.take(), child.stderr.take()) else {
        terminate(&mut child, &program);
        return Err(helper_error(
            &program,
            IslandError::unavailable("Helper output unavailable"),
            None,
            None,
        ));
    };
    for fd in [output.as_raw_fd(), errors.as_raw_fd()] {
        // SAFETY: these are live owned pipes; nonblocking reads prevent deadlocks.
        if unsafe { libc::fcntl(fd, libc::F_SETFL, libc::O_NONBLOCK) } < 0 {
            let error = std::io::Error::last_os_error();
            terminate(&mut child, &program);
            return Err(helper_error(
                &program,
                IslandError::io("Cannot read helper output"),
                Some(&error),
                None,
            ));
        }
    }
    // A pidfd wakes poll when a helper exits after closing both pipes. Keep the leader
    // unreaped until group cleanup; try_wait()/wait() would allow its PID to be reused.
    // Older kernels (or a denied pidfd_open) retain the existing bounded status check.
    // SAFETY: pidfd_open only observes our live, unreaped child and takes no pointers.
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, child.id(), 0) };
    let pidfd = if fd >= 0 {
        // SAFETY: a successful pidfd_open returns a new descriptor, owned here.
        Some(unsafe { OwnedFd::from_raw_fd(fd as i32) })
    } else {
        None
    };
    let start = Instant::now();
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 8192];
    let mut eof = [false; 2];
    loop {
        #[cfg(test)]
        WAKEUPS.with(|count| count.set(count.get() + 1));
        if start.elapsed() >= timeout {
            terminate(&mut child, &program);
            return Err(helper_error(
                &program,
                IslandError::timeout("Helper timed out"),
                None,
                None,
            ));
        }
        // Observe without reaping: retaining the session leader's PID prevents reuse before the
        // final, isolated process-group cleanup (including on success).
        let mut info = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
        // SAFETY: info is writable storage; child.id() is our unreaped child.
        if unsafe {
            libc::waitid(
                libc::P_PID,
                child.id(),
                info.as_mut_ptr(),
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        } < 0
        {
            let error = std::io::Error::last_os_error();
            terminate(&mut child, &program);
            return Err(helper_error(
                &program,
                IslandError::io("Cannot wait for helper"),
                Some(&error),
                None,
            ));
        }
        // SAFETY: successful waitid initialized info, including when no status is ready.
        let info = unsafe { info.assume_init() };
        // SAFETY: accessors read the initialized SIGCHLD fields from waitid.
        let status = unsafe { (info.si_pid() != 0).then(|| (info.si_code, info.si_status())) };
        for (index, pipe) in [&mut output as &mut dyn Read, &mut errors as &mut dyn Read]
            .into_iter()
            .enumerate()
        {
            if eof[index] {
                continue;
            }
            // Fair, bounded batches keep a noisy stream from starving the other stream or the
            // deadline. Stderr is drained, never forwarded/stored.
            for _ in 0..64 {
                match pipe.read(&mut chunk) {
                    Ok(0) => {
                        eof[index] = true;
                        break;
                    }
                    Ok(n) if index == 0 => {
                        if bytes.len().saturating_add(n) > max {
                            terminate(&mut child, &program);
                            return Err(helper_error(
                                &program,
                                IslandError::io("Helper output limit exceeded"),
                                None,
                                status,
                            ));
                        }
                        bytes.extend_from_slice(&chunk[..n]);
                    }
                    Ok(_) => {}
                    Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                    Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                    Err(error) => {
                        terminate(&mut child, &program);
                        return Err(helper_error(
                            &program,
                            IslandError::io("Cannot read helper output"),
                            Some(&error),
                            status,
                        ));
                    }
                }
            }
        }
        if let Some(status) = status {
            if eof.iter().all(|done| *done) {
                terminate(&mut child, &program);
                return if status == (libc::CLD_EXITED, 0) || accept_error {
                    Ok(bytes)
                } else {
                    Err(helper_error(
                        &program,
                        IslandError::unavailable("Helper failed"),
                        None,
                        Some(status),
                    ))
                };
            }
        }
        // Ignore EOF pipes and an already-observed exit: both stay permanently readable
        // and would spin while a descendant still holds the other pipe open.
        let mut fds = [
            libc::pollfd {
                fd: if eof[0] { -1 } else { output.as_raw_fd() },
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: if eof[1] { -1 } else { errors.as_raw_fd() },
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: if status.is_none() {
                    pidfd.as_ref().map_or(-1, AsRawFd::as_raw_fd)
                } else {
                    -1
                },
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        let remaining = timeout.saturating_sub(start.elapsed());
        let wait = if pidfd.is_none() && status.is_none() {
            remaining.min(Duration::from_millis(5))
        } else {
            remaining
        };
        // Round up so a sub-millisecond remainder blocks rather than busy-polling.
        let millis = wait.as_nanos().div_ceil(1_000_000).min(i32::MAX as u128) as i32;
        // SAFETY: fds contains only live owned descriptors or -1; poll borrows the array.
        if unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, millis) } < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == ErrorKind::Interrupted {
                continue;
            }
            terminate(&mut child, &program);
            return Err(helper_error(
                &program,
                IslandError::io("Cannot read helper output"),
                Some(&error),
                status,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::scratch;
    use serde_json::{json, Value};
    use std::{fs, path::PathBuf};

    #[test]
    fn helper_diagnostics_identify_program_errno_exit_and_signal_without_private_data() {
        use std::sync::{Arc, Mutex};
        #[derive(Clone)]
        struct Capture(Arc<Mutex<Vec<u8>>>);
        impl std::io::Write for Capture {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let writer = Capture(Arc::clone(&bytes));
        let subscriber = tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            let mut missing = Command::new("/private-fixture/missing-helper");
            missing
                .arg("secret-argument")
                .env("PRIVATE_FIXTURE", "secret-environment");
            let error = run_bounded(missing, Duration::from_secs(1), 1024, false).unwrap_err();
            assert_eq!(error.code, crate::error::ErrorCode::Unavailable);
            assert_eq!(error.message, "Could not launch helper");
            let mut command = Command::new("/bin/sh");
            command.args([
                "-c",
                "printf secret-output; printf secret-stderr >&2; exit 7",
            ]);
            let error = run_bounded(command, Duration::from_secs(1), 1024, false).unwrap_err();
            assert_eq!(error.message, "Helper failed");
            let mut command = Command::new("/bin/sh");
            command.args(["-c", "kill -TERM $$"]);
            assert_eq!(
                run_bounded(command, Duration::from_secs(1), 1024, false)
                    .unwrap_err()
                    .message,
                "Helper failed"
            );
        });
        let logs = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
        assert!(logs.contains("program=\"missing-helper\""), "{logs}");
        assert!(logs.contains("errno=Some(2)"), "{logs}");
        assert!(logs.contains("program=\"sh\""), "{logs}");
        assert!(logs.contains("exit_code=Some(7)"), "{logs}");
        assert!(logs.contains("signal=Some(15)"), "{logs}");
        for private in [
            "private-fixture",
            "secret-argument",
            "PRIVATE_FIXTURE",
            "secret-environment",
            "secret-output",
            "secret-stderr",
        ] {
            assert!(!logs.contains(private), "diagnostics disclosed {private}");
        }
    }

    #[test]
    fn helper_waits_for_exit_after_both_pipes_close_without_waiting_out_the_deadline() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "exec >/dev/null 2>&1; sleep 0.1; exit 7"]);
        let start = Instant::now();
        assert_eq!(
            run_bounded(command, Duration::from_secs(2), 1024, false)
                .unwrap_err()
                .message,
            "Helper failed"
        );
        assert!(start.elapsed() >= Duration::from_millis(90));
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn helper_blocks_without_spinning_while_a_descendant_holds_stdout() {
        // The shell exits at once (exit observed, stderr at EOF) while a descendant keeps
        // stdout open for 300 ms. Neither the exited pidfd nor the EOF stderr pipe may stay in
        // the poll set: both are permanently readable and would spin the loop thousands of
        // times. A blocking wait needs only a handful of wakeups.
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "(sleep 0.3; printf late) 2>/dev/null & exit 0"]);
        WAKEUPS.with(|count| count.set(0));
        let start = Instant::now();
        assert_eq!(
            run_bounded(command, Duration::from_secs(2), 1024, false).unwrap(),
            b"late"
        );
        assert!(start.elapsed() >= Duration::from_millis(250));
        let wakeups = WAKEUPS.with(std::cell::Cell::get);
        assert!(wakeups < 20, "wait loop spun {wakeups} times");
    }

    #[test]
    fn helper_deadline_still_applies_when_both_pipes_are_closed() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "exec >/dev/null 2>&1; sleep 30"]);
        let start = Instant::now();
        let error = run_bounded(command, Duration::from_millis(60), 1024, false).unwrap_err();
        assert_eq!(error.code, crate::error::ErrorCode::Timeout);
        assert_eq!(error.message, "Helper timed out");
        assert!(start.elapsed() >= Duration::from_millis(60));
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn helper_preserves_delayed_descendant_stdout() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "(sleep 0.15; printf delayed) & printf direct"]);
        assert_eq!(
            run_bounded(command, Duration::from_secs(2), 1024, false).unwrap(),
            b"directdelayed"
        );
    }

    #[test]
    fn helper_waits_for_descendant_stderr_eof() {
        let mut command = Command::new("/bin/sh");
        command.args([
            "-c",
            "(sleep 0.15; printf delayed >&2) >/dev/null & printf direct",
        ]);
        let start = Instant::now();
        assert_eq!(
            run_bounded(command, Duration::from_secs(2), 1024, false).unwrap(),
            b"direct"
        );
        assert!(start.elapsed() >= Duration::from_millis(140));
    }

    #[test]
    fn helper_cleans_descendants_on_timeout_and_normal_exit() {
        // Only test-owned sessions are signalled. The PID file is scratch-local; the shell's
        // sleep stays in its isolated group, never the test runner's.
        struct Fixture(PathBuf);
        impl Drop for Fixture {
            fn drop(&mut self) {
                if let Ok(ids) = fs::read_to_string(self.0.join("pids")) {
                    let ids: Vec<i32> = ids
                        .split_whitespace()
                        .filter_map(|s| s.parse().ok())
                        .collect();
                    // SAFETY: only signal the recorded descendant's isolated group if it still
                    // belongs to the session created by our helper.
                    if ids.len() == 2 {
                        unsafe {
                            if libc::getpgid(ids[1]) == ids[0] && ids[0] != libc::getpgrp() {
                                libc::kill(-ids[0], libc::SIGKILL);
                            }
                        }
                    }
                }
                if let Err(error) = fs::remove_dir_all(&self.0) {
                    eprintln!("cleanup of {:?} failed: {error}", self.0);
                }
            }
        }
        for (redirects, times_out) in [
            ("2>/dev/null", true),      // descendant retains stdout
            (">/dev/null", true),       // descendant retains only stderr
            (">/dev/null 2>&1", false), // both EOF, but descendant still alive
        ] {
            let fixture = Fixture(scratch("proc"));
            let mut command = Command::new("/bin/sh");
            command
                .args([
                    "-c",
                    &format!("sleep 30 {redirects} & printf '%s %s' \"$$\" \"$!\" > \"$1\""),
                    "fixture",
                ])
                .arg(fixture.0.join("pids"));
            let start = Instant::now();
            let result = run_bounded(command, Duration::from_millis(200), 1024, false);
            if times_out {
                assert_eq!(result.unwrap_err().message, "Helper timed out");
                assert!(start.elapsed() >= Duration::from_millis(200));
            } else {
                assert_eq!(result.unwrap(), b"");
            }
            assert!(start.elapsed() < Duration::from_secs(2));
            let ids = fs::read_to_string(fixture.0.join("pids")).unwrap();
            let ids: Vec<i32> = ids.split_whitespace().map(|s| s.parse().unwrap()).collect();
            assert!(
                !PathBuf::from(format!("/proc/{}", ids[0])).exists(),
                "leader not reaped"
            );
            // Orphan descendants are reaped by the system's reaper, not this process. Wait for
            // SIGKILL delivery; a zombie is no longer running.
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                let state = fs::read_to_string(format!("/proc/{}/stat", ids[1])).ok();
                let alive = state
                    .as_deref()
                    .and_then(|s| s.rsplit_once(") "))
                    .is_some_and(|(_, tail)| !tail.starts_with(['Z', 'X']));
                if !alive {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "descendant survived cleanup: {}",
                    ids[1]
                );
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }

    #[test]
    fn subprocess_limits_and_json_on_error() {
        let mut command = Command::new("/usr/bin/python3");
        command.args([
            "-I",
            "-c",
            "print('{\"provider\":null}'); raise SystemExit(1)",
        ]);
        let output = run_bounded(command, Duration::from_secs(2), 1024, true).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&output).unwrap(),
            json!({"provider":null})
        );
        let mut command = Command::new("/usr/bin/python3");
        command.args(["-I", "-c", "raise SystemExit(1)"]);
        assert!(run_bounded(command, Duration::from_secs(2), 1024, false).is_err());
        let mut command = Command::new("/usr/bin/python3");
        command.args(["-I", "-c", "import time; time.sleep(30)"]);
        let start = Instant::now();
        assert!(run_bounded(command, Duration::from_millis(60), 1024, false).is_err());
        assert!(start.elapsed() < Duration::from_secs(2));
        let mut command = Command::new("/usr/bin/python3");
        command.args(["-I", "-c", "print('x'*1000000)"]);
        assert!(run_bounded(command, Duration::from_secs(2), 100, false).is_err());
    }
}
