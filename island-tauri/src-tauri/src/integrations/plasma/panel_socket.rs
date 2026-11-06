//! Same-UID, one-request panel transport. Idle workers block in poll(2); a private
//! socketpair cancels readiness waits. Callbacks must use cached state / nonblocking
//! queue sends, never wait on the GUI or call a helper process.
use crate::singleton::InstanceGuard;
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::{
    io::{ErrorKind, Read, Write},
    os::{
        fd::AsRawFd,
        unix::net::{UnixListener, UnixStream},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub const MAX_COMMAND_BYTES: usize = 64;
pub const MAX_CLIENTS: usize = 16;
pub const MAX_REPLY_BYTES: usize = 16 * 1024;
const CLIENT_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelCommand {
    Status,
    Enter,
    Leave,
    Pin,
    PinAt(u32),
    /// The widget saw its panel dock/float (e.g. a maximized window): re-read its position.
    Moved,
    /// `scripts/deploy-ui.sh`: re-verify `dist/` and reload the island webview (`ui_assets`).
    ReloadUi,
}
/// Only one complete, allowlisted newline frame is legal per connection.
pub fn parse_command(bytes: &[u8]) -> Option<PanelCommand> {
    if bytes.len() > MAX_COMMAND_BYTES {
        return None;
    }
    match bytes {
        b"status\n" => Some(PanelCommand::Status),
        b"enter\n" => Some(PanelCommand::Enter),
        b"leave\n" => Some(PanelCommand::Leave),
        b"pin\n" => Some(PanelCommand::Pin),
        b"moved\n" => Some(PanelCommand::Moved),
        b"reload-ui\n" => Some(PanelCommand::ReloadUi),
        _ => {
            let text = std::str::from_utf8(bytes).ok()?;
            let id = text.strip_prefix("pin ")?.strip_suffix('\n')?;
            if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            id.parse::<u32>()
                .ok()
                .filter(|id| *id > 0)
                .map(PanelCommand::PinAt)
        }
    }
}

struct Client {
    stream: UnixStream,
    input: Vec<u8>,
    reply: Option<Vec<u8>>,
    written: usize,
    deadline: Instant,
}
struct LimitedReply(Vec<u8>);
impl Write for LimitedReply {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.0.len().saturating_add(bytes.len()) >= MAX_REPLY_BYTES {
            return Err(std::io::Error::other("Panel reply exceeds limit"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl Client {
    fn receive(
        &mut self,
        callback: &mut impl FnMut(PanelCommand) -> Result<Value>,
    ) -> Result<bool> {
        let mut closed = false;
        let mut chunk = [0; MAX_COMMAND_BYTES + 1];
        // At most 65 bytes retained/read before rejection. Drain currently readable
        // bytes before parsing, so pin\nstatus\n is never executed as pin.
        loop {
            let room = (MAX_COMMAND_BYTES + 1).saturating_sub(self.input.len());
            if room == 0 {
                break;
            }
            match self.stream.read(&mut chunk[..room]) {
                Ok(0) => {
                    closed = true;
                    break;
                }
                Ok(n) => self.input.extend_from_slice(&chunk[..n]),
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e)
                    if matches!(e.kind(), ErrorKind::ConnectionReset | ErrorKind::BrokenPipe) =>
                {
                    return Ok(false)
                }
                Err(e) => return Err(e).context("Reading panel request"),
            }
        }
        if self.input.len() > MAX_COMMAND_BYTES || self.input.contains(&b'\n') || closed {
            let response = match parse_command(&self.input) {
                Some(command) => match callback(command) {
                    Ok(value) => value,
                    Err(error) => {
                        tracing::warn!(%error, "Panel callback refused request");
                        json!({"ok":false})
                    }
                },
                None => json!({"ok":false}),
            };
            let mut bounded = LimitedReply(Vec::new());
            if serde_json::to_writer(&mut bounded, &response).is_err() {
                bounded.0 = b"{\"ok\":false}".to_vec();
            }
            let mut bytes = bounded.0;
            bytes.push(b'\n');
            self.reply = Some(bytes);
        }
        Ok(true)
    }
    fn send(&mut self) -> Result<bool> {
        let Some(reply) = &self.reply else {
            return Ok(true);
        };
        while self.written < reply.len() {
            match self.stream.write(&reply[self.written..]) {
                Ok(0) => return Ok(false),
                Ok(n) => self.written += n,
                Err(e) if e.kind() == ErrorKind::WouldBlock => return Ok(true),
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e)
                    if matches!(e.kind(), ErrorKind::ConnectionReset | ErrorKind::BrokenPipe) =>
                {
                    return Ok(false)
                }
                Err(e) => return Err(e).context("Writing panel reply"),
            }
        }
        Ok(false)
    }
}
fn same_uid(stream: &UnixStream) -> Result<bool> {
    let mut credentials = std::mem::MaybeUninit::<libc::ucred>::uninit();
    let mut size = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: getsockopt is given writable ucred storage and its exact size, and a live fd.
    let result = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            credentials.as_mut_ptr().cast(),
            &mut size,
        )
    };
    ensure!(
        result == 0 && size as usize == std::mem::size_of::<libc::ucred>(),
        "Cannot establish panel peer identity"
    );
    // SAFETY: successful getsockopt initialized the whole structure; geteuid is always valid.
    Ok(unsafe { credentials.assume_init().uid == libc::geteuid() })
}

pub struct PanelSocket {
    wake: UnixStream,
    worker: Option<JoinHandle<Result<()>>>,
}
impl PanelSocket {
    pub fn start(
        instance: &mut InstanceGuard,
        callback: impl FnMut(PanelCommand) -> Result<Value> + Send + 'static,
    ) -> Result<Self> {
        let listener = instance.bind_panel_socket()?;
        let (wake, cancel) = UnixStream::pair()?;
        wake.set_nonblocking(true)?;
        cancel.set_nonblocking(true)?;
        let worker = thread::Builder::new()
            .name("ripple-panel".into())
            .spawn(move || serve(listener, cancel, callback))?;
        Ok(Self {
            wake,
            worker: Some(worker),
        })
    }
    /// Call before instance.shutdown(), so the listening fd is closed first.
    pub fn shutdown(&mut self) -> Result<()> {
        let Some(worker) = self.worker.take() else {
            return Ok(());
        };
        // Closing the write half wakes poll even if its byte buffer is full.
        let wake_result = self.wake.shutdown(std::net::Shutdown::Write);
        let result = worker
            .join()
            .map_err(|_| anyhow::anyhow!("Panel worker panicked"))?;
        wake_result.context("Cancelling panel readiness")?;
        result
    }
}
impl Drop for PanelSocket {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown() {
            tracing::warn!(%error, "Panel shutdown failed");
        }
    }
}
fn serve(
    listener: UnixListener,
    cancel: UnixStream,
    mut callback: impl FnMut(PanelCommand) -> Result<Value>,
) -> Result<()> {
    let mut clients: Vec<Client> = Vec::new();
    loop {
        let mut fds = vec![
            libc::pollfd {
                fd: cancel.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: listener.as_raw_fd(),
                events: if clients.len() < MAX_CLIENTS {
                    libc::POLLIN
                } else {
                    0
                },
                revents: 0,
            },
        ];
        fds.extend(clients.iter().map(|client| libc::pollfd {
            fd: client.stream.as_raw_fd(),
            events: if client.reply.is_some() {
                libc::POLLOUT
            } else {
                libc::POLLIN
            },
            revents: 0,
        }));
        let now = Instant::now();
        // No clients => indefinite wait, not a periodic 10ms idle timer.
        let timeout = clients
            .iter()
            .map(|c| {
                c.deadline
                    .saturating_duration_since(now)
                    .as_millis()
                    .saturating_add(1)
                    .min(i32::MAX as u128) as i32
            })
            .min()
            .unwrap_or(-1);
        // SAFETY: fds is live initialized pollfd storage for the duration of poll.
        if unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, timeout) } < 0 {
            if std::io::Error::last_os_error().kind() == ErrorKind::Interrupted {
                continue;
            }
            return Err(std::io::Error::last_os_error()).context("Waiting for panel readiness");
        }
        if fds[0].revents != 0 {
            return Ok(());
        }
        ensure!(
            fds[1].revents & (libc::POLLERR | libc::POLLNVAL) == 0,
            "Panel listener failed"
        );
        // Process the fd snapshot before accepting new clients (indices stay aligned).
        let mut index = 0;
        clients.retain_mut(|client| {
            let ready = fds[index + 2].revents;
            index += 1;
            if Instant::now() >= client.deadline {
                return false;
            }
            if ready == 0 {
                return true;
            }
            let result = (|| -> Result<bool> {
                if ready & (libc::POLLNVAL | libc::POLLERR) != 0 {
                    return Ok(false);
                }
                if client.reply.is_none() && !client.receive(&mut callback)? {
                    return Ok(false);
                }
                client.send()
            })();
            match result {
                Ok(keep) => keep,
                Err(error) => {
                    tracing::warn!(%error, "Panel client failed");
                    false
                }
            }
        });
        if fds[1].revents & libc::POLLIN != 0 {
            for _ in clients.len()..MAX_CLIENTS {
                match listener.accept() {
                    Ok((stream, _)) => {
                        if !same_uid(&stream)? {
                            continue;
                        }
                        stream.set_nonblocking(true)?;
                        clients.push(Client {
                            stream,
                            input: Vec::with_capacity(MAX_COMMAND_BYTES + 1),
                            reply: None,
                            written: 0,
                            deadline: Instant::now() + CLIENT_TIMEOUT,
                        });
                    }
                    Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                    Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                    Err(e) => return Err(e).context("Accepting panel client"),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn source_specific_pin_is_strict_and_backwards_compatible() {
        use super::{parse_command as parse, PanelCommand};
        assert_eq!(parse(b"pin 42\n"), Some(PanelCommand::PinAt(42)));
        assert_eq!(parse(b"pin\n"), Some(PanelCommand::Pin));
        for input in [
            b"pin 0\n".as_slice(),
            b"pin -2\n",
            b"pin 4294967296\n",
            b"pin 2;quit\n",
            b"pin 2\nquit\n",
        ] {
            assert_eq!(parse(input), None);
        }
    }

    use super::*;
    use crate::singleton::{acquire, tests::Fixture};
    use std::{net::Shutdown, sync::mpsc};
    fn request(instance: &InstanceGuard, bytes: &[u8], eof: bool) -> Value {
        let mut stream = UnixStream::connect(instance.socket_path()).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        stream.write_all(bytes).unwrap();
        if eof {
            stream.shutdown(Shutdown::Write).unwrap();
        }
        let mut reply = Vec::new();
        stream.read_to_end(&mut reply).unwrap();
        serde_json::from_slice(&reply).unwrap()
    }
    #[test]
    fn panel_protocol_is_bounded_and_requires_a_complete_command() {
        assert_eq!(parse_command(b"pin\n"), Some(PanelCommand::Pin));
        assert_eq!(parse_command(b"status\n"), Some(PanelCommand::Status));
        assert_eq!(parse_command(b"moved\n"), Some(PanelCommand::Moved));
        assert_eq!(parse_command(b"reload-ui\n"), Some(PanelCommand::ReloadUi));
        assert_eq!(parse_command(b"reload-ui"), None);
        assert_eq!(parse_command(b"reload-ui x\n"), None);
        assert_eq!(parse_command(b"moved 4\n"), None);
        for input in [
            b"pin".as_slice(),
            b"launch\n",
            &[b'x'; 65],
            b"pin\nstatus\n",
            b"\npin\n",
            b"pin\n\n",
            b"pin\nextra",
        ] {
            assert_eq!(parse_command(input), None, "{input:?}");
        }
    }
    #[test]
    fn native_panel_roundtrip_emits_only_allowed_events() {
        let fixture = Fixture::new();
        let mut instance = acquire(&fixture.0).unwrap();
        let (tx, rx) = mpsc::channel();
        let mut panel = PanelSocket::start(&mut instance, move |command| {
            tx.send(command)?;
            Ok(json!({"online":true,"ok":true,"background":"#123456"}))
        })
        .unwrap();
        for (bytes, command) in [
            (b"status\n".as_slice(), PanelCommand::Status),
            (b"enter\n", PanelCommand::Enter),
            (b"leave\n", PanelCommand::Leave),
            (b"pin\n", PanelCommand::Pin),
        ] {
            assert_eq!(request(&instance, bytes, false)["background"], "#123456");
            assert_eq!(rx.recv_timeout(Duration::from_secs(1)).unwrap(), command);
        }
        for bytes in [
            b"launch\n".as_slice(),
            &[b'x'; 65],
            b"pin\nstatus\n",
            b"pin\nextra",
            b"pin\n\n",
        ] {
            assert_eq!(request(&instance, bytes, true)["ok"], false);
            assert!(rx.try_recv().is_err());
        }
        assert_eq!(
            request(&instance, b"pin", true)["ok"],
            false,
            "incomplete EOF must not execute"
        );
        assert!(rx.try_recv().is_err());
        panel.shutdown().unwrap();
        instance.shutdown().unwrap();
    }
    #[test]
    fn partial_clients_timeout_and_idle_shutdown_is_readiness_cancellable() {
        let fixture = Fixture::new();
        let mut instance = acquire(&fixture.0).unwrap();
        let mut panel = PanelSocket::start(&mut instance, |_| Ok(json!({"online":true}))).unwrap();
        let mut client = UnixStream::connect(instance.socket_path()).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        client.write_all(b"pi").unwrap();
        let mut reply = Vec::new();
        client.read_to_end(&mut reply).unwrap();
        assert!(reply.is_empty());
        let started = Instant::now();
        panel.shutdown().unwrap();
        assert!(started.elapsed() < Duration::from_secs(1));
        instance.shutdown().unwrap();
    }
    #[test]
    fn partial_command_does_not_block_other_clients_or_shutdown() {
        let fixture = Fixture::new();
        let mut instance = acquire(&fixture.0).unwrap();
        let mut panel = PanelSocket::start(&mut instance, |_| Ok(json!({"online":true}))).unwrap();
        let mut slow = UnixStream::connect(instance.socket_path()).unwrap();
        slow.write_all(b"sta").unwrap();
        assert_eq!(request(&instance, b"status\n", false)["online"], true);
        assert!(same_uid(&slow).unwrap());
        panel.shutdown().unwrap();
        instance.shutdown().unwrap();
    }
    #[test]
    fn panel_reply_serialization_never_allocates_beyond_its_wire_limit() {
        let mut bounded = LimitedReply(Vec::new());
        assert!(serde_json::to_writer(
            &mut bounded,
            &json!({"huge":"x".repeat(MAX_REPLY_BYTES * 2)})
        )
        .is_err());
        assert!(bounded.0.len() < MAX_REPLY_BYTES);
    }
    #[test]
    fn client_capacity_is_sixteen_and_shutdown_cancels_full_capacity() {
        let fixture = Fixture::new();
        let mut instance = acquire(&fixture.0).unwrap();
        let mut panel = PanelSocket::start(&mut instance, |_| Ok(json!({"online":true}))).unwrap();
        let mut held = Vec::new();
        for _ in 0..MAX_CLIENTS {
            let mut client = UnixStream::connect(instance.socket_path()).unwrap();
            client.write_all(b"sta").unwrap();
            held.push(client);
        }
        let mut waiting = UnixStream::connect(instance.socket_path()).unwrap();
        waiting.write_all(b"status\n").unwrap();
        waiting
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        let mut byte = [0];
        assert!(matches!(
            waiting.read(&mut byte).unwrap_err().kind(),
            ErrorKind::WouldBlock | ErrorKind::TimedOut
        ));
        let start = Instant::now();
        panel.shutdown().unwrap();
        assert!(start.elapsed() < Duration::from_secs(1));
        instance.shutdown().unwrap();
    }
    /// utime + stime (clock ticks) of one thread of this process.
    fn thread_cpu_ticks(tid: i32) -> u64 {
        let stat = std::fs::read_to_string(format!("/proc/self/task/{tid}/stat")).unwrap();
        let fields: Vec<_> = stat
            .rsplit_once(')')
            .unwrap()
            .1
            .split_whitespace()
            .collect();
        fields[11].parse::<u64>().unwrap() + fields[12].parse::<u64>().unwrap()
    }
    /// Controller audit (b): with `MAX_CLIENTS` slow clients connected, client
    /// `MAX_CLIENTS + 1` is NOT serviced and the worker does not spin on the (readable but
    /// un-acceptable) listener; once one slot frees, that client is serviced.
    #[test]
    fn client_beyond_capacity_waits_without_spinning_until_a_slot_frees() {
        use std::sync::{
            atomic::{AtomicI32, Ordering},
            Arc,
        };
        let fixture = Fixture::new();
        let mut instance = acquire(&fixture.0).unwrap();
        let tid = Arc::new(AtomicI32::new(0));
        let seen = Arc::clone(&tid);
        let mut panel = PanelSocket::start(&mut instance, move |_| {
            // SAFETY: gettid has no preconditions; the callback runs on the panel worker.
            seen.store(unsafe { libc::gettid() }, Ordering::SeqCst);
            Ok(json!({"online":true}))
        })
        .unwrap();
        assert_eq!(request(&instance, b"status\n", false)["online"], true);
        let worker = tid.load(Ordering::SeqCst);
        assert!(worker > 0, "callback must have run on the worker thread");
        let mut held = Vec::new();
        for _ in 0..MAX_CLIENTS {
            let mut client = UnixStream::connect(instance.socket_path()).unwrap();
            client.write_all(b"sta").unwrap();
            held.push(client);
        }
        let mut waiting = UnixStream::connect(instance.socket_path()).unwrap();
        waiting.write_all(b"status\n").unwrap();
        waiting
            .set_read_timeout(Some(Duration::from_millis(150)))
            .unwrap();
        let mut byte = [0];
        assert!(
            matches!(
                waiting.read(&mut byte).unwrap_err().kind(),
                ErrorKind::WouldBlock | ErrorKind::TimedOut
            ),
            "client {} must not be serviced while {MAX_CLIENTS} slots are held",
            MAX_CLIENTS + 1
        );
        // At capacity the listener is not polled: no busy loop (a 250 ms spin is ~25 ticks).
        let before = thread_cpu_ticks(worker);
        std::thread::sleep(Duration::from_millis(250));
        let spent = thread_cpu_ticks(worker) - before;
        assert!(spent <= 5, "panel worker spun at capacity: {spent} ticks");
        // Free exactly one slot by completing a held command.
        held[0].write_all(b"tus\n").unwrap();
        let mut reply = Vec::new();
        held[0]
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        held[0].read_to_end(&mut reply).unwrap();
        assert!(!reply.is_empty());
        waiting
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut reply = Vec::new();
        waiting.read_to_end(&mut reply).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&reply).unwrap()["online"],
            true,
            "the waiting client is serviced once a slot frees"
        );
        panel.shutdown().unwrap();
        instance.shutdown().unwrap();
    }
}
