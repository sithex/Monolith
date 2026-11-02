use mixer_core::{
    move_stream, mute_sink, mute_stream, needs_refresh, parse_default_sink, parse_event,
    parse_sink_inputs, parse_sink_inputs_report, parse_sinks, parse_sinks_report, set_app_volume,
    set_default_sink, set_sink_volume, set_stream_volume, AppGroup, CmdError, MixerModel, MuteOp,
    PaEvent, PaFacility, PaKind, ParseError, RefreshScope, Sink, Stream,
};

fn sink(index: u32, name: &str, muted: bool, pct: Option<u32>, channels: u8) -> Sink {
    Sink {
        index,
        name: name.to_string(),
        description: String::new(),
        muted,
        volume_percent: pct,
        channels,
        state: String::new(),
    }
}

fn stream(
    index: u32,
    sink_index: u32,
    app: &str,
    pid: Option<u32>,
    muted: bool,
    corked: bool,
    pct: Option<u32>,
) -> Stream {
    Stream {
        index,
        sink_index,
        app_name: app.to_string(),
        binary: None,
        pid,
        media_name: None,
        icon_name: None,
        muted,
        corked,
        volume_percent: pct,
    }
}

const SINKS_JSON: &str = r#"[
  {
    "index": 0,
    "name": "alsa_output.pci-0000_00_1f.3.analog-stereo",
    "description": "Built-in Audio Analog Stereo",
    "mute": false,
    "volume": {
      "front-left": { "value": 32768, "value_percent": "50%", "db": "-18.06 dB" },
      "front-right": { "value": 32768, "value_percent": "50%", "db": "-18.06 dB" }
    },
    "base_volume": { "value": 65536, "value_percent": "100%", "db": "0.00 dB" },
    "state": "RUNNING"
  },
  {
    "index": 1,
    "name": "bluez_output.00_00_00_00_00_00.1",
    "description": "Bluetooth Speaker",
    "mute": true,
    "volume": { "mono": { "value": 65536, "value_percent": "100%" } },
    "base_volume": { "value": 65536 },
    "state": "IDLE"
  }
]"#;

const STREAMS_JSON: &str = r#"[
  {
    "index": 2127,
    "sink": 0,
    "client": "2121",
    "corked": false,
    "mute": false,
    "volume": { "front-left": { "value": 65536 }, "front-right": { "value": 65536 } },
    "balance": 0.0,
    "properties": {
      "application.name": "Hermes Island",
      "application.process.binary": "WebKitWebProcess",
      "application.process.id": "13645",
      "media.name": "Playback Stream",
      "media.role": "webaudio",
      "application.icon_name": "hermes-island"
    }
  },
  {
    "index": 2128,
    "sink": 0,
    "client": "2122",
    "corked": true,
    "mute": true,
    "volume": { "front-left": { "value": 32768 }, "front-right": { "value": 32768 } },
    "properties": { "application.process.binary": "mpv", "application.process.id": "777" }
  },
  {
    "index": 2129,
    "sink": 1,
    "client": "2130",
    "corked": false,
    "mute": false,
    "volume": { "front-left": { "value": 98304 }, "front-right": { "value": 98304 } },
    "properties": { "media.name": "Notification" }
  }
]"#;

// ---------------------------------------------------------------------------
// parse_sinks
// ---------------------------------------------------------------------------

#[test]
fn parse_sinks_parses_fixture() {
    let sinks = parse_sinks(SINKS_JSON).unwrap();
    assert_eq!(sinks.len(), 2);

    assert_eq!(sinks[0].index, 0);
    assert_eq!(sinks[0].name, "alsa_output.pci-0000_00_1f.3.analog-stereo");
    assert_eq!(sinks[0].description, "Built-in Audio Analog Stereo");
    assert!(!sinks[0].muted);
    assert_eq!(sinks[0].volume_percent, Some(50));
    assert_eq!(sinks[0].channels, 2);
    assert_eq!(sinks[0].state, "RUNNING");

    assert_eq!(sinks[1].index, 1);
    assert_eq!(sinks[1].name, "bluez_output.00_00_00_00_00_00.1");
    assert!(sinks[1].muted);
    assert_eq!(sinks[1].volume_percent, Some(100));
    assert_eq!(sinks[1].channels, 1);
    assert_eq!(sinks[1].state, "IDLE");
}

#[test]
fn parse_sinks_non_array_top_level_rejected() {
    for bad in ["{}", "\"x\"", "42", "null", "true"] {
        assert_eq!(
            parse_sinks(bad),
            Err(ParseError::NotAnArray),
            "input: {bad}"
        );
    }
}

#[test]
fn parse_sinks_invalid_json_rejected() {
    for bad in ["", "{", "[1, 2", "[", "{\"index\":", "nonsense"] {
        assert_eq!(
            parse_sinks(bad),
            Err(ParseError::InvalidJson),
            "input: {bad}"
        );
    }
}

#[test]
fn parse_sinks_size_cap() {
    let just_over = format!("[{}]", " ".repeat(4 * 1024 * 1024));
    assert_eq!(parse_sinks(&just_over), Err(ParseError::TooLarge));

    let exactly = format!("[{}]", " ".repeat(4 * 1024 * 1024 - 2));
    assert_eq!(parse_sinks(&exactly), Ok(Vec::new()));
}

#[test]
fn parse_sinks_skips_malformed_and_counts() {
    let json = r#"[
      {"index": 0, "name": "ok", "volume": {"mono": {"value": 65536}}},
      {"index": 1, "volume": {"mono": {"value": 65536}}},
      {"index": 2, "name": "no-volume"},
      {"index": 3, "name": 7, "volume": {"mono": {"value": 65536}}},
      {"index": "4", "name": "bad-idx", "volume": {"mono": {"value": 65536}}},
      {"name": "no-idx", "volume": {"mono": {"value": 65536}}},
      {"index": 5, "name": "vol-not-obj", "volume": "50%"},
      {"index": 6, "name": "vol-array", "volume": [1, 2]},
      "not an object",
      7,
      null,
      {"index": 7, "name": "ok2", "volume": {"front-left": {"value": 32768}}}
    ]"#;
    let (sinks, skipped) = parse_sinks_report(json).unwrap();
    assert_eq!(sinks.len(), 2);
    assert_eq!(sinks[0].name, "ok");
    assert_eq!(sinks[1].name, "ok2");
    assert_eq!(sinks[1].volume_percent, Some(50));
    assert_eq!(skipped, 10);
}

#[test]
fn parse_sinks_missing_or_bad_volume_means_no_percent() {
    // volume present as an object but with no numeric channel values: kept, percent None
    let json = r#"[
      {"index": 0, "name": "a", "volume": {}},
      {"index": 1, "name": "b", "volume": {"front-left": {"db": "-18 dB"}}},
      {"index": 2, "name": "c", "volume": {"front-left": {"value": 32768.0}}}
    ]"#;
    let sinks = parse_sinks(json).unwrap();
    assert_eq!(sinks.len(), 3);
    for s in &sinks {
        assert_eq!(s.volume_percent, None);
        assert_eq!(s.channels, 0);
    }
}

#[test]
fn parse_sinks_channel_average_mixed() {
    let json = r#"[
      {"index": 0, "name": "a", "volume": {
        "front-left": {"value": 32768},
        "front-right": {"value": 65536}
      }}
    ]"#;
    let sinks = parse_sinks(json).unwrap();
    assert_eq!(sinks[0].volume_percent, Some(75));
    assert_eq!(sinks[0].channels, 2);
}

// ---------------------------------------------------------------------------
// parse_sink_inputs
// ---------------------------------------------------------------------------

#[test]
fn parse_sink_inputs_parses_fixture() {
    let streams = parse_sink_inputs(STREAMS_JSON).unwrap();
    assert_eq!(streams.len(), 3);

    // 2127: full properties, application.name wins
    assert_eq!(streams[0].index, 2127);
    assert_eq!(streams[0].sink_index, 0);
    assert_eq!(streams[0].app_name, "Hermes Island");
    assert_eq!(streams[0].binary.as_deref(), Some("WebKitWebProcess"));
    assert_eq!(streams[0].pid, Some(13645));
    assert_eq!(streams[0].media_name.as_deref(), Some("Playback Stream"));
    assert_eq!(streams[0].icon_name.as_deref(), Some("hermes-island"));
    assert!(!streams[0].muted);
    assert!(!streams[0].corked);
    assert_eq!(streams[0].volume_percent, Some(100));

    // 2128: fallback to application.process.binary, muted + corked
    assert_eq!(streams[1].index, 2128);
    assert_eq!(streams[1].app_name, "mpv");
    assert_eq!(streams[1].binary.as_deref(), Some("mpv"));
    assert_eq!(streams[1].pid, Some(777));
    assert_eq!(streams[1].media_name, None);
    assert_eq!(streams[1].icon_name, None);
    assert!(streams[1].muted);
    assert!(streams[1].corked);
    assert_eq!(streams[1].volume_percent, Some(50));

    // 2129: fallback to media.name, no pid
    assert_eq!(streams[2].index, 2129);
    assert_eq!(streams[2].sink_index, 1);
    assert_eq!(streams[2].app_name, "Notification");
    assert_eq!(streams[2].binary, None);
    assert_eq!(streams[2].pid, None);
    assert!(!streams[2].muted);
    assert!(!streams[2].corked);
    assert_eq!(streams[2].volume_percent, Some(150));
}

#[test]
fn parse_sink_inputs_app_name_fallback_chain() {
    let json = r#"[
      {"index": 0, "sink": 0, "volume": {}, "properties": {"media.name": "M"}},
      {"index": 1, "sink": 0, "volume": {}, "properties": {"application.process.binary": "B"}},
      {"index": 2, "sink": 0, "volume": {}, "properties": {}},
      {"index": 3, "sink": 0, "volume": {}, "properties": {"application.name": "A", "application.process.binary": "B", "media.name": "M"}}
    ]"#;
    let streams = parse_sink_inputs(json).unwrap();
    assert_eq!(streams[0].app_name, "M");
    assert_eq!(streams[1].app_name, "B");
    assert_eq!(streams[2].app_name, "Unknown");
    assert_eq!(streams[3].app_name, "A");
}

#[test]
fn parse_sink_inputs_pid_tolerance() {
    let json = r#"[
      {"index": 0, "sink": 0, "volume": {}, "properties": {"application.process.id": "13645"}},
      {"index": 1, "sink": 0, "volume": {}, "properties": {"application.process.id": "abc"}},
      {"index": 2, "sink": 0, "volume": {}, "properties": {"application.process.id": 123}}
    ]"#;
    let streams = parse_sink_inputs(json).unwrap();
    assert_eq!(streams[0].pid, Some(13645));
    assert_eq!(streams[1].pid, None);
    assert_eq!(streams[2].pid, None);
}

#[test]
fn parse_sink_inputs_skips_missing_sink_or_volume() {
    let json = r#"[
      {"index": 0, "volume": {"mono": {"value": 65536}}},
      {"index": 1, "sink": "bad", "volume": {"mono": {"value": 65536}}},
      {"index": 2, "sink": 0},
      {"index": 3, "sink": 0, "volume": "50%"}
    ]"#;
    let (streams, skipped) = parse_sink_inputs_report(json).unwrap();
    assert!(streams.is_empty());
    assert_eq!(skipped, 4);
}

// ---------------------------------------------------------------------------
// parse_default_sink
// ---------------------------------------------------------------------------

#[test]
fn parse_default_sink_trimmed_first_line() {
    assert_eq!(
        parse_default_sink("alsa_output.pci-0000_00_1f.3.analog-stereo\n"),
        Some("alsa_output.pci-0000_00_1f.3.analog-stereo".to_string())
    );
    assert_eq!(parse_default_sink("  name  \n"), Some("name".to_string()));
    assert_eq!(parse_default_sink("a\nb"), Some("a".to_string()));
    assert_eq!(parse_default_sink(""), None);
    assert_eq!(parse_default_sink("   \n "), None);
}

// ---------------------------------------------------------------------------
// parse_event / needs_refresh
// ---------------------------------------------------------------------------

#[test]
fn parse_event_all_facilities_and_kinds() {
    let cases = [
        (
            "Event 'new' on sink #0",
            PaKind::New,
            PaFacility::Sink,
            0u32,
        ),
        (
            "Event 'change' on source #12",
            PaKind::Change,
            PaFacility::Source,
            12,
        ),
        (
            "Event 'remove' on sink-input #2127",
            PaKind::Remove,
            PaFacility::SinkInput,
            2127,
        ),
        (
            "Event 'new' on source-output #9",
            PaKind::New,
            PaFacility::SourceOutput,
            9,
        ),
        (
            "Event 'change' on client #114354",
            PaKind::Change,
            PaFacility::Client,
            114354,
        ),
        (
            "Event 'new' on module #5",
            PaKind::New,
            PaFacility::Module,
            5,
        ),
        (
            "Event 'remove' on card #3",
            PaKind::Remove,
            PaFacility::Card,
            3,
        ),
        (
            "Event 'change' on server #1",
            PaKind::Change,
            PaFacility::Server,
            1,
        ),
        (
            "Event 'new' on weirdfacility #77",
            PaKind::New,
            PaFacility::Other("weirdfacility".to_string()),
            77,
        ),
    ];
    for (line, kind, facility, index) in cases {
        let ev = parse_event(line).unwrap_or_else(|| panic!("expected parse of {line:?}"));
        assert_eq!(
            ev,
            PaEvent {
                kind,
                facility,
                index,
            },
            "line: {line}"
        );
    }
}

#[test]
fn parse_event_malformed_is_none() {
    let bad = [
        "",
        "garbage",
        "Event 'new' on client",
        "Event 'new' on client #",
        "Event 'new' on client #abc",
        "Event 'new' on client #-1",
        "Event 'new' on client #1.5",
        "Event 'new' on client #1 extra",
        "Event 'new' client #1",
        "Event new on client #1",
        "new on client #1",
        "EVENT 'new' on client #1",
        "Event 'novel' on client #1",
        "Event 'new' on client #0x10",
        "Event 'new' on",
    ];
    for line in bad {
        assert_eq!(parse_event(line), None, "line: {line:?}");
    }
}

#[test]
fn needs_refresh_scopes() {
    let ev = |f: PaFacility| PaEvent {
        kind: PaKind::Change,
        facility: f,
        index: 1,
    };
    assert_eq!(needs_refresh(&ev(PaFacility::Sink)), RefreshScope::Sinks);
    assert_eq!(
        needs_refresh(&ev(PaFacility::SinkInput)),
        RefreshScope::Streams
    );
    assert_eq!(
        needs_refresh(&ev(PaFacility::Client)),
        RefreshScope::Streams
    );
    assert_eq!(needs_refresh(&ev(PaFacility::Server)), RefreshScope::Both);
    assert_eq!(needs_refresh(&ev(PaFacility::Source)), RefreshScope::None);
    assert_eq!(
        needs_refresh(&ev(PaFacility::SourceOutput)),
        RefreshScope::None
    );
    assert_eq!(needs_refresh(&ev(PaFacility::Module)), RefreshScope::None);
    assert_eq!(needs_refresh(&ev(PaFacility::Card)), RefreshScope::None);
    assert_eq!(
        needs_refresh(&ev(PaFacility::Other("x".to_string()))),
        RefreshScope::None
    );
}

// ---------------------------------------------------------------------------
// MixerModel grouping
// ---------------------------------------------------------------------------

#[test]
fn model_apps_from_fixture() {
    let sinks = parse_sinks(SINKS_JSON).unwrap();
    let streams = parse_sink_inputs(STREAMS_JSON).unwrap();
    let model = MixerModel::from(
        sinks,
        streams,
        Some("alsa_output.pci-0000_00_1f.3.analog-stereo".to_string()),
    );
    let apps = model.apps();
    assert_eq!(apps.len(), 3);

    // sorted case-insensitively: herm es island, mpv, notification
    assert_eq!(apps[0].app_name, "Hermes Island");
    assert_eq!(apps[0].streams, vec![2127]);
    assert_eq!(apps[0].volume_percent, Some(100));
    assert!(!apps[0].muted);
    assert_eq!(apps[0].sink_index, Some(0));
    assert!(!apps[0].idle);

    assert_eq!(apps[1].app_name, "mpv");
    assert_eq!(apps[1].streams, vec![2128]);
    assert_eq!(apps[1].volume_percent, Some(50));
    assert!(apps[1].muted);
    assert_eq!(apps[1].sink_index, Some(0));
    assert!(apps[1].idle);

    assert_eq!(apps[2].app_name, "Notification");
    assert_eq!(apps[2].streams, vec![2129]);
    assert_eq!(apps[2].volume_percent, Some(150));
    assert!(!apps[2].muted);
    assert_eq!(apps[2].sink_index, Some(1));
    assert!(!apps[2].idle);

    // default sink by name
    assert_eq!(model.default_sink().map(|s| s.index), Some(0),);
}

#[test]
fn model_same_pid_two_streams_one_group() {
    let streams = vec![
        stream(1, 0, "Firefox", Some(100), false, false, Some(40)),
        stream(2, 0, "Firefox", Some(100), false, false, Some(60)),
    ];
    let model = MixerModel::from(Vec::new(), streams, None);
    let apps = model.apps();
    assert_eq!(apps.len(), 1);
    assert_eq!(apps[0].app_name, "Firefox");
    assert_eq!(apps[0].streams, vec![1, 2]);
    assert_eq!(apps[0].volume_percent, Some(60));
    assert!(!apps[0].muted);
    assert!(!apps[0].idle);
}

#[test]
fn model_same_app_two_pids_two_groups() {
    let streams = vec![
        stream(1, 0, "Firefox", Some(100), false, false, Some(40)),
        stream(2, 0, "Firefox", Some(200), false, false, Some(40)),
    ];
    let apps = MixerModel::from(Vec::new(), streams, None).apps();
    assert_eq!(apps.len(), 2);
    assert_eq!(apps[0].streams, vec![1]);
    assert_eq!(apps[1].streams, vec![2]);
}

#[test]
fn model_sort_case_insensitive_then_pid() {
    let streams = vec![
        stream(1, 0, "Zebra", Some(1), false, false, Some(10)),
        stream(2, 0, "banana", Some(1), false, false, Some(10)),
        stream(3, 0, "Apple", Some(2), false, false, Some(10)),
        stream(4, 0, "apple", Some(1), false, false, Some(10)),
    ];
    let apps = MixerModel::from(Vec::new(), streams, None).apps();
    let names: Vec<&str> = apps.iter().map(|g| g.app_name.as_str()).collect();
    // case-insensitive app sort, ties broken by pid ascending: apple(pid 1)
    // sorts before Apple(pid 2)
    assert_eq!(names, vec!["apple", "Apple", "banana", "Zebra"]);
}

#[test]
fn model_sink_index_all_or_none() {
    // all on sink 0
    let streams = vec![
        stream(1, 0, "A", Some(1), false, false, Some(10)),
        stream(2, 0, "A", Some(1), false, false, Some(10)),
    ];
    let apps = MixerModel::from(Vec::new(), streams, None).apps();
    assert_eq!(apps[0].sink_index, Some(0));

    // mixed sinks
    let streams = vec![
        stream(1, 0, "A", Some(1), false, false, Some(10)),
        stream(2, 1, "A", Some(1), false, false, Some(10)),
    ];
    let apps = MixerModel::from(Vec::new(), streams, None).apps();
    assert_eq!(apps[0].sink_index, None);
}

#[test]
fn model_sink_index_mixed_stays_none() {
    // sinks 1, 2, 1: the disagreement must poison the group permanently
    let streams = vec![
        stream(1, 1, "A", Some(1), false, false, Some(10)),
        stream(2, 2, "A", Some(1), false, false, Some(10)),
        stream(3, 1, "A", Some(1), false, false, Some(10)),
    ];
    let apps = MixerModel::from(Vec::new(), streams, None).apps();
    assert_eq!(apps[0].sink_index, None);

    let streams = vec![
        stream(1, 1, "A", Some(1), false, false, Some(10)),
        stream(2, 1, "A", Some(1), false, false, Some(10)),
    ];
    let apps = MixerModel::from(Vec::new(), streams, None).apps();
    assert_eq!(apps[0].sink_index, Some(1));

    let streams = vec![stream(1, 2, "A", Some(1), false, false, Some(10))];
    let apps = MixerModel::from(Vec::new(), streams, None).apps();
    assert_eq!(apps[0].sink_index, Some(2));
}

#[test]
fn model_muted_is_all_members() {
    let streams = vec![
        stream(1, 0, "A", Some(1), true, false, Some(10)),
        stream(2, 0, "A", Some(1), false, false, Some(10)),
    ];
    let apps = MixerModel::from(Vec::new(), streams, None).apps();
    assert!(!apps[0].muted);

    let streams = vec![
        stream(1, 0, "A", Some(1), true, false, Some(10)),
        stream(2, 0, "A", Some(1), true, false, Some(10)),
    ];
    let apps = MixerModel::from(Vec::new(), streams, None).apps();
    assert!(apps[0].muted);
}

#[test]
fn model_idle_flag_corked_only() {
    // all corked -> idle
    let streams = vec![
        stream(1, 0, "A", Some(1), false, true, Some(10)),
        stream(2, 0, "A", Some(1), false, true, Some(10)),
    ];
    let apps = MixerModel::from(Vec::new(), streams, None).apps();
    assert!(apps[0].idle);

    // one not corked -> not idle
    let streams = vec![
        stream(1, 0, "A", Some(1), false, true, Some(10)),
        stream(2, 0, "A", Some(1), false, false, Some(10)),
    ];
    let apps = MixerModel::from(Vec::new(), streams, None).apps();
    assert!(!apps[0].idle);

    // last stream corked but an earlier one is not -> still not idle
    // (the final corked state must not override an earlier uncorked one)
    let streams = vec![
        stream(1, 0, "A", Some(1), false, false, Some(10)),
        stream(2, 0, "A", Some(1), false, true, Some(10)),
    ];
    let apps = MixerModel::from(Vec::new(), streams, None).apps();
    assert!(!apps[0].idle);
}

#[test]
fn model_volume_percent_max_and_none_when_no_known() {
    let streams = vec![
        stream(1, 0, "A", Some(1), false, false, Some(30)),
        stream(2, 0, "A", Some(1), false, false, Some(120)),
    ];
    let apps = MixerModel::from(Vec::new(), streams, None).apps();
    assert_eq!(apps[0].volume_percent, Some(120));

    // known + unknown members: max of the known ones (None never resets)
    let streams = vec![
        stream(1, 0, "A", Some(1), false, false, Some(70)),
        stream(2, 0, "A", Some(1), false, false, None),
    ];
    let apps = MixerModel::from(Vec::new(), streams, None).apps();
    assert_eq!(apps[0].volume_percent, Some(70));

    let streams = vec![
        stream(1, 0, "A", Some(1), false, false, None),
        stream(2, 0, "A", Some(1), false, false, None),
    ];
    let apps = MixerModel::from(Vec::new(), streams, None).apps();
    assert_eq!(apps[0].volume_percent, None);
}

#[test]
fn model_default_sink_missing_or_absent() {
    let sinks = vec![sink(0, "sink-a", false, Some(50), 1)];
    let model = MixerModel::from(sinks.clone(), Vec::new(), Some("sink-a".to_string()));
    assert_eq!(
        model.default_sink().map(|s| s.name.as_str()),
        Some("sink-a")
    );

    let model = MixerModel::from(sinks, Vec::new(), Some("sink-b".to_string()));
    assert_eq!(model.default_sink(), None);
    let model = MixerModel::from(Vec::new(), Vec::new(), None);
    assert_eq!(model.default_sink(), None);
}

#[test]
fn model_empty() {
    let model = MixerModel::from(Vec::new(), Vec::new(), None);
    assert!(model.apps().is_empty());
    assert_eq!(model.default_sink(), None);
}

// ---------------------------------------------------------------------------
// command builders
// ---------------------------------------------------------------------------

fn name_sink(name: &str) -> Sink {
    let mut s = sink(0, name, false, Some(50), 2);
    s.name = name.to_string();
    s
}

#[test]
fn set_sink_volume_argv() {
    let s = name_sink("alsa_output.pci-0000_00_1f.3.analog-stereo");
    assert_eq!(
        set_sink_volume(&s, 50).unwrap(),
        vec![
            "pactl".to_string(),
            "set-sink-volume".to_string(),
            "alsa_output.pci-0000_00_1f.3.analog-stereo".to_string(),
            "50%".to_string(),
        ]
    );
    assert_eq!(set_sink_volume(&s, 0).unwrap()[3], "0%");
    assert_eq!(set_sink_volume(&s, 150).unwrap()[3], "150%");
    assert_eq!(set_sink_volume(&s, 151), Err(CmdError::PercentOutOfRange));
    assert_eq!(
        set_sink_volume(&s, u32::MAX),
        Err(CmdError::PercentOutOfRange)
    );
}

#[test]
fn set_stream_volume_argv() {
    assert_eq!(
        set_stream_volume(2127, 60).unwrap(),
        vec![
            "pactl".to_string(),
            "set-sink-input-volume".to_string(),
            "2127".to_string(),
            "60%".to_string(),
        ]
    );
    assert_eq!(
        set_stream_volume(2127, 151),
        Err(CmdError::PercentOutOfRange)
    );
}

#[test]
fn set_app_volume_argv() {
    let g = AppGroup {
        app_name: "Firefox".to_string(),
        streams: vec![1, 2, 3],
        volume_percent: Some(50),
        muted: false,
        sink_index: Some(0),
        idle: false,
    };
    let argvs = set_app_volume(&g, 40).unwrap();
    assert_eq!(argvs.len(), 3);
    assert_eq!(
        argvs[1],
        vec![
            "pactl".to_string(),
            "set-sink-input-volume".to_string(),
            "2".to_string(),
            "40%".to_string(),
        ]
    );
    assert_eq!(set_app_volume(&g, 151), Err(CmdError::PercentOutOfRange));

    let empty = AppGroup {
        streams: Vec::new(),
        ..g
    };
    assert_eq!(
        set_app_volume(&empty, 40).unwrap(),
        Vec::<Vec<String>>::new()
    );
}

#[test]
fn mute_sink_and_stream_argv() {
    let s = name_sink("sink-a");
    assert_eq!(
        mute_sink(&s, MuteOp::On).unwrap(),
        vec!["pactl", "set-sink-mute", "sink-a", "1"]
    );
    assert_eq!(
        mute_sink(&s, MuteOp::Off).unwrap(),
        vec!["pactl", "set-sink-mute", "sink-a", "0"]
    );
    assert_eq!(
        mute_sink(&s, MuteOp::Toggle).unwrap(),
        vec!["pactl", "set-sink-mute", "sink-a", "toggle"]
    );
    assert_eq!(
        mute_stream(2127, MuteOp::Toggle).unwrap(),
        vec!["pactl", "set-sink-input-mute", "2127", "toggle"]
    );
    assert_eq!(
        mute_stream(0, MuteOp::Off).unwrap(),
        vec!["pactl", "set-sink-input-mute", "0", "0"]
    );
}

#[test]
fn set_default_sink_and_move_stream_argv() {
    let s = name_sink("sink-a");
    assert_eq!(
        set_default_sink(&s).unwrap(),
        vec!["pactl", "set-default-sink", "sink-a"]
    );
    assert_eq!(
        move_stream(2127, &s).unwrap(),
        vec!["pactl", "move-sink-input", "2127", "sink-a"]
    );
}

#[test]
fn invalid_sink_name_rejected_in_all_name_builders() {
    let bad_names = ["", "-x", "a b", "a\tb", "a\nb", "a\0b", &"a".repeat(257)];
    for bad in bad_names {
        let s = name_sink(bad);
        assert_eq!(
            set_sink_volume(&s, 50),
            Err(CmdError::InvalidSinkName),
            "name {bad:?}"
        );
        assert_eq!(
            mute_sink(&s, MuteOp::On),
            Err(CmdError::InvalidSinkName),
            "name {bad:?}"
        );
        assert_eq!(
            set_default_sink(&s),
            Err(CmdError::InvalidSinkName),
            "name {bad:?}"
        );
        assert_eq!(
            move_stream(1, &s),
            Err(CmdError::InvalidSinkName),
            "name {bad:?}"
        );
    }
}

#[test]
fn valid_sink_name_boundaries_accepted() {
    let long = "a".repeat(256);
    let s = name_sink(&long);
    assert_eq!(set_sink_volume(&s, 10).unwrap()[2], long);
    assert!(set_default_sink(&s).is_ok());
    assert!(mute_sink(&s, MuteOp::Toggle).is_ok());
    assert!(move_stream(1, &s).is_ok());

    let s = name_sink("a-b_c.0");
    assert!(set_sink_volume(&s, 10).is_ok());
}

// ---------------------------------------------------------------------------
// serde camelCase round-trips
// ---------------------------------------------------------------------------

#[test]
fn sink_serde_camel_case_round_trip() {
    let s = Sink {
        index: 3,
        name: "sink-a".to_string(),
        description: "d".to_string(),
        muted: true,
        volume_percent: Some(75),
        channels: 2,
        state: "RUNNING".to_string(),
    };
    let json = serde_json::to_string(&s).unwrap();
    assert!(json.contains("\"volumePercent\""));
    assert!(json.contains("\"channels\""));
    assert!(!json.contains("volume_percent"));
    let back: Sink = serde_json::from_str(&json).unwrap();
    assert_eq!(back, s);
}

#[test]
fn stream_serde_camel_case_round_trip() {
    let s = Stream {
        index: 9,
        sink_index: 0,
        app_name: "Firefox".to_string(),
        binary: Some("firefox".to_string()),
        pid: Some(42),
        media_name: Some("m".to_string()),
        icon_name: Some("i".to_string()),
        muted: false,
        corked: true,
        volume_percent: None,
    };
    let json = serde_json::to_string(&s).unwrap();
    for key in [
        "\"sinkIndex\"",
        "\"appName\"",
        "\"mediaName\"",
        "\"iconName\"",
        "\"volumePercent\"",
    ] {
        assert!(json.contains(key), "missing {key} in {json}");
    }
    assert!(!json.contains("sink_index"));
    let back: Stream = serde_json::from_str(&json).unwrap();
    assert_eq!(back, s);
}

#[test]
fn app_group_serde_round_trip() {
    let g = AppGroup {
        app_name: "A".to_string(),
        streams: vec![1, 2],
        volume_percent: Some(60),
        muted: true,
        sink_index: Some(0),
        idle: false,
    };
    let json = serde_json::to_string(&g).unwrap();
    assert!(json.contains("\"sinkIndex\""));
    let back: AppGroup = serde_json::from_str(&json).unwrap();
    assert_eq!(back, g);
}

#[test]
fn app_group_volume_percent_none_serializes_null() {
    let g = AppGroup {
        app_name: "A".to_string(),
        streams: vec![1],
        volume_percent: None,
        muted: false,
        sink_index: Some(0),
        idle: false,
    };
    let json = serde_json::to_string(&g).unwrap();
    assert!(json.contains("\"volumePercent\":null"), "json: {json}");
    assert!(!json.contains("\"volumePercent\":0"));
    let back: AppGroup = serde_json::from_str(&json).unwrap();
    assert_eq!(back, g);
}
