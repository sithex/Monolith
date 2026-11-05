//! Subprocess-only private compositor clipboard source. Parent owns/reaps the process.
use std::{
    env,
    fs::{self, OpenOptions},
    io::{Cursor, Write},
    path::{Path, PathBuf},
};
use wl_clipboard_rs::copy::{MimeSource, MimeType, Options, ServeRequests, Source};

/// The ignored parent fixture test invokes this function in an env_clear sandbox.
/// Failure is fatal to the fixture; this helper never falls back to the real session.
pub fn serve() {
    if let Err(error) = serve_checked() {
        panic!("private clipboard fixture failed: {error}");
    }
}
fn serve_checked() -> Result<(), Box<dyn std::error::Error>> {
    if env::var("HERMES_ISLAND_CLIPBOARD_FIXTURE").as_deref() != Ok("1") {
        return Err("clipboard fixture opt-in missing".into());
    }
    let root = PathBuf::from(env::var("HERMES_ISLAND_SCRATCH")?).canonicalize()?;
    // The parent passes its temp directory explicitly because env_clear removes
    // TMPDIR. Accept only a direct, named GUI sandbox child, on any developer PC.
    let temporary = env::temp_dir().canonicalize()?;
    if root.parent() != Some(temporary.as_path())
        || !root
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("hi-gui-"))
    {
        return Err("fixture root is not a private GUI scratch directory".into());
    }
    for key in [
        "HOME",
        "XDG_RUNTIME_DIR",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_CACHE_HOME",
    ] {
        let path = PathBuf::from(env::var(key)?).canonicalize()?;
        if !path.starts_with(&root) {
            return Err(format!("fixture {key} escapes sandbox").into());
        }
    }
    if env::var("WAYLAND_DISPLAY").as_deref() != Ok("hi-gui-wayland")
        || env::var_os("WAYLAND_SOCKET").is_some()
    {
        return Err("fixture Wayland endpoint is not the private compositor".into());
    }
    let socket = PathBuf::from(env::var("XDG_RUNTIME_DIR")?)
        .join("hi-gui-wayland")
        .canonicalize()?;
    if !socket.starts_with(&root) {
        return Err("fixture Wayland socket escapes sandbox".into());
    }
    let address = env::var("DBUS_SESSION_BUS_ADDRESS")?;
    let bus = address
        .strip_prefix("unix:path=")
        .ok_or("fixture requires a private path bus")?
        .split(',')
        .next()
        .ok_or("missing bus path")?;
    if !Path::new(bus).canonicalize()?.starts_with(&root) {
        return Err("fixture bus escapes sandbox".into());
    }
    let ready = PathBuf::from(env::var("HI_CLIPBOARD_READY")?);
    let parent = ready
        .parent()
        .ok_or("invalid fixture ready path")?
        .canonicalize()?;
    if !parent.starts_with(&root) || fs::symlink_metadata(&ready).is_ok() {
        return Err("fixture ready path escapes sandbox or exists".into());
    }
    let mode = env::var("HI_CLIPBOARD_MODE")?;
    let text = env::var("HI_CLIPBOARD_TEXT").unwrap_or_else(|_| "private clipboard fixture".into());
    // Extra MIME entries offered next to the primary one (Phase 6a test modes).
    let mut extra: Vec<(&str, Vec<u8>)> = Vec::new();
    let (mime, bytes) = match mode.as_str() {
        "text" | "sensitive" | "once" => ("text/plain;charset=utf-8", text.into_bytes()),
        "concealed" => {
            extra.push(("org.nspasteboard.ConcealedType", b"1".to_vec()));
            ("text/plain;charset=utf-8", text.into_bytes())
        }
        "transient" => {
            extra.push(("org.nspasteboard.TransientType", b"1".to_vec()));
            ("text/plain;charset=utf-8", text.into_bytes())
        }
        // Text of HI_CLIPBOARD_BYTES bytes (ASCII filler), for cap tests.
        "huge" => {
            let n: usize = env::var("HI_CLIPBOARD_BYTES")?.parse()?;
            ("text/plain;charset=utf-8", vec![b'a'; n])
        }
        // Normal text plus an oversized (> 64 byte) password-manager hint.
        "bighint" => {
            extra.push(("x-kde-passwordManagerHint", vec![b's'; 200]));
            ("text/plain;charset=utf-8", text.into_bytes())
        }
        "files" => (
            "text/uri-list",
            format!(
                "# private fixture\r\nfile://{}/one.txt\r\nfile://{}/two.txt\r\n",
                root.display(),
                root.display()
            )
            .into_bytes(),
        ),
        "png" => {
            let image = image::RgbaImage::from_pixel(2, 3, image::Rgba([23, 119, 211, 255]));
            let mut bytes = Cursor::new(Vec::new());
            image::DynamicImage::ImageRgba8(image).write_to(&mut bytes, image::ImageFormat::Png)?;
            ("image/png", bytes.into_inner())
        }
        _ => return Err("unknown clipboard fixture mode".into()),
    };
    let mut options = Options::new();
    options.foreground(true).sensitive(mode == "sensitive");
    // Modes with HI_CLIPBOARD_SERVED serve exactly one content request, then `serve()`
    // returns and the fixture writes that marker. A watcher that skips an offer by MIME
    // alone never requests the content, so the marker stays absent.
    let served = env::var_os("HI_CLIPBOARD_SERVED").map(PathBuf::from);
    if served.is_some() {
        options.serve_requests(ServeRequests::Only(1));
    }
    let mut sources = vec![MimeSource {
        source: Source::Bytes(bytes.into_boxed_slice()),
        mime_type: MimeType::Specific(mime.into()),
    }];
    for (extra_mime, extra_bytes) in extra {
        sources.push(MimeSource {
            source: Source::Bytes(extra_bytes.into_boxed_slice()),
            mime_type: MimeType::Specific(extra_mime.into()),
        });
    }
    let prepared = options.prepare_copy_multi(sources)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(ready)?;
    file.write_all(b"READY\n")?;
    file.sync_all()?;
    prepared.serve()?;
    if let Some(served) = served {
        let parent = served
            .parent()
            .ok_or("invalid served path")?
            .canonicalize()?;
        if !parent.starts_with(&root) {
            return Err("fixture served path escapes sandbox".into());
        }
        fs::write(served, b"SERVED\n")?;
    }
    Ok(())
}
