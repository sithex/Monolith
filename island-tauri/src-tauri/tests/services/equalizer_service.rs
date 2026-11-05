//! Phase 6m: the equalizer service. Two layers:
//!
//! * FAKE tools (symlinks to `helpers/fake_pw.py`): visibility / playback / enable gating, the
//!   emit rate, argv safety and stream targeting, with no audio system involved;
//! * a PRIVATE PipeWire instance (own `XDG_RUNTIME_DIR` in scratch, policy-only WirePlumber,
//!   null sink, synthetic sine streams): real `pw-record` capture and real stream isolation.
//!
//! Nothing here touches the user's PipeWire, microphone, volume or routing.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)]
use crate::pipewire_fixture;
use crate::private_bus;

use hermes_island_lib::integrations::media::{
    equalizer::{ChildEnv, EqConfig, EqualizerService, Tools},
    spectrum::{Analyzer, BandConfig},
    PlayerHint,
};
use pipewire_fixture::PipeWire;
use private_bus::wait_until;
use std::{
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

const T: Duration = Duration::from_secs(15);

fn hint(pid: Option<u32>, names: &[&str]) -> PlayerHint {
    PlayerHint {
        pid,
        names: names.iter().map(|n| (*n).to_owned()).collect(),
    }
}

/// Where a sine of `hz` lands in the default 16-band layout, per the audited analyzer itself.
fn reference_band(hz: f32) -> usize {
    let mut analyzer = Analyzer::new(BandConfig::default()).unwrap();
    let samples: Vec<f32> = (0..8192)
        .map(|i| 0.5 * (2.0 * std::f32::consts::PI * hz * i as f32 / 48_000.0).sin())
        .collect();
    analyzer.push(&samples, 1);
    let bands = analyzer.bands().unwrap().to_vec();
    bands
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(i, _)| i)
        .unwrap()
}

fn peak(bands: &[f64]) -> usize {
    bands
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(i, _)| i)
        .unwrap()
}

struct Fake {
    root: PathBuf,
    env: Vec<(String, String)>,
    config: EqConfig,
}

impl Fake {
    fn new(tag: &str, extra: &[(&str, &str)]) -> Self {
        let root = std::env::temp_dir().join(format!("hi-eqfake-{tag}-{}", std::process::id()));
        assert!(!root.exists(), "stale sandbox {}", root.display());
        fs::create_dir_all(root.join("bin")).unwrap();
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/helpers/fake_pw.py");
        for tool in ["pw-record", "pw-dump", "pactl"] {
            symlink(&script, root.join("bin").join(tool)).unwrap();
        }
        let mut env = vec![
            ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
            (
                "FAKE_PW_ARGV".to_owned(),
                root.join("argv.log").display().to_string(),
            ),
        ];
        env.extend(
            extra
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned())),
        );
        let config = EqConfig {
            tools: Tools {
                pw_record: root.join("bin/pw-record"),
                pw_dump: root.join("bin/pw-dump"),
                pactl: root.join("bin/pactl"),
            },
            env: ChildEnv::Only(env.clone()),
            sink_override: None,
        };
        Self { root, env, config }
    }

    fn starts(&self) -> Vec<String> {
        fs::read_to_string(self.root.join("argv.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

impl Drop for Fake {
    fn drop(&mut self) {
        let _ = &self.env;
        let _removed = fs::remove_dir_all(&self.root);
    }
}

fn alive(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

fn started(fake: &Fake) -> Arc<EqualizerService> {
    let service = EqualizerService::new(fake.config.clone(), false, 16);
    service.start();
    service
}

#[test]
fn capture_runs_only_while_enabled_visible_and_playing_and_dies_when_any_stops() {
    let fake = Fake::new("gating", &[]);
    let eq = started(&fake);
    let playing = Some(hint(None, &["spotify"]));
    // Each single condition missing → no capture, with a reason.
    eq.set_source(playing.clone());
    eq.set_visible(true);
    std::thread::sleep(Duration::from_millis(500));
    assert!(
        eq.capture_pid().is_none() && fake.starts().is_empty(),
        "disabled"
    );
    assert_eq!(eq.status().detail, "disabled");
    eq.configure(true, 16);
    eq.set_visible(false);
    std::thread::sleep(Duration::from_millis(500));
    assert!(
        eq.capture_pid().is_none() && fake.starts().is_empty(),
        "hidden"
    );
    assert_eq!(eq.status().detail, "hidden");
    eq.set_visible(true);
    eq.set_source(None);
    std::thread::sleep(Duration::from_millis(500));
    assert!(
        eq.capture_pid().is_none() && fake.starts().is_empty(),
        "nothing playing"
    );
    assert_eq!(eq.status().detail, "idle");
    assert!(!eq.current().active && eq.current().bands.is_empty());

    // All three: capture starts and a 1 kHz tone shows up in the expected band.
    eq.set_source(playing);
    let pid = wait_until("capture child", T, || eq.capture_pid());
    assert!(alive(pid));
    let tone = wait_until("bands published", T, || {
        let s = eq.current();
        (s.active && s.bands.len() == 16 && s.bands.iter().any(|b| *b > 0.5)).then_some(s)
    });
    let expected = reference_band(1000.0);
    assert_eq!(peak(&tone.bands), expected, "{:?}", tone.bands);
    assert!(tone.bands.iter().all(|b| (0.0..=1.0).contains(b)));
    assert_eq!(eq.status().detail, "capturing");

    // Hidden: the child is killed AND reaped, the frame is cleared, and nothing more is published.
    eq.set_visible(false);
    wait_until("child gone when hidden", T, || (!alive(pid)).then_some(()));
    wait_until("inactive frame", T, || {
        let s = eq.current();
        (!s.active && s.bands.is_empty()).then_some(())
    });
    let rev = eq.current().rev;
    std::thread::sleep(Duration::from_millis(600));
    assert_eq!(eq.current().rev, rev, "no capture, no frames while hidden");
    assert!(eq.capture_pid().is_none());

    // Visible again: a NEW capture; paused (no source): gone; resumed; disabled: gone.
    eq.set_visible(true);
    let second = wait_until("new capture", T, || eq.capture_pid());
    assert_ne!(second, pid);
    eq.set_source(None);
    wait_until("child gone when paused", T, || {
        (!alive(second)).then_some(())
    });
    assert!(eq.capture_pid().is_none());
    eq.set_source(Some(hint(None, &["spotify"])));
    let third = wait_until("resumed", T, || eq.capture_pid());
    eq.configure(false, 16);
    wait_until("child gone when disabled", T, || {
        (!alive(third)).then_some(())
    });
    std::thread::sleep(Duration::from_millis(500));
    assert!(eq.capture_pid().is_none());
    let starts = fake.starts().len();
    assert!(
        (1..=3).contains(&starts),
        "at most the three starts above: {starts}"
    );
    eq.shutdown();
}

#[test]
fn frames_are_emitted_at_most_thirty_times_per_second() {
    let fake = Fake::new("rate", &[("FAKE_PW_FLUTTER", "1")]);
    let eq = started(&fake);
    eq.configure(true, 16);
    eq.set_visible(true);
    eq.set_source(Some(hint(None, &[])));
    wait_until("capturing", T, || eq.current().active.then_some(()));
    let (rev0, t0) = (eq.current().rev, Instant::now());
    std::thread::sleep(Duration::from_secs(3));
    let frames = f64::from(eq.current().rev.wrapping_sub(rev0));
    let per_second = frames / t0.elapsed().as_secs_f64();
    // The tone flutters so every analysed frame differs; the topic only publishes a CHANGED
    // frame, so this is the true emit rate: pinned just under the 30 Hz cap and clearly live.
    assert!(per_second <= 30.0, "{per_second:.1} frames/s exceeds 30 Hz");
    assert!(per_second >= 15.0, "{per_second:.1} frames/s: not live");
    eq.shutdown();
}

#[test]
fn argv_names_the_default_sink_monitor_or_the_playing_stream_and_never_a_source() {
    // Unidentifiable player → the default sink's monitor.
    let fake = Fake::new("argv-sink", &[("FAKE_PW_SINK", "alsa_output.test")]);
    let eq = started(&fake);
    eq.configure(true, 16);
    eq.set_visible(true);
    eq.set_source(Some(hint(Some(1), &["nobody"])));
    wait_until("capture", T, || eq.capture_pid());
    let argv = eq.capture_argv().unwrap();
    assert_eq!(
        &argv[..4],
        [
            "--target",
            "alsa_output.test",
            "-P",
            "stream.capture.sink=true"
        ]
    );
    assert!(argv.windows(2).any(|w| w == ["--format", "f32"]));
    assert!(!argv.iter().any(|a| a.to_lowercase().contains("source")));
    eq.shutdown();

    // pw-dump finds the player's own output stream → exactly that stream, no sink flag.
    let dump = r#"[{"id":99,"info":{"props":{"media.class":"Stream/Output/Audio","object.serial":4242,"application.process.id":777,"application.name":"player"}}},
                   {"id":98,"info":{"props":{"media.class":"Audio/Source","object.serial":1,"application.process.id":777}}}]"#;
    let fake = Fake::new("argv-stream", &[("FAKE_PW_DUMP", dump)]);
    let eq = started(&fake);
    eq.configure(true, 16);
    eq.set_visible(true);
    eq.set_source(Some(hint(Some(777), &["unrelated"])));
    wait_until("capture", T, || eq.capture_pid());
    let argv = eq.capture_argv().unwrap();
    assert_eq!(&argv[..2], ["--target", "4242"]);
    assert!(!argv.iter().any(|a| a.contains("capture.sink")), "{argv:?}");
    eq.shutdown();
}

#[test]
fn no_resolvable_target_means_no_capture_and_a_reason_never_a_default_source() {
    let fake = Fake::new("notarget", &[("FAKE_PW_SINK", "")]);
    let eq = started(&fake);
    eq.configure(true, 16);
    eq.set_visible(true);
    eq.set_source(Some(hint(None, &["nobody"])));
    wait_until("explicit reason", T, || {
        (eq.status().detail == "no-default-sink").then_some(())
    });
    assert!(eq.capture_pid().is_none());
    assert!(
        fake.starts().is_empty(),
        "pw-record was never started without a target"
    );
    eq.shutdown();
}

#[test]
fn a_capture_that_exits_on_its_own_is_reported_and_retried_without_stale_bands() {
    let fake = Fake::new("exits", &[("FAKE_PW_EXIT_AFTER_MS", "300")]);
    let eq = started(&fake);
    eq.configure(true, 16);
    eq.set_visible(true);
    eq.set_source(Some(hint(None, &[])));
    wait_until("first capture ended", T, || {
        (eq.status().detail == "capture-ended").then_some(())
    });
    assert!(!eq.current().active && eq.current().bands.is_empty());
    wait_until(
        "retry started a second capture",
        Duration::from_secs(10),
        || (fake.starts().len() >= 2).then_some(()),
    );
    eq.shutdown();
}

#[test]
fn private_pipewire_captures_only_the_playing_stream_and_the_sink_monitor_hears_both() {
    if !PipeWire::available() {
        panic!("UNVERIFIED-LIVE: PipeWire tools are missing; the live capture test cannot run");
    }
    let mut pw = PipeWire::start("eq");
    let one_khz = pw.play(1000.0, "tone1k");
    let three_khz = pw.play(3000.0, "tone3k");
    let low = reference_band(1000.0);
    let high = reference_band(3000.0);
    assert_ne!(low, high);

    // Targeted at the 1 kHz player's own stream (by application name): the 3 kHz stream must not be heard.
    let eq = EqualizerService::new(
        EqConfig {
            tools: Tools::default(),
            env: ChildEnv::Only(pw.child_env()),
            sink_override: Some("test-sink".into()),
        },
        true,
        16,
    );
    eq.start();
    eq.set_visible(true);
    eq.set_source(Some(hint(None, &["tone1k"])));
    let argv = wait_until("stream capture", T, || eq.capture_argv());
    assert_eq!(argv[0], "--target");
    assert!(
        !argv.iter().any(|a| a.contains("capture.sink")),
        "stream target: {argv:?}"
    );
    let frame = wait_until("real bands", T, || {
        let s = eq.current();
        (s.active && s.bands.iter().any(|b| *b > 0.5)).then_some(s)
    });
    assert_eq!(peak(&frame.bands), low, "{:?}", frame.bands);
    // Give the smoothing time to settle, then compare the two tones' bands.
    std::thread::sleep(Duration::from_millis(800));
    let settled = eq.current();
    assert!(
        settled.bands[high] < 0.3 && settled.bands[low] > 0.5,
        "the other stream leaked into a stream-targeted capture: {:?}",
        settled.bands
    );

    // Unidentifiable player → the SINK monitor, which carries both streams.
    eq.set_source(Some(hint(Some(1), &["nobody"])));
    wait_until("sink capture", T, || {
        eq.capture_argv()
            .filter(|a| a.iter().any(|x| x.contains("capture.sink")))
    });
    wait_until("both tones audible on the sink monitor", T, || {
        let s = eq.current();
        (s.active && s.bands[low] > 0.4 && s.bands[high] > 0.4).then_some(())
    });
    let _ = three_khz;

    // Hidden: the real pw-record is gone, the stream keeps playing untouched.
    let pid = eq.capture_pid().unwrap();
    eq.set_visible(false);
    wait_until("pw-record gone", T, || (!alive(pid)).then_some(()));
    assert!(
        alive(one_khz) && alive(three_khz),
        "playback was not disturbed"
    );
    eq.shutdown();
}
