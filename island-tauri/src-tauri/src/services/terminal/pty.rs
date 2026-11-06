//! Low-level PTY helpers (port of the non-stateful parts of island-rust `terminal.rs`):
//! descriptors, readiness waiting, incremental UTF-8 decoding, environment scrubbing, Hermes
//! launcher resolution and owned-process-tree cleanup via pidfds.

use crate::error::IslandError;
use std::{
    fs,
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd, RawFd},
        unix::fs::PermissionsExt,
    },
    path::{Path, PathBuf},
};

pub fn checked_fd(fd: i32) -> Result<OwnedFd, IslandError> {
    if fd < 0 {
        return Err(IslandError::io(std::io::Error::last_os_error().to_string()));
    }
    // SAFETY: the successful syscall transfers exactly one new descriptor to this owner.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

/// No idle timer when pidfds are supported. The persistent eventfd wakes every waiter on
/// release. Returns whether the leader pidfd became readable (leader exited).
pub fn wait_ready(reader: Option<RawFd>, wake: &OwnedFd, leader: Option<&OwnedFd>) -> bool {
    let mut fds = [
        libc::pollfd {
            fd: reader.unwrap_or(-1),
            events: libc::POLLIN,
            revents: 0,
        },
        libc::pollfd {
            fd: wake.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        },
        libc::pollfd {
            fd: leader.map_or(-1, AsRawFd::as_raw_fd),
            events: libc::POLLIN,
            revents: 0,
        },
    ];
    // SAFETY: fds is a valid array of pollfd for the duration of the call. Older/restricted
    // kernels without pidfd support still have bounded cancellation (50 ms).
    unsafe {
        libc::poll(
            fds.as_mut_ptr(),
            fds.len() as libc::nfds_t,
            if leader.is_some() { -1 } else { 50 },
        );
    }
    fds[2].revents != 0
}

/// Environment keys scrubbed from the child (the original session's hooks), nothing else.
pub fn scrub_key(key: &str) -> bool {
    matches!(
        key,
        "NODE_OPTIONS"
            | "ELECTRON_RUN_AS_NODE"
            | "HERMES_TUI_QUERY"
            | "HERMES_TUI_IMAGE"
            | "HERMES_TUI_RESUME"
            // The island hands its own sink to its own child only (`Terminal::command`).
            | "HERMES_TUI_SIDECAR_URL"
    ) || key.starts_with("KITTY_")
        || key.starts_with("HERMES_BOOT_")
        || key.starts_with("HERMES_SESSION_")
}

/// `(python, script)` from the launcher's first matching `exec <abs>/python3 …` line.
pub fn resolve_hermes(home: &Path) -> Result<(PathBuf, PathBuf), IslandError> {
    let unresolved = || IslandError::unavailable("Cannot resolve Hermes Python from launcher");
    let launcher = fs::read_to_string(super::super::hermes_launcher(home))
        .map_err(|e| IslandError::unavailable(e.to_string()))?;
    let python = launcher
        .split_inclusive('\n')
        .filter_map(|line| line.strip_prefix("exec "))
        .find_map(|line| {
            let end = line.find(char::is_whitespace)?;
            let token = &line[..end];
            token.ends_with("python3").then(|| PathBuf::from(token))
        })
        .filter(|path| path.is_absolute())
        .ok_or_else(unresolved)?;
    let mode = fs::metadata(&python)
        .map_err(|e| IslandError::unavailable(e.to_string()))?
        .permissions()
        .mode();
    if mode & 0o111 == 0 {
        return Err(IslandError::unavailable("Hermes Python is not executable"));
    }
    let script = home.join("Hermes/Dropdown/custom_hermes.py");
    fs::File::open(&script).map_err(|e| IslandError::unavailable(e.to_string()))?;
    Ok((python, script))
}

/// Resolve a direct executable using the inherited app/systemd PATH, then trusted install
/// directories. Never invoke a shell or expand `~`/variables. Relative explicit paths use cwd.
pub fn resolve_custom_program(
    program: &str,
    home: &Path,
    cwd: &Path,
    path: Option<&std::ffi::OsStr>,
) -> Result<PathBuf, IslandError> {
    let executable = |p: &Path| {
        fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    };
    let absolute = |p: PathBuf| if p.is_absolute() { p } else { cwd.join(p) };
    let mut candidates = Vec::new();
    if program.contains('/') {
        candidates.push(absolute(PathBuf::from(program)));
    } else {
        if let Some(path) = path {
            candidates.extend(std::env::split_paths(path).map(|dir| absolute(dir.join(program))));
        }
        candidates.push(home.join(".local/bin").join(program));
        if let Some(parent) = super::super::hermes_launcher(home).parent() {
            candidates.push(parent.join(program));
        }
    }
    candidates
        .into_iter()
        .find(|p| executable(p))
        .ok_or_else(|| {
            IslandError::unavailable(format!(
                "Terminal executable not found or not executable: {program}"
            ))
        })
}

/// Incremental decoding keeps a multibyte scalar split across `read()` calls intact.
pub fn decode(pending: &mut Vec<u8>, eof: bool) -> String {
    let mut out = String::new();
    let mut offset = 0;
    while offset < pending.len() {
        match std::str::from_utf8(&pending[offset..]) {
            Ok(text) => {
                out.push_str(text);
                offset = pending.len();
            }
            Err(error) => {
                let end = offset + error.valid_up_to();
                // `valid_up_to` is a verified UTF-8 prefix; the lossy conversion never alters it.
                out.push_str(&String::from_utf8_lossy(&pending[offset..end]));
                offset = end;
                if let Some(n) = error.error_len() {
                    out.push('\u{fffd}');
                    offset += n;
                } else if eof {
                    out.push('\u{fffd}');
                    offset = pending.len();
                } else {
                    break;
                }
            }
        }
    }
    pending.drain(..offset);
    out
}

/// Split `text` into pieces of at most `max` bytes on char boundaries.
pub fn split_chunks(text: &str, max: usize) -> impl Iterator<Item = &str> {
    let mut rest = text;
    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        let mut cut = rest.len().min(max);
        while !rest.is_char_boundary(cut) {
            cut -= 1;
        }
        // `max` >= 4 always, so a boundary exists within 3 bytes.
        let (head, tail) = rest.split_at(cut);
        rest = tail;
        Some(head)
    })
}

#[derive(Clone, Copy, Debug)]
pub struct Process {
    pub pid: u32,
    pub parent: u32,
    pub session: u32,
    pub birth: u64,
}

pub fn process(pid: u32) -> Option<Process> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let fields: Vec<_> = stat.rsplit_once(") ")?.1.split_whitespace().collect();
    Some(Process {
        pid,
        parent: fields.get(1)?.parse().ok()?,
        session: fields.get(3)?.parse().ok()?,
        birth: fields.get(19)?.parse().ok()?,
    })
}

/// SIGKILL the PTY leader's session and every descendant started after it (also descendants
/// that created their own process group/session). Identity is pinned with pidfds, and a
/// recycled leader PID is never authority to signal anything.
pub fn kill_owned((pid, birth): (u32, u64)) {
    if process(pid).is_some_and(|p| p.birth != birth) {
        return;
    }
    let Ok(entries) = fs::read_dir("/proc") else {
        return;
    };
    let all: Vec<_> = entries
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.parse().ok())
        .filter_map(process)
        .collect();
    let mut owned: Vec<u32> = all
        .iter()
        .filter(|p| p.session == pid && p.birth >= birth)
        .map(|p| p.pid)
        .collect();
    loop {
        let next: Vec<_> = all
            .iter()
            .filter(|p| p.birth >= birth && owned.contains(&p.parent) && !owned.contains(&p.pid))
            .map(|p| p.pid)
            .collect();
        if next.is_empty() {
            break;
        }
        owned.extend(next);
    }
    for p in all.iter().rev().filter(|p| owned.contains(&p.pid)) {
        // SAFETY: plain syscall; the returned descriptor is closed below.
        let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, p.pid, 0) } as i32;
        if fd < 0 {
            continue;
        }
        // Pin the process identity before rechecking /proc: no PID-reuse kill race.
        if process(p.pid).is_some_and(|now| now.birth == p.birth && now.session == p.session) {
            // SAFETY: fd is a live pidfd; a null siginfo is permitted.
            unsafe {
                libc::syscall(
                    libc::SYS_pidfd_send_signal,
                    fd,
                    libc::SIGKILL,
                    std::ptr::null::<libc::siginfo_t>(),
                    0,
                );
            }
        }
        // SAFETY: fd was opened above and is owned here.
        unsafe {
            libc::close(fd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::scratch;

    #[test]
    fn scrub_policy_and_incremental_utf8() {
        for key in [
            "NODE_OPTIONS",
            "ELECTRON_RUN_AS_NODE",
            "KITTY_PID",
            "HERMES_BOOT_FOO",
            "HERMES_SESSION_ID",
            "HERMES_TUI_QUERY",
            "HERMES_TUI_IMAGE",
            "HERMES_TUI_RESUME",
        ] {
            assert!(scrub_key(key), "{key}");
        }
        for key in [
            "HOME",
            "HERMES_HOME",
            "HERMES_PROFILE",
            "TMPDIR",
            "PATH",
            "TERM_PROGRAM",
            "HERMES_TUI_THEME",
        ] {
            assert!(!scrub_key(key), "{key}");
        }
        let mut pending = vec![0xf0, 0x9f];
        assert_eq!(decode(&mut pending, false), "");
        pending.extend([0x98, 0x80, 0xff, 0xc3]);
        assert_eq!(decode(&mut pending, false), "😀\u{fffd}");
        assert_eq!(decode(&mut pending, true), "\u{fffd}");
    }

    #[test]
    fn chunks_respect_char_boundaries_and_the_limit() {
        let text = "é".repeat(100_000);
        let pieces: Vec<_> = split_chunks(&text, 65_536).collect();
        assert!(pieces.iter().all(|p| p.len() <= 65_536));
        assert_eq!(pieces.concat(), text);
        assert_eq!(split_chunks("", 10).count(), 0);
        assert_eq!(split_chunks("😀😀", 5).collect::<Vec<_>>(), ["😀", "😀"]);
    }

    #[test]
    fn launcher_matching_preserves_first_match_and_newline_whitespace() {
        let home = scratch("launcher");
        let launcher = home.join(".hermes/hermes-agent/.hermes/bin/hermes");
        let script = home.join("Hermes/Dropdown/custom_hermes.py");
        fs::create_dir_all(launcher.parent().unwrap()).unwrap();
        fs::create_dir_all(script.parent().unwrap()).unwrap();
        fs::write(&script, "# fixture only; never executed\n").unwrap();
        fs::write(&launcher, "#!/bin/sh\nexec /usr/bin/python3\n").unwrap();
        assert_eq!(
            resolve_hermes(&home).unwrap(),
            (PathBuf::from("/usr/bin/python3"), script)
        );
        fs::write(
            &launcher,
            "exec relative/python3 args\nexec /usr/bin/python3 args\n",
        )
        .unwrap();
        assert!(resolve_hermes(&home).is_err());
        fs::remove_dir_all(home).unwrap();
    }
}
