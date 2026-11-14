use hermes_island_lib::integrations::tiles::core::{
    argb_to_hex, argb_to_rgba_css, is_valid_voice_name, network_tile, network_tile_report,
    output_tile, parse_current, parse_default_sink, parse_sinks, parse_sinks_report,
    parse_tailscale_status, parse_tailscale_status_report, parse_voice_list,
    parse_voice_list_report, screenshot_argv, set_default_argv, switch_argv, Action, BackendState,
    CmdError, KeepAwake, KeepAwakeState, NetKind, NmConnectivity, NmSnapshot, NmState, OutputTile,
    ParseError, Peer, Sink, TailscaleTile, VoiceList, AUTO_OFF_1H, AUTO_OFF_30M, INHIBIT_APP,
    MAX_SINKS_BYTES, MAX_TAILSCALE_BYTES, MAX_VOICE_BYTES,
};

fn sink(index: u32, name: &str, description: &str, state: &str, muted: bool) -> Sink {
    Sink {
        index,
        name: name.to_string(),
        description: description.to_string(),
        state: state.to_string(),
        muted,
    }
}

fn nm_snapshot(
    state: Option<u32>,
    connectivity: Option<u32>,
    primary: Option<&str>,
    wifi: Option<bool>,
    hw: Option<bool>,
    has_wireless: bool,
) -> NmSnapshot {
    NmSnapshot {
        state,
        connectivity,
        primary_connection_type: primary.map(str::to_string),
        wireless_enabled: wifi,
        wireless_hardware_enabled: hw,
        has_wireless_device: has_wireless,
    }
}

const SINKS_JSON: &str = r#"[
  {
    "index": 0,
    "name": "alsa_output.pci-0000_00_1f.3.analog-stereo",
    "description": "Built-in Audio Analog Stereo",
    "state": "RUNNING",
    "mute": false,
    "ports": [
      { "name": "analog-output", "description": "Analog", "priority": 9000, "availability": "available" }
    ],
    "active_port": "analog-output",
    "volume": { "front-left": { "value": 65536 }, "front-right": { "value": 65536 } }
  },
  {
    "index": 1,
    "name": "bluez_output.00_00_00_00_00_00.1",
    "description": "Bluetooth Speaker",
    "state": "IDLE",
    "mute": true,
    "ports": [],
    "active_port": null
  }
]"#;

const TAILSCALE_JSON: &str = r#"{
  "BackendState": "Running",
  "Version": "1.104.0",
  "TUN": true,
  "Self": {
    "TailscaleIPs": ["100.64.0.1", "fd7a:115c:a1e0::1"],
    "HostName": "example-pc",
    "OS": "linux",
    "Online": true,
    "LastSeen": "2026-10-30T12:00:00Z",
    "DNSName": "example-pc.tailnet.ts.net"
  },
  "Peer": {
    "100.64.0.3": {
      "HostName": "zeta",
      "OS": "windows",
      "Online": false,
      "LastSeen": "2026-10-29T08:00:00Z",
      "TailscaleIPs": ["100.64.0.3"]
    },
    "100.64.0.2": {
      "HostName": "alpha",
      "OS": "android",
      "Online": true,
      "LastSeen": "2026-10-30T11:00:00Z",
      "TailscaleIPs": ["100.64.0.2", "fd7a:115c:a1e0::2"]
    },
    "100.64.0.4": {
      "HostName": "beta",
      "OS": "darwin",
      "Online": true,
      "LastSeen": "0001-01-01T00:00:00Z",
      "TailscaleIPs": []
    }
  }
}"#;

const VOICES_FIXTURE: &str = "Fish Audio voices:\n\n  necron4  [active]\n  alex\n  marina\n";

// ---------------------------------------------------------------------------
// Audio output
// ---------------------------------------------------------------------------

#[test]
fn parse_sinks_report_parses_fixture() {
    let (sinks, skipped) = parse_sinks_report(SINKS_JSON).unwrap();
    assert_eq!(skipped, 0);
    assert_eq!(sinks.len(), 2);
    assert_eq!(
        sinks[0],
        sink(
            0,
            "alsa_output.pci-0000_00_1f.3.analog-stereo",
            "Built-in Audio Analog Stereo",
            "RUNNING",
            false
        )
    );
    assert_eq!(
        sinks[1],
        sink(
            1,
            "bluez_output.00_00_00_00_00_00.1",
            "Bluetooth Speaker",
            "IDLE",
            true
        )
    );
    // pactl order preserved
    assert_eq!(sinks[0].index, 0);
    assert_eq!(sinks[1].index, 1);
}

#[test]
fn parse_sinks_skips_elements_missing_required_fields() {
    let input = r#"[
      { "index": 0, "name": "only-name" },
      { "index": 1, "description": "only-desc" },
      { "name": "n", "description": "d" },
      { "index": "notnum", "name": "x", "description": "y" },
      { "index": 2, "name": "ok", "description": "desc2", "state": "SUSPENDED", "mute": false }
    ]"#;
    let (sinks, skipped) = parse_sinks_report(input).unwrap();
    assert_eq!(skipped, 4);
    assert_eq!(sinks.len(), 1);
    assert_eq!(sinks[0], sink(2, "ok", "desc2", "SUSPENDED", false));
}

#[test]
fn parse_sinks_non_array_is_error() {
    assert_eq!(parse_sinks("{}"), Err(ParseError::NotAnArray));
    assert_eq!(parse_sinks("null"), Err(ParseError::NotAnArray));
    assert_eq!(parse_sinks("\"str\""), Err(ParseError::NotAnArray));
}

#[test]
fn parse_sinks_invalid_json_is_error() {
    assert_eq!(parse_sinks("[{"), Err(ParseError::InvalidJson));
    assert_eq!(parse_sinks(""), Err(ParseError::InvalidJson));
}

#[test]
fn parse_sinks_wrong_typed_optionals_default() {
    // mute/state wrong types are lenient (false / ""), element kept.
    let input = r#"[ { "index": 0, "name": "a", "description": "d", "mute": "yes", "state": 5 } ]"#;
    let sinks = parse_sinks(input).unwrap();
    assert_eq!(sinks.len(), 1);
    assert_eq!(sinks[0], sink(0, "a", "d", "", false));
}

#[test]
fn parse_sinks_oversize_rejected() {
    let input = " ".repeat(MAX_SINKS_BYTES + 1);
    assert_eq!(
        parse_sinks(&input),
        Err(ParseError::TooLarge {
            limit: MAX_SINKS_BYTES
        })
    );
}

#[test]
fn parse_sinks_exactly_cap_parses() {
    let base = r#"[{"index":0,"name":"a","description":"d"}]"#;
    let pad = MAX_SINKS_BYTES - base.len();
    let input = format!("{base}{}", " ".repeat(pad));
    assert_eq!(input.len(), MAX_SINKS_BYTES);
    let sinks = parse_sinks(&input).unwrap();
    assert_eq!(sinks.len(), 1);
}

#[test]
fn parse_default_sink_trims_first_line() {
    assert_eq!(
        parse_default_sink("alsa_output.pci-0000_00_1f.3.analog-stereo\n").as_deref(),
        Some("alsa_output.pci-0000_00_1f.3.analog-stereo")
    );
    assert_eq!(parse_default_sink("  x \n y\n").as_deref(), Some("x"));
    assert_eq!(parse_default_sink(""), None);
    assert_eq!(parse_default_sink("\n  \n"), None);
}

#[test]
fn output_tile_finds_current_by_name_and_keeps_order() {
    let sinks = vec![
        sink(0, "a", "A", "RUNNING", false),
        sink(1, "b", "B", "IDLE", true),
    ];
    let tile = output_tile(sinks.clone(), Some("b".to_string()));
    assert_eq!(tile.current, Some(sink(1, "b", "B", "IDLE", true)));
    assert_eq!(tile.choices, sinks);
}

#[test]
fn output_tile_current_none_when_absent_or_no_default() {
    let sinks = vec![sink(0, "a", "A", "RUNNING", false)];
    assert_eq!(
        output_tile(sinks.clone(), Some("zzz".to_string())).current,
        None
    );
    assert_eq!(output_tile(sinks, None).current, None);
}

// ---------------------------------------------------------------------------
// Network
// ---------------------------------------------------------------------------

#[test]
fn nm_state_from_u32_all_values() {
    assert_eq!(NmState::from_u32(0), Some(NmState::Unknown));
    assert_eq!(NmState::from_u32(10), Some(NmState::Asleep));
    assert_eq!(NmState::from_u32(20), Some(NmState::Disconnected));
    assert_eq!(NmState::from_u32(30), Some(NmState::Disconnecting));
    assert_eq!(NmState::from_u32(40), Some(NmState::Connecting));
    assert_eq!(NmState::from_u32(50), Some(NmState::ConnectedLocal));
    assert_eq!(NmState::from_u32(60), Some(NmState::ConnectedSite));
    assert_eq!(NmState::from_u32(70), Some(NmState::ConnectedGlobal));
}

#[test]
fn nm_state_unknown_numeric_is_none() {
    for v in [1u32, 7, 9, 71, 100, u32::MAX] {
        assert_eq!(NmState::from_u32(v), None);
    }
}

#[test]
fn nm_connectivity_from_u32() {
    assert_eq!(NmConnectivity::from_u32(0), Some(NmConnectivity::Unknown));
    assert_eq!(NmConnectivity::from_u32(1), Some(NmConnectivity::None));
    assert_eq!(NmConnectivity::from_u32(2), Some(NmConnectivity::Portal));
    assert_eq!(NmConnectivity::from_u32(3), Some(NmConnectivity::Limited));
    assert_eq!(NmConnectivity::from_u32(4), Some(NmConnectivity::Full));
    assert_eq!(NmConnectivity::from_u32(5), None);
    assert_eq!(NmConnectivity::from_u32(99), None);
}

#[test]
fn network_tile_none_is_unavailable() {
    let (tile, warnings) = network_tile_report(None);
    assert_eq!(warnings, 0);
    assert!(!tile.available);
    assert_eq!(
        tile.reason.as_deref(),
        Some("NetworkManager D-Bus service unavailable")
    );
    assert_eq!(tile.state, None);
    assert_eq!(tile.connectivity, None);
    assert_eq!(tile.kind, NetKind::None);
    assert!(!tile.show_wifi_toggle);
    assert_eq!(tile.wifi_enabled, None);
}

#[test]
fn network_tile_maps_full_snapshot() {
    let nm = nm_snapshot(
        Some(70),
        Some(4),
        Some("802-3-ethernet"),
        Some(false),
        Some(true),
        false,
    );
    let (tile, warnings) = network_tile_report(Some(nm));
    assert_eq!(warnings, 0);
    assert!(tile.available);
    assert_eq!(tile.reason, None);
    assert_eq!(tile.state, Some(NmState::ConnectedGlobal));
    assert_eq!(tile.connectivity, Some(NmConnectivity::Full));
    assert_eq!(tile.kind, NetKind::Wired);
    assert!(!tile.show_wifi_toggle);
    assert_eq!(tile.wifi_enabled, Some(false));
}

#[test]
fn network_tile_unknown_numeric_values_warn_and_stay_none() {
    let nm = nm_snapshot(Some(99), Some(6), None, None, None, false);
    let (tile, warnings) = network_tile_report(Some(nm));
    assert_eq!(warnings, 2);
    assert_eq!(tile.state, None);
    assert_eq!(tile.connectivity, None);
    assert_eq!(tile.kind, NetKind::None);
}

#[test]
fn network_tile_absent_properties_no_warnings() {
    let nm = nm_snapshot(None, None, None, None, None, false);
    let (tile, warnings) = network_tile_report(Some(nm));
    assert_eq!(warnings, 0);
    assert_eq!(tile.state, None);
    assert_eq!(tile.connectivity, None);
}

#[test]
fn network_tile_kind_wifi_and_toggle() {
    let nm = nm_snapshot(
        Some(50),
        Some(3),
        Some("802-11-wireless"),
        Some(true),
        Some(true),
        true,
    );
    let (tile, warnings) = network_tile_report(Some(nm));
    assert_eq!(warnings, 0);
    assert_eq!(tile.kind, NetKind::Wifi);
    assert!(tile.show_wifi_toggle);
    assert_eq!(tile.state, Some(NmState::ConnectedLocal));
    assert_eq!(tile.connectivity, Some(NmConnectivity::Limited));
}

#[test]
fn network_tile_kind_other_and_empty() {
    let nm = nm_snapshot(None, None, Some("bluetooth"), None, None, false);
    assert_eq!(network_tile(Some(nm)).kind, NetKind::Other);
    let nm = nm_snapshot(None, None, Some(""), None, None, false);
    assert_eq!(network_tile(Some(nm)).kind, NetKind::None);
    let nm = nm_snapshot(None, None, None, None, None, false);
    assert_eq!(network_tile(Some(nm)).kind, NetKind::None);
}

#[test]
fn network_tile_wifi_enabled_logic() {
    // hardware off wins
    let nm = nm_snapshot(None, None, None, Some(true), Some(false), true);
    assert_eq!(network_tile(Some(nm)).wifi_enabled, Some(false));
    let nm = nm_snapshot(None, None, None, None, Some(false), true);
    assert_eq!(network_tile(Some(nm)).wifi_enabled, Some(false));
    // software off
    let nm = nm_snapshot(None, None, None, Some(false), Some(true), true);
    assert_eq!(network_tile(Some(nm)).wifi_enabled, Some(false));
    // software on + hardware on
    let nm = nm_snapshot(None, None, None, Some(true), Some(true), true);
    assert_eq!(network_tile(Some(nm)).wifi_enabled, Some(true));
    // unknown software, hardware on -> unknown
    let nm = nm_snapshot(None, None, None, None, Some(true), true);
    assert_eq!(network_tile(Some(nm)).wifi_enabled, None);
    // unknown hardware, software on -> on
    let nm = nm_snapshot(None, None, None, Some(true), None, true);
    assert_eq!(network_tile(Some(nm)).wifi_enabled, Some(true));
    // both unknown
    let nm = nm_snapshot(None, None, None, None, None, true);
    assert_eq!(network_tile(Some(nm)).wifi_enabled, None);
}

// ---------------------------------------------------------------------------
// Tailscale
// ---------------------------------------------------------------------------

#[test]
fn parse_tailscale_status_fixture() {
    let (tile, skipped) = parse_tailscale_status_report(TAILSCALE_JSON).unwrap();
    assert_eq!(skipped, 0);
    assert_eq!(tile.backend, BackendState::Running);
    assert_eq!(tile.self_name.as_deref(), Some("example-pc"));
    // sorted online-first then host name: alpha, beta (online), zeta (offline)
    assert_eq!(tile.peers.len(), 3);
    assert_eq!(tile.peers[0].host_name, "alpha");
    assert_eq!(tile.peers[1].host_name, "beta");
    assert_eq!(tile.peers[2].host_name, "zeta");
    assert_eq!(tile.online_count, 2);
    assert_eq!(tile.peers[0].online, Some(true));
    assert_eq!(tile.peers[2].online, Some(false));
    // firstIp and lastSeen
    assert_eq!(tile.peers[0].first_ip.as_deref(), Some("100.64.0.2"));
    assert_eq!(tile.peers[0].os, "android");
    assert_eq!(
        tile.peers[0].last_seen.as_deref(),
        Some("2026-10-30T11:00:00Z")
    );
    // zero-time LastSeen -> None; empty TailscaleIPs -> firstIp None
    assert_eq!(tile.peers[1].last_seen, None);
    assert_eq!(tile.peers[1].first_ip, None);
}

#[test]
fn parse_tailscale_status_unknown_backend_kept_raw() {
    let input = r#"{ "BackendState": "Paused", "Self": {}, "Peer": {} }"#;
    let tile = parse_tailscale_status(input).unwrap();
    assert_eq!(tile.backend, BackendState::Unknown("Paused".to_string()));
}

#[test]
fn parse_tailscale_status_all_known_backends() {
    for (s, expected) in [
        ("Running", BackendState::Running),
        ("Stopped", BackendState::Stopped),
        ("NeedsLogin", BackendState::NeedsLogin),
        ("Starting", BackendState::Starting),
        ("NoState", BackendState::NoState),
    ] {
        let input = format!(r#"{{ "BackendState": "{s}" }}"#);
        let tile = parse_tailscale_status(&input).unwrap();
        assert_eq!(tile.backend, expected);
    }
}

#[test]
fn parse_tailscale_status_missing_peer_is_empty_not_error() {
    let input = r#"{ "BackendState": "Running" }"#;
    let tile = parse_tailscale_status(input).unwrap();
    assert_eq!(tile.peers, Vec::<Peer>::new());
    assert_eq!(tile.online_count, 0);
    assert_eq!(tile.self_name, None);
}

#[test]
fn parse_tailscale_status_missing_backend_state_is_error() {
    assert_eq!(
        parse_tailscale_status("{}"),
        Err(ParseError::MissingBackendState)
    );
    assert_eq!(
        parse_tailscale_status(r#"{ "Peer": {} }"#),
        Err(ParseError::MissingBackendState)
    );
    // non-string BackendState is also an error
    assert_eq!(
        parse_tailscale_status(r#"{ "BackendState": 5 }"#),
        Err(ParseError::MissingBackendState)
    );
}

#[test]
fn parse_tailscale_status_non_object_is_error() {
    assert_eq!(parse_tailscale_status("[]"), Err(ParseError::NotAnObject));
    assert_eq!(
        parse_tailscale_status("\"x\""),
        Err(ParseError::NotAnObject)
    );
}

#[test]
fn parse_tailscale_status_invalid_json_is_error() {
    assert_eq!(parse_tailscale_status("{"), Err(ParseError::InvalidJson));
}

#[test]
fn parse_tailscale_status_oversize_rejected() {
    let input = " ".repeat(MAX_TAILSCALE_BYTES + 1);
    assert_eq!(
        parse_tailscale_status(&input),
        Err(ParseError::TooLarge {
            limit: MAX_TAILSCALE_BYTES
        })
    );
}

#[test]
fn parse_tailscale_status_report_counts_malformed_peers() {
    let input = r#"{
      "BackendState": "Running",
      "Peer": {
        "100.64.0.2": { "HostName": "ok", "Online": true },
        "100.64.0.9": { "Online": false },
        "100.64.0.10": 42
      }
    }"#;
    let (tile, skipped) = parse_tailscale_status_report(input).unwrap();
    assert_eq!(skipped, 2);
    assert_eq!(tile.peers.len(), 1);
    assert_eq!(tile.peers[0].host_name, "ok");
    assert_eq!(tile.online_count, 1);
}

// ---------------------------------------------------------------------------
// Voice switcher
// ---------------------------------------------------------------------------

#[test]
fn parse_voice_list_fixture() {
    let (list, warnings) = parse_voice_list_report(VOICES_FIXTURE).unwrap();
    assert_eq!(warnings, 0);
    assert_eq!(list.voices, vec!["necron4", "alex", "marina"]);
    assert_eq!(list.active.as_deref(), Some("necron4"));
}

#[test]
fn parse_voice_list_no_header_tolerated() {
    let (list, warnings) = parse_voice_list_report("  a\n  b\n").unwrap();
    assert_eq!(warnings, 0);
    assert_eq!(list.voices, vec!["a", "b"]);
    assert_eq!(list.active, None);
}

#[test]
fn parse_voice_list_multiple_active_is_error() {
    let input = "  a  [active]\n  b  [active]\n";
    assert_eq!(
        parse_voice_list(input),
        Err(ParseError::MultipleActiveVoices)
    );
}

#[test]
fn parse_voice_list_invalid_lines_warn_and_skip() {
    let input = "  a\n  !bad\nfoo\n  b  [active] extra\n  c\n";
    let (list, warnings) = parse_voice_list_report(input).unwrap();
    assert_eq!(warnings, 3);
    assert_eq!(list.voices, vec!["a", "c"]);
    assert_eq!(list.active, None);
}

#[test]
fn parse_voice_list_header_mid_list_dropped() {
    let input = "  a\nFish Audio voices:\n  b\n";
    let (list, warnings) = parse_voice_list_report(input).unwrap();
    assert_eq!(warnings, 0);
    assert_eq!(list.voices, vec!["a", "b"]);
}

#[test]
fn parse_voice_list_name_boundaries() {
    let ok64 = format!("  {}", "a".repeat(64));
    let bad65 = format!("  {}", "a".repeat(65));
    let (list, warnings) =
        parse_voice_list_report(&format!("{ok64}\n{bad65}\n  \u{4e2d}\n  a b\n")).unwrap();
    assert_eq!(warnings, 3);
    assert_eq!(list.voices, vec!["a".repeat(64)]);
}

#[test]
fn parse_voice_list_oversize_rejected() {
    let input = " ".repeat(MAX_VOICE_BYTES + 1);
    assert_eq!(
        parse_voice_list(&input),
        Err(ParseError::TooLarge {
            limit: MAX_VOICE_BYTES
        })
    );
}

#[test]
fn parse_current_ok_and_invalid() {
    assert_eq!(parse_current("necron4\n").unwrap(), "necron4");
    assert_eq!(parse_current("  necron4  \n").unwrap(), "necron4");
    assert_eq!(parse_current("a-b_c.0").unwrap(), "a-b_c.0");
    assert_eq!(parse_current("bad name"), Err(ParseError::InvalidVoiceName));
    assert_eq!(parse_current(""), Err(ParseError::InvalidVoiceName));
    assert_eq!(parse_current("\n  \n"), Err(ParseError::InvalidVoiceName));
    assert_eq!(
        parse_current(&"a".repeat(65)),
        Err(ParseError::InvalidVoiceName)
    );
}

#[test]
fn is_valid_voice_name_rule() {
    assert!(is_valid_voice_name("a"));
    assert!(is_valid_voice_name("a-b_c.0"));
    assert!(is_valid_voice_name(&"a".repeat(64)));
    assert!(!is_valid_voice_name(""));
    assert!(!is_valid_voice_name(&"a".repeat(65)));
    assert!(!is_valid_voice_name("a b"));
    assert!(!is_valid_voice_name("a\nb"));
    assert!(!is_valid_voice_name("声音"));
}

#[test]
fn switch_argv_ok_and_errors() {
    let list = VoiceList {
        voices: vec!["necron4".to_string(), "alex".to_string()],
        active: Some("necron4".to_string()),
    };
    assert_eq!(
        switch_argv(&list, "alex").unwrap(),
        vec!["hermes-fish", "alex"]
    );
    assert_eq!(switch_argv(&list, "ghost"), Err(CmdError::UnknownVoice));
    for reserved in ["current", "add", "remove", "id", "list"] {
        assert_eq!(switch_argv(&list, reserved), Err(CmdError::ReservedWord));
    }
}

#[test]
fn switch_argv_rejects_reserved_even_if_in_list() {
    let list = VoiceList {
        voices: vec!["remove".to_string()],
        active: None,
    };
    assert_eq!(switch_argv(&list, "remove"), Err(CmdError::ReservedWord));
}

#[test]
fn set_default_argv_ok() {
    assert_eq!(
        set_default_argv("alsa_output.pci-0000_00_1f.3.analog-stereo").unwrap(),
        vec![
            "pactl",
            "set-default-sink",
            "alsa_output.pci-0000_00_1f.3.analog-stereo"
        ]
    );
}

#[test]
fn set_default_argv_rejects_invalid_names() {
    assert_eq!(set_default_argv(""), Err(CmdError::InvalidSinkName));
    assert_eq!(set_default_argv("-x"), Err(CmdError::InvalidSinkName));
    assert_eq!(set_default_argv("a b"), Err(CmdError::InvalidSinkName));
    assert_eq!(set_default_argv("a\tb"), Err(CmdError::InvalidSinkName));
    assert_eq!(set_default_argv("a\nb"), Err(CmdError::InvalidSinkName));
    assert_eq!(set_default_argv("a\u{1}b"), Err(CmdError::InvalidSinkName));
    assert_eq!(
        set_default_argv(&"a".repeat(257)),
        Err(CmdError::InvalidSinkName)
    );
    assert_eq!(set_default_argv(&"a".repeat(256)).unwrap().len(), 3);
}

#[test]
fn screenshot_argv_constant() {
    assert_eq!(screenshot_argv(), vec!["spectacle", "-r"]);
}

// ---------------------------------------------------------------------------
// Colour picker
// ---------------------------------------------------------------------------

#[test]
fn argb_to_hex_chanks() {
    assert_eq!(argb_to_hex(0xFF336699), "#336699");
    assert_eq!(argb_to_hex(0x00123456), "#123456");
    assert_eq!(argb_to_hex(0xA0FF00FF), "#ff00ff");
    assert_eq!(argb_to_hex(0x00000000), "#000000");
    assert_eq!(argb_to_hex(0xFFFFFFFF), "#ffffff");
}

#[test]
fn argb_to_rgba_css_values() {
    assert_eq!(argb_to_rgba_css(0xFF000000), "rgba(0, 0, 0, 1)");
    assert_eq!(
        argb_to_rgba_css(0x80FFFFFF),
        "rgba(255, 255, 255, 0.5019608)"
    );
    assert_eq!(
        argb_to_rgba_css(0x4080C0E0),
        "rgba(128, 192, 224, 0.2509804)"
    );
    assert_eq!(argb_to_rgba_css(0x00000000), "rgba(0, 0, 0, 0)");
}

// ---------------------------------------------------------------------------
// Wire shape (camelCase, no aliases)
// ---------------------------------------------------------------------------

#[test]
fn wire_types_serialize_camel_case() {
    let tile = OutputTile {
        current: Some(sink(0, "a", "A", "RUNNING", false)),
        choices: vec![sink(0, "a", "A", "RUNNING", false)],
    };
    let json = serde_json::to_string(&tile).unwrap();
    assert!(json.contains("\"current\""));
    assert!(json.contains("\"choices\""));
    assert!(json.contains("\"muted\""));

    let nm = nm_snapshot(
        Some(70),
        Some(4),
        Some("802-3-ethernet"),
        Some(true),
        Some(true),
        true,
    );
    let (net, _) = network_tile_report(Some(nm.clone()));
    let json = serde_json::to_string(&net).unwrap();
    assert!(json.contains("\"showWifiToggle\""));
    assert!(json.contains("\"wifiEnabled\""));
    assert!(json.contains("\"connectedGlobal\""));
    // NmSnapshot is itself a wire type (Tauri -> crate): camelCase keys.
    let json = serde_json::to_string(&nm).unwrap();
    assert!(json.contains("\"primaryConnectionType\""));
    assert!(json.contains("\"hasWirelessDevice\""));
    assert!(json.contains("\"wirelessHardwareEnabled\""));

    let ts = TailscaleTile {
        backend: BackendState::Running,
        self_name: Some("example-pc".to_string()),
        peers: vec![Peer {
            host_name: "alpha".to_string(),
            online: Some(true),
            os: "android".to_string(),
            last_seen: None,
            first_ip: Some("100.64.0.2".to_string()),
        }],
        online_count: 1,
    };
    let json = serde_json::to_string(&ts).unwrap();
    assert!(json.contains("\"onlineCount\""));
    assert!(json.contains("\"selfName\""));
    assert!(json.contains("\"hostName\""));
    assert!(json.contains("\"lastSeen\""));
    assert!(json.contains("\"firstIp\""));

    let vl = VoiceList {
        voices: vec!["a".to_string()],
        active: Some("a".to_string()),
    };
    let json = serde_json::to_string(&vl).unwrap();
    assert!(json.contains("\"voices\""));
    assert!(json.contains("\"active\""));
}

// ---------------------------------------------------------------------------
// Keep awake state machine
// ---------------------------------------------------------------------------

#[test]
fn keepawake_turn_on_inhibits_and_requesting() {
    let mut k = KeepAwake::new();
    assert_eq!(k.state(), KeepAwakeState::Off);
    let action = k.turn_on(1000, "manual");
    assert_eq!(
        action,
        Some(Action::Inhibit {
            app: INHIBIT_APP.to_string(),
            reason: "manual".to_string()
        })
    );
    assert_eq!(
        k.state(),
        KeepAwakeState::Requesting {
            cancel_pending: false,
            since_ms: 1000
        }
    );
    assert_eq!(k.last_error(), None);
}

#[test]
fn keepawake_inhibited_confirms_on_with_since() {
    let mut k = KeepAwake::new();
    k.turn_on(5000, "demo");
    assert_eq!(k.inhibited(7), None);
    assert_eq!(
        k.state(),
        KeepAwakeState::On {
            cookie: 7,
            since_ms: 5000
        }
    );
}

#[test]
fn keepawake_turn_off_from_on_releases() {
    let mut k = KeepAwake::new();
    k.turn_on(0, "x");
    k.inhibited(3);
    assert_eq!(k.turn_off(), Some(Action::UnInhibit { cookie: 3 }));
    assert_eq!(k.state(), KeepAwakeState::Off);
}

#[test]
fn keepawake_turn_off_while_requesting_marks_cancel_pending() {
    let mut k = KeepAwake::new();
    k.turn_on(10, "x");
    assert_eq!(k.turn_off(), None);
    assert_eq!(
        k.state(),
        KeepAwakeState::Requesting {
            cancel_pending: true,
            since_ms: 10
        }
    );
}

#[test]
fn keepawake_late_cookie_released_after_cancel_pending() {
    let mut k = KeepAwake::new();
    k.turn_on(10, "x");
    k.turn_off();
    assert_eq!(k.inhibited(9), Some(Action::UnInhibit { cookie: 9 }));
    assert_eq!(k.state(), KeepAwakeState::Off);
}

#[test]
fn keepawake_failed_from_requesting_off_with_error() {
    let mut k = KeepAwake::new();
    k.turn_on(0, "x");
    k.failed("timeout");
    assert_eq!(k.state(), KeepAwakeState::Off);
    assert_eq!(k.last_error(), Some("timeout"));
}

#[test]
fn keepawake_failed_from_on_ignored() {
    let mut k = KeepAwake::new();
    k.turn_on(0, "x");
    k.inhibited(1);
    k.failed("nope");
    assert_eq!(
        k.state(),
        KeepAwakeState::On {
            cookie: 1,
            since_ms: 0
        }
    );
    assert_eq!(k.last_error(), None);
}

#[test]
fn keepawake_tick_not_due_noop() {
    let mut k = KeepAwake::new();
    k.turn_on(100, "x");
    k.inhibited(5);
    k.set_auto_off_at_ms(Some(200));
    assert_eq!(k.tick(199), None);
    assert_eq!(
        k.state(),
        KeepAwakeState::On {
            cookie: 5,
            since_ms: 100
        }
    );
    assert_eq!(k.auto_off_at_ms(), Some(200));
}

#[test]
fn keepawake_tick_due_releases_exactly_at_boundary() {
    let mut k = KeepAwake::new();
    k.turn_on(100, "x");
    k.inhibited(5);
    k.set_auto_off_at_ms(Some(200));
    assert_eq!(k.tick(200), Some(Action::UnInhibit { cookie: 5 }));
    assert_eq!(k.state(), KeepAwakeState::Off);
    assert_eq!(k.auto_off_at_ms(), None);
}

#[test]
fn keepawake_tick_due_while_requesting_marks_cancel_pending() {
    let mut k = KeepAwake::new();
    k.turn_on(100, "x");
    k.set_auto_off_at_ms(Some(200));
    assert_eq!(k.tick(200), None);
    assert_eq!(
        k.state(),
        KeepAwakeState::Requesting {
            cancel_pending: true,
            since_ms: 100
        }
    );
    assert_eq!(k.auto_off_at_ms(), None);
    assert_eq!(k.inhibited(8), Some(Action::UnInhibit { cookie: 8 }));
}

#[test]
fn keepawake_bus_lost_off_no_action_clears_autooff() {
    let mut k = KeepAwake::new();
    k.turn_on(0, "x");
    k.inhibited(1);
    k.set_auto_off_at_ms(Some(999));
    k.on_bus_lost();
    assert_eq!(k.state(), KeepAwakeState::Off);
    assert_eq!(k.auto_off_at_ms(), None);
}

#[test]
fn keepawake_turn_on_while_on_or_requesting_noop() {
    let mut k = KeepAwake::new();
    k.turn_on(0, "x");
    k.inhibited(1);
    assert_eq!(k.turn_on(5, "again"), None);
    assert_eq!(
        k.state(),
        KeepAwakeState::On {
            cookie: 1,
            since_ms: 0
        }
    );

    let mut k = KeepAwake::new();
    k.turn_on(0, "x");
    assert_eq!(k.turn_on(5, "again"), None);
    assert_eq!(
        k.state(),
        KeepAwakeState::Requesting {
            cancel_pending: false,
            since_ms: 0
        }
    );
}

#[test]
fn keepawake_turn_on_clears_last_error() {
    let mut k = KeepAwake::new();
    k.turn_on(0, "x");
    k.failed("boom");
    assert_eq!(k.last_error(), Some("boom"));
    k.turn_on(0, "y");
    assert_eq!(k.last_error(), None);
}

#[test]
fn keepawake_auto_off_duration_and_constants() {
    assert_eq!(AUTO_OFF_30M, 1_800_000);
    assert_eq!(AUTO_OFF_1H, 3_600_000);
    let mut k = KeepAwake::new();
    k.set_auto_off(Some(AUTO_OFF_30M), 1000);
    assert_eq!(k.auto_off_at_ms(), Some(1_801_000));
    k.set_auto_off(None, 0);
    assert_eq!(k.auto_off_at_ms(), None);
    k.set_auto_off_at_ms(Some(50));
    assert_eq!(k.auto_off_at_ms(), Some(50));
}

#[test]
fn keepawake_turn_off_clears_autooff() {
    let mut k = KeepAwake::new();
    k.turn_on(0, "x");
    k.inhibited(1);
    k.set_auto_off_at_ms(Some(50));
    k.turn_off();
    assert_eq!(k.auto_off_at_ms(), None);
    assert_eq!(k.state(), KeepAwakeState::Off);
}

#[test]
fn keepawake_inhibited_from_off_releases() {
    let mut k = KeepAwake::new();
    assert_eq!(k.inhibited(4), Some(Action::UnInhibit { cookie: 4 }));
    assert_eq!(k.state(), KeepAwakeState::Off);
}

#[test]
fn tailscale_peer_missing_online_is_unknown_not_offline_and_sorts_between() {
    let json = r#"{"BackendState":"Running","Self":{"HostName":"me"},"Peer":{
        "a":{"HostName":"off","Online":false,"OS":"linux"},
        "b":{"HostName":"unk","OS":"linux"},
        "c":{"HostName":"on","Online":true,"OS":"linux"}}}"#;
    let (tile, _) = parse_tailscale_status_report(json).unwrap();
    let seen: Vec<_> = tile
        .peers
        .iter()
        .map(|p| (p.host_name.as_str(), p.online))
        .collect();
    assert_eq!(
        seen,
        vec![("on", Some(true)), ("unk", None), ("off", Some(false))]
    );
    assert_eq!(tile.online_count, 1);
}
