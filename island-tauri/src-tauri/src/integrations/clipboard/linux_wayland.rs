//! One owned Wayland connection/event queue, one reader, cancellable poll loop.
//! ext-data-control is preferred after a bounded registry sync; wlr is fallback.
//! Selection metadata is synchronized and allowed to settle both before reads and
//! before commit (KWin's XWayland bridge can append MIME announcements late).
#[path = "linux_wayland/connect.rs"]
mod connect;
#[path = "linux_wayland/protocol.rs"]
mod protocol;
#[cfg(test)]
#[path = "linux_wayland/tests.rs"]
mod tests;
use super::{
    history::{ClipContent, ClipPayload},
    Shared,
};
use crate::error::{ErrorCode, IslandError};
use base64::Engine;
use std::{
    collections::VecDeque,
    fs::File,
    io::{self, Cursor, Read, Write},
    os::{
        fd::{AsFd, AsRawFd, OwnedFd},
        unix::net::UnixStream,
    },
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use wayland_client::{
    protocol::{wl_callback, wl_registry, wl_seat},
    Connection, EventQueue, Proxy, QueueHandle,
};
use wayland_protocols::ext::data_control::v1::client::{
    ext_data_control_device_v1::{self, ExtDataControlDeviceV1 as ExtDevice},
    ext_data_control_manager_v1::ExtDataControlManagerV1 as ExtManager,
    ext_data_control_offer_v1::{self, ExtDataControlOfferV1 as ExtOffer},
    ext_data_control_source_v1::{self, ExtDataControlSourceV1 as ExtSource},
};
use wayland_protocols_wlr::data_control::v1::client::{
    zwlr_data_control_device_v1::{self, ZwlrDataControlDeviceV1 as WlrDevice},
    zwlr_data_control_manager_v1::ZwlrDataControlManagerV1 as WlrManager,
    zwlr_data_control_offer_v1::{self, ZwlrDataControlOfferV1 as WlrOffer},
    zwlr_data_control_source_v1::{self, ZwlrDataControlSourceV1 as WlrSource},
};
const HINT: &str = "x-kde-passwordManagerHint";
const QUIET: Duration = Duration::from_millis(100);
const MAX_OFFERS: usize = 32;
const MAX_MIMES: usize = 64;
const MAX_TX: usize = 8;
const TEXT_CAP: usize = 1024 * 1024;
const PNG_CAP: usize = 8 * 1024 * 1024;

struct CopyRequest {
    content: ClipContent,
    deadline: Instant,
    reply: SyncSender<Result<(), IslandError>>,
}
pub(super) struct Worker {
    stop: Arc<AtomicBool>,
    wake: UnixStream,
    commands: SyncSender<CopyRequest>,
    task: Option<JoinHandle<()>>,
    shared: Arc<Shared>,
}
impl Worker {
    pub fn start(shared: Arc<Shared>) -> Result<Self, IslandError> {
        let (wake, mut cancellation) = UnixStream::pair()
            .map_err(|_| IslandError::io("Cannot create clipboard cancellation channel"))?;
        wake.set_nonblocking(true)
            .and_then(|()| cancellation.set_nonblocking(true))
            .map_err(|_| IslandError::io("Cannot configure clipboard cancellation channel"))?;
        let stop = Arc::new(AtomicBool::new(false));
        let halted = stop.clone();
        let state = shared.clone();
        let (commands, receiver) = mpsc::sync_channel::<CopyRequest>(32);
        let task = thread::Builder::new()
            .name("hi-clipboard".into())
            .spawn(move || {
                state.status_change(|s| s.watcher_running = true);
                let mut backoff = 1u64;
                while !halted.load(Ordering::Acquire) && state.enabled.load(Ordering::Acquire) {
                    let result = match connect::connect(&state, &mut cancellation, &halted) {
                        Ok(connection) => {
                            state.status_change(|s| {
                                s.connections_open = s.connections_open.saturating_add(1)
                            });
                            let result = run_connection(
                                connection,
                                state.clone(),
                                &mut cancellation,
                                &halted,
                                &receiver,
                            );
                            state.status_change(|s| {
                                s.connections_open = s.connections_open.saturating_sub(1)
                            });
                            result
                        }
                        Err(error) => Err(error),
                    };
                    state.status_change(|s| {
                        s.available = false;
                        if let Err(error) = &result {
                            s.detail = error.message.clone();
                        }
                    });
                    if halted.load(Ordering::Acquire) || !state.enabled.load(Ordering::Acquire) {
                        break;
                    }
                    let mut fds = [pfd(cancellation.as_raw_fd(), libc::POLLIN)];
                    if let Err(error) = poll(&mut fds, Duration::from_secs(backoff)) {
                        tracing::warn!(%error,"Clipboard reconnect wait failed");
                        break;
                    }
                    drain(&mut cancellation);
                    // Reject commands promptly while disconnected; never write after their timeout.
                    while let Ok(request) = receiver.try_recv() {
                        if request
                            .reply
                            .send(Err(IslandError::unavailable(
                                "Wayland clipboard unavailable",
                            )))
                            .is_err()
                        {
                            tracing::debug!("Clipboard copy caller left");
                        }
                    }
                    backoff = (backoff * 2).min(60);
                }
                state.status_change(|s| {
                    s.watcher_running = false;
                    s.available = false;
                    s.connections_open = 0;
                });
            })
            .map_err(|_| IslandError::io("Cannot start clipboard watcher"))?;
        Ok(Self {
            stop,
            wake,
            commands,
            task: Some(task),
            shared,
        })
    }
    pub fn is_finished(&self) -> bool {
        self.task.as_ref().is_none_or(JoinHandle::is_finished)
    }
    pub fn copy(&self, content: ClipContent) -> Result<(), IslandError> {
        if !self.shared.status.borrow().available {
            return Err(IslandError::unavailable("Wayland clipboard unavailable"));
        }
        let (reply, result) = mpsc::sync_channel(1);
        self.commands
            .try_send(CopyRequest {
                content,
                deadline: Instant::now() + Duration::from_secs(2),
                reply,
            })
            .map_err(|_| {
                IslandError::new(ErrorCode::Busy, "Clipboard copy queue full or stopped")
            })?;
        wake(&self.wake)?;
        result
            .recv_timeout(Duration::from_millis(2500))
            .map_err(|_| IslandError::timeout("Clipboard copy timed out"))?
    }
    pub fn stop(&mut self) -> Result<(), IslandError> {
        self.stop.store(true, Ordering::Release);
        wake(&self.wake)?;
        if let Some(task) = self.task.take() {
            task.join()
                .map_err(|_| IslandError::internal("Clipboard watcher panicked"))?;
        }
        Ok(())
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        if let Err(error) = self.stop() {
            tracing::error!(%error,"Clipboard worker teardown failed");
        }
    }
}
fn wake(mut stream: &UnixStream) -> Result<(), IslandError> {
    match stream.write(&[1]) {
        Ok(_) => Ok(()),
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::WouldBlock
                    | io::ErrorKind::BrokenPipe
                    | io::ErrorKind::ConnectionReset
            ) =>
        {
            Ok(())
        }
        Err(_) => Err(IslandError::io("Cannot wake clipboard watcher")),
    }
}
fn drain(stream: &mut UnixStream) {
    let mut b = [0; 128];
    loop {
        match stream.read(&mut b) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
    }
}

#[derive(Clone)]
enum Offer {
    Ext(ExtOffer),
    Wlr(WlrOffer),
}
impl Offer {
    fn id(&self) -> u32 {
        match self {
            Self::Ext(o) => o.id().protocol_id(),
            Self::Wlr(o) => o.id().protocol_id(),
        }
    }
    fn destroy(&self) {
        match self {
            Self::Ext(o) => o.destroy(),
            Self::Wlr(o) => o.destroy(),
        }
    }
    fn receive(&self, mime: String, fd: std::os::fd::BorrowedFd<'_>) {
        match self {
            Self::Ext(o) => o.receive(mime, fd),
            Self::Wlr(o) => o.receive(mime, fd),
        }
    }
}
enum Manager {
    Ext(ExtManager),
    Wlr(WlrManager),
}
enum Device {
    Ext(ExtDevice),
    Wlr(WlrDevice),
}
enum Source {
    Ext(ExtSource),
    Wlr(WlrSource),
}
impl Source {
    fn id(&self) -> u32 {
        match self {
            Self::Ext(s) => s.id().protocol_id(),
            Self::Wlr(s) => s.id().protocol_id(),
        }
    }
    fn destroy(self) {
        match self {
            Self::Ext(s) => s.destroy(),
            Self::Wlr(s) => s.destroy(),
        }
    }
}
struct Record {
    offer: Offer,
    mimes: Vec<String>,
    last_mime: Instant,
    overflow: bool,
    created: Instant,
}
struct Selection {
    id: u32,
    seq: u32,
    epoch: u32,
    deadline: Instant,
    ready_at: Instant,
    metadata_done: bool,
    commit_done: bool,
    hint_checked: bool,
    payload: Option<ClipPayload>,
    payload_mime: Option<String>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum ReadKind {
    Hint,
    Text,
    Png,
    Files,
}
struct Rx {
    reader: UnixStream,
    kind: ReadKind,
    mime: String,
    data: Vec<u8>,
    deadline: Instant,
}
struct Tx {
    file: File,
    data: Arc<[u8]>,
    offset: usize,
    deadline: Instant,
}
struct SendData {
    mimes: Vec<(String, Arc<[u8]>)>,
}
#[derive(Clone, Copy)]
enum Sync {
    Registry,
    Metadata(u32),
    Commit(u32),
    Fence(u32),
}
struct State {
    shared: Arc<Shared>,
    globals: u16,
    ext: Option<(u32, u32)>,
    wlr: Option<(u32, u32)>,
    seat: Option<(u32, u32)>,
    registry_done: bool,
    manager: Option<Manager>,
    device: Option<Device>,
    source: Option<Source>,
    offers: Vec<Record>,
    selected: Option<Selection>,
    rx: Option<Rx>,
    tx: Vec<Tx>,
    seq: u32,
    failed: bool,
    seen_epoch: u32,
    capture_ready: bool,
    copy_times: VecDeque<Instant>,
}
impl State {
    fn new(shared: Arc<Shared>) -> Self {
        let seen_epoch = shared.epoch.load(Ordering::Acquire);
        Self {
            shared,
            globals: 0,
            ext: None,
            wlr: None,
            seat: None,
            registry_done: false,
            manager: None,
            device: None,
            source: None,
            offers: vec![],
            selected: None,
            rx: None,
            tx: vec![],
            seq: 0,
            failed: false,
            seen_epoch,
            capture_ready: true,
            copy_times: VecDeque::with_capacity(32),
        }
    }
    fn fence(&mut self, connection: &Connection, qh: &QueueHandle<Self>) {
        let epoch = self.shared.epoch.load(Ordering::Acquire);
        if epoch != self.seen_epoch {
            self.seen_epoch = epoch;
            self.capture_ready = false;
            self.cancel_selection();
            // Discard selection events queued while paused; never replay on resume.
            connection.display().sync(qh, Sync::Fence(epoch));
        }
    }
    fn dropped(&self) {
        self.shared
            .status_change(|s| s.dropped = s.dropped.saturating_add(1));
    }
    fn sensitive(&mut self) {
        self.shared
            .status_change(|s| s.sensitive_skipped = s.sensitive_skipped.saturating_add(1));
        self.cancel_selection();
    }
    fn new_offer(&mut self, offer: Offer) {
        if self.offers.len() >= MAX_OFFERS {
            offer.destroy();
            self.dropped();
            return;
        }
        self.offers.push(Record {
            offer,
            mimes: vec![],
            last_mime: Instant::now(),
            overflow: false,
            created: Instant::now(),
        });
    }
    fn mime(&mut self, id: u32, mime: String) {
        if let Some(record) = self.offers.iter_mut().find(|r| r.offer.id() == id) {
            record.last_mime = Instant::now();
            if mime.len() > 256 || record.mimes.len() >= MAX_MIMES {
                record.overflow = true;
            } else if !record.mimes.contains(&mime) {
                record.mimes.push(mime);
            }
        }
    }
    fn select(&mut self, offer: Option<Offer>, connection: &Connection, qh: &QueueHandle<Self>) {
        self.cancel_selection();
        let Some(offer) = offer else {
            return;
        };
        let id = offer.id();
        let epoch = self.shared.epoch.load(Ordering::Acquire);
        if !self.capture_ready || !self.shared.active(epoch) {
            self.forget(id);
            return;
        }
        if !self.offers.iter().any(|r| r.offer.id() == id) {
            return;
        }
        self.seq = self.seq.wrapping_add(1);
        self.selected = Some(Selection {
            id,
            seq: self.seq,
            epoch,
            deadline: Instant::now() + Duration::from_secs(4),
            ready_at: Instant::now(),
            metadata_done: false,
            commit_done: false,
            hint_checked: false,
            payload: None,
            payload_mime: None,
        });
        connection.display().sync(qh, Sync::Metadata(self.seq));
    }
    fn forget(&mut self, id: u32) {
        if let Some(i) = self.offers.iter().position(|r| r.offer.id() == id) {
            self.offers.remove(i).offer.destroy();
        }
    }
    fn cancel_selection(&mut self) {
        self.rx = None;
        if let Some(s) = self.selected.take() {
            self.forget(s.id);
        }
    }
    fn begin_read(
        &mut self,
        offer: Offer,
        mime: String,
        kind: ReadKind,
    ) -> Result<(), IslandError> {
        let (reader, writer) =
            UnixStream::pair().map_err(|_| IslandError::io("Cannot create clipboard transfer"))?;
        reader
            .set_nonblocking(true)
            .map_err(|_| IslandError::io("Cannot configure clipboard transfer"))?;
        offer.receive(mime.clone(), writer.as_fd());
        drop(writer);
        self.rx = Some(Rx {
            reader,
            kind,
            mime,
            data: vec![],
            deadline: Instant::now() + Duration::from_millis(1500),
        });
        Ok(())
    }
    fn progress(
        &mut self,
        connection: &Connection,
        qh: &QueueHandle<Self>,
    ) -> Result<(), IslandError> {
        // A pause transition invalidates even transfers that complete after resume.
        if self
            .selected
            .as_ref()
            .is_some_and(|s| !self.shared.active(s.epoch))
        {
            self.cancel_selection();
        }
        let now = Instant::now();
        // Orphan offers (including unselected primary-selection objects) cannot accumulate.
        if self.rx.as_ref().is_some_and(|rx| now >= rx.deadline) {
            self.dropped();
            self.cancel_selection();
        }
        let selected = self.selected.as_ref().map(|s| s.id);
        let mut i = 0;
        while i < self.offers.len() {
            if Some(self.offers[i].offer.id()) != selected
                && now.duration_since(self.offers[i].created) > Duration::from_secs(2)
            {
                self.offers.remove(i).offer.destroy();
            } else {
                i += 1;
            }
        }
        self.tx.retain(|t| now < t.deadline);
        let Some(s) = self.selected.as_ref() else {
            return Ok(());
        };
        if now >= s.deadline {
            self.dropped();
            self.cancel_selection();
            return Ok(());
        }
        let Some(record) = self.offers.iter().find(|r| r.offer.id() == s.id) else {
            self.cancel_selection();
            return Ok(());
        };
        if record.overflow {
            self.dropped();
            self.cancel_selection();
            return Ok(());
        }
        if record.mimes.iter().any(|m| {
            matches!(
                m.trim().to_ascii_lowercase().as_str(),
                "org.nspasteboard.concealedtype"
                    | "org.nspasteboard.transienttype"
                    | "excludeclipboardcontentfrommonitorprocessing"
            )
        }) {
            self.sensitive();
            return Ok(());
        }
        if !s.metadata_done || now < record.last_mime + QUIET {
            return Ok(());
        }
        let offer = record.offer.clone();
        // Always read the tiny password hint before requesting any content.
        if record.mimes.iter().any(|m| m == HINT) && !s.hint_checked {
            if self.rx.as_ref().is_some_and(|r| r.kind != ReadKind::Hint) {
                self.rx = None;
            }
            if self.rx.is_none() {
                self.begin_read(offer, HINT.into(), ReadKind::Hint)?;
            }
            return Ok(());
        }
        if self.rx.is_some() {
            return Ok(());
        }
        let Some((mime, kind)) = preferred(&record.mimes) else {
            self.dropped();
            self.cancel_selection();
            return Ok(());
        };
        if s.payload.is_none() || s.payload_mime.as_deref() != Some(mime.as_str()) {
            self.begin_read(offer, mime, kind)?;
            return Ok(());
        }
        if !s.commit_done || now < s.ready_at + QUIET {
            return Ok(());
        }
        let mimes = record.mimes.clone();
        if let Some(mut selected) = self.selected.take() {
            if let Some(payload) = selected.payload.take() {
                self.shared.offer(payload, mimes, selected.epoch)?;
            }
            self.forget(selected.id);
        }
        // Keep the synchronization arguments explicit: all requests share this owned queue.
        let _ = (connection, qh);
        Ok(())
    }
    fn read_ready(
        &mut self,
        connection: &Connection,
        qh: &QueueHandle<Self>,
    ) -> Result<(), IslandError> {
        let Some(mut rx) = self.rx.take() else {
            return Ok(());
        };
        if Instant::now() >= rx.deadline {
            self.dropped();
            self.cancel_selection();
            return Ok(());
        }
        let cap = match rx.kind {
            ReadKind::Hint => 64,
            ReadKind::Png => PNG_CAP,
            _ => TEXT_CAP,
        };
        let mut buffer = [0u8; 8192];
        for _ in 0..8 {
            match rx.reader.read(&mut buffer) {
                Ok(0) => {
                    if rx.kind == ReadKind::Hint {
                        if rx.data == b"secret" {
                            self.sensitive();
                            return Ok(());
                        }
                        if let Some(s) = self.selected.as_mut() {
                            s.hint_checked = true;
                        }
                    } else {
                        let payload = match rx.kind {
                            ReadKind::Text => {
                                String::from_utf8(rx.data).ok().map(ClipPayload::Text)
                            }
                            ReadKind::Png => decode_png(rx.data).ok(),
                            ReadKind::Files => parse_uris(&rx.data).ok().map(ClipPayload::Files),
                            ReadKind::Hint => None,
                        };
                        if let Some(payload) = payload {
                            if let Some(s) = self.selected.as_mut() {
                                s.payload = Some(payload);
                                s.payload_mime = Some(rx.mime);
                                s.ready_at = Instant::now();
                                s.commit_done = false;
                                connection.display().sync(qh, Sync::Commit(s.seq));
                            }
                        } else {
                            self.dropped();
                            self.cancel_selection();
                        }
                    }
                    return Ok(());
                }
                Ok(n) => {
                    if rx.data.len() + n > cap {
                        self.dropped();
                        self.cancel_selection();
                        return Ok(());
                    }
                    rx.data.extend_from_slice(&buffer[..n]);
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    self.rx = Some(rx);
                    return Ok(());
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(_) => {
                    self.dropped();
                    self.cancel_selection();
                    return Ok(());
                }
            }
        }
        self.rx = Some(rx);
        Ok(())
    }
    fn send(&mut self, mime: &str, fd: OwnedFd, data: &SendData) {
        if self.tx.len() >= MAX_TX {
            self.dropped();
            return;
        }
        let Some((_, bytes)) = data.mimes.iter().find(|(m, _)| m == mime) else {
            return;
        };
        // SAFETY: fd is owned and valid. fcntl changes only this open file description.
        let flags = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFL) };
        if flags < 0
            || unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
        {
            self.dropped();
            return;
        }
        self.tx.push(Tx {
            file: File::from(fd),
            data: bytes.clone(),
            offset: 0,
            deadline: Instant::now() + Duration::from_secs(2),
        });
    }
    fn write_ready(&mut self, index: usize) {
        if index >= self.tx.len() {
            return;
        }
        let t = &mut self.tx[index];
        let end = (t.offset + 64 * 1024).min(t.data.len());
        let done = match t.file.write(&t.data[t.offset..end]) {
            Ok(0) => true,
            Ok(n) => {
                t.offset += n;
                t.offset == t.data.len()
            }
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                false
            }
            Err(_) => true,
        };
        if done {
            self.tx.remove(index);
        }
    }
    fn copy(
        &mut self,
        content: ClipContent,
        connection: &Connection,
        qh: &QueueHandle<Self>,
    ) -> Result<(), IslandError> {
        let now = Instant::now();
        self.copy_times
            .retain(|t| now.duration_since(*t) < Duration::from_secs(2));
        if self.copy_times.len() >= 32 {
            return Err(IslandError::new(
                ErrorCode::Busy,
                "Clipboard copy rate exceeded",
            ));
        }
        self.copy_times.push_back(now);
        let payload = payload(content)?;
        let data = Arc::new(send_data(&payload));
        // Register BEFORE set_selection: an immediate compositor echo must be guarded.
        self.shared.expect_echo(payload)?;
        let source = match (&self.manager, &self.device) {
            (Some(Manager::Ext(manager)), Some(Device::Ext(device))) => {
                let source = manager.create_data_source(qh, data.clone());
                for (mime, _) in &data.mimes {
                    source.offer(mime.clone());
                }
                device.set_selection(Some(&source));
                Source::Ext(source)
            }
            (Some(Manager::Wlr(manager)), Some(Device::Wlr(device))) => {
                let source = manager.create_data_source(qh, data.clone());
                for (mime, _) in &data.mimes {
                    source.offer(mime.clone());
                }
                device.set_selection(Some(&source));
                Source::Wlr(source)
            }
            _ => {
                return Err(IslandError::unavailable(
                    "Wayland clipboard manager unavailable",
                ))
            }
        };
        if let Some(old) = self.source.replace(source) {
            old.destroy();
        }
        flush(connection)?;
        Ok(())
    }
    fn next_wait(&self) -> Duration {
        let now = Instant::now();
        // Idle, nothing is time-bound: sleep until Wayland, a copy request or a stop wakes
        // the loop (all three are fds in the poll set). Pauses need no wake: every capture
        // re-checks `Shared::active` and a stale epoch is dropped at offer time.
        let mut deadline = now + Duration::from_secs(60);
        if let Some(s) = &self.selected {
            deadline = deadline.min(s.deadline);
            if s.payload.is_some() && s.ready_at + QUIET > now {
                deadline = deadline.min(s.ready_at + QUIET);
            }
            if let Some(r) = self.offers.iter().find(|r| r.offer.id() == s.id) {
                let quiet = r.last_mime + QUIET;
                if quiet > now {
                    deadline = deadline.min(quiet);
                }
            }
        }
        if let Some(r) = &self.rx {
            deadline = deadline.min(r.deadline);
        }
        for t in &self.tx {
            deadline = deadline.min(t.deadline);
        }
        deadline.saturating_duration_since(now)
    }
}
fn run_connection(
    connection: Connection,
    shared: Arc<Shared>,
    cancellation: &mut UnixStream,
    stop: &AtomicBool,
    commands: &Receiver<CopyRequest>,
) -> Result<(), IslandError> {
    let mut queue: EventQueue<State> = connection.new_event_queue();
    let qh = queue.handle();
    let registry = connection.display().get_registry(&qh, ());
    let mut state = State::new(shared.clone());
    connection.display().sync(&qh, Sync::Registry);
    let init_deadline = Instant::now() + Duration::from_secs(2);
    while !stop.load(Ordering::Acquire) && shared.enabled.load(Ordering::Acquire) {
        state.fence(&connection, &qh);
        queue
            .dispatch_pending(&mut state)
            .map_err(|_| IslandError::unavailable("Wayland clipboard dispatch failed"))?;
        if state.failed {
            return Err(IslandError::unavailable(
                "Wayland clipboard device closed or registry exceeded limits",
            ));
        }
        if state.registry_done && state.device.is_none() {
            let (seat_name, _) = state
                .seat
                .ok_or_else(|| IslandError::unavailable("Wayland clipboard has no seat"))?;
            let seat: wl_seat::WlSeat = registry.bind(seat_name, 1, &qh, ());
            if let Some((name, _)) = state.ext {
                let manager: ExtManager = registry.bind(name, 1, &qh, ());
                state.device = Some(Device::Ext(manager.get_data_device(&seat, &qh, ())));
                state.manager = Some(Manager::Ext(manager));
                shared.status_change(|s| {
                    s.available = true;
                    s.detail = "ext-data-control".into();
                });
            } else if let Some((name, version)) = state.wlr {
                let manager: WlrManager = registry.bind(name, version.min(2), &qh, ());
                state.device = Some(Device::Wlr(manager.get_data_device(&seat, &qh, ())));
                state.manager = Some(Manager::Wlr(manager));
                shared.status_change(|s| {
                    s.available = true;
                    s.detail = "wlr-data-control".into();
                });
            } else {
                return Err(IslandError::unsupported(
                    "Wayland compositor has no data-control protocol",
                ));
            }
        }
        if state.device.is_none() && Instant::now() >= init_deadline {
            return Err(IslandError::timeout("Wayland clipboard registry timed out"));
        }
        for _ in 0..32 {
            let Ok(request) = commands.try_recv() else {
                break;
            };
            let result = if Instant::now() >= request.deadline {
                Err(IslandError::timeout("Clipboard copy expired"))
            } else if stop.load(Ordering::Acquire) || !shared.enabled.load(Ordering::Acquire) {
                Err(IslandError::unavailable("Clipboard disabled"))
            } else {
                state.copy(request.content, &connection, &qh)
            };
            if request.reply.send(result).is_err() {
                tracing::debug!("Clipboard copy caller left");
            }
        }
        state.progress(&connection, &qh)?;
        let need_write = flush(&connection)?;
        let Some(guard) = queue.prepare_read() else {
            continue;
        };
        let mut fds = vec![
            pfd(cancellation.as_raw_fd(), libc::POLLIN),
            pfd(
                guard.connection_fd().as_raw_fd(),
                libc::POLLIN | if need_write { libc::POLLOUT } else { 0 },
            ),
        ];
        let has_rx = state.rx.is_some();
        if let Some(rx) = &state.rx {
            fds.push(pfd(rx.reader.as_raw_fd(), libc::POLLIN));
        }
        for tx in &state.tx {
            fds.push(pfd(tx.file.as_raw_fd(), libc::POLLOUT));
        }
        let wait = if state.device.is_none() {
            state
                .next_wait()
                .min(init_deadline.saturating_duration_since(Instant::now()))
        } else {
            state.next_wait()
        };
        poll(&mut fds, wait)?;
        if fds[0].revents != 0 {
            drop(guard);
            drain(cancellation);
            continue;
        }
        if fds[1].revents & (libc::POLLIN | libc::POLLERR | libc::POLLHUP) != 0 {
            match guard.read() {
                Ok(_) => {}
                Err(wayland_client::backend::WaylandError::Io(e))
                    if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(_) => return Err(IslandError::unavailable("Wayland clipboard disconnected")),
            }
        } else {
            drop(guard);
        }
        // Dispatch before transfer completion so a late sensitive MIME/pause invalidates it.
        state.fence(&connection, &qh);
        queue
            .dispatch_pending(&mut state)
            .map_err(|_| IslandError::unavailable("Wayland clipboard dispatch failed"))?;
        if state
            .selected
            .as_ref()
            .is_some_and(|s| !shared.active(s.epoch))
        {
            state.cancel_selection();
        }
        // Inspect newly dispatched MIME hints before reading any ready content FD.
        // This may replace/cancel a transfer, so match subsequent writes by FD,
        // not by a slot index captured before dispatch.
        state.progress(&connection, &qh)?;
        if has_rx && fds[2].revents != 0 {
            state.read_ready(&connection, &qh)?;
        }
        // Reverse order because completed writes remove their slot.
        let start = 2 + usize::from(has_rx);
        for ready in fds[start..].iter().rev() {
            if ready.revents != 0 {
                if let Some(index) = state
                    .tx
                    .iter()
                    .position(|tx| tx.file.as_raw_fd() == ready.fd)
                {
                    state.write_ready(index);
                }
            }
        }
    }
    // Dropping State, queue and the last Connection closes every offer/source/transfer FD.
    Ok(())
}
fn flush(connection: &Connection) -> Result<bool, IslandError> {
    match connection.flush() {
        Ok(()) => Ok(false),
        Err(wayland_client::backend::WaylandError::Io(e))
            if e.kind() == io::ErrorKind::WouldBlock =>
        {
            Ok(true)
        }
        Err(_) => Err(IslandError::unavailable(
            "Cannot flush Wayland clipboard requests",
        )),
    }
}
fn pfd(fd: i32, events: i16) -> libc::pollfd {
    libc::pollfd {
        fd,
        events,
        revents: 0,
    }
}
fn poll(fds: &mut [libc::pollfd], wait: Duration) -> Result<(), IslandError> {
    // SAFETY: the slice is live and writable for its entire advertised length.
    let n = unsafe {
        libc::poll(
            fds.as_mut_ptr(),
            fds.len() as libc::nfds_t,
            wait.as_millis().max(1).min(i32::MAX as u128) as i32,
        )
    };
    if n < 0 && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
        return Err(IslandError::io("Clipboard poll failed"));
    }
    Ok(())
}
fn preferred(mimes: &[String]) -> Option<(String, ReadKind)> {
    for (mime, kind) in [
        ("text/uri-list", ReadKind::Files),
        ("image/png", ReadKind::Png),
        ("text/plain;charset=utf-8", ReadKind::Text),
        ("text/plain;charset=UTF-8", ReadKind::Text),
        ("UTF8_STRING", ReadKind::Text),
        ("text/plain", ReadKind::Text),
    ] {
        if mimes.iter().any(|m| m == mime) {
            return Some((mime.into(), kind));
        }
    }
    None
}
fn parse_uris(bytes: &[u8]) -> Result<Vec<String>, IslandError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| IslandError::invalid("Invalid clipboard URI list"))?;
    let mut uris = Vec::new();
    for line in text.lines() {
        let uri = line.trim();
        if uri.is_empty() || uri.starts_with('#') {
            continue;
        }
        if uris.len() >= 4096
            || uri.len() > 8192
            || !uri.contains(':')
            || uri.chars().any(char::is_control)
        {
            return Err(IslandError::invalid("Clipboard URI list exceeds limits"));
        }
        uris.push(uri.into());
    }
    Ok(uris)
}
pub(super) fn decode_png(png: Vec<u8>) -> Result<ClipPayload, IslandError> {
    if png.len() > PNG_CAP {
        return Err(IslandError::invalid("Clipboard PNG exceeds 8 MiB"));
    }
    let mut reader = image::ImageReader::with_format(Cursor::new(&png), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|_| IslandError::invalid("Invalid or oversized clipboard PNG"))?;
    let (width, height) = (image.width(), image.height());
    if width == 0 || height == 0 || width > 4096 || height > 4096 {
        return Err(IslandError::invalid("Invalid clipboard PNG dimensions"));
    }
    Ok(ClipPayload::Image { png, width, height })
}
fn payload(content: ClipContent) -> Result<ClipPayload, IslandError> {
    match content {
        ClipContent::Text { text } => Ok(ClipPayload::Text(text)),
        ClipContent::Files { uris } => Ok(ClipPayload::Files(uris)),
        ClipContent::Image { png_data_url, .. } => {
            let data = png_data_url
                .strip_prefix("data:image/png;base64,")
                .ok_or_else(|| IslandError::invalid("Invalid stored clipboard PNG"))?;
            let png = base64::engine::general_purpose::STANDARD
                .decode(data)
                .map_err(|_| IslandError::invalid("Invalid stored clipboard PNG"))?;
            decode_png(png)
        }
    }
}
fn send_data(payload: &ClipPayload) -> SendData {
    let mimes = match payload {
        ClipPayload::Text(text) => {
            let data: Arc<[u8]> = Arc::from(text.as_bytes());
            vec![
                ("text/plain;charset=utf-8".into(), data.clone()),
                ("UTF8_STRING".into(), data.clone()),
                ("text/plain".into(), data),
            ]
        }
        ClipPayload::Image { png, .. } => vec![("image/png".into(), Arc::from(png.as_slice()))],
        ClipPayload::Files(uris) => vec![(
            "text/uri-list".into(),
            Arc::from(format!("{}\r\n", uris.join("\r\n")).into_bytes()),
        )],
    };
    SendData { mimes }
}
