#!/usr/bin/env python3
"""Require actual named runtime acceptance, not just cargo's exit status (Phase 6b2 + chain)."""
import re,sys
from pathlib import Path
text=Path(sys.argv[1]).read_text()
names=[
# 6b1 scenarios (must keep running)
'scratchpad_debounce_atomic_private_reload_and_caps','downloads_real_inotify_add_rename_delete_and_only_injected_opener',
'calendar_secret_url_is_never_in_topic_or_error_or_cache','apps_commands_events_and_secret_never_cross_ipc_logs_or_status',
# 6b2 carried items (1)-(3)
'calendar_secret_with_userinfo_is_rejected_and_nothing_is_fetched',
'downloads_open_refuses_a_regular_file_that_is_not_a_visible_model_item',
'calendar_floating_zoned_and_all_day_resolve_dst_from_the_tz_database',
'calendar_all_day_events_span_local_midnights_across_the_dst_change',
'calendar_local_zone_follows_the_injected_zone_not_the_client_offset',
'calendar_published_default_window_uses_the_local_zone_not_utc_offset_zero',
'calendar_unknown_tzid_is_rejected_without_fabricating_a_local_instant',
'calendar_default_config_uses_the_system_zone_from_tz_env_in_a_child',
'calendar_dst_gap_advances_and_fold_chooses_the_first_instant',
'calendar_recurrences_recompute_local_midnights_and_end_offsets_per_occurrence',
'calendar_invalid_zone_configuration_is_unavailable_not_silently_utc',
# ported-core port notes
'tailscale_peer_missing_online_is_unknown_not_offline_and_sorts_between',
# brightness adapter and IPC, against a private PowerDevil only
'brightness_private_methods_events_loss_recovery_and_shutdown',
'brightness_settle_deadline_polls_without_any_followup_signal_or_command',
'brightness_rogue_brightness_changed_is_ignored',
'brightness_rogue_range_changed_is_ignored',
'brightness_rogue_display_added_is_ignored',
'brightness_rogue_display_removed_is_ignored',
'brightness_rogue_name_owner_changed_is_ignored',
'brightness_failed_write_is_unavailable_and_recovers_without_optimistic_stale_value',
'brightness_ipc_event_and_module_disable_reenable_use_only_the_private_daemon',
'agents_fixture_database_is_read_only_orphans_are_stale_and_completion_survives_stalling',
'agents_database_loss_and_recovery_emit_unavailable_without_false_completions',
'agents_test_instance_without_fixture_never_infers_a_home_database',
'agents_worker_is_bounded_and_shutdown_wakes_the_fifteen_second_wait',
'agents_started_finished_reach_the_private_hub_and_activity_and_clear_on_shutdown',
'agents_ipc_fixture_poll_event_and_module_cleanup_are_private',
# The gate must fail closed until the rest of Controls has real adapter acceptance.
'mixer_private_subscribe_commands_loss_and_recovery',
'toggles_private_power_profile_mic_dnd_and_optimistic_timeout',
'tiles_private_keepawake_output_network_colour_screenshot_and_voice',
'updates_private_read_only_argv_exit_codes_schedule_and_recovery',
'remaining_controls_ipc_events_commands_bootstrap_and_module_cleanup_are_private',
'toggles_rogue_properties_changed_is_ignored','toggles_rogue_name_owner_changed_is_ignored',
'network_rogue_properties_changed_is_ignored','network_rogue_name_owner_changed_is_ignored',
'screensaver_rogue_name_owner_changed_is_ignored','picker_rogue_name_owner_changed_is_ignored',
'tiles_keepawake_deadline_releases_without_another_command',
'agents_failed_finish_delivery_retries_once_without_losing_the_transition',
'agents_outbox_cap_refuses_before_advancing_and_recovers_when_the_batch_shrinks',
]
for name in names:
    assert re.search(r'^test (?:[\w]+::)*'+re.escape(name)+r' \.\.\. ok$',text,re.M),f'{name} did not run/pass'
assert not re.search(r'^test .* \.\.\. FAILED$',text,re.M)
print(f'PASS: {len(names)} named Phase 6b2 acceptance scenarios actually ran and passed')
