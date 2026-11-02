#!/usr/bin/env python3
"""Phase 6m: the named acceptance tests must have run and passed in the whole-suite log."""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
text = Path(sys.argv[1] if len(sys.argv) > 1 else ROOT / 'evidence/phase6m/gate-all-tests.log').read_text()
NAMES = [
    # the four carried-over Phase 6a gaps
    'power_ignores_a_spoofed_name_owner_changed_from_a_rogue_peer',
    'bluetooth_ignores_a_spoofed_name_owner_changed_from_a_rogue_peer',
    'bluetooth_interfaces_removed_is_accepted_only_from_the_root_object',
    'weather_late_first_place_answer_is_never_shown_when_the_second_place_fails',
    'weather_enabled_without_a_location_reports_no_location_not_fetching',
    # media (MPRIS against private-bus mocks)
    'media_initial_snapshot_property_change_event_seeked_and_quit_clears',
    'media_without_players_is_empty_and_available_and_without_a_bus_it_says_why',
    'media_prefers_the_most_recently_playing_player',
    'media_control_reaches_only_the_preferred_mock_and_only_advertised_capabilities',
    'media_position_is_extrapolated_in_milliseconds_at_the_players_rate',
    'media_ignores_properties_changed_from_a_rogue_sender',
    'media_ignores_seeked_from_a_rogue_sender',
    'media_ignores_a_spoofed_name_owner_changed_from_a_rogue_peer',
    'media_bus_loss_clears_the_state_and_says_why',
    # lyrics (local LRCLIB fixture)
    'lyrics_are_off_by_default_and_make_no_request_or_cache',
    'synced_lyrics_follow_the_mpris_position_seek_and_pause_and_are_cached_privately',
    'plain_instrumental_not_found_array_wrong_type_and_rate_limit_are_each_classified',
    'json_array_body_is_rejected_not_read_as_a_positional_record',
    # equalizer (fake tools + a private PipeWire instance)
    'capture_runs_only_while_enabled_visible_and_playing_and_dies_when_any_stops',
    'frames_are_emitted_at_most_thirty_times_per_second',
    'argv_names_the_default_sink_monitor_or_the_playing_stream_and_never_a_source',
    'no_resolvable_target_means_no_capture_and_a_reason_never_a_default_source',
    'a_capture_that_exits_on_its_own_is_reported_and_retried_without_stale_bands',
    'private_pipewire_captures_only_the_playing_stream_and_the_sink_monitor_hears_both',
    # contract + real app on nested KWin
    'music_settings_are_opt_in_validated_and_round_trip',
    'mpris_bus_override_applies_to_test_instances_only_and_tests_never_use_the_real_bus',
    'media_lyrics_and_equalizer_work_end_to_end_in_the_real_app',
    'a_test_instance_without_a_mock_bus_has_an_empty_media_state_and_stays_honest',
]
missing = [n for n in NAMES if not re.search(rf'^test (?:\S+::)?{n} \.\.\. ok$', text, re.M)]
assert not missing, f'missing Phase 6m acceptance tests: {missing}'
assert 'test result: FAILED' not in text, 'a test binary failed'
print(f'PASS: {len(NAMES)} named Phase 6m acceptance tests ran and passed')
