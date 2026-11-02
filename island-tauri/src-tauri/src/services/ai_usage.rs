//! AI usage + account connection adapters (port of island-rust `ai_usage` / `ai_connect`).

use super::{hermes_launcher, process::run_bounded};
use crate::{
    contract::ai::{AiConnectResult, AiProvider, AiUsage, ProviderInfo},
    error::IslandError,
};
use serde_json::{json, Value};
use std::{
    os::unix::process::CommandExt,
    path::PathBuf,
    process::{Command, Stdio},
    sync::Mutex,
    time::{Duration, Instant},
};

const CACHE_TTL: Duration = Duration::from_secs(60);

pub struct AiUsageService {
    root: PathBuf,
    home: PathBuf,
    python: PathBuf,
    connect_program: PathBuf,
    /// Used when the launcher file does not exist (`hermes` from PATH).
    fallback_hermes: PathBuf,
    /// Serializes refreshes: concurrent callers wait, then find the fresh cache (coalescing).
    refresh: Mutex<()>,
    cache: Mutex<Option<(Instant, AiUsage)>>,
}

fn json_object(bytes: &[u8]) -> Option<Value> {
    serde_json::from_slice::<Value>(bytes)
        .ok()
        .filter(Value::is_object)
}

/// `YYYY-MM-DDTHH:MM:SS.mmmZ` (UTC), 24 chars.
pub fn iso_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let seconds = time.as_secs() as libc::time_t;
    // SAFETY: pointers refer to valid time_t/tm storage; gmtime_r fills tm on success.
    let tm = unsafe {
        let mut tm = std::mem::zeroed::<libc::tm>();
        if libc::gmtime_r(&seconds, &mut tm).is_null() {
            return String::new();
        }
        tm
    };
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec,
        time.subsec_millis()
    )
}

impl AiUsageService {
    pub fn new(root: PathBuf, home: PathBuf) -> Self {
        Self {
            root,
            home,
            python: PathBuf::from("/usr/bin/python3"),
            connect_program: PathBuf::from("konsole"),
            fallback_hermes: PathBuf::from("hermes"),
            refresh: Mutex::new(()),
            cache: Mutex::new(None),
        }
    }

    /// Trusted host/test configuration: the program `ai_connect` launches (default `konsole`).
    pub fn with_connect_program(mut self, program: PathBuf) -> Self {
        self.connect_program = program;
        self
    }

    /// Trusted host/test configuration: the executable used when the launcher is missing.
    pub fn with_fallback_hermes(mut self, program: PathBuf) -> Self {
        self.fallback_hermes = program;
        self
    }

    fn cached(&self) -> Option<AiUsage> {
        let cache = self.cache.lock().ok()?;
        cache
            .as_ref()
            .filter(|(when, _)| when.elapsed() < CACHE_TTL)
            .map(|(_, usage)| usage.clone())
    }

    pub fn usage(&self) -> Result<AiUsage, IslandError> {
        if let Some(hit) = self.cached() {
            return Ok(hit);
        }
        let _turn = self
            .refresh
            .lock()
            .map_err(|_| IslandError::internal("usage refresh is poisoned"))?;
        if let Some(hit) = self.cached() {
            return Ok(hit);
        }
        let launcher = hermes_launcher(&self.home);
        let executable = if launcher.is_file() {
            launcher
        } else {
            self.fallback_hermes.clone()
        };
        // Query both accounts, not just whichever provider is selected in Hermes.
        // Only the public usage command is read; credentials never enter Ripple.
        let account = std::thread::spawn(move || {
            let workers: Vec<_> = ["anthropic", "openai-codex"]
                .into_iter()
                .map(|provider| {
                    let executable = executable.clone();
                    std::thread::spawn(move || {
                        let mut command = Command::new(executable);
                        command.args(["usage", "--provider", provider, "--json"]);
                        let mut value =
                            run_bounded(command, Duration::from_secs(30), 256 * 1024, true)
                                .ok()
                                .and_then(|bytes| json_object(&bytes))
                                .unwrap_or_else(|| {
                                    json!({"windows":[], "plan":null,
                                "unavailable_reason":"Not connected"})
                                });
                        value["provider"] = json!(provider);
                        value
                    })
                })
                .collect();
            let accounts: Vec<Value> = workers.into_iter().filter_map(|w| w.join().ok()).collect();
            let mut primary = accounts
                .iter()
                .find(|a| a["provider"] == "openai-codex")
                .cloned()
                .unwrap_or_else(|| json!({"windows":[]}));
            primary["accounts"] = json!(accounts);
            primary
        });
        let mut command = Command::new(&self.python);
        command
            .args(["-I", "-B"])
            .arg(self.root.join("island/ai-local-usage.py"));
        let local = run_bounded(command, Duration::from_secs(30), 1024 * 1024, false)
            .ok()
            .and_then(|bytes| json_object(&bytes))
            .unwrap_or_else(|| {
                json!({"available":false,"periodDays":30,"periodBasis":"session_started_at","totals":null,"models":[],"unavailableReason":"Local usage could not be read."})
            });
        let account = account.join().unwrap_or_else(|_| {
            json!({"provider":null,"source":null,"title":"Account limits","plan":null,"windows":[],"details":[],"unavailable_reason":"Hermes account usage is unavailable. Check sign-in and refresh."})
        });
        let usage = AiUsage {
            account,
            local,
            providers: AiProvider::ALL
                .iter()
                .map(|p| ProviderInfo {
                    id: p.id().to_owned(),
                    label: p.label().to_owned(),
                })
                .collect(),
            fetched_at: iso_now(),
        };
        if let Ok(mut cache) = self.cache.lock() {
            *cache = Some((Instant::now(), usage.clone()));
        }
        Ok(usage)
    }

    /// Open an interactive sign-in (detached session). The provider is already an allow-listed
    /// enum, so no renderer-controlled string reaches argv.
    pub fn connect(&self, provider: AiProvider) -> AiConnectResult {
        let mut command = Command::new(&self.connect_program);
        command
            .args(provider.auth_args())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // SAFETY: setsid is async-signal-safe; the interactive terminal must outlive this app.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
        match command.spawn() {
            Ok(mut child) => {
                std::thread::spawn(move || {
                    if let Err(error) = child.wait() {
                        tracing::debug!(%error, "sign-in terminal wait failed");
                    }
                });
                if let Ok(mut cache) = self.cache.lock() {
                    *cache = None;
                }
                AiConnectResult {
                    launched: true,
                    provider: Some(provider),
                    message: Some("Complete sign-in in Konsole, then Refresh. Launching does not confirm sign-in.".into()),
                    error: None,
                }
            }
            Err(error) => {
                tracing::warn!(%error, "could not launch the sign-in terminal");
                AiConnectResult {
                    launched: false,
                    provider: Some(provider),
                    message: None,
                    error: Some(
                        "Could not launch Konsole. Check that Konsole and Hermes are installed."
                            .into(),
                    ),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::scratch;
    use std::{fs, os::unix::fs::PermissionsExt};

    fn fixture() -> (PathBuf, PathBuf) {
        let dir = scratch("usage");
        let root = dir.join("app with spaces");
        fs::create_dir_all(root.join("island")).unwrap();
        let launcher = dir.join(".hermes/hermes-agent/.hermes/bin/hermes");
        fs::create_dir_all(launcher.parent().unwrap()).unwrap();
        let account = dir.join("account.py");
        fs::write(
            &account,
            "print('{\"provider\":\"openai-codex\",\"windows\":[],\"details\":[]}'); raise SystemExit(1)",
        )
        .unwrap();
        fs::write(
            &launcher,
            format!(
                "#!/bin/sh\nexec /usr/bin/python3 {} \"$@\"\n",
                account.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&launcher, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(
            root.join("island/ai-local-usage.py"),
            "print('{\"available\":false,\"periodDays\":30,\"periodBasis\":\"session_started_at\",\"totals\":null,\"models\":[],\"unavailableReason\":\"fixture\"}')",
        )
        .unwrap();
        (dir, root)
    }

    #[test]
    fn usage_payload_shape_and_sixty_second_cache() {
        let (dir, root) = fixture();
        let svc = AiUsageService::new(root.clone(), dir.clone());
        let usage = svc.usage().unwrap();
        assert_eq!(usage.account["provider"], "openai-codex");
        assert_eq!(usage.account["accounts"][0]["provider"], "anthropic");
        assert_eq!(usage.account["accounts"][1]["provider"], "openai-codex");
        assert_eq!(usage.local["periodBasis"], "session_started_at");
        assert_eq!(usage.providers.len(), 4);
        assert_eq!(usage.providers[0].id, "openai-codex");
        assert_eq!(usage.providers[0].label, "OpenAI Codex");
        assert!(usage.fetched_at.ends_with('Z'));
        assert_eq!(usage.fetched_at.len(), 24);
        // Cache: removing the local script must not change the answer within 60 s.
        fs::remove_file(root.join("island/ai-local-usage.py")).unwrap();
        assert_eq!(svc.usage().unwrap(), usage);
        // The wire shape is camelCase with exactly these four keys.
        let wire = serde_json::to_value(&usage).unwrap();
        let mut keys: Vec<_> = wire.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(keys, ["account", "fetchedAt", "local", "providers"]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unavailable_sources_degrade_to_documented_placeholders() {
        let dir = scratch("usage-missing");
        // The PATH fallback is pointed at a missing file: the real `hermes` must never run here.
        let svc = AiUsageService::new(dir.join("none"), dir.clone())
            .with_fallback_hermes(dir.join("no-such-hermes"));
        let usage = svc.usage().unwrap();
        assert_eq!(usage.local["available"], false);
        assert_eq!(
            usage.local["unavailableReason"],
            "Local usage could not be read."
        );
        assert_eq!(
            usage.account["accounts"][0]["unavailable_reason"],
            "Not connected"
        );
        assert_eq!(
            usage.account["accounts"][1]["unavailable_reason"],
            "Not connected"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn connect_uses_the_allow_listed_argv_and_never_the_real_konsole() {
        let dir = scratch("connect");
        let program = dir.join("fake-konsole");
        let log = dir.join("argv");
        fs::write(
            &program,
            format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > {}\n", log.display()),
        )
        .unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o700)).unwrap();
        let svc = AiUsageService::new(dir.clone(), dir.clone()).with_connect_program(program);
        let result = svc.connect(AiProvider::OpenaiCodex);
        assert!(result.launched);
        assert_eq!(result.provider, Some(AiProvider::OpenaiCodex));
        let deadline = Instant::now() + Duration::from_secs(3);
        while !log.exists() {
            assert!(Instant::now() < deadline, "fake terminal never ran");
            std::thread::sleep(Duration::from_millis(10));
        }
        std::thread::sleep(Duration::from_millis(50));
        let argv: Vec<String> = fs::read_to_string(&log)
            .unwrap()
            .lines()
            .map(String::from)
            .collect();
        assert_eq!(argv, AiProvider::OpenaiCodex.auth_args());
        // A missing terminal is a reported failure, not a panic.
        let missing = AiUsageService::new(dir.clone(), dir.clone())
            .with_connect_program(dir.join("does-not-exist"))
            .connect(AiProvider::Nous);
        assert!(!missing.launched);
        assert!(missing.error.is_some());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn provider_allow_list_and_argv() {
        for bad in [";sh", "untrusted-provider", "OpenAI-Codex", ""] {
            assert!(
                serde_json::from_value::<AiProvider>(json!(bad)).is_err(),
                "{bad}"
            );
        }
        assert_eq!(
            AiProvider::OpenaiCodex.auth_args(),
            [
                "--separate",
                "-e",
                "hermes",
                "auth",
                "add",
                "openai-codex",
                "--type",
                "oauth",
                "--browser"
            ]
        );
        assert!(!AiProvider::Anthropic
            .auth_args()
            .contains(&"--browser".into()));
        for p in AiProvider::ALL {
            let round: AiProvider = serde_json::from_value(json!(p.id())).unwrap();
            assert_eq!(round, p);
            assert_eq!(serde_json::to_value(p).unwrap(), json!(p.id()));
        }
    }
}
