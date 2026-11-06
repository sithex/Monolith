//! Loopback WebSocket sink for the Hermes TUI's `HERMES_TUI_SIDECAR_URL` mirror.
//!
//! Receive-only and deliberately small: one handshake shape, text messages in, nothing out
//! but the handshake reply, pongs and a close. Guards, in order:
//! * the listener is bound to 127.0.0.1 only;
//! * the request must carry the current run's token (constant-time compared), the exact path
//!   `/sidecar`, a loopback `Host`, and NO `Origin` (browsers always send one, so a web page
//!   cannot reach the sink even if it guessed the port);
//! * handshake head capped at 8 KiB and 5 s; at most [`MAX_CONNECTIONS`] sockets;
//! * client frames must be masked; messages above [`MAX_MESSAGE`] are discarded unread-by-us
//!   (streamed through, never buffered) and never reach the parser.

use super::{frame::MAX_FRAME_BYTES, ApprovalService};
use base64::Engine;
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

pub const MAX_CONNECTIONS: usize = 4;
pub const MAX_HEAD: usize = 8 * 1024;
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
/// Largest message that is buffered (and parsed); larger ones are drained and dropped.
pub const MAX_MESSAGE: usize = MAX_FRAME_BYTES;
const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

pub fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [
        0x6745_2301,
        0xEFCD_AB89,
        0x98BA_DCFE,
        0x1032_5476,
        0xC3D2_E1F0,
    ];
    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for chunk in message.chunks_exact(64) {
        let mut w = [0u32; 80];
        for (i, word) in chunk.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, word) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        for (slot, value) in h.iter_mut().zip([a, b, c, d, e]) {
            *slot = slot.wrapping_add(value);
        }
    }
    let mut out = [0u8; 20];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

pub fn accept_key(client_key: &str) -> String {
    let digest = sha1(format!("{client_key}{WS_GUID}").as_bytes());
    base64::engine::general_purpose::STANDARD.encode(digest)
}

#[derive(Debug, PartialEq, Eq)]
pub struct Handshake {
    pub token: String,
    pub key: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Reject {
    Malformed,
    Forbidden,
}

fn header<'a>(lines: &[&'a str], name: &str) -> Option<&'a str> {
    lines.iter().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim().eq_ignore_ascii_case(name).then(|| value.trim())
    })
}

fn loopback_host(host: &str, port: u16) -> bool {
    [format!("127.0.0.1:{port}"), format!("localhost:{port}")]
        .iter()
        .any(|ok| ok.eq_ignore_ascii_case(host))
}

/// Validate the request head (everything before the blank line). Token comparison happens
/// later against the service; this checks shape only.
pub fn parse_request(head: &str, port: u16) -> Result<Handshake, Reject> {
    let mut lines = head.split("\r\n");
    let request_line = lines.next().ok_or(Reject::Malformed)?;
    let rest: Vec<&str> = lines.collect();
    let mut parts = request_line.split(' ');
    let (method, target, version) = (parts.next(), parts.next(), parts.next());
    if method != Some("GET") || version != Some("HTTP/1.1") || parts.next().is_some() {
        return Err(Reject::Malformed);
    }
    let target = target.ok_or(Reject::Malformed)?;
    let (path, query) = target.split_once('?').ok_or(Reject::Forbidden)?;
    if path != "/sidecar" {
        return Err(Reject::Forbidden);
    }
    let token = query
        .split('&')
        .find_map(|pair| pair.strip_prefix("token="))
        .filter(|t| !t.is_empty() && t.len() <= 128 && t.bytes().all(|b| b.is_ascii_alphanumeric()))
        .ok_or(Reject::Forbidden)?;
    if header(&rest, "origin").is_some() {
        return Err(Reject::Forbidden);
    }
    if !header(&rest, "host").is_some_and(|h| loopback_host(h, port)) {
        return Err(Reject::Forbidden);
    }
    let upgrade = header(&rest, "upgrade").is_some_and(|v| v.eq_ignore_ascii_case("websocket"));
    let connection = header(&rest, "connection").is_some_and(|v| {
        v.split(',')
            .any(|t| t.trim().eq_ignore_ascii_case("upgrade"))
    });
    let version13 = header(&rest, "sec-websocket-version") == Some("13");
    let key = header(&rest, "sec-websocket-key").filter(|k| !k.is_empty() && k.len() <= 64);
    match (upgrade && connection && version13, key) {
        (true, Some(key)) => Ok(Handshake {
            token: token.to_owned(),
            key: key.to_owned(),
        }),
        _ => Err(Reject::Malformed),
    }
}

async fn read_head<S: AsyncRead + Unpin>(stream: &mut S) -> Option<String> {
    let mut head = Vec::with_capacity(512);
    let mut byte = [0u8; 1];
    // Byte-wise on purpose: nothing past the blank line (the client waits for the reply, but
    // a pipelined first frame must not be swallowed or trusted) is read as part of the head.
    while !head.ends_with(b"\r\n\r\n") {
        if head.len() >= MAX_HEAD {
            return None;
        }
        stream.read_exact(&mut byte).await.ok()?;
        head.push(byte[0]);
    }
    String::from_utf8(head).ok()
}

async fn refuse<S: AsyncWrite + Unpin>(stream: &mut S, status: &str) {
    let reply = format!("HTTP/1.1 {status}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n");
    let _ = stream.write_all(reply.as_bytes()).await;
    let _ = stream.shutdown().await;
}

struct Slot(Arc<AtomicUsize>);
impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

pub async fn serve(
    listener: std::net::TcpListener,
    service: Arc<ApprovalService>,
) -> std::io::Result<()> {
    let port = listener.local_addr()?.port();
    let listener = TcpListener::from_std(listener)?;
    let open = Arc::new(AtomicUsize::new(0));
    loop {
        let (stream, peer) = listener.accept().await?;
        if !peer.ip().is_loopback() {
            continue;
        }
        if open.fetch_add(1, Ordering::AcqRel) >= MAX_CONNECTIONS {
            open.fetch_sub(1, Ordering::AcqRel);
            continue;
        }
        let slot = Slot(open.clone());
        let service = service.clone();
        tokio::spawn(async move {
            let _slot = slot;
            connection(stream, port, service).await;
        });
    }
}

pub async fn connection(mut stream: TcpStream, port: u16, service: Arc<ApprovalService>) {
    let head = match tokio::time::timeout(HANDSHAKE_TIMEOUT, read_head(&mut stream)).await {
        Ok(Some(head)) => head,
        _ => return refuse(&mut stream, "400 Bad Request").await,
    };
    let handshake = match parse_request(&head, port) {
        Ok(h) => h,
        Err(Reject::Malformed) => return refuse(&mut stream, "400 Bad Request").await,
        Err(Reject::Forbidden) => return refuse(&mut stream, "403 Forbidden").await,
    };
    let Some(run) = service.connection_open(&handshake.token) else {
        return refuse(&mut stream, "403 Forbidden").await;
    };
    let reply = format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
        accept_key(&handshake.key)
    );
    if stream.write_all(reply.as_bytes()).await.is_ok() {
        messages(&mut stream, run, &service).await;
    }
    service.connection_closed(run);
}

async fn discard<S: AsyncRead + Unpin>(stream: &mut S, mut remaining: u64) -> bool {
    let mut sink = [0u8; 4096];
    while remaining > 0 {
        let want = remaining.min(sink.len() as u64) as usize;
        match stream.read(&mut sink[..want]).await {
            Ok(0) | Err(_) => return false,
            Ok(n) => remaining -= n as u64,
        }
    }
    true
}

/// Read frames until close/error; every complete text message goes to the service.
pub async fn messages<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
    run: u64,
    service: &ApprovalService,
) {
    let mut assembling: Vec<u8> = Vec::new();
    let mut oversized = false;
    let mut text_message = false;
    loop {
        let mut head = [0u8; 2];
        if stream.read_exact(&mut head).await.is_err() {
            return;
        }
        let fin = head[0] & 0x80 != 0;
        let rsv = head[0] & 0x70;
        let opcode = head[0] & 0x0f;
        let masked = head[1] & 0x80 != 0;
        let mut len = u64::from(head[1] & 0x7f);
        if rsv != 0 || !masked {
            return;
        }
        if len == 126 {
            let mut ext = [0u8; 2];
            if stream.read_exact(&mut ext).await.is_err() {
                return;
            }
            len = u64::from(u16::from_be_bytes(ext));
        } else if len == 127 {
            let mut ext = [0u8; 8];
            if stream.read_exact(&mut ext).await.is_err() {
                return;
            }
            len = u64::from_be_bytes(ext);
            if len >> 63 != 0 {
                return;
            }
        }
        let mut mask = [0u8; 4];
        if stream.read_exact(&mut mask).await.is_err() {
            return;
        }
        let control = opcode & 0x08 != 0;
        if control && (len > 125 || !fin) {
            return;
        }
        let keep = control || (!oversized && assembling.len() as u64 + len <= MAX_MESSAGE as u64);
        let mut payload = Vec::new();
        if keep {
            payload.resize(len as usize, 0);
            if stream.read_exact(&mut payload).await.is_err() {
                return;
            }
            for (i, byte) in payload.iter_mut().enumerate() {
                *byte ^= mask[i % 4];
            }
        } else {
            oversized = true;
            assembling.clear();
            if !discard(stream, len).await {
                return;
            }
        }
        match opcode {
            0x8 => {
                let _ = stream.write_all(&[0x88, 0x00]).await;
                return;
            }
            0x9 => {
                let mut pong = vec![0x8a, payload.len() as u8];
                pong.extend_from_slice(&payload);
                if stream.write_all(&pong).await.is_err() {
                    return;
                }
            }
            0xa => {}
            0x0..=0x2 => {
                if opcode != 0 {
                    text_message = opcode == 0x1;
                    assembling.clear();
                    oversized = !keep;
                }
                if keep {
                    assembling.extend_from_slice(&payload);
                }
                if fin {
                    if text_message && !oversized {
                        if let Ok(text) = std::str::from_utf8(&assembling) {
                            service.ingest(run, text);
                        }
                    }
                    assembling.clear();
                    oversized = false;
                    text_message = false;
                }
            }
            _ => return,
        }
        if !service.run_is_current(run) {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn sha1_known_vectors() {
        assert_eq!(hex(&sha1(b"")), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(
            hex(&sha1(b"abc")),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            hex(&sha1(&[b'a'; 1000])),
            "291e9a6c66994949b57ba5e650361e98fc36b1ba"
        );
    }

    #[test]
    fn accept_key_matches_rfc_6455_example() {
        assert_eq!(
            accept_key("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
    }

    fn request(path: &str, extra: &str) -> String {
        format!(
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:4000\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n{extra}\r\n"
        )
    }

    #[test]
    fn handshake_shape_is_strict() {
        let ok = parse_request(&request("/sidecar?token=abc123", ""), 4000);
        assert_eq!(
            ok,
            Ok(Handshake {
                token: "abc123".into(),
                key: "dGhlIHNhbXBsZSBub25jZQ==".into()
            })
        );
        for (path, extra, port) in [
            ("/other?token=abc", "", 4000),
            ("/sidecar", "", 4000),
            ("/sidecar?token=", "", 4000),
            ("/sidecar?token=a%20b", "", 4000),
            ("/sidecar?x=1", "", 4000),
            (
                "/sidecar?token=abc",
                "Origin: http://evil.example\r\n",
                4000,
            ),
            ("/sidecar?token=abc", "", 4001),
        ] {
            assert_eq!(
                parse_request(&request(path, extra), port),
                Err(Reject::Forbidden),
                "{path} {extra} {port}"
            );
        }
        let rebinding =
            request("/sidecar?token=abc", "").replace("127.0.0.1:4000", "evil.test:4000");
        assert_eq!(parse_request(&rebinding, 4000), Err(Reject::Forbidden));
        let no_upgrade = request("/sidecar?token=abc", "").replace("Upgrade: websocket\r\n", "");
        assert_eq!(parse_request(&no_upgrade, 4000), Err(Reject::Malformed));
        let post = request("/sidecar?token=abc", "").replacen("GET", "POST", 1);
        assert_eq!(parse_request(&post, 4000), Err(Reject::Malformed));
    }
}
