//! Owned loopback-only ICS server. No detached threads, request logging or real endpoints.
#![allow(clippy::unwrap_used)]
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread::JoinHandle,
    time::Duration,
};
#[derive(Clone)]
pub struct Reply {
    pub status: u16,
    pub body: String,
    pub slow: bool,
    pub chunked: bool,
}
pub struct CalendarFixture {
    addr: SocketAddr,
    reply: Arc<Mutex<Reply>>,
    stop: Arc<AtomicBool>,
    hits: Arc<AtomicUsize>,
    task: Option<JoinHandle<()>>,
}
impl CalendarFixture {
    pub fn start(body: String) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let reply = Arc::new(Mutex::new(Reply {
            status: 200,
            body,
            slow: false,
            chunked: false,
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let hits = Arc::new(AtomicUsize::new(0));
        let h = Arc::clone(&hits);
        let (r, s) = (Arc::clone(&reply), Arc::clone(&stop));
        let task = std::thread::spawn(move || {
            for stream in listener.incoming() {
                if s.load(Ordering::Acquire) {
                    break;
                }
                h.fetch_add(1, Ordering::AcqRel);
                let Ok(mut stream) = stream else {
                    break;
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut buffer = [0u8; 16384];
                let mut len = 0;
                loop {
                    match stream.read(&mut buffer[len..]) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            len += n;
                            if buffer[..len].windows(4).any(|w| w == b"\r\n\r\n")
                                || len == buffer.len()
                            {
                                break;
                            }
                        }
                    }
                }
                let response = r.lock().unwrap().clone();
                if response.slow {
                    for _ in 0..30 {
                        if s.load(Ordering::Acquire) {
                            return;
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
                let header = if response.chunked {
                    "Transfer-Encoding: chunked".into()
                } else {
                    format!("Content-Length: {}", response.body.len())
                };
                let head=format!("HTTP/1.1 {} X\r\nContent-Type: text/calendar\r\n{}\r\nConnection: close\r\n\r\n",response.status,header);
                if stream.write_all(head.as_bytes()).is_err() {
                    continue;
                }
                if response.chunked {
                    for chunk in response.body.as_bytes().chunks(4096) {
                        if write!(stream, "{:x}\r\n", chunk.len())
                            .and_then(|()| stream.write_all(chunk))
                            .and_then(|()| stream.write_all(b"\r\n"))
                            .is_err()
                        {
                            break;
                        }
                    }
                    let _finished = stream.write_all(b"0\r\n\r\n");
                } else {
                    let _sent = stream.write_all(response.body.as_bytes());
                }
            }
        });
        Self {
            addr,
            reply,
            stop,
            hits,
            task: Some(task),
        }
    }
    pub fn url(&self) -> String {
        format!("http://{}/private-canary-token.ics", self.addr)
    }
    #[allow(dead_code)]
    pub fn hits(&self) -> usize {
        self.hits.load(Ordering::Acquire)
    }
    pub fn set(&self, reply: Reply) {
        *self.reply.lock().unwrap() = reply;
    }
}
impl Drop for CalendarFixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _wake = TcpStream::connect(self.addr);
        if let Some(t) = self.task.take() {
            assert!(t.join().is_ok());
        }
    }
}
