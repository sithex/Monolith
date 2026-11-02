"""Remaining Controls mutation rows; only private mocks/fakes are ever executed."""
EXTRA_CASES: list[tuple[str, str, str | list[str], str | list[str], str, str]] = [
    ('mixer-volume-upper-bound-lowered','src/integrations/mixer/core/cmd.rs',
     'pub const MAX_PERCENT: u32 = 150;','pub const MAX_PERCENT: u32 = 149;',
     '--lib','integrations::mixer::core::cmd::tests::percent_bounds'),
    ('mic-mute-targets-named-device','src/integrations/toggles/core/mic.rs',
     '"set-source-mute".to_string(),\n        DEFAULT_SOURCE_ARG.to_string(),',
     '"set-source-mute".to_string(),\n        "fixture-named-device".to_string(),',
     'toggles6b2','toggles_private_power_profile_mic_dnd_and_optimistic_timeout'),
    ('toggles-pending-expires-one-ms-early','src/integrations/toggles/core/toggle.rs',
     'now_ms.saturating_sub(p.sent_at_ms) >= PENDING_TIMEOUT_MS',
     'now_ms.saturating_sub(p.sent_at_ms).saturating_add(1) >= PENDING_TIMEOUT_MS',
     'toggles_core_toggle_tests','toggle_tick_timeout_drops_pending'),
    ('keepawake-core-drops-release','src/integrations/tiles/core/keepawake.rs',
     'let cookie = *cookie;\n                self.state = KeepAwakeState::Off;\n                Some(Action::UnInhibit { cookie })',
     'let cookie = *cookie;\n                self.state = KeepAwakeState::Off;\n                let _ = cookie; None',
     'tiles_core_system_tiles_tests','keepawake_turn_off_from_on_releases'),
    ('keepawake-adapter-skips-cookie-release','src/integrations/tiles/service.rs',
     'async fn release(&self) -> Result<(), IslandError> {',
     'async fn release(&self) -> Result<(), IslandError> { if self.cookie > 0 { return Ok(()); }',
     'tiles6b2','tiles_private_keepawake_output_network_colour_screenshot_and_voice'),
    ('updates-argv-gains-write-flag','src/integrations/updates/core/argv.rs',
     'pub const PARU_QUA_ARGV: [&str; 2] = ["paru", "-Qua"];',
     'pub const PARU_QUA_ARGV: [&str; 2] = ["paru", "-Syu"];',  # controller amendment: same-length write flag (3-element form did not compile)
     'updates_core_updates_tests','argv_never_sudo_or_upgrade'),
    ('updates-count-includes-ignored','src/integrations/updates/core/model.rs',
     'updates.iter().filter(|u| !u.ignored).count() as u32',
     'updates.iter().filter(|u| u.ignored || !u.ignored).count() as u32',
     'updates_core_updates_tests','badge_count_excludes_ignored'),
    ('agents-outbox-drops-failed-delivery','src/integrations/agents/service.rs',
     'if let Err(error) = self.sink.deliver(&event) {',
     'if let Err(error) = self.sink.deliver(&event) { self.state.lock().map_err(|_|IslandError::internal("Agent state poisoned"))?.outbox.pop_front();',
     'agents6b2','agents_failed_finish_delivery_retries_once_without_losing_the_transition'),
    ('agents-outbox-cap-lowered','src/integrations/agents/service.rs',
     'if update.events.len() > 128 {','if update.events.len() > 127 {',
     'agents6b2','agents_outbox_cap_refuses_before_advancing_and_recovers_when_the_batch_shrinks'),
]
# Same protocol signal type at separate observed interfaces; each consumer has its own row.
for name, test, target in [
    ('PPD','toggles_rogue_properties_changed_is_ignored','toggles6b2'),
    ('NM','network_rogue_properties_changed_is_ignored','toggles6b2'),
]:
    EXTRA_CASES.append((f'{name.lower()}-rogue-properties-accepted','src/integrations/control_dbus.rs',
        'let authenticated = owner.is_some() && sender == owner;',
        f'let authenticated = owner.is_some() && (sender == owner || name == {name});',target,test))
# org.freedesktop.DBus is filtered client-side by zbus; remove BOTH defences. Change the
# in-code predicate only for the target service, so each row proves that consumer's test.
for name, test, target in [
    ('PPD','toggles_rogue_name_owner_changed_is_ignored','toggles6b2'),
    ('NM','network_rogue_name_owner_changed_is_ignored','toggles6b2'),
    ('SCREENSAVER','screensaver_rogue_name_owner_changed_is_ignored','tiles6b2'),
    ('KWIN','picker_rogue_name_owner_changed_is_ignored','tiles6b2'),
]:
    EXTRA_CASES.append((f'{name.lower()}-rogue-owner-accepted','src/integrations/control_dbus.rs',
        ["type='signal',sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',member='NameOwnerChanged',arg0='{name}'",
         'message.header().sender().map(|s|s.as_str())!=Some(DBUS)'],
        ["type='signal',interface='org.freedesktop.DBus',member='NameOwnerChanged',arg0='{name}'",
         f'(message.header().sender().map(|s|s.as_str())!=Some(DBUS) && name!={name})'],target,test))
