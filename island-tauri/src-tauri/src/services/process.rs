//! Bounded subprocess runner (port of island-rust `run_bounded`).
//!
//! No shell, renderer-selected executable, stderr forwarding, or unbounded pipes. The helper
//! runs in its own session; the whole process group is SIGKILLed on every exit path.

use crate::error::IslandError;
use std::{
    io::{ErrorKind, Read},
    os::{fd::AsRawFd, unix::process::CommandExt},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

fn terminate(child: &mut Child) {
    // SAFETY: setsid isolated this helper group; the leader is not reaped until after this
    // signal, so its PID/PGID cannot be reused for an unrelated group.
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    if let Err(error) = child.wait() {
        tracing::debug!(%error, "helper wait failed after kill");
    }
}

pub fn run_bounded(
    mut command: Command,
    timeout: Duration,
    max: usize,
    accept_error: bool,
) -> Result<Vec<u8>, IslandError> {
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
    let mut child = command
        .spawn()
        .map_err(|_| IslandError::unavailable("Could not launch helper"))?;
    let (Some(mut output), Some(mut errors)) = (child.stdout.take(), child.stderr.take()) else {
        terminate(&mut child);
        return Err(IslandError::unavailable("Helper output unavailable"));
    };
    for fd in [output.as_raw_fd(), errors.as_raw_fd()] {
        // SAFETY: these are live owned pipes; nonblocking reads prevent deadlocks.
        if unsafe { libc::fcntl(fd, libc::F_SETFL, libc::O_NONBLOCK) } < 0 {
            terminate(&mut child);
            return Err(IslandError::io("Cannot read helper output"));
        }
    }
    let start = Instant::now();
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 8192];
    let mut eof = [false; 2];
    loop {
        if start.elapsed() >= timeout {
            terminate(&mut child);
            return Err(IslandError::timeout("Helper timed out"));
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
            terminate(&mut child);
            return Err(IslandError::io("Cannot wait for helper"));
        }
        // SAFETY: successful waitid initialized info, including when no status is ready.
        let info = unsafe { info.assume_init() };
        // SAFETY: accessors read the initialized SIGCHLD fields from waitid.
        let status = unsafe {
            (info.si_pid() != 0).then(|| info.si_code == libc::CLD_EXITED && info.si_status() == 0)
        };
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
                            terminate(&mut child);
                            return Err(IslandError::io("Helper output limit exceeded"));
                        }
                        bytes.extend_from_slice(&chunk[..n]);
                    }
                    Ok(_) => {}
                    Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                    Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                    Err(_) => {
                        terminate(&mut child);
                        return Err(IslandError::io("Cannot read helper output"));
                    }
                }
            }
        }
        if let Some(status) = status {
            if eof.iter().all(|done| *done) {
                terminate(&mut child);
                return if status || accept_error {
                    Ok(bytes)
                } else {
                    Err(IslandError::unavailable("Helper failed"))
                };
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::scratch;
    use serde_json::{json, Value};
    use std::{fs, path::PathBuf};

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
