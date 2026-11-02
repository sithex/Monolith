//! Local HTTP fixture standing in for LRCLIB (Phase 6m). Binds 127.0.0.1 only, records every
//! request target, and answers per `track_name`. No real network host is involved.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::JoinHandle,
};

pub struct Lrclib {
    pub addr: SocketAddr,
    requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

fn respond(stream: &mut TcpStream, status: u16, content_type: &str, extra: &str, body: &str) {
    let head = format!(
        "HTTP/1.1 {status} X\r\nContent-Type: {content_type}\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body.as_bytes());
}

fn decode(value: &str) -> String {
    let bytes = value.replace('+', " ");
    let mut out = Vec::new();
    let raw = bytes.as_bytes();
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'%' && i + 2 < raw.len() {
            if let Ok(byte) = u8::from_str_radix(&bytes[i + 1..i + 3], 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(raw[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn param(target: &str, key: &str) -> Option<String> {
    let query = target.split_once('?')?.1;
    query
        .split('&')
        .find_map(|pair| pair.strip_prefix(&format!("{key}=")).map(decode))
}

fn serve(mut stream: TcpStream, requests: &Mutex<Vec<String>>) {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 1024];
    while !buffer.windows(4).any(|w| w == b"\r\n\r\n") {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
        }
        if buffer.len() > 16 * 1024 {
            return;
        }
    }
    let text = String::from_utf8_lossy(&buffer).into_owned();
    let target = text
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .unwrap_or("")
        .to_owned();
    requests.lock().unwrap().push(target.clone());
    if !target.starts_with("/api/get?") {
        return respond(&mut stream, 404, "application/json", "", "{}");
    }
    let json = "application/json; charset=utf-8";
    match param(&target, "track_name").as_deref() {
        Some("Synced Song") => respond(
            &mut stream,
            200,
            json,
            "",
            r#"{"id":1,"instrumental":false,"plainLyrics":"one\ntwo\nthree","syncedLyrics":"[00:00.50]one\n[00:01.00]two\n[00:02.00]three"}"#,
        ),
        Some("Plain Song") => respond(
            &mut stream,
            200,
            json,
            "",
            r#"{"instrumental":false,"plainLyrics":"just words\nno timing","syncedLyrics":null}"#,
        ),
        Some("Instrumental Song") => respond(
            &mut stream,
            200,
            json,
            "",
            r#"{"instrumental":true,"plainLyrics":null,"syncedLyrics":null}"#,
        ),
        Some("Array Song") => respond(
            &mut stream,
            200,
            json,
            "",
            r#"[false,null,"[00:01.00]smuggled"]"#,
        ),
        Some("Limited Song") => respond(&mut stream, 429, json, "Retry-After: 7\r\n", "{}"),
        Some("Wrong Type Song") => respond(
            &mut stream,
            200,
            "text/html",
            "",
            r#"{"syncedLyrics":"[00:01.00]x"}"#,
        ),
        _ => respond(
            &mut stream,
            404,
            json,
            "",
            r#"{"code":404,"name":"NotFound"}"#,
        ),
    }
}

impl Lrclib {
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (r, s) = (Arc::clone(&requests), Arc::clone(&stop));
        let thread = std::thread::spawn(move || {
            for stream in listener.incoming() {
                if s.load(Ordering::Acquire) {
                    break;
                }
                let Ok(stream) = stream else { continue };
                let r = Arc::clone(&r);
                std::thread::spawn(move || serve(stream, &r));
            }
        });
        Self {
            addr,
            requests,
            stop,
            thread: Some(thread),
        }
    }

    pub fn base(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }

    pub fn count(&self) -> usize {
        self.requests().len()
    }
}

impl Drop for Lrclib {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = TcpStream::connect(self.addr);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
