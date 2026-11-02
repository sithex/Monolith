use scratchpad_core::{LoadReport, Scratchpad, ScratchpadError};

/// Flush-save to make the pad clean (the same flow the Tauri actor uses at
/// quit / after a debounced save).
fn clean(pad: &mut Scratchpad, now: u64) {
    pad.scheduler_mut().flush_requested();
    match pad.scheduler_mut().begin_save(now) {
        Some(g) => pad.scheduler_mut().saved(now, g),
        None => panic!("expected a save to begin after flush"),
    }
}

/// List ids in display order.
fn ids(pad: &Scratchpad) -> Vec<String> {
    pad.list().into_iter().map(|s| s.id).collect()
}

#[test]
fn new_scratchpad_is_empty_and_clean() {
    let pad = Scratchpad::new();
    assert!(pad.is_empty());
    assert_eq!(pad.len(), 0);
    assert!(!pad.is_dirty());
    assert_eq!(pad.list(), Vec::new());
}

#[test]
fn new_note_sets_defaults_and_marks_dirty() {
    let mut pad = Scratchpad::new();
    pad.new_note(1000, "n1".to_string()).unwrap();
    assert!(pad.is_dirty());
    let list = pad.list();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, "n1");
    assert_eq!(list[0].title, "Untitled");
    assert_eq!(list[0].created_ms, 1000);
    assert_eq!(list[0].updated_ms, 1000);
    assert!(!list[0].pinned);
    let n = pad.get("n1").unwrap();
    assert_eq!(n.body, "");
    assert_eq!(n.title, "Untitled");
    assert!(!n.pinned);
    assert_eq!(n.created_ms, 1000);
    assert_eq!(n.updated_ms, 1000);
}

#[test]
fn new_note_rejects_invalid_ids() {
    let mut pad = Scratchpad::new();
    let bad = vec![
        String::new(),
        "a".repeat(65),
        "has space".to_string(),
        "has.dot".to_string(),
        "slash/".to_string(),
        "é".to_string(),
        "#".to_string(),
        " ".to_string(),
        "a\n".to_string(),
    ];
    for id in bad {
        assert_eq!(pad.new_note(0, id), Err(ScratchpadError::InvalidId));
    }
    assert_eq!(pad.len(), 0);
    assert!(!pad.is_dirty());
}

#[test]
fn new_note_accepts_boundary_ids() {
    let mut pad = Scratchpad::new();
    pad.new_note(0, "a".to_string()).unwrap();
    pad.new_note(0, "0".to_string()).unwrap();
    pad.new_note(0, "-".to_string()).unwrap();
    pad.new_note(0, "_".to_string()).unwrap();
    pad.new_note(0, "A-Z_09".to_string()).unwrap();
    pad.new_note(0, "a".repeat(64)).unwrap();
    assert_eq!(pad.len(), 6);
}

#[test]
fn new_note_rejects_duplicate_id() {
    let mut pad = Scratchpad::new();
    pad.new_note(0, "same".to_string()).unwrap();
    assert_eq!(
        pad.new_note(1, "same".to_string()),
        Err(ScratchpadError::DuplicateId)
    );
    assert_eq!(pad.len(), 1);
}

#[test]
fn new_note_max_200_notes() {
    let mut pad = Scratchpad::new();
    for i in 0..200 {
        pad.new_note(i as u64, format!("n{i}")).unwrap();
    }
    assert_eq!(pad.len(), 200);
    assert_eq!(
        pad.new_note(200, "overflow".to_string()),
        Err(ScratchpadError::TooManyNotes)
    );
    assert_eq!(pad.len(), 200);
}

#[test]
fn edit_updates_body_title_and_updated_ms() {
    let mut pad = Scratchpad::new();
    pad.new_note(1000, "n1".to_string()).unwrap();
    pad.edit(2000, "n1", "hello\nworld".to_string()).unwrap();
    let n = pad.get("n1").unwrap();
    assert_eq!(n.body, "hello\nworld");
    assert_eq!(n.title, "hello");
    assert_eq!(n.updated_ms, 2000);
    assert_eq!(n.created_ms, 1000);
}

#[test]
fn edit_identical_body_does_not_mark_dirty() {
    let mut pad = Scratchpad::new();
    pad.new_note(1000, "n1".to_string()).unwrap();
    let body = "same body".to_string();
    pad.edit(1100, "n1", body.clone()).unwrap();
    clean(&mut pad, 1200);
    assert!(!pad.is_dirty());
    pad.edit(1300, "n1", body).unwrap();
    assert!(!pad.is_dirty(), "identical edit must not mark dirty");
}

#[test]
fn edit_identical_body_does_not_touch_updated_ms() {
    let mut pad = Scratchpad::new();
    pad.new_note(1000, "n1".to_string()).unwrap();
    let body = "same body".to_string();
    pad.edit(1100, "n1", body.clone()).unwrap();
    let before = pad.to_json();
    pad.edit(1300, "n1", body).unwrap();
    assert_eq!(pad.to_json(), before);
}

#[test]
fn edit_missing_note_errors() {
    let mut pad = Scratchpad::new();
    assert_eq!(
        pad.edit(0, "ghost", String::new()),
        Err(ScratchpadError::NotFound)
    );
}

#[test]
fn edit_body_over_256kib_rejected_note_unchanged() {
    let mut pad = Scratchpad::new();
    pad.new_note(1000, "n1".to_string()).unwrap();
    clean(&mut pad, 1100);
    let body = "x".repeat(256 * 1024 + 1);
    assert_eq!(
        pad.edit(1200, "n1", body),
        Err(ScratchpadError::BodyTooLarge)
    );
    assert!(!pad.is_dirty(), "rejected edit must not mark dirty");
    let n = pad.get("n1").unwrap();
    assert_eq!(n.body, "");
    assert_eq!(n.updated_ms, 1000);
}

#[test]
fn edit_body_exactly_256kib_accepted() {
    let mut pad = Scratchpad::new();
    pad.new_note(0, "n1".to_string()).unwrap();
    let body = "y".repeat(256 * 1024);
    pad.edit(1, "n1", body.clone()).unwrap();
    let n = pad.get("n1").unwrap();
    assert_eq!(n.body.len(), 256 * 1024);
    assert_eq!(n.body, body);
}

#[test]
fn delete_removes_note_and_marks_dirty() {
    let mut pad = Scratchpad::new();
    pad.new_note(1000, "a".to_string()).unwrap();
    pad.new_note(2000, "b".to_string()).unwrap();
    clean(&mut pad, 3000);
    assert!(!pad.is_dirty());
    pad.delete(3100, "a").unwrap();
    assert!(pad.is_dirty());
    assert_eq!(pad.len(), 1);
    assert!(pad.get("a").is_none());
    assert!(pad.get("b").is_some());
}

#[test]
fn delete_missing_note_errors() {
    let mut pad = Scratchpad::new();
    assert_eq!(pad.delete(0, "ghost"), Err(ScratchpadError::NotFound));
}

#[test]
fn set_pinned_marks_dirty_and_reorders() {
    let mut pad = Scratchpad::new();
    pad.new_note(1000, "a".to_string()).unwrap();
    pad.new_note(2000, "b".to_string()).unwrap();
    assert_eq!(ids(&pad), ["b", "a"]);
    clean(&mut pad, 3000);
    pad.set_pinned(3100, "a", true).unwrap();
    assert!(pad.is_dirty());
    assert_eq!(ids(&pad), ["a", "b"]);
}

#[test]
fn set_pinned_same_value_noop() {
    let mut pad = Scratchpad::new();
    pad.new_note(1000, "n1".to_string()).unwrap();
    clean(&mut pad, 1500);
    let before = pad.to_json();
    pad.set_pinned(2000, "n1", false).unwrap();
    assert!(!pad.is_dirty());
    assert_eq!(pad.to_json(), before);
}

#[test]
fn set_pinned_missing_note_errors() {
    let mut pad = Scratchpad::new();
    assert_eq!(
        pad.set_pinned(0, "ghost", true),
        Err(ScratchpadError::NotFound)
    );
}

#[test]
fn list_order_pinned_first_then_updated_desc_then_id_asc() {
    let mut pad = Scratchpad::new();
    pad.new_note(1000, "a".to_string()).unwrap();
    pad.new_note(2000, "b".to_string()).unwrap();
    pad.new_note(3000, "c".to_string()).unwrap();
    assert_eq!(ids(&pad), ["c", "b", "a"]);

    pad.set_pinned(3500, "a", true).unwrap();
    assert_eq!(ids(&pad), ["a", "c", "b"]);

    // tie by id ascending within equal updated_ms
    let mut pad2 = Scratchpad::new();
    pad2.new_note(1000, "b".to_string()).unwrap();
    pad2.new_note(1000, "a".to_string()).unwrap();
    assert_eq!(ids(&pad2), ["a", "b"]);

    // the pinned group also ties by id ascending
    pad2.set_pinned(1000, "a", true).unwrap();
    pad2.set_pinned(1000, "b", true).unwrap();
    assert_eq!(ids(&pad2), ["a", "b"]);
}

#[test]
fn mutating_ops_mark_dirty_delete_and_pin() {
    let mut pad = Scratchpad::new();
    pad.new_note(1000, "a".to_string()).unwrap();
    pad.new_note(2000, "b".to_string()).unwrap();
    clean(&mut pad, 3000);
    assert!(!pad.is_dirty());
    pad.delete(3100, "a").unwrap();
    assert!(pad.is_dirty());
    clean(&mut pad, 3200);
    pad.set_pinned(3300, "b", true).unwrap();
    assert!(pad.is_dirty());
}

#[test]
fn to_json_wire_shape_is_camelcase() {
    let mut pad = Scratchpad::new();
    pad.new_note(1000, "n1".to_string()).unwrap();
    pad.edit(2000, "n1", "hi".to_string()).unwrap();
    pad.set_pinned(3000, "n1", true).unwrap();
    let v: serde_json::Value = serde_json::from_str(&pad.to_json()).unwrap();
    assert_eq!(v["version"], 1);
    let note = &v["notes"][0];
    assert_eq!(note["id"], "n1");
    assert_eq!(note["title"], "hi");
    assert_eq!(note["body"], "hi");
    assert_eq!(note["createdMs"], 1000);
    assert_eq!(note["updatedMs"], 2000);
    assert_eq!(note["pinned"], true);
    assert!(note.get("created_ms").is_none());
    assert!(note.get("updated_ms").is_none());
}

#[test]
fn json_round_trip_preserves_state() {
    let mut pad = Scratchpad::new();
    pad.new_note(1000, "alpha".to_string()).unwrap();
    pad.new_note(1500, "beta".to_string()).unwrap();
    pad.edit(2000, "alpha", "first note body".to_string())
        .unwrap();
    pad.edit(2500, "beta", "second".to_string()).unwrap();
    pad.set_pinned(3000, "beta", true).unwrap();
    let json = pad.to_json();

    let (loaded, report) = Scratchpad::from_json(&json).unwrap();
    assert_eq!(
        report,
        LoadReport {
            skipped: 0,
            truncated_to_max: false,
        }
    );
    assert_eq!(loaded.to_json(), json);
    assert_eq!(loaded.list(), pad.list());
    assert!(!loaded.is_dirty());
    assert_eq!(loaded.len(), 2);
}

#[test]
fn from_json_rejects_non_object_top_level() {
    let bad = ["[]", "[1,2]", "null", "42", "\"str\"", "true"];
    for s in bad {
        assert_eq!(
            Scratchpad::from_json(s),
            Err(ScratchpadError::NotAnObject),
            "input: {s}"
        );
    }
}

#[test]
fn from_json_rejects_invalid_json() {
    for s in [
        "",
        "{",
        "not json at all",
        r#"{"version":1,"notes":[}"#,
        "{\"version\":1,\"notes\":[1,2",
    ] {
        assert_eq!(
            Scratchpad::from_json(s),
            Err(ScratchpadError::InvalidJson),
            "input: {s}"
        );
    }
}

#[test]
fn from_json_rejects_bad_version() {
    assert_eq!(
        Scratchpad::from_json(r#"{"notes":[]}"#),
        Err(ScratchpadError::InvalidVersion)
    );
    assert_eq!(
        Scratchpad::from_json(r#"{"version":2,"notes":[]}"#),
        Err(ScratchpadError::InvalidVersion)
    );
    assert_eq!(
        Scratchpad::from_json(r#"{"version":"1","notes":[]}"#),
        Err(ScratchpadError::InvalidVersion)
    );
    assert_eq!(
        Scratchpad::from_json(r#"{"version":1.0,"notes":[]}"#),
        Err(ScratchpadError::InvalidVersion)
    );
    assert_eq!(
        Scratchpad::from_json(r#"{"version":true,"notes":[]}"#),
        Err(ScratchpadError::InvalidVersion)
    );
}

#[test]
fn from_json_rejects_notes_not_array() {
    assert_eq!(
        Scratchpad::from_json(r#"{"version":1,"notes":5}"#),
        Err(ScratchpadError::NotesNotArray)
    );
    assert_eq!(
        Scratchpad::from_json(r#"{"version":1,"notes":"x"}"#),
        Err(ScratchpadError::NotesNotArray)
    );
    assert_eq!(
        Scratchpad::from_json(r#"{"version":1,"notes":{}}"#),
        Err(ScratchpadError::NotesNotArray)
    );
}

#[test]
fn from_json_missing_notes_is_empty() {
    let (pad, report) = Scratchpad::from_json(r#"{"version":1}"#).unwrap();
    assert_eq!(pad.len(), 0);
    assert_eq!(
        report,
        LoadReport {
            skipped: 0,
            truncated_to_max: false,
        }
    );
    let (pad2, report2) = Scratchpad::from_json(r#"{"version":1,"notes":[]}"#).unwrap();
    assert_eq!(pad2.len(), 0);
    assert_eq!(
        report2,
        LoadReport {
            skipped: 0,
            truncated_to_max: false,
        }
    );
}

#[test]
fn from_json_skips_each_invalid_kind_and_counts() {
    let big_body = "z".repeat(256 * 1024 + 1);
    let json = format!(
        r#"{{"version":1,"notes":[
            {{"id":"","body":"empty id"}},
            {{"id":"bad id","body":"space"}},
            {{"id":42,"body":"numeric id"}},
            {{"id":"dup","body":"first"}},
            {{"id":"dup","body":"second"}},
            {{"id":"big","body":"{big_body}"}},
            {{"id":"wt1","createdMs":"x","body":"ok"}},
            {{"id":"wt2","updatedMs":"y","body":"ok"}},
            {{"id":"wt3","pinned":"yes","body":"ok"}},
            {{"id":"wt4","body":123}},
            {{"id":"wt5","createdMs":-1,"body":"ok"}},
            "not an object",
            {{"id":"good","body":"fine"}}
        ]}}"#
    );
    let (pad, report) = Scratchpad::from_json(&json).unwrap();
    // invalid: empty id, bad id, numeric id, dup-second, big body, wt1..wt5,
    // not-an-object = 11 skipped; valid: dup (first), good
    assert_eq!(report.skipped, 11);
    assert!(!report.truncated_to_max);
    assert_eq!(ids(&pad), ["dup", "good"]);
    assert_eq!(pad.get("dup").unwrap().title, "first");
    assert_eq!(pad.get("good").unwrap().body, "fine");
}

#[test]
fn from_json_truncates_to_newest_200() {
    let mut notes = String::new();
    for i in 0..205 {
        if i > 0 {
            notes.push(',');
        }
        notes.push_str(&format!(r#"{{"id":"n{i}","updatedMs":{i},"body":"b"}}"#));
    }
    let json = format!(r#"{{"version":1,"notes":[{notes}]}}"#);
    let (pad, report) = Scratchpad::from_json(&json).unwrap();
    assert_eq!(report.skipped, 0);
    assert!(report.truncated_to_max);
    assert_eq!(pad.len(), 200);
    let sum: u64 = pad.list().iter().map(|s| s.updated_ms).sum();
    let expected: u64 = (5..205).sum();
    assert_eq!(sum, expected);
    assert!(pad.get("n5").is_some());
    assert!(pad.get("n4").is_none());
    assert!(pad.get("n204").is_some());
}

#[test]
fn from_json_recomputes_titles_and_ignores_stored_title() {
    let json = r#"{"version":1,"notes":[{"id":"n1","title":"TRUSTED WRONG","body":"real title\nsecond line","updatedMs":10}]}"#;
    let (pad, report) = Scratchpad::from_json(json).unwrap();
    assert_eq!(report.skipped, 0);
    assert_eq!(pad.get("n1").unwrap().title, "real title");

    // even a non-string stored title never fails the note
    let json2 =
        r#"{"version":1,"notes":[{"id":"n1","title":42,"body":"still fine","updatedMs":10}]}"#;
    let (pad2, report2) = Scratchpad::from_json(json2).unwrap();
    assert_eq!(report2.skipped, 0);
    assert_eq!(pad2.get("n1").unwrap().title, "still fine");
}

#[test]
fn from_json_ignores_unknown_fields() {
    let json = r#"{"version":1,"extra":"zzz","notes":[{"id":"n1","body":"x","futureField":123,"nested":{"a":1},"pinned":false}]}"#;
    let (pad, report) = Scratchpad::from_json(json).unwrap();
    assert_eq!(report.skipped, 0);
    assert_eq!(pad.len(), 1);
    assert_eq!(pad.get("n1").unwrap().body, "x");
}

#[test]
fn from_json_preserves_created_updated_pinned() {
    let json = r#"{"version":1,"notes":[{"id":"n1","body":"x","createdMs":111,"updatedMs":222,"pinned":true}]}"#;
    let (pad, _) = Scratchpad::from_json(json).unwrap();
    let n = pad.get("n1").unwrap();
    assert_eq!(n.created_ms, 111);
    assert_eq!(n.updated_ms, 222);
    assert!(n.pinned);
}

#[test]
fn from_json_defaults_missing_scalar_fields() {
    let json = r#"{"version":1,"notes":[{"id":"n1","body":"x"}]}"#;
    let (pad, report) = Scratchpad::from_json(json).unwrap();
    assert_eq!(report.skipped, 0);
    let n = pad.get("n1").unwrap();
    assert_eq!(n.created_ms, 0);
    assert_eq!(n.updated_ms, 0);
    assert!(!n.pinned);
}

#[test]
fn from_json_normalizes_to_canonical_order() {
    let json = r#"{"version":1,"notes":[
        {"id":"old","updatedMs":10,"body":"x"},
        {"id":"fresh","updatedMs":99,"body":"x"},
        {"id":"pinned-old","updatedMs":5,"body":"x","pinned":true}
    ]}"#;
    let (pad, _) = Scratchpad::from_json(json).unwrap();
    assert_eq!(ids(&pad), ["pinned-old", "fresh", "old"]);
}
