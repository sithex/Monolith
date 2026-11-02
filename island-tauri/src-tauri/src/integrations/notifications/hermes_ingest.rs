//! Private, bounded NDJSON transport. No panel protocol changes, no unbounded read_line.
use super::ingest::{IngestSession, MAX_CONNECTIONS, MAX_LINE_BYTES};
use crate::error::IslandError;
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tauri::async_runtime::JoinHandle;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
    sync::watch,
    task::JoinSet,
};

pub trait Handler: Send + Sync + 'static {
    fn handle(
        &self,
        session: &mut IngestSession,
        frame: &[u8],
        now_ms: f64,
    ) -> Result<String, IslandError>;
}

pub struct IngestServer {
    stop: watch::Sender<bool>,
    task: Option<JoinHandle<Result<(), IslandError>>>,
    path: PathBuf,
    identity: (u64, u64),
}
impl IngestServer {
    pub fn start(data: &Path, handler: Arc<dyn Handler>) -> Result<Self, IslandError> {
        // Bind never replaces a pre-existing socket. The app holds both instance locks.
        let path = data.join("ingest.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path)
            .map_err(|_| IslandError::io("Cannot bind ingest socket"))?;
        let meta = fs::symlink_metadata(&path)
            .map_err(|_| IslandError::io("Cannot inspect ingest socket"))?;
        let identity = (meta.dev(), meta.ino());
        let prepared = fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
            .and_then(|()| listener.set_nonblocking(true));
        if prepared.is_err() {
            fs::remove_file(&path)
                .map_err(|_| IslandError::io("Cannot clean failed ingest socket"))?;
            return Err(IslandError::io("Cannot secure ingest socket"));
        }
        let (stop, stopped) = watch::channel(false);
        let uid = unsafe { libc::geteuid() };
        let task = tauri::async_runtime::spawn(async move {
            let listener = UnixListener::from_std(listener)
                .map_err(|_| IslandError::io("Cannot register ingest listener"))?;
            serve(listener, uid, handler, stopped).await
        });

        Ok(Self {
            stop,
            task: Some(task),
            path,
            identity,
        })
    }
    pub async fn shutdown(&mut self) -> Result<(), IslandError> {
        self.stop.send_replace(true);
        if let Some(task) = self.task.take() {
            tokio::time::timeout(Duration::from_secs(2), task)
                .await
                .map_err(|_| IslandError::timeout("Ingest shutdown timed out"))?
                .map_err(|_| IslandError::internal("Ingest worker panicked"))??;
        }
        self.cleanup()
    }
    fn cleanup(&self) -> Result<(), IslandError> {
        match fs::symlink_metadata(&self.path) {
            Ok(meta) if (meta.dev(), meta.ino()) == self.identity => fs::remove_file(&self.path)
                .map_err(|_| IslandError::io("Cannot remove owned ingest socket")),
            Ok(_) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(IslandError::io("Cannot inspect owned ingest socket")),
        }
    }
}
impl Drop for IngestServer {
    fn drop(&mut self) {
        self.stop.send_replace(true);
        // Ordinary application shutdown awaits the task first. Drop remains fail-safe.
        if let Err(error) = self.cleanup() {
            tracing::warn!(%error, "Ingest cleanup failed");
        }
    }
}

async fn serve(
    listener: UnixListener,
    uid: u32,
    handler: Arc<dyn Handler>,
    mut stop: watch::Receiver<bool>,
) -> Result<(), IslandError> {
    let mut clients = JoinSet::new();
    let origin = Instant::now();
    loop {
        tokio::select! { biased;
            result = stop.changed() => { if result.is_err() || *stop.borrow() { break; } }
            result = clients.join_next(), if !clients.is_empty() => {
                match result {
                    Some(Ok(Err(error))) => tracing::debug!(%error, "Ingest client ended"),
                    Some(Err(error)) => tracing::warn!(%error, "Ingest client panicked"),
                    _ => {}
                }
            }
            accepted = listener.accept(), if clients.len() < MAX_CONNECTIONS as usize => {
                let (stream, _) = accepted.map_err(|_| IslandError::io("Ingest accept failed"))?;
                if !peer_allowed(&stream, uid)? { continue; }
                let handler = handler.clone();
                let mut stopped = stop.clone();
                clients.spawn(async move {
                    tokio::select! { biased;
                        _ = stopped.changed() => Ok(()),
                        result = client(stream, handler, origin) => result,
                    }
                });
            }
        }
    }
    clients.abort_all();
    while let Some(result) = clients.join_next().await {
        if let Err(error) = result {
            if !error.is_cancelled() {
                tracing::warn!(%error, "Ingest client cleanup failed");
            }
        }
    }
    Ok(())
}
fn peer_allowed(stream: &UnixStream, uid: u32) -> Result<bool, IslandError> {
    let peer = stream
        .peer_cred()
        .map_err(|_| IslandError::unavailable("Cannot establish ingest peer UID"))?;
    Ok(peer.uid() == uid)
}
async fn client(
    mut stream: UnixStream,
    handler: Arc<dyn Handler>,
    origin: Instant,
) -> Result<(), IslandError> {
    let mut session = IngestSession::new(origin.elapsed().as_secs_f64() * 1000.)
        .map_err(|_| IslandError::internal("Cannot create ingest session"))?;
    let mut frame = Vec::with_capacity(MAX_LINE_BYTES + 2);
    let mut chunk = [0; 1024];
    loop {
        let count = tokio::time::timeout(Duration::from_secs(10), stream.read(&mut chunk))
            .await
            .map_err(|_| IslandError::timeout("Ingest client idle"))?
            .map_err(|_| IslandError::io("Ingest read failed"))?;
        if count == 0 {
            return Ok(());
        }
        for &byte in &chunk[..count] {
            frame.push(byte);
            let complete = byte == b'\n';
            // One optional CR and LF are not payload. Overflow is answered ONCE, then this
            // connection closes (never allocate/drain an attacker's unbounded remainder).
            let overflow = frame.len()
                > MAX_LINE_BYTES
                    + usize::from(frame.last() == Some(&b'\r'))
                    + usize::from(complete)
                    + usize::from(
                        complete && frame.get(frame.len().saturating_sub(2)) == Some(&b'\r'),
                    );
            if complete || overflow {
                let reply =
                    handler.handle(&mut session, &frame, origin.elapsed().as_secs_f64() * 1000.)?;
                if reply.len() > 4096 || !reply.ends_with('\n') {
                    return Err(IslandError::internal("Invalid ingest reply framing"));
                }
                tokio::time::timeout(Duration::from_secs(10), stream.write_all(reply.as_bytes()))
                    .await
                    .map_err(|_| IslandError::timeout("Ingest reply stalled"))?
                    .map_err(|_| IslandError::io("Ingest reply failed"))?;
                frame.clear();
                if overflow {
                    return Ok(());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::NotificationStore;
    use super::*;
    use std::sync::Mutex;
    use tokio::io::{AsyncBufReadExt, BufReader};
    struct Core(Mutex<NotificationStore>);
    impl Handler for Core {
        fn handle(
            &self,
            session: &mut IngestSession,
            frame: &[u8],
            now: f64,
        ) -> Result<String, IslandError> {
            session
                .handle(&mut self.0.lock().unwrap(), frame, now)
                .reply
                .to_line()
                .map_err(|_| IslandError::internal("encode"))
        }
    }
    #[tokio::test]
    async fn ingest_real_socket_bounds_rate_split_frames_and_cleanup() {
        let fixture = crate::singleton::tests::Fixture::new();
        let core = Arc::new(Core(Mutex::new(NotificationStore::default())));
        let mut server = IngestServer::start(&fixture.0, core.clone()).unwrap();
        let path = fixture.0.join("ingest.sock");
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let mut stream = BufReader::new(UnixStream::connect(&path).await.unwrap());
        stream
            .get_mut()
            .write_all(b"{\"v\":1,\"op\":\"notify\",")
            .await
            .unwrap();
        stream
            .get_mut()
            .write_all(b"\"title\":\"real\"}\n")
            .await
            .unwrap();
        let mut line = String::new();
        stream.read_line(&mut line).await.unwrap();
        assert!(line.contains("\"ok\":true"));
        // Malformed attempts are charged too; batch minimizes clock/refill uncertainty.
        stream
            .get_mut()
            .write_all(&b"bad\n".repeat(40))
            .await
            .unwrap();
        let mut limited = false;
        for _ in 0..40 {
            line.clear();
            stream.read_line(&mut line).await.unwrap();
            limited |= line.contains("rateLimited");
        }
        assert!(limited);
        let mut oversize = BufReader::new(UnixStream::connect(&path).await.unwrap());
        oversize
            .get_mut()
            .write_all(&vec![b' '; MAX_LINE_BYTES + 1])
            .await
            .unwrap();
        line.clear();
        oversize.read_line(&mut line).await.unwrap();
        assert!(line.contains("invalid"));
        assert_eq!(core.0.lock().unwrap().len(), 1);
        server.shutdown().await.unwrap();
        assert!(!path.exists());
    }
    #[tokio::test]
    async fn ingest_uid_check_uses_kernel_credentials_and_refuses_other_uid() {
        let (a, _b) = UnixStream::pair().unwrap();
        let uid = unsafe { libc::geteuid() };
        assert!(peer_allowed(&a, uid).unwrap());
        assert!(!peer_allowed(&a, uid.wrapping_add(1)).unwrap());
    }
}
