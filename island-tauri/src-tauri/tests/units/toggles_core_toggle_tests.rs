use hermes_island_lib::integrations::toggles::core::{ToggleState, PENDING_TIMEOUT_MS};

#[test]
fn toggle_request_shows_pending() {
    let mut s = ToggleState::new();
    let id = s.request(true, 1000);
    assert!(id >= 1);
    assert_eq!(s.display(), Some(true));
}

#[test]
fn toggle_confirm_equal_clears_pending() {
    let mut s = ToggleState::new();
    s.request(true, 1000);
    s.confirm(true, 1500);
    assert_eq!(s.display(), Some(true));
}

#[test]
fn toggle_confirm_different_keeps_pending() {
    let mut s = ToggleState::new();
    s.request(true, 1000);
    s.confirm(false, 1500);
    // The optimistic pending value still wins until it times out.
    assert_eq!(s.display(), Some(true));
}

#[test]
fn toggle_fail_stale_id_ignored() {
    let mut s = ToggleState::new();
    let id1 = s.request(true, 1000);
    s.request(false, 1100); // replaces pending
    s.fail(id1, 1200); // stale id must not clear the newer pending
    assert_eq!(s.display(), Some(false));
}

#[test]
fn toggle_tick_timeout_drops_pending() {
    let mut s = ToggleState::new();
    s.request(true, 1000);
    s.tick(1000 + PENDING_TIMEOUT_MS - 1); // 2999 ms: kept
    assert_eq!(s.display(), Some(true));
    s.tick(1000 + PENDING_TIMEOUT_MS); // 3000 ms: dropped -> fall back
    assert_eq!(s.display(), None);
}

#[test]
fn toggle_new_request_replaces_pending_visible() {
    let mut s = ToggleState::new();
    s.request(true, 1000);
    s.request(false, 1100);
    assert_eq!(s.display(), Some(false));
}
