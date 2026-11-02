use launcher_arrange_core::{
    ArrangeCommit, ArrangeError, Arrangement, EnabledChange, GridItem, MAX_ID_BYTES, MAX_ITEMS,
};

fn item(id: &str, enabled: bool, locked: bool) -> GridItem {
    GridItem {
        id: id.to_string(),
        enabled,
        locked,
    }
}

fn start(items: Vec<GridItem>) -> Arrangement {
    match Arrangement::new(items) {
        Ok(a) => a,
        Err(e) => panic!("fixture failed: {e}"),
    }
}

fn begin(a: &mut Arrangement) {
    match a.begin_edit() {
        Ok(()) => {}
        Err(e) => panic!("begin failed: {e}"),
    }
}

fn vis_ids(a: &Arrangement) -> Vec<String> {
    a.visible().into_iter().map(|g| g.id.clone()).collect()
}

fn hid_ids(a: &Arrangement) -> Vec<String> {
    a.hidden().into_iter().map(|g| g.id.clone()).collect()
}

#[test]
fn new_rejects_empty_ids() {
    assert_eq!(Arrangement::new(vec![]), Err(ArrangeError::EmptyIds));
}

#[test]
fn new_rejects_id_longer_than_64_bytes() {
    let long = "x".repeat(MAX_ID_BYTES + 1);
    assert_eq!(
        Arrangement::new(vec![item(&long, true, false)]),
        Err(ArrangeError::IdTooLong)
    );
    // 33 two-byte chars = 66 bytes
    let uni = "é".repeat(33);
    assert_eq!(
        Arrangement::new(vec![item(&uni, true, false)]),
        Err(ArrangeError::IdTooLong)
    );
}

#[test]
fn new_accepts_boundary_ids() {
    let ok64 = "x".repeat(MAX_ID_BYTES);
    let uni64 = "é".repeat(32); // exactly 64 bytes
    let a = start(vec![item(&ok64, true, false), item(&uni64, false, false)]);
    assert_eq!(vis_ids(&a), vec![ok64]);
    assert_eq!(hid_ids(&a), vec![uni64]);
}

#[test]
fn new_rejects_duplicate_ids() {
    assert_eq!(
        Arrangement::new(vec![item("a", true, false), item("a", false, false)]),
        Err(ArrangeError::DuplicateId)
    );
}

#[test]
fn new_rejects_more_than_256_items() {
    let items: Vec<GridItem> = (0..=MAX_ITEMS)
        .map(|i| item(&format!("i{i}"), true, false))
        .collect();
    assert_eq!(Arrangement::new(items), Err(ArrangeError::TooManyItems));
}

#[test]
fn new_accepts_exactly_256_items() {
    let items: Vec<GridItem> = (0..MAX_ITEMS)
        .map(|i| item(&format!("i{i}"), true, false))
        .collect();
    let a = start(items);
    assert_eq!(a.visible().len(), MAX_ITEMS);
}

#[test]
fn new_error_precedence_length_then_dup_then_cap() {
    // too-long (second item) fires before the duplicate (third item)
    assert_eq!(
        Arrangement::new(vec![
            item("a", true, false),
            item(&"x".repeat(65), true, false),
            item("a", false, false),
        ]),
        Err(ArrangeError::IdTooLong)
    );
    // duplicate fires before the too-many cap
    let mut items: Vec<GridItem> = (0..MAX_ITEMS)
        .map(|i| item(&format!("i{i}"), true, false))
        .collect();
    items.push(item("i0", true, false));
    assert_eq!(Arrangement::new(items), Err(ArrangeError::DuplicateId));
}

#[test]
fn new_forces_locked_items_enabled() {
    let a = start(vec![item("locked-off", false, true)]);
    assert_eq!(vis_ids(&a), vec!["locked-off".to_string()]);
    assert!(a.hidden().is_empty());
}

#[test]
fn visible_lists_enabled_in_order() {
    let a = start(vec![
        item("a", true, false),
        item("b", false, false),
        item("c", true, false),
        item("d", false, false),
    ]);
    assert_eq!(vis_ids(&a), vec!["a", "c"]);
    assert_eq!(hid_ids(&a), vec!["b", "d"]);
}

#[test]
fn begin_edit_snapshots_and_flags_editing() {
    let mut a = start(vec![item("a", true, false)]);
    assert!(!a.is_editing());
    assert_eq!(a.begin_edit(), Ok(()));
    assert!(a.is_editing());
    assert_eq!(a.begin_edit(), Err(ArrangeError::AlreadyEditing));
    a.cancel().unwrap();
    assert!(!a.is_editing());
}

#[test]
fn move_item_outside_session_returns_not_editing() {
    let mut a = start(vec![item("a", true, false), item("b", true, false)]);
    assert_eq!(a.move_item("a", 1), Err(ArrangeError::NotEditing));
}

#[test]
fn move_unknown_id_returns_unknown_id() {
    let mut a = start(vec![item("a", true, false)]);
    begin(&mut a);
    assert_eq!(a.move_item("zzz", 0), Err(ArrangeError::UnknownId));
}

#[test]
fn move_hidden_item_returns_not_visible() {
    let mut a = start(vec![item("a", true, false), item("b", false, false)]);
    begin(&mut a);
    assert_eq!(a.move_item("b", 0), Err(ArrangeError::NotVisible));
}

#[test]
fn move_to_zero_places_at_front() {
    let mut a = start(vec![
        item("a", true, false),
        item("b", true, false),
        item("c", true, false),
    ]);
    begin(&mut a);
    a.move_item("c", 0).unwrap();
    assert_eq!(vis_ids(&a), vec!["c", "a", "b"]);
}

#[test]
fn move_to_last_visible_slot() {
    let mut a = start(vec![
        item("a", true, false),
        item("b", true, false),
        item("c", true, false),
    ]);
    begin(&mut a);
    a.move_item("a", 2).unwrap();
    assert_eq!(vis_ids(&a), vec!["b", "c", "a"]);
}

#[test]
fn move_past_end_clamps_to_last_visible_slot() {
    let mut a = start(vec![
        item("a", true, false),
        item("b", true, false),
        item("c", true, false),
    ]);
    begin(&mut a);
    a.move_item("a", 999).unwrap();
    assert_eq!(vis_ids(&a), vec!["b", "c", "a"]);
}

#[test]
fn move_onto_itself_is_noop() {
    let mut a = start(vec![
        item("a", true, false),
        item("b", true, false),
        item("c", true, false),
    ]);
    begin(&mut a);
    a.move_item("b", 1).unwrap();
    assert_eq!(vis_ids(&a), vec!["a", "b", "c"]);
}

#[test]
fn move_onto_itself_via_clamp_is_noop() {
    let mut a = start(vec![
        item("a", true, false),
        item("b", true, false),
        item("c", true, false),
    ]);
    begin(&mut a);
    a.move_item("c", 999).unwrap(); // clamps to itself
    assert_eq!(vis_ids(&a), vec!["a", "b", "c"]);
}

#[test]
fn move_single_visible_item_is_noop() {
    let mut a = start(vec![item("a", true, false), item("h", false, false)]);
    begin(&mut a);
    a.move_item("a", 5).unwrap();
    assert_eq!(vis_ids(&a), vec!["a"]);
    assert_eq!(hid_ids(&a), vec!["h"]);
}

#[test]
fn move_with_hidden_items_interleaved_preserves_hidden_relative_order() {
    let mut a = start(vec![
        item("a", true, false),
        item("h1", false, false),
        item("b", true, false),
        item("h2", false, false),
        item("c", true, false),
    ]);
    begin(&mut a);
    a.move_item("c", 0).unwrap();
    assert_eq!(vis_ids(&a), vec!["c", "a", "b"]);
    assert_eq!(hid_ids(&a), vec!["h1", "h2"]);
    a.cancel().unwrap();
    begin(&mut a);
    a.move_item("b", 2).unwrap(); // last visible slot, across hidden items
    assert_eq!(vis_ids(&a), vec!["a", "c", "b"]);
    assert_eq!(hid_ids(&a), vec!["h1", "h2"]);
}

#[test]
fn move_past_end_clamps_to_last_visible_slot_across_hidden() {
    // moving "a" (first visible) past the end must land it in the last
    // visible slot (a becomes the second visible item) — the absolute end
    let mut a = start(vec![
        item("a", true, false),
        item("h1", false, false),
        item("b", true, false),
        item("h2", false, false),
    ]);
    begin(&mut a);
    a.move_item("a", 999).unwrap();
    let c = a.commit().unwrap().unwrap();
    assert_eq!(c.order, vec!["h1", "b", "h2", "a"]);
    assert_eq!(vis_ids(&a), vec!["b", "a"]);
    assert_eq!(hid_ids(&a), vec!["h1", "h2"]);
}

#[test]
fn disable_locked_returns_locked() {
    let mut a = start(vec![item("home", true, true), item("app", true, false)]);
    begin(&mut a);
    assert_eq!(a.disable("home"), Err(ArrangeError::Locked));
    assert_eq!(vis_ids(&a), vec!["home", "app"]);
}

#[test]
fn disable_already_disabled_is_noop() {
    let mut a = start(vec![item("a", false, false), item("b", true, false)]);
    begin(&mut a);
    a.disable("a").unwrap();
    assert_eq!(hid_ids(&a), vec!["a"]);
    assert_eq!(vis_ids(&a), vec!["b"]);
}

#[test]
fn disable_moves_item_to_hidden() {
    let mut a = start(vec![
        item("a", true, false),
        item("b", true, false),
        item("c", true, false),
    ]);
    begin(&mut a);
    a.disable("b").unwrap();
    assert_eq!(vis_ids(&a), vec!["a", "c"]);
    assert_eq!(hid_ids(&a), vec!["b"]);
}

#[test]
fn disable_outside_session_returns_not_editing() {
    let mut a = start(vec![item("a", true, false)]);
    assert_eq!(a.disable("a"), Err(ArrangeError::NotEditing));
}

#[test]
fn disable_unknown_id_returns_unknown_id() {
    let mut a = start(vec![item("a", true, false)]);
    begin(&mut a);
    assert_eq!(a.disable("zzz"), Err(ArrangeError::UnknownId));
}

#[test]
fn enable_puts_item_at_end_of_visible_grid() {
    let mut a = start(vec![
        item("a", true, false),
        item("b", true, false),
        item("h", false, false),
    ]);
    begin(&mut a);
    a.enable("h").unwrap();
    assert_eq!(vis_ids(&a), vec!["a", "b", "h"]);
    assert!(a.hidden().is_empty());
}

#[test]
fn enable_with_hidden_before_and_after_last_visible() {
    let mut a = start(vec![
        item("a", true, false),
        item("h1", false, false),
        item("b", true, false),
        item("h2", false, false),
    ]);
    begin(&mut a);
    a.enable("h1").unwrap();
    // h1 lands just after the last enabled item (b), i.e. before h2
    assert_eq!(vis_ids(&a), vec!["a", "b", "h1"]);
    assert_eq!(hid_ids(&a), vec!["h2"]);
}

#[test]
fn enable_already_enabled_is_noop() {
    let mut a = start(vec![item("a", true, false), item("h", false, false)]);
    begin(&mut a);
    a.enable("a").unwrap();
    assert_eq!(vis_ids(&a), vec!["a"]);
    assert_eq!(hid_ids(&a), vec!["h"]);
}

#[test]
fn enable_outside_session_returns_not_editing() {
    let mut a = start(vec![item("a", false, false)]);
    assert_eq!(a.enable("a"), Err(ArrangeError::NotEditing));
}

#[test]
fn enable_unknown_id_returns_unknown_id() {
    let mut a = start(vec![item("a", true, false)]);
    begin(&mut a);
    assert_eq!(a.enable("zzz"), Err(ArrangeError::UnknownId));
}

#[test]
fn cancel_restores_exact_state() {
    let mut a = start(vec![
        item("a", true, false),
        item("b", true, false),
        item("c", false, false),
    ]);
    begin(&mut a);
    a.move_item("a", 1).unwrap();
    a.disable("b").unwrap();
    a.enable("c").unwrap();
    a.cancel().unwrap();
    assert_eq!(vis_ids(&a), vec!["a", "b"]);
    assert_eq!(hid_ids(&a), vec!["c"]);
    assert!(!a.is_editing());
}

#[test]
fn cancel_outside_session_returns_not_editing() {
    let mut a = start(vec![item("a", true, false)]);
    assert_eq!(a.cancel(), Err(ArrangeError::NotEditing));
}

#[test]
fn commit_returns_none_when_unchanged() {
    let mut a = start(vec![item("a", true, false), item("b", false, false)]);
    begin(&mut a);
    assert_eq!(a.commit().unwrap(), None);
    assert!(!a.is_editing());
}

#[test]
fn commit_after_move_and_move_back_returns_none() {
    let mut a = start(vec![
        item("a", true, false),
        item("b", true, false),
        item("c", true, false),
    ]);
    begin(&mut a);
    a.move_item("c", 0).unwrap();
    a.move_item("c", 2).unwrap();
    assert_eq!(a.commit().unwrap(), None);
}

#[test]
fn commit_order_is_full_permutation_including_hidden() {
    let mut a = start(vec![
        item("a", true, false),
        item("h", false, false),
        item("b", true, false),
    ]);
    begin(&mut a);
    a.move_item("b", 0).unwrap();
    let c = a.commit().unwrap().unwrap();
    assert_eq!(c.order, vec!["b", "a", "h"]);
}

#[test]
fn commit_enabled_changes_only_lists_diffs_in_order() {
    let mut a = start(vec![
        item("a", true, false),
        item("b", true, false),
        item("c", false, false),
    ]);
    begin(&mut a);
    a.disable("b").unwrap();
    a.enable("c").unwrap();
    let c = a.commit().unwrap().unwrap();
    // diff order follows commit order (internal order): c then b
    assert_eq!(
        c.enabled_changes,
        vec![
            EnabledChange {
                id: "c".to_string(),
                enabled: true
            },
            EnabledChange {
                id: "b".to_string(),
                enabled: false
            },
        ]
    );
}

#[test]
fn commit_order_matches_state_after_change() {
    let mut a = start(vec![item("a", true, false), item("b", true, false)]);
    begin(&mut a);
    a.disable("a").unwrap();
    let c = a.commit().unwrap().unwrap();
    assert_eq!(c.order, vec!["a", "b"]); // full permutation, hidden included
    assert_eq!(
        c.enabled_changes,
        vec![EnabledChange {
            id: "a".to_string(),
            enabled: false
        }]
    );
}

#[test]
fn commit_ends_editing() {
    let mut a = start(vec![item("a", true, false)]);
    begin(&mut a);
    a.move_item("a", 0).unwrap();
    assert_eq!(a.commit().unwrap(), None);
    assert!(!a.is_editing());
    assert_eq!(a.move_item("a", 0), Err(ArrangeError::NotEditing));
    begin(&mut a);
    assert!(a.is_editing());
}

#[test]
fn commit_outside_session_returns_not_editing() {
    let mut a = start(vec![item("a", true, false)]);
    assert_eq!(a.commit(), Err(ArrangeError::NotEditing));
}

#[test]
fn reconcile_outside_keeps_order_drops_missing_appends_new() {
    let mut a = start(vec![
        item("a", true, false),
        item("b", true, false),
        item("c", true, false),
    ]);
    a.reconcile(&[
        item("c", true, false),
        item("a", true, false),
        item("d", false, false),
    ]);
    // survivors a,c keep their old relative order; d appended
    assert_eq!(vis_ids(&a), vec!["a", "c"]);
    assert_eq!(hid_ids(&a), vec!["d"]);
}

#[test]
fn reconcile_keeps_survivor_enabled_state() {
    let mut a = start(vec![item("a", false, false), item("b", true, false)]);
    a.reconcile(&[item("a", true, false), item("b", false, false)]);
    // enabled flags for survivors are untouched; locked comes from current_ids
    assert_eq!(vis_ids(&a), vec!["b"]);
    assert_eq!(hid_ids(&a), vec!["a"]);
}

#[test]
fn reconcile_takes_locked_from_current_ids() {
    let mut a = start(vec![item("a", true, false), item("b", true, false)]);
    a.reconcile(&[item("a", true, true), item("b", true, false)]);
    begin(&mut a);
    assert_eq!(a.disable("a"), Err(ArrangeError::Locked));
    a.cancel().unwrap();
    a.reconcile(&[item("a", true, false), item("b", true, false)]);
    begin(&mut a);
    assert_eq!(a.disable("a"), Ok(()));
}

#[test]
fn reconcile_locking_a_disabled_survivor_forces_it_enabled() {
    let mut a = start(vec![item("a", false, false), item("b", true, false)]);
    a.reconcile(&[item("a", false, true), item("b", true, false)]);
    // a is locked -> forced enabled; survivors keep old order
    assert_eq!(vis_ids(&a), vec!["a", "b"]);
    begin(&mut a);
    assert_eq!(a.disable("a"), Err(ArrangeError::Locked));
}

#[test]
fn reconcile_during_editing_updates_snapshot() {
    let mut a = start(vec![item("a", true, false), item("b", true, false)]);
    begin(&mut a);
    a.disable("a").unwrap();
    a.reconcile(&[item("b", true, false), item("c", true, false)]);
    // a dropped from both; c appended to both; cancel must not resurrect a
    a.cancel().unwrap();
    assert_eq!(vis_ids(&a), vec!["b", "c"]);
    assert!(a.hidden().is_empty());
}

#[test]
fn reconcile_during_editing_does_not_resurrect_removed_on_cancel() {
    let mut a = start(vec![
        item("a", true, false),
        item("b", true, false),
        item("c", true, false),
    ]);
    begin(&mut a);
    a.reconcile(&[item("a", true, false), item("c", true, false)]); // b removed
    a.cancel().unwrap();
    assert_eq!(vis_ids(&a), vec!["a", "c"]);
}

#[test]
fn reconcile_does_not_end_editing() {
    let mut a = start(vec![item("a", true, false)]);
    begin(&mut a);
    a.reconcile(&[item("a", true, false), item("b", true, false)]);
    assert!(a.is_editing());
}

#[test]
fn reconcile_empty_current_ids_empties_arrangement() {
    let mut a = start(vec![item("a", true, false)]);
    a.reconcile(&[]);
    assert!(a.visible().is_empty());
    assert!(a.hidden().is_empty());
}

#[test]
fn reconcile_deduplicates_current_ids() {
    let mut a = start(vec![item("a", true, false)]);
    a.reconcile(&[
        item("a", true, false),
        item("a", false, true),
        item("b", true, false),
    ]);
    assert_eq!(vis_ids(&a), vec!["a", "b"]);
}

#[test]
fn op_error_leaves_state_unchanged() {
    let mut a = start(vec![
        item("a", true, false),
        item("b", true, false),
        item("locked", true, true),
    ]);
    begin(&mut a);
    assert_eq!(a.disable("locked"), Err(ArrangeError::Locked));
    assert_eq!(vis_ids(&a), vec!["a", "b", "locked"]);
    assert_eq!(a.move_item("zzz", 0), Err(ArrangeError::UnknownId));
    assert_eq!(vis_ids(&a), vec!["a", "b", "locked"]);
    // no edit ever succeeded, so commit is None
    assert_eq!(a.commit().unwrap(), None);
    // after re-opening, a failed op still leaves the snapshot intact
    begin(&mut a);
    assert_eq!(a.disable("locked"), Err(ArrangeError::Locked));
    a.cancel().unwrap();
    assert_eq!(vis_ids(&a), vec!["a", "b", "locked"]);
}

#[test]
fn serde_grid_item_camelcase_roundtrip() {
    let g = GridItem {
        id: "home".to_string(),
        enabled: true,
        locked: true,
    };
    let json = serde_json::to_string(&g).unwrap();
    assert_eq!(json, r#"{"id":"home","enabled":true,"locked":true}"#);
    let back: GridItem = serde_json::from_str(&json).unwrap();
    assert_eq!(back, g);
}

#[test]
fn serde_commit_camelcase_roundtrip() {
    let c = ArrangeCommit {
        order: vec!["a".to_string(), "b".to_string()],
        enabled_changes: vec![EnabledChange {
            id: "b".to_string(),
            enabled: false,
        }],
    };
    let json = serde_json::to_string(&c).unwrap();
    assert_eq!(
        json,
        r#"{"order":["a","b"],"enabledChanges":[{"id":"b","enabled":false}]}"#
    );
    let back: ArrangeCommit = serde_json::from_str(&json).unwrap();
    assert_eq!(back, c);
    // snake_case keys are rejected: no aliases are accepted
    assert!(serde_json::from_str::<ArrangeCommit>(r#"{"order":[],"enabled_changes":[]}"#).is_err());
}
