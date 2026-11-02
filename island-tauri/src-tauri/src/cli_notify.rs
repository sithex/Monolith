//! Thin notification client. Runs before launch parsing, GTK, services or singleton locks.
use crate::{
    error::IslandError,
    integrations::notifications::ingest::{parse_line, MAX_LINE_BYTES},
};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    os::unix::{
        fs::{FileTypeExt, MetadataExt, PermissionsExt},
        net::UnixStream,
    },
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Debug)]
pub struct NotifyCli {
    pub data: PathBuf,
    pub test: bool,
    pub frame: Vec<u8>,
}
/// The launch parser remains separate: a notify client must not acquire application state.
pub fn parse(args: &[String]) -> Option<Result<NotifyCli, IslandError>> {
    let mut command = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "notify" => {
                command = Some(index);
                break;
            }
            "--data-dir" => index += 2,
            "--test-instance" | "--overlay" => index += 1,
            v if v.starts_with("--data-dir=") => index += 1,
            _ => break,
        }
    }
    let at = command?;
    Some((|| {
        let mut title = None;
        let mut body = None;
        let mut level = "info".to_owned();
        let mut ttl = None;
        let mut data = None;
        let mut test = false;
        let mut i = 0;
        while i < args.len() {
            if i == at {
                i += 1;
                continue;
            }
            match args[i].as_str() {
                "--test-instance" => {
                    test = true;
                    i += 1;
                }
                "--overlay" => {
                    i += 1;
                }
                "--title" | "--body" | "--level" | "--ttl-ms" | "--data-dir" => {
                    let key = &args[i];
                    let value = args
                        .get(i + 1)
                        .ok_or_else(|| IslandError::invalid("Notify option requires a value"))?
                        .clone();
                    match key.as_str() {
                        "--title" => {
                            if title.replace(value).is_some() {
                                return Err(IslandError::invalid("Duplicate title"));
                            }
                        }
                        "--body" => {
                            if body.replace(value).is_some() {
                                return Err(IslandError::invalid("Duplicate body"));
                            }
                        }
                        "--level" => level = value,
                        "--ttl-ms" => {
                            ttl = Some(
                                value
                                    .parse::<f64>()
                                    .map_err(|_| IslandError::invalid("Invalid TTL"))?,
                            )
                        }
                        _ => {
                            if data.replace(PathBuf::from(value)).is_some() {
                                return Err(IslandError::invalid("Duplicate data directory"));
                            }
                        }
                    }
                    i += 2;
                }
                value if value.starts_with("--data-dir=") => {
                    if data.replace(PathBuf::from(&value[11..])).is_some() {
                        return Err(IslandError::invalid("Duplicate data directory"));
                    }
                    i += 1;
                }
                _ => return Err(IslandError::invalid("Unknown notify argument")),
            }
        }
        if test && data.is_none() {
            return Err(IslandError::invalid("Test notify requires --data-dir"));
        }
        if !test && data.is_some() {
            return Err(IslandError::invalid("--data-dir is test-only"));
        }
        let data = data.unwrap_or_else(|| crate::paths::repo_root().join(".island-data-overlay"));
        if test {
            validate_test_target(&data)?;
        }
        let mut value = json!({"v":1,"op":"notify","title":title.ok_or_else(||IslandError::invalid("Title required"))?,"level":level});
        if let Some(body) = body {
            value["body"] = json!(body);
        }
        if let Some(ttl) = ttl {
            if !ttl.is_finite() {
                return Err(IslandError::invalid("Invalid TTL"));
            }
            value["ttlMs"] = json!(ttl);
        }
        let mut frame = serde_json::to_vec(&value)
            .map_err(|_| IslandError::invalid("Cannot encode notification"))?;
        parse_line(&frame).map_err(|_| IslandError::invalid("Invalid notification request"))?;
        frame.push(b'\n');
        Ok(NotifyCli { data, test, frame })
    })())
}
fn validate_test_target(path: &Path) -> Result<(), IslandError> {
    let forbidden = |p: &Path| {
        p.components().any(|c|matches!(c,std::path::Component::Normal(n) if n==".island-data"||n==".island-data-overlay"))
    };
    if forbidden(path) {
        return Err(IslandError::invalid(
            "Test notify refuses live data directories",
        ));
    }
    let resolved = std::fs::canonicalize(path)
        .map_err(|_| IslandError::invalid("Test notify directory must already exist"))?;
    if forbidden(&resolved) {
        return Err(IslandError::invalid(
            "Test notify refuses live data directories",
        ));
    }
    Ok(())
}
pub fn send(request: NotifyCli) -> Result<Value, IslandError> {
    let path = request.data.join("ingest.sock");
    let meta = std::fs::symlink_metadata(&path)
        .map_err(|_| IslandError::unavailable("Island ingest socket unavailable"))?;
    if !meta.file_type().is_socket()
        || meta.uid() != unsafe { libc::geteuid() }
        || meta.permissions().mode() & 0o777 != 0o600
    {
        return Err(IslandError::unavailable("Unsafe ingest socket"));
    }
    let mut stream = UnixStream::connect(path)
        .map_err(|_| IslandError::unavailable("Cannot connect to island ingest"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .map_err(|_| IslandError::io("Cannot bound ingest reply"))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(10)))
        .map_err(|_| IslandError::io("Cannot bound ingest write"))?;
    stream
        .write_all(&request.frame)
        .map_err(|_| IslandError::io("Cannot send notification"))?;
    let mut reply = Vec::new();
    for byte in std::io::BufReader::new(stream)
        .take((MAX_LINE_BYTES + 1) as u64)
        .bytes()
    {
        let byte = byte.map_err(|_| IslandError::io("Cannot read ingest reply"))?;
        reply.push(byte);
        if byte == b'\n' {
            break;
        }
    }
    if reply.len() > MAX_LINE_BYTES || reply.last() != Some(&b'\n') {
        return Err(IslandError::invalid("Invalid ingest reply framing"));
    }
    let value: Value =
        serde_json::from_slice(&reply).map_err(|_| IslandError::invalid("Invalid ingest reply"))?;
    if value["ok"] != true {
        return Err(IslandError::invalid("Island rejected notification"));
    }
    Ok(value)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }
    #[test]
    fn notify_parser_accepts_spec_flags_and_validates_without_launching() {
        let p = parse(&args(&[
            "notify", "--title", "Build", "--body", "done", "--level", "success", "--ttl-ms", "0",
        ]))
        .unwrap()
        .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&p.frame).unwrap()["ttlMs"],
            0.
        );
        assert!(parse(&args(&["--overlay"])).is_none());
        for a in [
            vec!["notify"],
            vec!["notify", "--title", ""],
            vec!["notify", "--title", "x", "--level", "bad"],
            vec!["notify", "--title", "x", "--ttl-ms", "NaN"],
        ] {
            assert!(parse(&args(&a)).unwrap().is_err());
        }
    }
    #[test]
    fn test_notify_rejects_live_targets_before_connect_or_creation() {
        for path in [
            "/home/nick/Hermes/Ripple/.island-data-overlay",
            "/x/.island-data/inner",
        ] {
            assert!(parse(&args(&[
                "notify",
                "--test-instance",
                "--data-dir",
                path,
                "--title",
                "x"
            ]))
            .unwrap()
            .is_err());
        }
        assert!(parse(&args(&["notify", "--test-instance", "--title", "x"]))
            .unwrap()
            .is_err());
    }
}
