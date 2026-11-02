#!/usr/bin/env python3
"""Require actual named runtime acceptance, not just cargo's exit status."""
import re,sys
from pathlib import Path
text=Path(sys.argv[1]).read_text()
names=[
'cache_roundtrips_private_files_and_ignores_everything_else',
'scratchpad_debounce_atomic_private_reload_and_caps','scratchpad_saved_file_is_private_even_when_replacing_a_wide_target',
'scratchpad_permission_failure_preserves_the_original_file',
'downloads_real_inotify_add_rename_delete_and_only_injected_opener','downloads_missing_injected_directory_is_unavailable_not_real_home',
'calendar_local_create_edit_delete_are_atomic_private_and_reload','calendar_subscribed_events_cannot_be_edited_even_when_uid_collides',
'calendar_subscription_loopback_merges_and_failure_keeps_stale_with_age','calendar_secret_url_is_never_in_topic_or_error_or_cache',
'calendar_body_limit_rejects_one_extra_byte_in_chunked_response','calendar_timeout_is_bounded_and_has_no_fabricated_last_success',
'calendar_group_only_url_file_is_rejected_before_any_request','calendar_port_rejects_utf8_in_fixed_width_dates_without_panicking',
'module_registry_maps_flat_keys_and_preserves_explicit_inert_unknowns',
'timer_completion_reaches_own_hub_and_activity_after_view_switch','timer_actor_pomodoro_phases_stopwatch_and_cancel_use_only_injected_clock',
'apps_commands_events_and_secret_never_cross_ipc_logs_or_status',
]
for name in names:
    assert re.search(r'^test (?:[\w]+::)*'+re.escape(name)+r' \.\.\. ok$',text,re.M),f'{name} did not run/pass'
assert not re.search(r'^test .* \.\.\. FAILED$',text,re.M)
print(f'PASS: {len(names)} named Phase 6b1 acceptance scenarios actually ran and passed')
