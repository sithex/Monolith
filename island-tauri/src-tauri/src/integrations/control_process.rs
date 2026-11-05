//! Controls-only bounded, cancellable process ownership. No renderer-supplied executable.
//! A test runner has an explicit scratch-only bin directory and NEVER falls back to PATH.
use crate::error::IslandError;
use std::{path::PathBuf, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, BufReader},
    process::{Child, ChildStderr, ChildStdout, Command},
};

#[derive(Clone)]
pub struct Runner {
    fixture: Option<PathBuf>,
    enabled: bool,
}
pub struct Output {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}
pub struct OwnedChild {
    child: Child,
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if let Some(pid) = self.child.id() {
            // SAFETY: our child is an unreaped session leader; its PGID cannot be reused.
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
    }
}
impl OwnedChild {
    async fn exited(&self) -> Result<(), IslandError> {
        let pid = self
            .child
            .id()
            .ok_or_else(|| IslandError::internal("Helper already reaped"))?;
        loop {
            if exited_without_reaping(pid)? {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
    pub fn stdout(&mut self) -> Result<ChildStdout, IslandError> {
        self.child
            .stdout
            .take()
            .ok_or_else(|| IslandError::internal("Helper stdout absent"))
    }
    pub fn stderr(&mut self) -> Result<ChildStderr, IslandError> {
        self.child
            .stderr
            .take()
            .ok_or_else(|| IslandError::internal("Helper stderr absent"))
    }
    pub async fn finish(mut self) -> Result<i32, IslandError> {
        if let Some(pid) = self.child.id() {
            // Kill descendants BEFORE reaping the leader (PID reuse protection).
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
        self.child
            .wait()
            .await
            .map(|s| s.code().unwrap_or(-1))
            .map_err(|_| IslandError::io("Cannot reap helper"))
    }
}
fn exited_without_reaping(pid: u32) -> Result<bool, IslandError> {
    let mut info = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
    // SAFETY: own unreaped child and writable storage. The non-Send siginfo stays off await points.
    if unsafe {
        libc::waitid(
            libc::P_PID,
            pid,
            info.as_mut_ptr(),
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    } < 0
    {
        return Err(IslandError::io("Cannot observe Controls helper"));
    }
    Ok(unsafe { info.assume_init().si_pid() } != 0)
}
impl Runner {
    pub fn for_instance(test: bool, fixture: Option<PathBuf>) -> Self {
        if !test {
            return Self {
                fixture: None,
                enabled: true,
            };
        }
        match fixture.and_then(|p| p.canonicalize().ok()).filter(|p| {
            std::env::temp_dir()
                .canonicalize()
                .is_ok_and(|root| p.starts_with(&root) && p != &root)
        }) {
            Some(root) => Self {
                fixture: Some(root),
                enabled: true,
            },
            None => Self {
                fixture: None,
                enabled: false,
            },
        }
    }
    pub fn spawn(&self, argv: &[String]) -> Result<OwnedChild, IslandError> {
        let executable = argv
            .first()
            .ok_or_else(|| IslandError::invalid("Empty helper command"))?;
        if !self.enabled {
            return Err(IslandError::unavailable(
                "No private Controls runner configured",
            ));
        }
        if ![
            "pactl",
            "tailscale",
            "spectacle",
            "hermes-fish",
            "checkupdates",
            "paru",
        ]
        .contains(&executable.as_str())
        {
            return Err(IslandError::invalid("Controls executable not allowed"));
        }
        let mut command = Command::new(self.fixture.as_ref().map_or_else(
            || PathBuf::from(executable),
            |p| p.join("bin").join(executable),
        ));
        command
            .args(&argv[1..])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(root) = &self.fixture {
            command
                .env_clear()
                .env("PATH", root.join("bin"))
                .env("HOME", root.join("home"))
                .env("XDG_RUNTIME_DIR", root.join("run"))
                .env("XDG_CONFIG_HOME", root.join("config"))
                .env("XDG_DATA_HOME", root.join("data"))
                .env("XDG_CACHE_HOME", root.join("cache"))
                .env("HERMES_ISLAND_CONTROL_FIXTURE", root);
        }
        command.env("LC_ALL", "C");
        let parent = std::process::id() as libc::pid_t;
        // SAFETY: setsid, prctl and getppid are async-signal-safe; the hook does not allocate or
        // access shared state. PDEATHSIG ends subscriptions (`pactl subscribe`) if the island dies
        // without a clean shutdown (a crash, SIGKILL); they used to outlive it indefinitely.
        unsafe {
            command.pre_exec(move || {
                if libc::setsid() < 0
                    || libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) < 0
                {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::getppid() != parent {
                    return Err(std::io::Error::from_raw_os_error(libc::ESRCH));
                }
                Ok(())
            });
        }
        let child = command
            .spawn()
            .map_err(|_| IslandError::unavailable("Controls helper could not start"))?;
        Ok(OwnedChild { child })
    }
    pub async fn run(
        &self,
        argv: &[String],
        timeout: Duration,
        cap: usize,
    ) -> Result<Output, IslandError> {
        let mut child = self.spawn(argv)?;
        let stdout = child.stdout()?;
        let stderr = child.stderr()?;
        let read = async {
            let output = tokio::try_join!(capture(stdout, cap), capture(stderr, cap))?;
            child.exited().await?;
            Ok::<_, IslandError>(output)
        };
        let result = tokio::time::timeout(timeout, read).await;
        // All paths explicitly reap this owned leader, including caps and timeouts.
        let code = child.finish().await?;
        let (stdout, stderr) =
            result.map_err(|_| IslandError::timeout("Controls helper timed out"))??;
        Ok(Output {
            code,
            stdout,
            stderr,
        })
    }
    pub async fn text(&self, argv: &[String]) -> Result<String, IslandError> {
        let result = self
            .run(argv, Duration::from_secs(2), 4 * 1024 * 1024)
            .await?;
        if result.code != 0 {
            return Err(IslandError::unavailable("Controls helper failed"));
        }
        Ok(result.stdout)
    }
}
async fn capture(reader: impl AsyncRead + Unpin, cap: usize) -> Result<String, IslandError> {
    let mut bytes = Vec::new();
    reader
        .take(cap as u64 + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| IslandError::io("Controls pipe failed"))?;
    if bytes.len() > cap {
        return Err(IslandError::unavailable("Controls output limit exceeded"));
    }
    String::from_utf8(bytes).map_err(|_| IslandError::unavailable("Controls output is not UTF-8"))
}
/// Bounded subscription line; `read_line` by itself would allocate without a cap.
pub struct Lines {
    reader: BufReader<ChildStdout>,
    bytes: Vec<u8>,
}
impl Lines {
    pub fn new(reader: ChildStdout) -> Self {
        Self {
            reader: BufReader::new(reader),
            bytes: Vec::new(),
        }
    }
    pub async fn next(&mut self) -> Result<String, IslandError> {
        loop {
            let buf = self
                .reader
                .fill_buf()
                .await
                .map_err(|_| IslandError::io("Controls subscription failed"))?;
            if buf.is_empty() {
                return Err(IslandError::unavailable("Controls subscription ended"));
            }
            let end = buf.iter().position(|b| *b == b'\n').map(|n| n + 1);
            let count = end.unwrap_or(buf.len());
            if self.bytes.len().saturating_add(count) > 1024 {
                return Err(IslandError::unavailable("Controls subscription line limit"));
            }
            self.bytes.extend_from_slice(&buf[..count]);
            self.reader.consume(count);
            if end.is_some() {
                return String::from_utf8(std::mem::take(&mut self.bytes))
                    .map_err(|_| IslandError::unavailable("Controls event is not UTF-8"));
            }
        }
    }
}
pub async fn drain(reader: ChildStderr) -> Result<(), IslandError> {
    capture(reader, 64 * 1024).await.map(|_| ())
}
pub fn argv(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| (*s).to_owned()).collect()
}
pub fn fixture_from_env() -> Option<PathBuf> {
    std::env::var_os("HERMES_ISLAND_CONTROLS_FIXTURE").map(PathBuf::from)
}
pub fn join(task: Option<tauri::async_runtime::JoinHandle<()>>) -> Result<(), IslandError> {
    if let Some(mut task) = task {
        std::thread::spawn(move || {
            tauri::async_runtime::block_on(async {
                match tokio::time::timeout(Duration::from_secs(8), &mut task).await {
                    Ok(result) => {
                        result.map_err(|_| IslandError::internal("Controls worker panicked"))
                    }
                    Err(_) => {
                        task.abort();
                        if task.await.is_err() {
                            tracing::debug!("Controls worker cancelled");
                        }
                        Err(IslandError::timeout("Controls shutdown"))
                    }
                }
            })
        })
        .join()
        .map_err(|_| IslandError::internal("Controls joiner panicked"))??;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn a_subscription_dies_with_the_thread_that_started_it() {
        let root = std::env::temp_dir().join(format!("controls-pdeathsig-{}", std::process::id()));
        let bin = root.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let fake = bin.join("pactl");
        std::fs::write(&fake, "#!/bin/sh\nexec /usr/bin/sleep 30\n").unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let runner = Runner::for_instance(true, Some(root.clone()));
        // Stand-in for an owner that vanishes without cleanup: no Drop, no finish().
        let pid = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(async {
                let child = runner.spawn(&argv(&["pactl", "subscribe"])).unwrap();
                let pid = child.child.id().unwrap();
                std::mem::forget(child);
                pid
            })
        })
        .join()
        .unwrap();
        let gone = (0..100).any(|_| {
            std::thread::sleep(Duration::from_millis(20));
            exited_without_reaping(pid).unwrap_or(true)
        });
        // SAFETY: reap our own child either way so the test leaves no zombie or sleeper.
        unsafe {
            libc::kill(pid as i32, libc::SIGKILL);
            libc::waitpid(pid as i32, std::ptr::null_mut(), 0);
        }
        let _ = std::fs::remove_dir_all(&root);
        assert!(gone, "an orphaned subscription must not outlive its owner");
    }
}
