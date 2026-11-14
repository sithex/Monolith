//! Linux singleton/coexistence guard, independent of Tauri and the display.
//! Locks are acquired in fixed order: SPEC island.lock, then legacy instance.lock.
//! Existing status/socket artifacts are never unlinked to attempt takeover. This
//! deliberately fails closed even for stale artifacts; recovery is an explicit
//! operator action, not an assumption about another runtime's ownership.
#[cfg(target_os = "linux")]
mod linux {
    use crate::integrations::plasma::status_file::{Identity, StatusFile};
    use anyhow::{ensure, Context, Result};
    use serde_json::Value;
    use std::{
        fs::{self, File, OpenOptions},
        io::Read,
        os::{
            fd::AsRawFd,
            unix::{
                fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
                net::UnixListener,
            },
        },
        path::{Path, PathBuf},
    };

    pub struct InstanceGuard {
        // Locks are never unlinked: unlinking a lock inode permits a second owner.
        _locks: Vec<File>,
        socket: PathBuf,
        socket_identity: Option<Identity>,
        status: StatusFile,
        closed: bool,
    }
    pub fn acquire(data: &Path) -> Result<InstanceGuard> {
        InstanceGuard::acquire(data)
    }

    #[derive(Debug, PartialEq, Eq)]
    enum Probe {
        /// `connect` failed with ECONNREFUSED: the path is a socket nobody listens on.
        Refused,
        /// A listener exists (connect succeeded, or its backlog is full).
        Live,
        Unverifiable(i32),
    }
    /// Non-blocking `connect(2)`: a live listener with a full backlog must not stall startup.
    fn probe_socket(path: &Path) -> Result<Probe> {
        use std::os::{
            fd::{FromRawFd, OwnedFd},
            unix::ffi::OsStrExt,
        };
        let bytes = path.as_os_str().as_bytes();
        // SAFETY: an all-zero sockaddr_un is a valid value for this plain-data C struct.
        let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
        ensure!(
            bytes.len() < address.sun_path.len() && !bytes.contains(&0),
            "Socket path cannot be probed"
        );
        address.sun_family = libc::AF_UNIX as libc::sa_family_t;
        for (dst, src) in address.sun_path.iter_mut().zip(bytes) {
            *dst = *src as libc::c_char;
        }
        // SAFETY: plain syscall with constant arguments.
        let fd = unsafe {
            libc::socket(
                libc::AF_UNIX,
                libc::SOCK_STREAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
                0,
            )
        };
        ensure!(fd >= 0, "Cannot create the probe socket");
        // SAFETY: `fd` was just created, is valid and is owned by nothing else.
        let socket = unsafe { OwnedFd::from_raw_fd(fd) };
        // SAFETY: `address` is a fully initialized sockaddr_un and the length is its exact size.
        let result = unsafe {
            libc::connect(
                std::os::fd::AsRawFd::as_raw_fd(&socket),
                (&raw const address).cast::<libc::sockaddr>(),
                std::mem::size_of::<libc::sockaddr_un>() as libc::socklen_t,
            )
        };
        if result == 0 {
            return Ok(Probe::Live);
        }
        Ok(match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::ECONNREFUSED) => Probe::Refused,
            Some(libc::EAGAIN) => Probe::Live,
            other => Probe::Unverifiable(other.unwrap_or(-1)),
        })
    }

    /// SPEC §8 Phase 3 (revised): with BOTH locks held, stale artifacts are reclaimed only when
    /// the status PID is not alive AND the socket path is a socket owned by `euid` whose
    /// `connect` fails with ECONNREFUSED. Anything live, foreign-owned, non-socket or
    /// unverifiable (including a lone status or a lone socket) is refused and left untouched.
    /// No signal (not even signal 0) is ever sent to another process.
    pub fn reclaim_stale(data: &Path, euid: u32) -> Result<()> {
        reclaim_as(data, euid, euid)
    }
    /// Separate expected owners for the two artifacts, so each ownership check is testable on
    /// its own (production always passes the effective UID for both).
    pub fn reclaim_as(data: &Path, status_uid: u32, socket_uid: u32) -> Result<()> {
        reclaim_with_status(data, data, status_uid, socket_uid)
    }
    fn reclaim_with_status(
        data: &Path,
        state: &Path,
        status_uid: u32,
        socket_uid: u32,
    ) -> Result<()> {
        let status_path = state.join("status.json");
        let socket_path = data.join("panel.sock");
        let stat = |path: &Path, what: &str| -> Result<Option<fs::Metadata>> {
            match fs::symlink_metadata(path) {
                Ok(m) => Ok(Some(m)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(e).with_context(|| format!("Checking existing {what}")),
            }
        };
        let (status, socket) = match (
            stat(&status_path, "status")?,
            stat(&socket_path, "panel socket")?,
        ) {
            (None, None) => return Ok(()),
            (None, Some(_)) => {
                anyhow::bail!("Existing panel socket without a status; refusing takeover")
            }
            (Some(_), None) => {
                anyhow::bail!("Existing status without a stale panel socket; refusing takeover")
            }
            (Some(status), Some(socket)) => (status, socket),
        };
        ensure!(
            status.is_file() && status.uid() == status_uid && status.len() <= 1024 * 1024,
            "Existing status is not a regular file of ours; refusing takeover"
        );
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
            .open(&status_path)
            .context("Opening existing status")?;
        let mut bytes = Vec::new();
        file.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= 1024 * 1024, "Existing status exceeds limit");
        let pid = serde_json::from_slice::<Value>(&bytes)
            .ok()
            .and_then(|value| value["pid"].as_u64())
            .filter(|pid| *pid > 0 && *pid <= i32::MAX as u64)
            .context("Existing status names no verifiable owner; refusing takeover")?;
        ensure!(
            !Path::new("/proc").join(pid.to_string()).exists(),
            "Existing status has a live owner (pid {pid}); refusing takeover"
        );
        ensure!(
            socket.file_type().is_socket() && socket.uid() == socket_uid,
            "Existing panel socket is not a socket of ours; refusing takeover"
        );
        match probe_socket(&socket_path)? {
            Probe::Refused => {}
            Probe::Live => anyhow::bail!("Existing panel socket is live; refusing takeover"),
            Probe::Unverifiable(errno) => anyhow::bail!(
                "Existing panel socket cannot be verified (errno {errno}); refusing takeover"
            ),
        }
        // Nothing may have been swapped between verification and removal.
        ensure!(
            Identity::at(&socket_path)? == Identity(socket.dev(), socket.ino())
                && Identity::at(&status_path)? == Identity(status.dev(), status.ino()),
            "Artifacts changed during verification; refusing takeover"
        );
        fs::remove_file(&socket_path).context("Removing stale panel socket")?;
        fs::remove_file(&status_path).context("Removing stale status")?;
        tracing::info!(pid, "Reclaimed stale status and panel socket");
        Ok(())
    }
    /// What happened to a leftover `ingest.sock` at startup.
    #[derive(Debug, PartialEq, Eq)]
    pub enum IngestReclaim {
        Absent,
        Removed,
        /// Left untouched: live, foreign, not a socket, or unverifiable. Binding will then
        /// fail loudly instead of the island silently taking over someone else's path.
        Kept(&'static str),
    }
    /// A crash or power loss can leave `ingest.sock` behind with nobody listening; `bind`
    /// then refuses forever. Called only with BOTH instance locks held, so no legitimate
    /// island can own it. Removed only when it is a socket of ours that refuses connections
    /// and was not swapped between verification and removal.
    pub fn reclaim_stale_ingest(data: &Path, euid: u32) -> Result<IngestReclaim> {
        let path = data.join("ingest.sock");
        let meta = match fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(IngestReclaim::Absent),
            Err(e) => return Err(e).context("Checking existing ingest socket"),
        };
        if !meta.file_type().is_socket() || meta.uid() != euid {
            return Ok(IngestReclaim::Kept("not a socket of ours"));
        }
        match probe_socket(&path)? {
            Probe::Refused => {}
            Probe::Live => return Ok(IngestReclaim::Kept("live listener")),
            Probe::Unverifiable(_) => return Ok(IngestReclaim::Kept("unverifiable")),
        }
        if Identity::at(&path)? != Identity(meta.dev(), meta.ino()) {
            return Ok(IngestReclaim::Kept("changed during verification"));
        }
        fs::remove_file(&path).context("Removing stale ingest socket")?;
        Ok(IngestReclaim::Removed)
    }
    impl InstanceGuard {
        pub fn acquire(data: &Path) -> Result<Self> {
            Self::acquire_with_status(data, data)
        }
        pub fn acquire_with_status(data: &Path, state: &Path) -> Result<Self> {
            match fs::DirBuilder::new().mode(0o700).create(data) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e).context("Creating instance directory"),
            }
            let metadata = fs::symlink_metadata(data)?;
            // SAFETY: geteuid has no preconditions.
            ensure!(
                metadata.is_dir() && metadata.uid() == unsafe { libc::geteuid() },
                "Unsafe instance directory"
            );
            // Do not chmod a pre-existing directory behind a foreign owner.
            ensure!(
                metadata.permissions().mode() & 0o077 == 0,
                "Instance directory must be private (0700)"
            );
            let lock = |data: &Path, name: &str| -> Result<File> {
                let file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .truncate(false)
                    .mode(0o600)
                    .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
                    .open(data.join(name))
                    .context("Opening singleton lock")?;
                let m = file.metadata()?;
                // SAFETY: geteuid is always valid; flock operates on this owned fd.
                ensure!(
                    m.is_file()
                        && m.nlink() == 1
                        && m.uid() == unsafe { libc::geteuid() }
                        && m.permissions().mode() & 0o077 == 0,
                    "Unsafe singleton lock"
                );
                ensure!(
                    unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0,
                    "Another island owns the singleton lock"
                );
                let current = fs::symlink_metadata(data.join(name))?;
                ensure!(
                    current.dev() == m.dev() && current.ino() == m.ino(),
                    "Singleton lock was replaced"
                );
                Ok(file)
            };
            let spec_lock = lock(data, "island.lock")?;
            let legacy_lock = lock(data, "instance.lock")?;
            let mut locks = vec![spec_lock, legacy_lock];
            if data != state {
                // Different checkouts share XDG state. They must also share ownership.
                let parent = state.parent().context("State directory has no parent")?;
                locks.push(lock(parent, ".monolith-instance.lock")?);
            }
            // Both locks are ours: nothing legitimate can be using these artifacts. They are
            // reclaimed only when PROVEN stale; anything live, foreign or unverifiable is refused.
            // SAFETY: geteuid has no preconditions.
            if data == state {
                reclaim_stale(data, unsafe { libc::geteuid() })?;
            } else {
                reclaim_with_status(data, state, unsafe { libc::geteuid() }, unsafe {
                    libc::geteuid()
                })?;
            }
            // SAFETY: geteuid has no preconditions.
            match reclaim_stale_ingest(data, unsafe { libc::geteuid() })? {
                IngestReclaim::Removed => tracing::info!("Reclaimed stale ingest socket"),
                IngestReclaim::Kept(why) => {
                    tracing::warn!(why, "Leaving existing ingest socket untouched")
                }
                IngestReclaim::Absent => {}
            }
            // The crash-leftover sweep is allowed only after BOTH locks and successful
            // status/socket validation. A live PID's pending rename remains untouched.
            crate::status_temps::reclaim(state, unsafe { libc::geteuid() })?;
            let socket = data.join("panel.sock");
            Ok(Self {
                _locks: locks,
                socket,
                socket_identity: None,
                status: StatusFile::new(state)?,
                closed: false,
            })
        }
        pub fn socket_path(&self) -> &Path {
            &self.socket
        }
        /// Bind and record ownership together; no API can adopt a foreign socket.
        pub fn bind_panel_socket(&mut self) -> Result<UnixListener> {
            ensure!(
                !self.closed && self.socket_identity.is_none(),
                "Instance socket already bound or closed"
            );
            let listener =
                UnixListener::bind(&self.socket).context("Binding private panel socket")?;
            self.socket_identity = Some(Identity::at(&self.socket)?);
            fs::set_permissions(&self.socket, fs::Permissions::from_mode(0o600))?;
            listener.set_nonblocking(true)?;
            Ok(listener)
        }
        pub fn update_status(&mut self, snapshot: &Value) -> Result<()> {
            self.status.update(snapshot)
        }
        pub fn shutdown(&mut self) -> Result<()> {
            if self.closed {
                return Ok(());
            }
            // Do not let a status error suppress independent socket cleanup.
            let status_result = self.status.shutdown();
            let socket_result = if self
                .socket_identity
                .is_some_and(|id| id.matches(&self.socket))
            {
                fs::remove_file(&self.socket).context("Removing recorded own socket")
            } else {
                Ok(())
            };
            status_result?;
            socket_result?;
            self.closed = true;
            Ok(())
        }
    }
    impl Drop for InstanceGuard {
        fn drop(&mut self) {
            if let Err(error) = self.shutdown() {
                tracing::warn!(%error, "Instance cleanup failed");
            }
        }
    }
}
#[cfg(target_os = "linux")]
pub use linux::{acquire, InstanceGuard};
#[cfg(all(test, target_os = "linux"))]
pub(crate) use linux::{reclaim_as, reclaim_stale};

#[cfg(all(test, target_os = "linux"))]
#[allow(clippy::unwrap_used)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;
    use std::{
        fs,
        os::{
            fd::AsRawFd,
            unix::{
                fs::{symlink, DirBuilderExt, MetadataExt, PermissionsExt},
                net::{UnixListener, UnixStream},
            },
        },
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    pub(crate) struct Fixture(pub PathBuf);
    impl Fixture {
        pub(crate) fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "hi-phase3-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    #[test]
    fn stale_ingest_socket_is_reclaimed_but_live_and_foreign_paths_are_kept() {
        use super::linux::{reclaim_stale_ingest, IngestReclaim};
        let euid = unsafe { libc::geteuid() };
        let fixture = Fixture::new();
        let path = fixture.0.join("ingest.sock");
        assert_eq!(
            reclaim_stale_ingest(&fixture.0, euid).unwrap(),
            IngestReclaim::Absent
        );
        // Live listener: untouched.
        let live = UnixListener::bind(&path).unwrap();
        assert_eq!(
            reclaim_stale_ingest(&fixture.0, euid).unwrap(),
            IngestReclaim::Kept("live listener")
        );
        assert!(path.exists());
        // Listener gone, socket file left behind (crash / power loss): reclaimed.
        drop(live);
        // Parallel tests fork children; a child briefly holds inherited fds between fork and
        // exec, so the closed listener can look live for a moment. Production just keeps it.
        let mut outcome = reclaim_stale_ingest(&fixture.0, euid).unwrap();
        for _ in 0..100 {
            if outcome == IngestReclaim::Removed {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
            outcome = reclaim_stale_ingest(&fixture.0, euid).unwrap();
        }
        assert_eq!(outcome, IngestReclaim::Removed);
        assert!(!path.exists());
        // A stale socket owned by someone else is never removed.
        drop(UnixListener::bind(&path).unwrap());
        assert_eq!(
            reclaim_stale_ingest(&fixture.0, euid.wrapping_add(1)).unwrap(),
            IngestReclaim::Kept("not a socket of ours")
        );
        fs::remove_file(&path).unwrap();
        // A regular file at that path is never removed.
        fs::write(&path, b"not a socket").unwrap();
        assert_eq!(
            reclaim_stale_ingest(&fixture.0, euid).unwrap(),
            IngestReclaim::Kept("not a socket of ours")
        );
        fs::remove_file(&path).unwrap();
        // Full startup path: a stale ingest socket no longer blocks the next launch.
        // Wait until no forked child still holds the dropped listener (see above); once it
        // refuses it can never become live again, so `acquire` then sees a truly stale socket.
        drop(UnixListener::bind(&path).unwrap());
        for _ in 0..100 {
            if std::os::unix::net::UnixStream::connect(&path)
                .is_err_and(|e| e.kind() == std::io::ErrorKind::ConnectionRefused)
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let guard = acquire(&fixture.0).unwrap();
        assert!(!path.exists());
        drop(guard);
    }
    #[test]
    fn instance_lock_excludes_second_owner_without_unlinking_socket() {
        let fixture = Fixture::new();
        let mut first = acquire(&fixture.0).unwrap();
        let listener = first.bind_panel_socket().unwrap();
        let inode = fs::symlink_metadata(first.socket_path()).unwrap().ino();
        assert!(acquire(&fixture.0).is_err());
        assert!(UnixStream::connect(first.socket_path()).is_ok());
        assert_eq!(
            fs::symlink_metadata(first.socket_path()).unwrap().ino(),
            inode
        );
        assert_eq!(
            fs::metadata(first.socket_path())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        drop(listener);
        first.shutdown().unwrap();
        assert!(!first.socket_path().exists());
        assert!(fixture.0.join("island.lock").exists());
        assert!(fixture.0.join("instance.lock").exists());
    }
    #[test]
    fn both_spec_and_legacy_locks_independently_exclude_ownership() {
        for name in ["island.lock", "instance.lock"] {
            let fixture = Fixture::new();
            fs::write(fixture.0.join(name), "").unwrap();
            fs::set_permissions(fixture.0.join(name), fs::Permissions::from_mode(0o600)).unwrap();
            let file = fs::File::open(fixture.0.join(name)).unwrap();
            assert_eq!(
                unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
                0
            );
            assert!(acquire(&fixture.0).is_err(), "{name} must exclude us");
        }
    }
    #[test]
    fn live_socket_without_our_lock_is_never_unlinked() {
        let fixture = Fixture::new();
        let path = fixture.0.join("panel.sock");
        let _foreign = UnixListener::bind(&path).unwrap();
        let inode = fs::metadata(&path).unwrap().ino();
        assert!(acquire(&fixture.0).is_err());
        assert_eq!(fs::metadata(&path).unwrap().ino(), inode);
        assert!(UnixStream::connect(&path).is_ok());
    }
    #[test]
    fn live_legacy_status_without_socket_blocks_rust_and_preserves_status() {
        let fixture = Fixture::new();
        let path = fixture.0.join("status.json");
        let bytes = json!({"pid":std::process::id(),"anchor":{"native":false}}).to_string();
        fs::write(&path, &bytes).unwrap();
        assert!(acquire(&fixture.0).is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), bytes);
    }
    #[test]
    fn cleanup_never_removes_another_instances_status() {
        let fixture = Fixture::new();
        let mut instance = acquire(&fixture.0).unwrap();
        instance
            .update_status(&json!({"pid":std::process::id(),"mode":"idle"}))
            .unwrap();
        fs::remove_file(fixture.0.join("status.json")).unwrap();
        let bytes = json!({"pid":std::process::id(),"foreign":true}).to_string();
        fs::write(fixture.0.join("status.json"), &bytes).unwrap();
        instance.shutdown().unwrap();
        assert_eq!(
            fs::read_to_string(fixture.0.join("status.json")).unwrap(),
            bytes
        );
    }
    #[test]
    fn cleanup_preserves_replaced_foreign_socket() {
        let fixture = Fixture::new();
        let mut instance = acquire(&fixture.0).unwrap();
        let _own = instance.bind_panel_socket().unwrap();
        fs::remove_file(instance.socket_path()).unwrap();
        let _foreign = UnixListener::bind(instance.socket_path()).unwrap();
        let inode = fs::metadata(instance.socket_path()).unwrap().ino();
        instance.shutdown().unwrap();
        assert_eq!(fs::metadata(instance.socket_path()).unwrap().ino(), inode);
        assert!(UnixStream::connect(instance.socket_path()).is_ok());
    }
    /// A PID that certainly belonged to a process which has exited and been reaped.
    fn dead_pid() -> u32 {
        let mut child = std::process::Command::new("true").spawn().unwrap();
        let pid = child.id();
        child.wait().unwrap();
        assert!(!Path::new("/proc").join(pid.to_string()).exists());
        pid
    }
    /// A socket FILE nobody listens on: bind, then close without unlinking.
    fn stale_socket(path: &Path) {
        drop(UnixListener::bind(path).unwrap());
        assert!(path.exists());
        // A concurrently forking test (`dead_pid` spawns a child) can briefly hold a copy of the
        // closed listener's fd until its exec closes it (CLOEXEC); wait until it is truly stale.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            match UnixStream::connect(path) {
                Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => break,
                other => {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "socket never became stale: {other:?}"
                    );
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
        }
    }
    fn write_status(dir: &Path, pid: u32) {
        fs::write(
            dir.join("status.json"),
            json!({"pid":pid,"mode":"idle"}).to_string(),
        )
        .unwrap();
        fs::set_permissions(dir.join("status.json"), fs::Permissions::from_mode(0o600)).unwrap();
    }
    fn snapshot(dir: &Path) -> Vec<(String, u64, Vec<u8>)> {
        let mut rows: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap())
            .filter(|e| !e.file_name().to_string_lossy().ends_with(".lock"))
            .map(|e| {
                let meta = fs::symlink_metadata(e.path()).unwrap();
                let bytes = if meta.is_file() {
                    fs::read(e.path()).unwrap()
                } else {
                    Vec::new()
                };
                (
                    e.file_name().to_string_lossy().into_owned(),
                    meta.ino(),
                    bytes,
                )
            })
            .collect();
        rows.sort();
        rows
    }
    fn refused(dir: &Path, needle: &str) {
        let before = snapshot(dir);
        let error = match acquire(dir) {
            Ok(_) => panic!("takeover must be refused"),
            Err(error) => format!("{error:#}"),
        };
        assert!(error.contains(needle), "{error}");
        assert_eq!(
            snapshot(dir),
            before,
            "a refusal must leave every artifact untouched"
        );
    }

    /// Reclaim: dead status PID + a socket of ours that refuses connections.
    #[test]
    fn stale_status_and_refusing_socket_are_reclaimed_then_ownership_is_ours() {
        let fixture = Fixture::new();
        write_status(&fixture.0, dead_pid());
        stale_socket(&fixture.0.join("panel.sock"));
        let mut instance = acquire(&fixture.0).unwrap();
        assert!(!fixture.0.join("status.json").exists());
        assert!(!fixture.0.join("panel.sock").exists());
        // fully usable afterwards: bind our own socket and publish our own status
        let listener = instance.bind_panel_socket().unwrap();
        instance
            .update_status(&json!({"pid":std::process::id(),"mode":"idle"}))
            .unwrap();
        assert!(UnixStream::connect(instance.socket_path()).is_ok());
        drop(listener);
        instance.shutdown().unwrap();
        assert!(!fixture.0.join("status.json").exists());
    }
    #[test]
    fn reclaim_refuses_a_live_status_owner_even_with_a_stale_socket() {
        let fixture = Fixture::new();
        write_status(&fixture.0, std::process::id());
        stale_socket(&fixture.0.join("panel.sock"));
        refused(&fixture.0, "live owner");
    }
    #[test]
    fn reclaim_refuses_a_live_socket_even_with_a_dead_status_pid() {
        let fixture = Fixture::new();
        write_status(&fixture.0, dead_pid());
        let _live = UnixListener::bind(fixture.0.join("panel.sock")).unwrap();
        refused(&fixture.0, "socket is live");
        assert!(UnixStream::connect(fixture.0.join("panel.sock")).is_ok());
    }
    #[test]
    fn reclaim_refuses_non_socket_symlinked_and_missing_sockets() {
        // a regular file squatting on the socket name
        let fixture = Fixture::new();
        write_status(&fixture.0, dead_pid());
        fs::write(fixture.0.join("panel.sock"), "not a socket").unwrap();
        refused(&fixture.0, "not a socket of ours");
        // a symlink (even to a real stale socket) is not "a socket at the path"
        let fixture = Fixture::new();
        write_status(&fixture.0, dead_pid());
        stale_socket(&fixture.0.join("real.sock"));
        symlink("real.sock", fixture.0.join("panel.sock")).unwrap();
        refused(&fixture.0, "not a socket of ours");
        // a directory
        let fixture = Fixture::new();
        write_status(&fixture.0, dead_pid());
        fs::create_dir(fixture.0.join("panel.sock")).unwrap();
        refused(&fixture.0, "not a socket of ours");
        // no socket at all: nothing to prove the status stale against
        let fixture = Fixture::new();
        write_status(&fixture.0, dead_pid());
        refused(&fixture.0, "without a stale panel socket");
        // a lone socket without any status
        let fixture = Fixture::new();
        stale_socket(&fixture.0.join("panel.sock"));
        refused(&fixture.0, "socket without a status");
    }
    #[test]
    fn reclaim_refuses_unverifiable_or_non_regular_status() {
        for (label, content) in [
            ("garbage", "{not json".to_owned()),
            ("no pid", json!({"mode":"idle"}).to_string()),
            ("pid zero", json!({"pid":0}).to_string()),
            ("pid string", json!({"pid":"123"}).to_string()),
            ("pid out of range", json!({"pid":u64::MAX}).to_string()),
        ] {
            let fixture = Fixture::new();
            fs::write(fixture.0.join("status.json"), content).unwrap();
            stale_socket(&fixture.0.join("panel.sock"));
            let before = snapshot(&fixture.0);
            let error = match acquire(&fixture.0) {
                Ok(_) => panic!("{label}: takeover must be refused"),
                Err(error) => format!("{error:#}"),
            };
            assert!(error.contains("no verifiable owner"), "{label}: {error}");
            assert_eq!(snapshot(&fixture.0), before, "{label}: artifacts touched");
        }
        // oversized status
        let fixture = Fixture::new();
        fs::write(fixture.0.join("status.json"), vec![b' '; 1024 * 1024 + 1]).unwrap();
        stale_socket(&fixture.0.join("panel.sock"));
        refused(&fixture.0, "not a regular file of ours");
        // symlinked status (even naming a dead pid) is never followed
        let fixture = Fixture::new();
        fs::write(
            fixture.0.join("target"),
            json!({"pid":dead_pid()}).to_string(),
        )
        .unwrap();
        symlink("target", fixture.0.join("status.json")).unwrap();
        stale_socket(&fixture.0.join("panel.sock"));
        refused(&fixture.0, "not a regular file of ours");
    }
    /// Ownership is checked against the effective UID for BOTH artifacts, independently.
    #[test]
    fn reclaim_refuses_foreign_owned_artifacts() {
        let fixture = Fixture::new();
        write_status(&fixture.0, dead_pid());
        stale_socket(&fixture.0.join("panel.sock"));
        // SAFETY: geteuid has no preconditions.
        let ours = unsafe { libc::geteuid() };
        let before = snapshot(&fixture.0);
        let error = format!("{:#}", reclaim_as(&fixture.0, ours + 1, ours).unwrap_err());
        assert!(error.contains("not a regular file of ours"), "{error}");
        let error = format!("{:#}", reclaim_as(&fixture.0, ours, ours + 1).unwrap_err());
        assert!(error.contains("not a socket of ours"), "{error}");
        assert_eq!(snapshot(&fixture.0), before, "refusals touch nothing");
        reclaim_stale(&fixture.0, ours).unwrap();
        assert!(!fixture.0.join("status.json").exists());
        assert!(!fixture.0.join("panel.sock").exists());
    }
    #[test]
    fn unsafe_directory_symlink_lock_and_hardlink_lock_fail_closed() {
        let fixture = Fixture::new();
        let link = fixture.0.join("link");
        symlink(&fixture.0, &link).unwrap();
        assert!(acquire(&link).is_err());
        fs::write(fixture.0.join("target"), "untouched").unwrap();
        symlink("target", fixture.0.join("island.lock")).unwrap();
        assert!(acquire(&fixture.0).is_err());
        assert_eq!(
            fs::read_to_string(fixture.0.join("target")).unwrap(),
            "untouched"
        );
        fs::remove_file(fixture.0.join("island.lock")).unwrap();
        fs::hard_link(fixture.0.join("target"), fixture.0.join("island.lock")).unwrap();
        assert!(acquire(&fixture.0).is_err());
    }
}
