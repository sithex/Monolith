"""Phase 6m mutation rows. (label, file under src-tauri, old text, new text, test target or
'--lib', exact test name). Each row is ONE compiling behaviour change; no size/count/time cap
is ever removed (caps are lowered)."""

CASES = [
    # --- the four carried-over Phase 6a gaps (the spoof gap is one row per service) ---
    # NOTE (finding): removing ONLY the in-code `sender != DBUS` check is an EQUIVALENT mutation:
    # zbus parses "org.freedesktop.DBus" as a UNIQUE name (zbus_names special case), so the match
    # rule's sender='org.freedesktop.DBus' IS filtered client-side, and the rogue unicast never
    # reaches the stream (verified with an unfiltered MessageStream: the message arrives at the
    # connection, not at the rule's stream). The two spoof rows therefore remove BOTH layers
    # (rule sender + in-code check); the rogue-peer test kills that and pins the observable
    # behaviour of the combined defence.
    ('power-spoofed-name-owner-changed-accepted', 'src/integrations/power/upower.rs',
     ["Some(DBUS) {\n                    continue;", "sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',member='NameOwnerChanged'"],
     ["Some(DBUS) && false {\n                    continue;", "interface='org.freedesktop.DBus',member='NameOwnerChanged'"],
     'power_service', 'power_ignores_a_spoofed_name_owner_changed_from_a_rogue_peer'),
    ('bluetooth-spoofed-name-owner-changed-accepted', 'src/integrations/bluetooth/bluez.rs',
     ["Some(DBUS) {\n                    continue;", "sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',member='NameOwnerChanged'"],
     ["Some(DBUS) && false {\n                    continue;", "interface='org.freedesktop.DBus',member='NameOwnerChanged'"],
     'bluetooth_service', 'bluetooth_ignores_a_spoofed_name_owner_changed_from_a_rogue_peer'),
    ('bluetooth-non-root-interfaces-removed-accepted', 'src/integrations/bluetooth/bluez.rs',
     '(IFACE_OBJECT_MANAGER, "InterfacesRemoved") => {\n            if path != "/" {',
     '(IFACE_OBJECT_MANAGER, "InterfacesRemoved") => {\n            if path != "/" && false {',
     'bluetooth_service', 'bluetooth_interfaces_removed_is_accepted_only_from_the_root_object'),
    ('weather-superseded-response-accepted', 'src/integrations/weather/mod.rs',
     'if shared.generation != generation {', 'if false {',
     'weather_service', 'weather_late_first_place_answer_is_never_shown_when_the_second_place_fails'),
    ('weather-no-location-reported-as-fetching', 'src/integrations/weather/mod.rs',
     'self.enabled && self.location.is_some()', 'self.enabled',
     'weather_service', 'weather_enabled_without_a_location_reports_no_location_not_fetching'),
    # --- MPRIS (rows 6-10) ---
    ('mpris-name-lost-not-reported', 'src/integrations/media/mpris.rs',
     '                if new.is_empty() {\n                    service.player_removed(&name);',
     '                if new.is_empty() {\n                    service.status.available("mpris");',
     'media_service', 'media_initial_snapshot_property_change_event_seeked_and_quit_clears'),
    ('mpris-properties-changed-sender-unchecked', 'src/integrations/media/mpris.rs',
     'let bus = trusted_player(service, message)?;',
     'let bus = String::from("org.mpris.MediaPlayer2.alpha");',
     'media_service', 'media_ignores_properties_changed_from_a_rogue_sender'),
    ('mpris-seeked-sender-unchecked', 'src/integrations/media/mpris.rs',
     'let Some(bus) = trusted_player(service, message) else {\n        return;\n    };',
     'let bus = String::from("org.mpris.MediaPlayer2.alpha");',
     'media_service', 'media_ignores_seeked_from_a_rogue_sender'),
    # Both layers, as for the 6a spoof rows (the rule's sender is a unique-name match in zbus).
    ('mpris-name-owner-changed-sender-unchecked', 'src/integrations/media/mpris.rs',
     ["Some(DBUS) {\n                    continue;", "sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',member='NameOwnerChanged'"],
     ["Some(DBUS) && false {\n                    continue;", "interface='org.freedesktop.DBus',member='NameOwnerChanged'"],
     'media_service', 'media_ignores_a_spoofed_name_owner_changed_from_a_rogue_peer'),
    ('mpris-position-clock-off-by-unit', 'src/integrations/media/mod.rs',
     'self.origin.elapsed().as_millis() as u64', 'self.origin.elapsed().as_micros() as u64',
     'media_service', 'media_position_is_extrapolated_in_milliseconds_at_the_players_rate'),
    # --- lyrics ---
    ('lrclib-array-body-accepted', 'src/integrations/media/lrclib.rs',
     'Ok(value) if value.is_object() => serde_json::from_value::<LyricsRecord>(value),',
     'Ok(value) => serde_json::from_value::<LyricsRecord>(value),',
     'lrclib_core_parse_response', 'json_array_body_is_rejected_not_read_as_a_positional_record'),
    ('lyrics-fetch-while-disabled', 'src/integrations/media/lyrics.rs',
     'let on = *enabled.borrow_and_update();',
     'let on = *enabled.borrow_and_update() || true;',
     'lyrics_service', 'lyrics_are_off_by_default_and_make_no_request_or_cache'),
    ('lyrics-next-line-timer-doubled', 'src/integrations/media/lyrics.rs',
     'Duration::from_millis(wait_ms.ceil() as u64 + 1)',
     'Duration::from_millis(wait_ms.ceil() as u64 * 2 + 1)',
     'lyrics_service', 'synced_lyrics_follow_the_mpris_position_seek_and_pause_and_are_cached_privately'),
    # --- equalizer ---
    ('equalizer-keeps-capturing-while-hidden', 'src/integrations/media/equalizer.rs',
     'self.enabled && self.visible && self.hint.is_some()', 'self.enabled && self.hint.is_some()',
     'equalizer_service', 'capture_runs_only_while_enabled_visible_and_playing_and_dies_when_any_stops'),
    ('equalizer-emit-gap-lowered', 'src/integrations/media/equalizer.rs',
     'pub const MIN_EMIT_GAP: Duration = Duration::from_millis(34);',
     'pub const MIN_EMIT_GAP: Duration = Duration::from_millis(4);',
     'equalizer_service', 'frames_are_emitted_at_most_thirty_times_per_second'),
    ('equalizer-argv-without-explicit-target', 'src/integrations/media/equalizer.rs',
     'let mut args = vec!["--target".to_owned(), target.arg.clone()];', 'let mut args: Vec<String> = Vec::new();',
     'equalizer_service', 'argv_names_the_default_sink_monitor_or_the_playing_stream_and_never_a_source'),
    ('equalizer-stream-lookup-accepts-any-media-class', 'src/integrations/media/equalizer.rs',
     'if props["media.class"].as_str() != Some("Stream/Output/Audio") {',
     'if props["media.class"].as_str() != Some("Stream/Output/Audio") && false {',
     '--lib', 'integrations::media::equalizer::tests::stream_lookup_matches_pid_first_then_name_and_only_output_streams'),
    # --- one boundary in each ported staged core ---
    ('mpris-core-length-microseconds-divisor', 'src/integrations/media/model.rs',
     'Some(us / 1000)', 'Some(us / 100)',
     'mpris_core_parse', 'metadata_length_microseconds_to_ms'),
    ('lrclib-core-duration-upper-bound', 'src/integrations/media/lrclib.rs',
     '(1..=3600).contains(&secs)', '(1..=3601).contains(&secs)',
     '--lib', 'integrations::media::lrclib::tests::duration_seconds_rounds_nearest_500_up'),
    ('lrc-core-line-active-at-its-exact-start', 'src/integrations/media/lrc.rs',
     '.partition_point(|l| (l.start_ms as i128) <= bound);', '.partition_point(|l| (l.start_ms as i128) < bound);',
     'lrc_core_parse', 'active_index_boundaries'),
    ('spectrum-core-bands-ready-boundary', 'src/integrations/media/spectrum.rs',
     'if self.total < n as u64 {', 'if self.total <= n as u64 {',
     'spectrum_core', 'none_until_fft_size_samples'),
]
