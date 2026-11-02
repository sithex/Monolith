"""Phase 6b2 compiling behavior mutations, kept separate from accepted earlier belts.

Row = (label, file under src-tauri, old anchor(s), new text(s), test target, exact test name).
"""
def require_complete_coverage(cases):
    paths = {c[1] for c in cases}
    for family in ('mixer', 'brightness', 'toggles', 'tiles', 'agents', 'updates'):
        assert any(p.startswith(f'src/integrations/{family}/core/') for p in paths), f'missing {family} core boundary mutation'
    for path in ('src/integrations/toggles/core/mic.rs',
                 'src/integrations/tiles/core/keepawake.rs',
                 'src/integrations/updates/core/argv.rs'):
        assert path in paths, f'missing mandatory mic/cookie/read-only argv mutation: {path}'


CASES = [
    # --- carried 6b1 items ---------------------------------------------------------------
    ('calendar-userinfo-url-accepted', 'src/integrations/calendar/mod.rs',
     '            || !url.username().is_empty()\n', '',
     'carried6b2', 'calendar_secret_with_userinfo_is_rejected_and_nothing_is_fetched'),
    ('downloads-open-skips-model-membership', 'src/integrations/downloads/mod.rs',
     '!state.available || !state.model.items(self.now()).iter().any(|i| i.name == name)',
     '!state.available',
     'carried6b2', 'downloads_open_refuses_a_regular_file_that_is_not_a_visible_model_item'),
    ('calendar-local-zone-is-utc', 'src/integrations/calendar/mod.rs',
     '        offset_in(&self.local, n)\n', '        {\n            let _ = n;\n            Some(0)\n        }\n',
     'carried6b2', 'calendar_published_default_window_uses_the_local_zone_not_utc_offset_zero'),
    ('calendar-tzid-ignored', 'src/integrations/calendar/mod.rs',
     'offset_in(&self.named(tzid)?, n)', '{ let _ = tzid; offset_in(&self.local, n) }',
     'carried6b2', 'calendar_floating_zoned_and_all_day_resolve_dst_from_the_tz_database'),
    ('calendar-system-zone-ignored', 'src/integrations/calendar/mod.rs',
     'None => jiff::tz::TimeZone::try_system()', 'None => Ok::<_, jiff::Error>(jiff::tz::TimeZone::UTC)',
     'carried6b2', 'calendar_default_config_uses_the_system_zone_from_tz_env_in_a_child'),
    ('calendar-all-day-fixed-24h', 'src/integrations/calendar/core/agenda.rs',
     'naive_to_unix(next) - ctx.offset(start_kind, next) as i64\n                } else {',
     'cs + 86_400 + 0 * (naive_to_unix(next) - ctx.offset(start_kind, next) as i64)\n                } else {',
     'carried6b2', 'calendar_all_day_events_span_local_midnights_across_the_dst_change'),
    ('calendar-gap-offset-post-transition', 'src/integrations/calendar/mod.rs',
     'i32::try_from(core::naive_to_unix(n) - ts.as_second()).ok()',
     'Some(tz.to_offset(ts).seconds() + 0 * core::naive_to_unix(n) as i32)',
     'carried6b2', 'calendar_dst_gap_advances_and_fold_chooses_the_first_instant'),
    ('calendar-unknown-tzid-accepted', 'src/integrations/calendar/mod.rs',
     'if self.named(tzid).is_none() {', 'if self.named(tzid).is_none() && false {',
     'carried6b2', 'calendar_unknown_tzid_is_rejected_without_fabricating_a_local_instant'),
    ('calendar-recurring-end-reuses-master-offset', 'src/integrations/calendar/core/agenda.rs',
     'naive_to_unix(end_naive) - ctx.offset(&end.kind, end_naive) as i64',
     'naive_to_unix(end_naive) - ctx.offset(&end.kind, end.naive) as i64',
     'carried6b2', 'calendar_recurrences_recompute_local_midnights_and_end_offsets_per_occurrence'),
    # --- ported cores (port notes) ---------------------------------------------------------
    ('agents-null-ended-alone-means-running', 'src/integrations/agents/core/classify.rs',
     '} else if active {', '} else if active || row.ended_at.is_none() {',
     'agents6b2', 'agents_fixture_database_is_read_only_orphans_are_stale_and_completion_survives_stalling'),
    ('agents-clock-skew-boundary-lowered', 'src/integrations/agents/core/classify.rs',
     'pub const CLOCK_SKEW_SECS: f64 = 300.0;', 'pub const CLOCK_SKEW_SECS: f64 = 299.0;',
     'agents_core_agent_tracker_tests', 'classify_future_skew_boundary'),
    ('agents-completion-forgets-stalled-runs', 'src/integrations/agents/core/tracker.rs',
     '&& self.seen_running.contains(id)', '&& self.prev_running.contains(id)',
     'agents6b2', 'agents_core_finished_after_stale_has_one_completion_and_no_startup_history_spam'),
    ('brightness-settle-not-polled', 'src/integrations/brightness/service.rs',
     'for request in model.poll(self.now()) {', 'for request in model.poll(0) {',
     'brightness6b2', 'brightness_settle_deadline_polls_without_any_followup_signal_or_command'),
    ('brightness-throttle-boundary-lowered', 'src/integrations/brightness/core/model.rs',
     'pub const THROTTLE_MS: u64 = 50;', 'pub const THROTTLE_MS: u64 = 49;',
     'brightness_core_brightness_tests', 'throttle_boundaries_latest_wins'),
    ('brightness-rogue-value-accepted', 'src/integrations/brightness/service.rs',
     '|| header.sender().map(|s| s.as_str()) != Some(owner)',
     '|| (header.sender().map(|s| s.as_str()) != Some(owner) && header.member().map(|m|m.as_str()) != Some("BrightnessChanged"))',
     'brightness6b2', 'brightness_rogue_brightness_changed_is_ignored'),
    ('brightness-rogue-range-accepted', 'src/integrations/brightness/service.rs',
     '|| header.sender().map(|s| s.as_str()) != Some(owner)',
     '|| (header.sender().map(|s| s.as_str()) != Some(owner) && header.member().map(|m|m.as_str()) != Some("BrightnessRangeChanged"))',
     'brightness6b2', 'brightness_rogue_range_changed_is_ignored'),
    ('brightness-rogue-added-accepted', 'src/integrations/brightness/service.rs',
     '|| header.sender().map(|s| s.as_str()) != Some(owner)',
     '|| (header.sender().map(|s| s.as_str()) != Some(owner) && header.member().map(|m|m.as_str()) != Some("DisplayAdded"))',
     'brightness6b2', 'brightness_rogue_display_added_is_ignored'),
    ('brightness-rogue-removed-accepted', 'src/integrations/brightness/service.rs',
     '|| header.sender().map(|s| s.as_str()) != Some(owner)',
     '|| (header.sender().map(|s| s.as_str()) != Some(owner) && header.member().map(|m|m.as_str()) != Some("DisplayRemoved"))',
     'brightness6b2', 'brightness_rogue_display_removed_is_ignored'),
    ('brightness-rogue-owner-accepted', 'src/integrations/brightness/service.rs',
     ["type='signal',sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',member='NameOwnerChanged',arg0='org.kde.ScreenBrightness'",
      '|| message.header().sender().map(|s|s.as_str()) != Some(DBUS)'],
     ["type='signal',interface='org.freedesktop.DBus',member='NameOwnerChanged',arg0='org.kde.ScreenBrightness'",
      '|| (message.header().sender().map(|s|s.as_str()) != Some(DBUS) && false)'],
     'brightness6b2', 'brightness_rogue_name_owner_changed_is_ignored'),
    ('tailscale-missing-online-reported-false', 'src/integrations/tiles/core/parse.rs',
     'online: get_bool(obj, "Online"),', 'online: get_bool(obj, "Online").or(Some(false)),',
     'tiles_core_system_tiles_tests', 'tailscale_peer_missing_online_is_unknown_not_offline_and_sorts_between'),
]

from phase6b2_extra_cases import EXTRA_CASES
CASES += EXTRA_CASES
