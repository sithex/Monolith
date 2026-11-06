//! Release UI from disk. A release binary serves `island-tauri/dist` straight from disk when the
//! manifest the build writes there (`dist/.monolith-manifest.json`) verifies, so a UI-only
//! change deploys with `scripts/deploy-ui.sh` (rebuild `dist/`, then the `reload-ui` panel
//! command) instead of a Rust rebuild. Every other case serves the assets embedded at compile
//! time, and the log says why.
//!
//! The disk set is accepted only when ALL of these hold:
//! - `dist/`, every directory on the way to a listed file, every listed file and the manifest
//!   are real (non-symlink) entries owned by the effective UID and not group/world-writable;
//! - every listed file's SHA-256 matches the manifest;
//! - the manifest's `contractHash` equals the hash of the `ui/ipc/bindings.ts` this binary was
//!   compiled with (a UI built against a newer Rust contract never runs on an older binary);
//! - every HTML page the embedded set has is present, and no HTML page needs the compile-time
//!   CSP rewriting Tauri applies to embedded HTML (inline `<style>`/`<script>` or a remote
//!   `<script src>`); see `html_needs_codegen`.
//!
//! Verified bytes are read into memory once, so later edits to `dist/` cannot change what is
//! served until the next verified reload. Tauri's own `get_asset` still does the path fallbacks,
//! the MIME sniffing and the CSP header; this module only supplies bytes and the same per-script
//! `sha256-` CSP sources `tauri-codegen` computes for embedded `.js`/`.mjs` files.
//!
//! Escape hatches: `--embedded-ui` or `HERMES_ISLAND_EMBEDDED_UI=1` force the embedded assets.
//! Debug builds always use the embedded assets (tests stay byte-for-byte unchanged).

use anyhow::{anyhow, bail, ensure, Context as _, Result};
use base64::Engine as _;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex, OnceLock, RwLock},
};
use tauri::utils::assets::{AssetKey, AssetsIter, CspHash};
use tauri::{Assets, Runtime};

pub const MANIFEST_NAME: &str = ".monolith-manifest.json";
pub const FORCE_EMBEDDED_ENV: &str = "HERMES_ISLAND_EMBEDDED_UI";
pub const FORCE_EMBEDDED_FLAG: &str = "--embedded-ui";
pub const PRINT_CONTRACT_FLAG: &str = "--print-contract-hash";
const MANIFEST_VERSION: u32 = 1;
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_FILES: usize = 4096;
const MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024;

/// The IPC contract the UI was generated against; `scripts/write-ui-manifest.mjs` hashes the
/// same file. Cargo tracks `include_bytes!` inputs, so a bindings change rebuilds this.
static BINDINGS: &[u8] = include_bytes!("../../ui/ipc/bindings.ts");

pub fn contract_hash() -> &'static str {
    static HASH: OnceLock<String> = OnceLock::new();
    HASH.get_or_init(|| sha256_hex(BINDINGS))
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Removes `--embedded-ui` from the argument list; `true` when it was present.
pub fn take_force_flag(args: &mut Vec<String>) -> bool {
    let before = args.len();
    args.retain(|a| a != FORCE_EMBEDDED_FLAG);
    args.len() != before
}

/// Why the embedded assets are forced, if they are. Pure: inputs are passed in.
pub fn forced_reason(flag: bool, env: Option<&str>) -> Option<&'static str> {
    if flag {
        Some("--embedded-ui")
    } else if env.is_some_and(|v| !v.is_empty() && v != "0") {
        Some("HERMES_ISLAND_EMBEDDED_UI")
    } else {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expect {
    pub uid: u32,
    pub contract: String,
    /// Asset keys (`/island.html`) that must be present: the embedded HTML pages.
    pub required: Vec<String>,
}

#[derive(Debug)]
pub struct Snapshot {
    files: HashMap<String, Vec<u8>>,
    script_hashes: Vec<&'static str>,
    /// First 12 hex digits of the manifest's SHA-256, for logs and the reload reply.
    pub manifest_id: String,
}

impl Snapshot {
    pub fn len(&self) -> usize {
        self.files.len()
    }
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    version: u32,
    contract_hash: String,
    /// Informational only (written by `scripts/write-ui-manifest.mjs`).
    #[serde(default)]
    #[allow(dead_code)]
    generated_at: Option<String>,
    files: BTreeMap<String, String>,
}

/// A rejection that only `./build.sh` (a matching binary) can fix.
#[derive(Debug)]
pub struct ContractMismatch {
    pub dist: String,
    pub binary: String,
}

impl std::fmt::Display for ContractMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "contract mismatch: dist/ was built against bindings {} but this binary has {} (needs ./build.sh)",
            short(&self.dist),
            short(&self.binary)
        )
    }
}

impl std::error::Error for ContractMismatch {}

fn short(hash: &str) -> &str {
    hash.get(..12).unwrap_or(hash)
}

fn check_private(meta: &fs::Metadata, uid: u32, what: &Path) -> Result<()> {
    ensure!(
        meta.uid() == uid,
        "{} is owned by uid {}, not {uid}",
        what.display(),
        meta.uid()
    );
    ensure!(
        meta.mode() & 0o022 == 0,
        "{} is group/world-writable (mode {:o})",
        what.display(),
        meta.mode() & 0o7777
    );
    Ok(())
}

fn check_dir(path: &Path, uid: u32) -> Result<()> {
    let meta = fs::symlink_metadata(path).with_context(|| format!("{}", path.display()))?;
    ensure!(
        meta.file_type().is_dir(),
        "{} is not a real directory",
        path.display()
    );
    check_private(&meta, uid, path)
}

/// Opens a regular private file without following a final symlink and reads at most `cap` bytes.
fn read_private(path: &Path, uid: u32, cap: u64) -> Result<Vec<u8>> {
    let file: File = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
        .with_context(|| format!("{}", path.display()))?;
    let meta = file.metadata()?;
    ensure!(meta.is_file(), "{} is not a regular file", path.display());
    check_private(&meta, uid, path)?;
    ensure!(
        meta.len() <= cap,
        "{} is larger than the {cap}-byte budget",
        path.display()
    );
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    file.take(cap + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= cap,
        "{} grew while reading",
        path.display()
    );
    Ok(bytes)
}

/// Manifest paths are plain relative paths below `dist/` (no `..`, `.`, root or empty parts).
fn validate_rel(rel: &str) -> Result<()> {
    ensure!(
        !rel.is_empty() && !rel.contains('\\') && !rel.contains('\0'),
        "bad manifest path {rel:?}"
    );
    let path = Path::new(rel);
    ensure!(
        path.components().all(|c| matches!(c, Component::Normal(_)))
            && !rel.ends_with('/')
            && !rel.contains("//"),
        "bad manifest path {rel:?}"
    );
    ensure!(rel != MANIFEST_NAME, "manifest lists itself");
    Ok(())
}

/// Embedded HTML is rewritten at compile time (style nonces, remote-script nonces, inline
/// script hashes). A page needing any of that cannot be served from disk identically, so it
/// is refused. Vite's output (module `<script src="/assets/..">` + `<link>`) needs none.
pub fn html_needs_codegen(html: &[u8]) -> Option<&'static str> {
    let lower = String::from_utf8_lossy(html).to_ascii_lowercase();
    if lower.contains("<style") {
        return Some("an inline <style> element");
    }
    let mut rest = lower.as_str();
    while let Some(at) = rest.find("<script") {
        let after = &rest[at + "<script".len()..];
        let Some(end) = after.find('>') else {
            return Some("an unterminated <script> tag");
        };
        let tag = &after[..end];
        let body = &after[end + 1..];
        let close = body.find("</script").unwrap_or(body.len());
        if !tag.trim_end().ends_with('/') && !body[..close].trim().is_empty() {
            return Some("an inline <script>");
        }
        let compact: String = tag.chars().filter(|c| !c.is_whitespace()).collect();
        if ["src=http", "src=\"http", "src='http"]
            .iter()
            .any(|p| compact.contains(p))
        {
            return Some("a remote <script src>");
        }
        rest = &body[close..];
    }
    None
}

/// The CSP source `tauri-codegen` adds for an embedded `.js`/`.mjs` file:
/// `'sha256-<base64(sha256(script with CRLF/CR normalised to LF))>'`.
pub fn script_csp_source(bytes: &[u8]) -> String {
    let mut normal = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\r' {
            normal.push(b'\n');
            i += if bytes.get(i + 1) == Some(&b'\n') {
                2
            } else {
                1
            };
        } else {
            normal.push(bytes[i]);
            i += 1;
        }
    }
    format!(
        "'sha256-{}'",
        base64::engine::general_purpose::STANDARD.encode(Sha256::digest(&normal))
    )
}

/// CSP sources must be `&'static str` (`CspHash<'_>` borrows from the provider, which sits
/// behind a lock). Each distinct string is leaked once, so reloads stay bounded.
fn intern(value: String) -> &'static str {
    static POOL: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let mut pool = POOL
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    if let Some(existing) = pool.get(value.as_str()) {
        return existing;
    }
    let leaked: &'static str = Box::leak(value.into_boxed_str());
    pool.insert(leaked);
    leaked
}

/// Verifies `dir` against its manifest and returns the in-memory asset set.
pub fn verify(dir: &Path, expect: &Expect) -> Result<Snapshot> {
    check_dir(dir, expect.uid)?;
    let manifest_path = dir.join(MANIFEST_NAME);
    let raw = read_private(&manifest_path, expect.uid, MAX_MANIFEST_BYTES)
        .context("no usable manifest (run ./build.sh or scripts/deploy-ui.sh)")?;
    let manifest: Manifest = serde_json::from_slice(&raw).context("manifest is not valid")?;
    ensure!(
        manifest.version == MANIFEST_VERSION,
        "manifest version {} (expected {MANIFEST_VERSION})",
        manifest.version
    );
    if manifest.contract_hash != expect.contract {
        return Err(anyhow!(ContractMismatch {
            dist: manifest.contract_hash,
            binary: expect.contract.clone(),
        }));
    }
    ensure!(
        !manifest.files.is_empty() && manifest.files.len() <= MAX_FILES,
        "manifest lists {} files",
        manifest.files.len()
    );
    let mut checked_dirs: HashSet<PathBuf> = HashSet::new();
    let mut budget = MAX_TOTAL_BYTES;
    let mut files = HashMap::with_capacity(manifest.files.len());
    let mut script_hashes = Vec::new();
    for (rel, want) in &manifest.files {
        validate_rel(rel)?;
        ensure!(
            want.len() == 64 && want.bytes().all(|b| b.is_ascii_hexdigit()),
            "bad hash for {rel}"
        );
        let mut parent = dir.to_path_buf();
        let parts: Vec<_> = Path::new(rel).components().collect();
        for part in &parts[..parts.len() - 1] {
            parent.push(part);
            if checked_dirs.insert(parent.clone()) {
                check_dir(&parent, expect.uid)?;
            }
        }
        let bytes = read_private(&dir.join(rel), expect.uid, budget)?;
        budget -= bytes.len() as u64;
        ensure!(
            sha256_hex(&bytes).eq_ignore_ascii_case(want),
            "{rel} does not match the manifest (modified after the build?)"
        );
        if rel.ends_with(".html") {
            if let Some(what) = html_needs_codegen(&bytes) {
                bail!("{rel} contains {what}, which only the embedded build can serve");
            }
        }
        if rel.ends_with(".js") || rel.ends_with(".mjs") {
            script_hashes.push(intern(script_csp_source(&bytes)));
        }
        files.insert(AssetKey::from(rel.as_str()).into(), bytes);
    }
    if let Some(missing) = expect.required.iter().find(|k| !files.contains_key(*k)) {
        bail!("{missing} is missing from the manifest");
    }
    Ok(Snapshot {
        files,
        script_hashes,
        manifest_id: sha256_hex(&raw)[..12].to_owned(),
    })
}

/// Shared between the asset provider and the `reload-ui` command.
pub struct UiState {
    dir: PathBuf,
    expect: Expect,
    disk: RwLock<Option<Arc<Snapshot>>>,
}

#[derive(Debug)]
pub enum Refresh {
    Disk { files: usize, manifest: String },
    Rejected(anyhow::Error),
}

impl UiState {
    pub fn new(dir: PathBuf, expect: Expect) -> Self {
        Self {
            dir,
            expect,
            disk: RwLock::new(None),
        }
    }

    /// Re-verifies `dist/`. Success swaps in the new set; a rejection keeps whatever is being
    /// served now (the embedded set, or the last verified disk set).
    pub fn refresh(&self) -> Refresh {
        match verify(&self.dir, &self.expect) {
            Ok(snapshot) => {
                let out = Refresh::Disk {
                    files: snapshot.len(),
                    manifest: snapshot.manifest_id.clone(),
                };
                *self.disk.write().unwrap_or_else(|p| p.into_inner()) = Some(Arc::new(snapshot));
                out
            }
            Err(error) => Refresh::Rejected(error),
        }
    }

    pub fn source(&self) -> &'static str {
        if self.current().is_some() {
            "disk"
        } else {
            "embedded"
        }
    }

    fn current(&self) -> Option<Arc<Snapshot>> {
        self.disk.read().unwrap_or_else(|p| p.into_inner()).clone()
    }
}

/// Disk set when one verified, otherwise the embedded set. Never mixes the two.
pub struct UiAssets<R: Runtime> {
    embedded: Box<dyn Assets<R>>,
    state: Arc<UiState>,
}

impl<R: Runtime> UiAssets<R> {
    pub fn new(embedded: Box<dyn Assets<R>>, state: Arc<UiState>) -> Self {
        Self { embedded, state }
    }
}

impl<R: Runtime> Assets<R> for UiAssets<R> {
    fn setup(&self, app: &tauri::App<R>) {
        self.embedded.setup(app);
    }

    fn get(&self, key: &AssetKey) -> Option<Cow<'_, [u8]>> {
        match self.state.current() {
            Some(disk) => disk.files.get(key.as_ref()).cloned().map(Cow::Owned),
            None => self.embedded.get(key),
        }
    }

    fn iter(&self) -> Box<AssetsIter<'_>> {
        match self.state.current() {
            Some(disk) => {
                let mut all: Vec<(Cow<'_, str>, Cow<'_, [u8]>)> = disk
                    .files
                    .iter()
                    .map(|(k, v)| (Cow::Owned(k.clone()), Cow::Owned(v.clone())))
                    .collect();
                all.sort_by(|a, b| a.0.cmp(&b.0));
                Box::new(all.into_iter())
            }
            None => self.embedded.iter(),
        }
    }

    fn csp_hashes(&self, html_path: &AssetKey) -> Box<dyn Iterator<Item = CspHash<'_>> + '_> {
        match self.state.current() {
            Some(disk) => Box::new(disk.script_hashes.clone().into_iter().map(CspHash::Script)),
            None => self.embedded.csp_hashes(html_path),
        }
    }
}

struct NoAssets;

impl<R: Runtime> Assets<R> for NoAssets {
    fn get(&self, _: &AssetKey) -> Option<Cow<'_, [u8]>> {
        None
    }
    fn iter(&self) -> Box<AssetsIter<'_>> {
        Box::new(std::iter::empty())
    }
    fn csp_hashes(&self, _: &AssetKey) -> Box<dyn Iterator<Item = CspHash<'_>> + '_> {
        Box::new(std::iter::empty())
    }
}

static STATE: OnceLock<Arc<UiState>> = OnceLock::new();

/// Wraps the context's embedded assets (release builds, unless forced embedded) and logs
/// which set the island starts with and why.
pub fn install<R: Runtime>(
    context: &mut tauri::Context<R>,
    repo_root: &Path,
    forced: Option<&str>,
) {
    if cfg!(debug_assertions) {
        tracing::debug!("UI: embedded assets (debug build)");
        return;
    }
    if let Some(reason) = forced {
        tracing::info!("UI: embedded assets (forced by {reason})");
        return;
    }
    let required: Vec<String> = context
        .assets()
        .iter()
        .map(|(key, _)| key.into_owned())
        .filter(|key| key.ends_with(".html"))
        .collect();
    let dir = repo_root.join("island-tauri/dist");
    let state = Arc::new(UiState::new(
        dir.clone(),
        Expect {
            // SAFETY: geteuid has no preconditions and cannot fail.
            uid: unsafe { libc::geteuid() },
            contract: contract_hash().to_owned(),
            required,
        },
    ));
    match state.refresh() {
        Refresh::Disk { files, manifest } => tracing::info!(
            dir = %dir.display(), files, manifest = %manifest,
            "UI: serving the verified dist/ from disk"
        ),
        Refresh::Rejected(error) => tracing::warn!(
            dir = %dir.display(),
            "UI: embedded assets (disk UI rejected: {error:#})"
        ),
    }
    let embedded = context.set_assets(Box::new(NoAssets));
    context.set_assets(Box::new(UiAssets::new(embedded, state.clone())));
    let _ = STATE.set(state);
}

/// `reload-ui` panel command: re-verify `dist/` and, when it verifies, reload the island
/// webview. Reads ~2 MB on the panel thread (no GUI wait); the reload itself is queued to the
/// event loop. A rejected set keeps the current page untouched.
pub fn reload_ui<R: Runtime>(app: &tauri::AppHandle<R>) -> serde_json::Value {
    use serde_json::json;
    use tauri::Manager;
    let contract = contract_hash();
    let Some(state) = STATE.get() else {
        let why = if cfg!(debug_assertions) {
            "debug build"
        } else {
            "embedded UI forced"
        };
        return json!({"ok":false,"ui":"embedded","contract":contract,"error":format!("disk UI disabled ({why})")});
    };
    match state.refresh() {
        Refresh::Disk { files, manifest } => {
            let Some(window) = app.get_webview_window(crate::window::ISLAND_LABEL) else {
                return json!({"ok":false,"ui":"disk","contract":contract,"error":"no island window"});
            };
            if let Err(error) = window.reload() {
                return json!({"ok":false,"ui":"disk","contract":contract,"error":format!("webview reload failed: {error}")});
            }
            tracing::info!(files, manifest = %manifest, "UI: reloaded the island from the verified dist/");
            json!({"ok":true,"ui":"disk","files":files,"manifest":manifest,"contract":contract})
        }
        Refresh::Rejected(error) => {
            let needs_build = error.downcast_ref::<ContractMismatch>().is_some();
            tracing::warn!(
                "UI: reload refused, still serving {}: {error:#}",
                state.source()
            );
            json!({"ok":false,"ui":state.source(),"contract":contract,"needsBuild":needs_build,"error":format!("{error:#}")})
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    struct Fixture(PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const ISLAND: &str = "<!doctype html><html><head><script type=\"module\" crossorigin src=\"/assets/island.js\"></script><link rel=\"stylesheet\" href=\"/assets/island.css\"></head><body><div id=\"root\"></div></body></html>";

    fn uid() -> u32 {
        unsafe { libc::geteuid() }
    }

    fn expect() -> Expect {
        Expect {
            uid: uid(),
            contract: "c".repeat(64),
            required: vec!["/island.html".into()],
        }
    }

    fn write_manifest(dir: &Path, contract: &str) {
        let mut files = serde_json::Map::new();
        for rel in ["island.html", "assets/island.js", "assets/island.css"] {
            let bytes = fs::read(dir.join(rel)).unwrap();
            files.insert(rel.into(), sha256_hex(&bytes).into());
        }
        let manifest = serde_json::json!({"version":1,"contractHash":contract,"generatedAt":"t","files":files});
        fs::write(dir.join(MANIFEST_NAME), manifest.to_string()).unwrap();
    }

    fn dist(name: &str) -> Fixture {
        let dir = std::env::temp_dir().join(format!("hi-ui-assets-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("assets")).unwrap();
        for d in [dir.clone(), dir.join("assets")] {
            fs::set_permissions(&d, fs::Permissions::from_mode(0o755)).unwrap();
        }
        fs::write(dir.join("island.html"), ISLAND).unwrap();
        fs::write(dir.join("assets/island.js"), "console.log(1)\r\n").unwrap();
        fs::write(dir.join("assets/island.css"), "body{}").unwrap();
        write_manifest(&dir, &"c".repeat(64));
        Fixture(dir)
    }

    fn reason(dir: &Path, expect: &Expect) -> String {
        format!("{:#}", verify(dir, expect).unwrap_err())
    }

    #[test]
    fn verified_dist_serves_every_listed_file_with_codegen_script_hashes() {
        let f = dist("ok");
        let snap = verify(&f.0, &expect()).unwrap();
        assert_eq!(snap.len(), 3);
        assert_eq!(snap.files["/island.html"], ISLAND.as_bytes());
        // Same normalisation as tauri-utils `normalize_script_for_csp` (CRLF -> LF).
        assert_eq!(
            snap.script_hashes,
            vec![script_csp_source(b"console.log(1)\n").as_str()]
        );
        assert!(
            snap.script_hashes[0].starts_with("'sha256-") && snap.script_hashes[0].ends_with("='")
        );
    }

    #[test]
    fn tampered_file_is_rejected() {
        let f = dist("tamper");
        fs::write(f.0.join("assets/island.js"), "evil()").unwrap();
        assert!(reason(&f.0, &expect()).contains("assets/island.js does not match"));
    }

    #[test]
    fn wrong_owner_is_rejected_for_dir_and_files() {
        let f = dist("owner");
        let mut other = expect();
        other.uid = uid() + 1;
        assert!(reason(&f.0, &other).contains("is owned by uid"));
    }

    #[test]
    fn group_or_world_writable_entries_are_rejected() {
        let f = dist("perm");
        let file = f.0.join("assets/island.css");
        fs::set_permissions(&file, fs::Permissions::from_mode(0o664)).unwrap();
        assert!(reason(&f.0, &expect()).contains("island.css is group/world-writable"));
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        fs::set_permissions(f.0.join("assets"), fs::Permissions::from_mode(0o757)).unwrap();
        assert!(reason(&f.0, &expect()).contains("assets is group/world-writable"));
        fs::set_permissions(f.0.join("assets"), fs::Permissions::from_mode(0o755)).unwrap();
        fs::set_permissions(&f.0, fs::Permissions::from_mode(0o775)).unwrap();
        assert!(reason(&f.0, &expect()).contains("group/world-writable"));
        fs::set_permissions(&f.0, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(verify(&f.0, &expect()).is_ok());
    }

    #[test]
    fn wrong_contract_hash_is_a_contract_mismatch() {
        let f = dist("contract");
        write_manifest(&f.0, &"d".repeat(64));
        let error = verify(&f.0, &expect()).unwrap_err();
        assert!(error.downcast_ref::<ContractMismatch>().is_some());
        assert!(format!("{error:#}").contains("needs ./build.sh"));
    }

    #[test]
    fn symlinks_missing_pages_bad_paths_and_missing_manifest_are_rejected() {
        let f = dist("links");
        let mut more = expect();
        more.required.push("/index.html".into());
        assert!(reason(&f.0, &more).contains("/index.html is missing"));

        fs::remove_file(f.0.join("assets/island.css")).unwrap();
        std::os::unix::fs::symlink("island.js", f.0.join("assets/island.css")).unwrap();
        assert!(verify(&f.0, &expect()).is_err());

        for bad in [
            "../x",
            "/etc/passwd",
            "a/../b",
            "./a",
            "a//b",
            "",
            MANIFEST_NAME,
        ] {
            assert!(validate_rel(bad).is_err(), "{bad:?}");
        }
        fs::remove_file(f.0.join(MANIFEST_NAME)).unwrap();
        assert!(reason(&f.0, &expect()).contains("no usable manifest"));
    }

    #[test]
    fn html_that_needs_compile_time_csp_rewriting_is_refused() {
        assert_eq!(html_needs_codegen(ISLAND.as_bytes()), None);
        assert_eq!(
            html_needs_codegen(b"<script src=x></script><script\n src=y />"),
            None
        );
        assert!(html_needs_codegen(b"<head><style>a{}</style>").is_some());
        assert!(html_needs_codegen(b"<script>alert(1)</script>").is_some());
        assert!(
            html_needs_codegen(b"<script type=module src = 'https://x/y.js'></script>").is_some()
        );
        assert!(html_needs_codegen(b"<script src=x").is_some());
    }

    struct Fake(Vec<u8>);
    impl<R: Runtime> Assets<R> for Fake {
        fn get(&self, _: &AssetKey) -> Option<Cow<'_, [u8]>> {
            Some(Cow::Borrowed(&self.0))
        }
        fn iter(&self) -> Box<AssetsIter<'_>> {
            Box::new(std::iter::empty())
        }
        fn csp_hashes(&self, _: &AssetKey) -> Box<dyn Iterator<Item = CspHash<'_>> + '_> {
            Box::new(std::iter::once(CspHash::Script("'embedded'")))
        }
    }

    #[test]
    fn provider_falls_back_to_embedded_and_keeps_the_last_verified_set_on_rejection() {
        let f = dist("provider");
        let mut wrong = expect();
        wrong.contract = "e".repeat(64);
        let state = Arc::new(UiState::new(f.0.clone(), wrong));
        let assets: UiAssets<tauri::Wry> =
            UiAssets::new(Box::new(Fake(b"EMBEDDED".to_vec())), state.clone());
        let key = AssetKey::from("island.html");
        assert!(matches!(state.refresh(), Refresh::Rejected(_)));
        assert_eq!(assets.get(&key).unwrap().as_ref(), b"EMBEDDED");
        assert_eq!(state.source(), "embedded");
        let hashes: Vec<_> = assets.csp_hashes(&key).collect();
        assert!(matches!(hashes[..], [CspHash::Script("'embedded'")]));

        let state = Arc::new(UiState::new(f.0.clone(), expect()));
        let assets: UiAssets<tauri::Wry> =
            UiAssets::new(Box::new(Fake(b"EMBEDDED".to_vec())), state.clone());
        assert!(matches!(state.refresh(), Refresh::Disk { files: 3, .. }));
        assert_eq!(assets.get(&key).unwrap().as_ref(), ISLAND.as_bytes());
        assert!(
            assets.get(&AssetKey::from("nope.js")).is_none(),
            "never mixes in embedded"
        );
        // Tampering after a verified load: the reload is refused, the verified bytes stay.
        fs::write(f.0.join("island.html"), "<p>tampered</p>").unwrap();
        assert!(matches!(state.refresh(), Refresh::Rejected(_)));
        assert_eq!(assets.get(&key).unwrap().as_ref(), ISLAND.as_bytes());
    }

    #[test]
    fn force_flag_and_env() {
        let mut args = vec!["--overlay".to_owned(), FORCE_EMBEDDED_FLAG.to_owned()];
        assert!(take_force_flag(&mut args));
        assert_eq!(args, ["--overlay"]);
        assert!(!take_force_flag(&mut args));
        assert_eq!(forced_reason(true, None), Some("--embedded-ui"));
        assert_eq!(
            forced_reason(false, Some("1")),
            Some("HERMES_ISLAND_EMBEDDED_UI")
        );
        assert_eq!(forced_reason(false, Some("0")), None);
        assert_eq!(forced_reason(false, Some("")), None);
        assert_eq!(contract_hash().len(), 64);
    }
}
