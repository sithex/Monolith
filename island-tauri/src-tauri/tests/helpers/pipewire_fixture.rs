//! A PRIVATE PipeWire instance for the equalizer tests (Phase 6m): its own `XDG_RUNTIME_DIR`
//! in scratch, `wireplumber -p policy` (policy only: NO ALSA / Bluetooth / camera monitors, so
//! no real device is ever opened), a null sink and synthetic sine streams played into it.
//! Every child runs with a cleared environment that points only at the scratch directory; the
//! user's PipeWire, PulseAudio sockets and devices are never reachable from here.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]
use std::{
    fs,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread::sleep,
    time::{Duration, Instant},
};

fn killpg(child: &mut Child) {
    // SAFETY: killpg on a process group this fixture created (process_group(0)).
    unsafe {
        libc::killpg(child.id() as i32, libc::SIGKILL);
    }
    let _ = child.wait();
}

pub struct PipeWire {
    pub root: PathBuf,
    daemons: Vec<Child>,
    players: Vec<Child>,
}

fn wav(path: &Path, hz: f64, seconds: usize) {
    let rate = 48_000usize;
    let frames = rate * seconds;
    let mut data = Vec::with_capacity(frames * 4);
    for i in 0..frames {
        let v =
            (12_000.0 * (2.0 * std::f64::consts::PI * hz * i as f64 / rate as f64).sin()) as i16;
        data.extend_from_slice(&v.to_le_bytes());
        data.extend_from_slice(&v.to_le_bytes());
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&2u16.to_le_bytes()); // stereo
    out.extend_from_slice(&(rate as u32).to_le_bytes());
    out.extend_from_slice(&(rate as u32 * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    fs::write(path, out).unwrap();
}

impl PipeWire {
    /// `None` when this machine lacks the PipeWire tools (the caller reports it, never fakes).
    pub fn available() -> bool {
        [
            "pipewire",
            "wireplumber",
            "pw-play",
            "pw-record",
            "pw-dump",
            "pw-cli",
        ]
        .iter()
        .all(|tool| {
            ["/usr/bin", "/bin"]
                .iter()
                .any(|dir| Path::new(dir).join(tool).exists())
        })
    }

    pub fn env(root: &Path) -> Vec<(String, String)> {
        let home = root.join("home");
        vec![
            ("PATH".into(), "/usr/bin:/bin".into()),
            ("LANG".into(), "C.UTF-8".into()),
            ("HOME".into(), home.display().to_string()),
            (
                "XDG_RUNTIME_DIR".into(),
                root.join("run").display().to_string(),
            ),
            (
                "XDG_CONFIG_HOME".into(),
                home.join(".config").display().to_string(),
            ),
            (
                "XDG_STATE_HOME".into(),
                home.join(".state").display().to_string(),
            ),
            (
                "XDG_CACHE_HOME".into(),
                home.join(".cache").display().to_string(),
            ),
            (
                "DBUS_SESSION_BUS_ADDRESS".into(),
                "unix:path=/nonexistent".into(),
            ),
            (
                "DBUS_SYSTEM_BUS_ADDRESS".into(),
                "unix:path=/nonexistent".into(),
            ),
        ]
    }

    fn command(root: &Path, program: &str) -> Command {
        let mut command = Command::new(Path::new("/usr/bin").join(program));
        command.env_clear().envs(Self::env(root));
        command
    }

    pub fn start(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("hi-pw-{tag}-{}", std::process::id()));
        assert!(!root.exists(), "stale test sandbox: {}", root.display());
        for dir in ["home", "run"] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root.join("run"), fs::Permissions::from_mode(0o700)).unwrap();
        let log = |name: &str| fs::File::create(root.join(name)).unwrap();
        let mut me = Self {
            root: root.clone(),
            daemons: Vec::new(),
            players: Vec::new(),
        };
        let pipewire = Self::command(&root, "pipewire")
            .process_group(0)
            .stdout(log("pipewire.log"))
            .stderr(log("pipewire.err"))
            .spawn()
            .unwrap();
        me.daemons.push(pipewire);
        wait("private PipeWire socket", Duration::from_secs(10), || {
            root.join("run/pipewire-0").exists()
        });
        let wireplumber = Self::command(&root, "wireplumber")
            .args(["-p", "policy"])
            .process_group(0)
            .stdout(log("wireplumber.log"))
            .stderr(log("wireplumber.err"))
            .spawn()
            .unwrap();
        me.daemons.push(wireplumber);
        sleep(Duration::from_millis(1500));
        let created = Self::command(&root, "pw-cli")
            .args([
                "create-node",
                "adapter",
                "{ factory.name=support.null-audio-sink node.name=test-sink media.class=Audio/Sink object.linger=true audio.position=[FL FR] }",
            ])
            .output()
            .unwrap();
        assert!(
            created.status.success(),
            "{}",
            String::from_utf8_lossy(&created.stderr)
        );
        wait("null sink listed", Duration::from_secs(10), || {
            me.dump().contains("\"test-sink\"")
        });
        me
    }

    pub fn dump(&self) -> String {
        let out = Self::command(&self.root, "pw-dump").output().unwrap();
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// Play a sine of `hz` into the null sink (`pw-play`); returns its pid (the stream's
    /// client `application.process.id`); the stream node is named `name` (pw-cat sets no pid on it).
    pub fn play(&mut self, hz: f64, name: &str) -> u32 {
        let file = self.root.join(format!("sine-{hz}.wav"));
        if !file.exists() {
            wav(&file, hz, 40);
        }
        let child = Self::command(&self.root, "pw-play")
            .args(["--target", "test-sink", "-P"])
            .arg(format!("{{ application.name={name} node.name={name} }}"))
            .arg(&file)
            .process_group(0)
            .stdout(Stdio::null())
            .stderr(fs::File::create(self.root.join(format!("play-{hz}.err"))).unwrap())
            .spawn()
            .unwrap();
        let pid = child.id();
        self.players.push(child);
        wait("stream listed", Duration::from_secs(10), || {
            self.dump()
                .contains(&format!("\"application.name\": \"{name}\""))
        });
        pid
    }

    pub fn child_env(&self) -> Vec<(String, String)> {
        Self::env(&self.root)
    }
}

fn wait(what: &str, timeout: Duration, mut probe: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    while !probe() {
        assert!(Instant::now() < deadline, "timed out waiting for: {what}");
        sleep(Duration::from_millis(100));
    }
}

impl Drop for PipeWire {
    fn drop(&mut self) {
        for child in self.players.iter_mut().chain(self.daemons.iter_mut().rev()) {
            killpg(child);
        }
        for _ in 0..30 {
            if fs::remove_dir_all(&self.root).is_ok() || !self.root.exists() {
                break;
            }
            sleep(Duration::from_millis(50));
        }
    }
}
