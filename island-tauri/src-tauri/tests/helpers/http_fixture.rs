//! Local HTTP fixture standing in for Open-Meteo (Phase 6a). Binds 127.0.0.1 only, counts every
//! request, and answers per a switchable mode. Scratch-only: no real network host is involved.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::JoinHandle,
    time::Duration,
};

#[derive(Clone, Debug)]
pub enum Mode {
    Ok {
        temp: f64,
        code: u8,
    },
    Status(u16),
    Malformed,
    Oversize,
    Slow {
        ms: u64,
        temp: f64,
    },
    /// A body that parses but carries an out-of-range temperature.
    Implausible,
}

pub struct Fixture {
    pub addr: SocketAddr,
    requests: Arc<Mutex<Vec<String>>>,
    mode: Arc<Mutex<Mode>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

fn forecast(temp: f64, code: u8) -> String {
    format!(
        r#"{{"latitude":52.5,"longitude":13.4,"current_units":{{"time":"iso8601","interval":"seconds","temperature_2m":"°C","weather_code":"wmo code","is_day":""}},"current":{{"time":"2026-07-01T12:15","interval":900,"temperature_2m":{temp},"weather_code":{code},"is_day":1}}}}"#
    )
}

const GEOCODE: &str = include_str!("../fixtures/weather/geocode.json");

fn respond(stream: &mut TcpStream, status: u16, body: &[u8]) {
    let head = format!(
        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
}

fn serve(mut stream: TcpStream, requests: &Mutex<Vec<String>>, mode: &Mutex<Mode>) {
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
    let line = text.lines().next().unwrap_or("");
    let target = line.split_whitespace().nth(1).unwrap_or("").to_owned();
    requests.lock().unwrap().push(target.clone());
    let mode = mode.lock().unwrap().clone();
    if target.starts_with("/v1/search") {
        match mode {
            Mode::Status(code) => respond(&mut stream, code, b"{}"),
            _ => respond(&mut stream, 200, GEOCODE.as_bytes()),
        }
        return;
    }
    match mode {
        Mode::Ok { temp, code } => respond(&mut stream, 200, forecast(temp, code).as_bytes()),
        Mode::Status(code) => respond(&mut stream, code, br#"{"error":true,"reason":"nope"}"#),
        Mode::Malformed => respond(&mut stream, 200, b"{not json"),
        Mode::Oversize => respond(&mut stream, 200, &vec![b' '; 300 * 1024]),
        Mode::Slow { ms, temp } => {
            std::thread::sleep(Duration::from_millis(ms));
            respond(&mut stream, 200, forecast(temp, 2).as_bytes());
        }
        Mode::Implausible => respond(&mut stream, 200, forecast(500.0, 2).as_bytes()),
    }
}

impl Fixture {
    pub fn start(mode: Mode) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let mode = Arc::new(Mutex::new(mode));
        let stop = Arc::new(AtomicBool::new(false));
        let (r, m, s) = (Arc::clone(&requests), Arc::clone(&mode), Arc::clone(&stop));
        let thread = std::thread::spawn(move || {
            for stream in listener.incoming() {
                if s.load(Ordering::Acquire) {
                    break;
                }
                let Ok(stream) = stream else { continue };
                let (r, m) = (Arc::clone(&r), Arc::clone(&m));
                std::thread::spawn(move || serve(stream, &r, &m));
            }
        });
        Self {
            addr,
            requests,
            mode,
            stop,
            thread: Some(thread),
        }
    }

    pub fn base(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub fn set_mode(&self, mode: Mode) {
        *self.mode.lock().unwrap() = mode;
    }

    pub fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }

    pub fn count(&self, prefix: &str) -> usize {
        self.requests()
            .iter()
            .filter(|r| r.starts_with(prefix))
            .count()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = TcpStream::connect(self.addr);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
