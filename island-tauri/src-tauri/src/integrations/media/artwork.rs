//! Bounded host-owned artwork. No player-supplied URL ever reaches the webview.
use super::MediaService;
use base64::{engine::general_purpose::STANDARD, Engine};
use std::{
    collections::VecDeque,
    io::{Cursor, Read},
    net::IpAddr,
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use tokio::sync::watch;

const MAX_SOURCE: usize = 2 * 1024 * 1024;
const MAX_OUTPUT: usize = 1_400_000;
const TIMEOUT: Duration = Duration::from_secs(8);
pub(super) type Key = (String, Option<String>, String, String);

fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            !v.is_private()
                && !v.is_loopback()
                && !v.is_link_local()
                && !v.is_unspecified()
                && !v.is_multicast()
                && !v.is_broadcast()
                && v.octets()[0] != 0
                && v.octets()[0] < 224
        }
        IpAddr::V6(v) => v
            .to_ipv4_mapped()
            .map(|v| public_ip(IpAddr::V4(v)))
            .unwrap_or_else(|| {
                !v.is_loopback()
                    && !v.is_unspecified()
                    && !v.is_multicast()
                    && v.segments()[0] & 0xfe00 != 0xfc00
                    && v.segments()[0] & 0xffc0 != 0xfe80
            }),
    }
}

fn https_url(raw: &str) -> Option<reqwest::Url> {
    let url = reqwest::Url::parse(raw).ok()?;
    (url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port_or_known_default() == Some(443)
        && url.domain().is_some()
        && url.fragment().is_none())
    .then_some(url)
}

fn file_path(raw: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".cache"));
    let mut roots = vec![cache, home.join("Music"), home.join("Pictures")];
    if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR") {
        roots.push(PathBuf::from(runtime));
    }
    file_path_in(raw, &roots, true)
}

fn file_path_in(raw: &str, roots: &[PathBuf], browser_tmp: bool) -> Option<PathBuf> {
    let url = reqwest::Url::parse(raw).ok()?;
    if url.scheme() != "file"
        || url
            .host_str()
            .is_some_and(|h| !h.is_empty() && h != "localhost")
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    let path = url.to_file_path().ok()?;
    let canonical = path.canonicalize().ok()?;
    // Reject symlinks (including directory symlinks) rather than following them into secrets.
    if canonical != path {
        return None;
    }
    let allowed = roots.iter().any(|d| canonical.starts_with(d));
    let browser_art = browser_tmp
        && canonical.parent() == Some(std::path::Path::new("/tmp"))
        && canonical
            .file_name()?
            .to_str()?
            .starts_with("plasma-browser-integration_artwork_");
    (allowed || browser_art).then_some(canonical)
}

fn local_bytes(raw: &str) -> Option<Vec<u8>> {
    if raw.starts_with("data:") {
        if raw.len() > MAX_SOURCE * 2 {
            return None;
        }
        let (head, body) = raw.split_once(',')?;
        if !matches!(
            head,
            "data:image/png;base64" | "data:image/jpeg;base64" | "data:image/webp;base64"
        ) {
            return None;
        }
        let bytes = STANDARD.decode(body).ok()?;
        return (bytes.len() <= MAX_SOURCE).then_some(bytes);
    }
    read_image_file(file_path(raw)?)
}

fn read_image_file(path: PathBuf) -> Option<Vec<u8>> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .ok()?;
    let meta = file.metadata().ok()?;
    // SAFETY: geteuid takes no pointers and cannot fail.
    if !meta.is_file() || meta.uid() != unsafe { libc::geteuid() } || meta.len() > MAX_SOURCE as u64
    {
        return None;
    }
    let mut bytes = Vec::new();
    file.take((MAX_SOURCE + 1) as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() <= MAX_SOURCE).then_some(bytes)
}

fn normalize(bytes: &[u8]) -> Option<String> {
    if bytes.len() > MAX_SOURCE {
        return None;
    }
    let format = image::guess_format(bytes).ok()?;
    if !matches!(
        format,
        image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::WebP
    ) {
        return None;
    }
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().ok()?.thumbnail(512, 512).to_rgb8();
    let mut encoded = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut encoded, 85)
        .encode_image(&image)
        .ok()?;
    let data = format!("data:image/jpeg;base64,{}", STANDARD.encode(encoded));
    (data.len() <= MAX_OUTPUT).then_some(data)
}

async fn remote_bytes(raw: &str) -> Option<Vec<u8>> {
    let url = https_url(raw)?;
    let host = url.domain()?.to_owned();
    let addresses: Vec<_> = tokio::net::lookup_host((host.as_str(), 443))
        .await
        .ok()?
        .collect();
    if addresses.is_empty() || addresses.iter().any(|a| !public_ip(a.ip())) {
        return None;
    }
    // Pin the validated DNS answer; no redirect or environment proxy can reach localhost.
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(TIMEOUT)
        .connect_timeout(TIMEOUT)
        .resolve_to_addrs(&host, &addresses)
        .user_agent(super::lrclib::USER_AGENT)
        .build()
        .ok()?;
    let mut response = client.get(url).send().await.ok()?;
    if !response.status().is_success()
        || response
            .content_length()
            .is_some_and(|n| n > MAX_SOURCE as u64)
    {
        return None;
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        if bytes.len() + chunk.len() > MAX_SOURCE {
            return None;
        }
        bytes.extend_from_slice(&chunk);
    }
    Some(bytes)
}

async fn load(raw: String) -> Option<String> {
    let bytes = if raw.starts_with("https:") {
        tokio::time::timeout(TIMEOUT, remote_bytes(&raw))
            .await
            .ok()??
    } else {
        tokio::task::spawn_blocking(move || local_bytes(&raw))
            .await
            .ok()??
    };
    tokio::task::spawn_blocking(move || normalize(&bytes))
        .await
        .ok()?
}

pub(super) async fn worker(media: Arc<MediaService>, mut stop: watch::Receiver<bool>) {
    let mut changes = media.art_changes.subscribe();
    let mut cache: VecDeque<(Key, Option<String>)> = VecDeque::new();
    loop {
        changes.borrow_and_update();
        if *stop.borrow() {
            return;
        }
        if let Some(key) = media.artwork_request() {
            if let Some((_, image)) = cache.iter().find(|(k, _)| *k == key) {
                media.artwork_loaded(&key, image.clone());
            } else {
                let image = tokio::select! {
                    _ = stop.changed() => return,
                    _ = changes.changed() => continue,
                    image = load(key.3.clone()) => image,
                };
                media.artwork_loaded(&key, image.clone());
                cache.push_back((key, image));
                if cache.len() > 12 {
                    cache.pop_front();
                }
            }
        }
        tokio::select! { _ = stop.changed() => return, _ = changes.changed() => {} }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore = "explicit read-only network smoke"]
    async fn spotify_https_jpeg_normalizes_through_the_real_loader() {
        let data = load("https://i.scdn.co/image/ab67616d0000b2739870c9519822a973411b6594".into())
            .await
            .unwrap();
        assert!(data.starts_with("data:image/jpeg;base64,"));
        assert!(data.len() <= MAX_OUTPUT);
        println!(
            "Spotify JPEG host loader returned {} bounded data-URL bytes",
            data.len()
        );
    }
    fn jpeg() -> Vec<u8> {
        let image = image::RgbImage::new(3, 3);
        let mut bytes = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut bytes)
            .encode_image(&image)
            .unwrap();
        bytes
    }
    #[test]
    fn jpeg_data_is_decoded_and_normalized_but_svg_and_oversize_are_rejected() {
        let raw = format!("data:image/jpeg;base64,{}", STANDARD.encode(jpeg()));
        assert!(normalize(&local_bytes(&raw).unwrap())
            .unwrap()
            .starts_with("data:image/jpeg;base64,"));
        assert!(local_bytes("data:image/svg+xml;base64,PHN2Zy8+").is_none());
        assert!(normalize(&vec![0; MAX_SOURCE + 1]).is_none());
        assert!(local_bytes(&format!(
            "data:image/png;base64,{}",
            STANDARD.encode(vec![0; MAX_SOURCE + 1])
        ))
        .is_none());
    }
    #[test]
    fn https_only_public_targets_no_credentials_or_alternate_ports() {
        assert!(https_url("https://i.scdn.co/image/abc").is_some());
        for raw in [
            "http://example.org/a",
            "https://127.0.0.1/a",
            "https://a:b@example.org/a",
            "https://example.org:444/a",
            "file:///etc/passwd",
        ] {
            assert!(https_url(raw).is_none(), "{raw}");
        }
        for ip in [
            "127.0.0.1",
            "10.0.0.1",
            "169.254.169.254",
            "::1",
            "fc00::1",
            "::ffff:127.0.0.1",
        ] {
            assert!(!public_ip(ip.parse().unwrap()));
        }
    }
    #[test]
    fn file_art_is_allowed_only_in_known_art_directories() {
        let root = std::env::temp_dir();
        let path = root.join(format!("media-art-{}.jpg", std::process::id()));
        std::fs::write(&path, jpeg()).unwrap();
        let url = reqwest::Url::from_file_path(&path).unwrap().to_string();
        let approved = file_path_in(&url, std::slice::from_ref(&root), false).unwrap();
        assert!(normalize(&read_image_file(approved.clone()).unwrap()).is_some());
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(MAX_SOURCE as u64 + 1)
            .unwrap();
        assert!(
            read_image_file(approved).is_none(),
            "oversize local art is rejected before reading"
        );
        std::fs::remove_file(&path).unwrap();
        assert!(file_path("file:///etc/passwd").is_none());
        assert!(file_path("file://remote/etc/passwd").is_none());
    }

    #[test]
    fn png_and_webp_are_decoded_and_bounded() {
        for format in [image::ImageFormat::Png, image::ImageFormat::WebP] {
            let image = image::DynamicImage::new_rgb8(8, 8);
            let mut bytes = Cursor::new(Vec::new());
            image.write_to(&mut bytes, format).unwrap();
            assert!(normalize(bytes.get_ref())
                .unwrap()
                .starts_with("data:image/jpeg;base64,"));
        }
    }
}
