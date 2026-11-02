use hermes_island_lib::integrations::brightness::core::{
    display_path, percent, step_request, validate_display_name, BrightnessModel, Display,
    DragError, Event, NameError, SetRequest, Step, StepRequest, CONTEXT, SUPPRESS_INDICATOR,
};

fn props(model: &mut BrightnessModel, name: &str, max: i32, brightness: i32) {
    model.apply(Event::Props {
        name: name.to_string(),
        label: format!("label-{name}"),
        is_internal: false,
        max_brightness: max,
        brightness,
    });
}

fn display_names(model: &mut BrightnessModel, names: &[&str]) {
    model.apply(Event::DisplayNames(
        names.iter().map(|s| s.to_string()).collect(),
    ));
}

fn names(model: &BrightnessModel) -> Vec<String> {
    model.displays().iter().map(|v| v.name.clone()).collect()
}

// ---------------------------------------------------------------------------
// name validation / path
// ---------------------------------------------------------------------------

#[test]
fn name_validation_and_path() {
    assert!(validate_display_name("display0").is_ok());
    assert!(validate_display_name(&"a".repeat(255)).is_ok());
    assert_eq!(validate_display_name(""), Err(NameError::Invalid));
    assert_eq!(
        validate_display_name(&"a".repeat(256)),
        Err(NameError::Invalid)
    );
    for bad in ["a-b", "a/b", "é", "a b", "a.b", "-a"] {
        assert_eq!(
            validate_display_name(bad),
            Err(NameError::Invalid),
            "bad: {bad:?}"
        );
    }
    assert_eq!(
        display_path("display0").unwrap(),
        "/org/kde/ScreenBrightness/display0"
    );
    assert_eq!(display_path("a-b"), Err(NameError::Invalid));
}

#[test]
fn public_percent_helper() {
    let d = Display {
        name: "display0".to_string(),
        label: String::new(),
        is_internal: false,
        max_brightness: 10000,
        brightness: 50,
    };
    assert_eq!(percent(&d), Some(1)); // 0.5% half up
    let d = Display {
        max_brightness: 0,
        brightness: 50,
        ..d
    };
    assert_eq!(percent(&d), None);
}

// ---------------------------------------------------------------------------
// model: display names / fetch
// ---------------------------------------------------------------------------

#[test]
fn display_names_order_fetch_once() {
    let mut m = BrightnessModel::new();
    let a = m.apply(Event::DisplayNames(vec![
        "d0".to_string(),
        "d1".to_string(),
    ]));
    assert!(a.changed);
    assert_eq!(a.fetch, vec!["d0", "d1"]);
    assert_eq!(names(&m), vec!["d0", "d1"]);

    let a = m.apply(Event::DisplayNames(vec![
        "d0".to_string(),
        "d1".to_string(),
    ]));
    assert!(!a.changed);
    assert!(a.fetch.is_empty());

    let a = m.apply(Event::DisplayNames(vec![
        "d1".to_string(),
        "d2".to_string(),
    ]));
    assert!(a.changed);
    assert_eq!(a.fetch, vec!["d2"]);
    assert_eq!(names(&m), vec!["d1", "d2"]);

    let a = m.apply(Event::DisplayNames(vec![
        "d2".to_string(),
        "d1".to_string(),
    ]));
    assert!(a.changed);
    assert!(a.fetch.is_empty());
    assert_eq!(names(&m), vec!["d2", "d1"]);
}

#[test]
fn fetch_at_most_once_until_removed() {
    let mut m = BrightnessModel::new();
    display_names(&mut m, &["d0"]);
    assert_eq!(
        m.apply(Event::DisplayNames(vec!["d0".to_string()])).fetch,
        Vec::<String>::new()
    );
    props(&mut m, "d0", 10000, 5000);
    assert!(m
        .apply(Event::DisplayNames(vec!["d0".to_string()]))
        .fetch
        .is_empty());
    // remove then re-add: new entry fetches again
    m.apply(Event::DisplayRemoved("d0".to_string()));
    let a = m.apply(Event::DisplayNames(vec!["d0".to_string()]));
    assert_eq!(a.fetch, vec!["d0"]);
}

#[test]
fn display_added_then_props_clears_fetch() {
    let mut m = BrightnessModel::new();
    display_names(&mut m, &["d0"]);
    let a = m.apply(Event::DisplayAdded("d1".to_string()));
    assert_eq!(a.fetch, vec!["d1"]); // d0 already fetched once
    let a = m.apply(Event::DisplayAdded("d1".to_string()));
    assert!(!a.changed);
    assert!(a.fetch.is_empty());
    let a = m.apply(Event::Props {
        name: "d1".to_string(),
        label: "L".to_string(),
        is_internal: false,
        max_brightness: 10000,
        brightness: 0,
    });
    assert!(a.changed);
    assert!(a.fetch.is_empty());
}

#[test]
fn invalid_names_dropped_and_counted() {
    let mut m = BrightnessModel::new();
    let a = m.apply(Event::DisplayNames(vec![
        "ok".to_string(),
        "bad-name".to_string(),
        "a/b".to_string(),
        String::new(),
    ]));
    assert_eq!(a.fetch, vec!["ok"]);
    assert_eq!(m.displays().len(), 1);
    assert_eq!(m.ignored(), 3);
    let a = m.apply(Event::DisplayAdded("x-y".to_string()));
    assert!(!a.changed);
    assert_eq!(m.ignored(), 4);
}

#[test]
fn max_16_displays_extra_counted() {
    let mut m = BrightnessModel::new();
    let names: Vec<String> = (0..17).map(|i| format!("d{i}")).collect();
    let a = m.apply(Event::DisplayNames(names));
    assert_eq!(a.fetch.len(), 16);
    assert_eq!(m.displays().len(), 16);
    assert_eq!(m.ignored(), 1);
    let a = m.apply(Event::DisplayAdded("d17".to_string()));
    assert!(!a.changed);
    assert!(a.fetch.is_empty());
    assert_eq!(m.displays().len(), 16);
    assert_eq!(m.ignored(), 2);
}

#[test]
fn unknown_and_invalid_signals_counted() {
    let mut m = BrightnessModel::new();
    display_names(&mut m, &["d0"]);
    let a = m.apply(Event::DisplayRemoved("nope".to_string()));
    assert!(!a.changed);
    assert_eq!(m.ignored(), 1);
    m.apply(Event::Props {
        name: "nope".to_string(),
        label: String::new(),
        is_internal: false,
        max_brightness: 10000,
        brightness: 0,
    });
    m.apply(Event::BrightnessChanged {
        name: "nope".to_string(),
        brightness: 1,
        source_client: String::new(),
        context: String::new(),
    });
    m.apply(Event::RangeChanged {
        name: "nope".to_string(),
        max_brightness: 10000,
        brightness: 1,
    });
    assert_eq!(m.ignored(), 4);
    assert_eq!(m.displays().len(), 1);
}

// ---------------------------------------------------------------------------
// model: props / range / availability / clamping
// ---------------------------------------------------------------------------

#[test]
fn props_range_availability_and_clamping() {
    let mut m = BrightnessModel::new();
    display_names(&mut m, &["d0"]);
    props(&mut m, "d0", 10000, 5000);
    assert_eq!(m.displays()[0].percent, Some(50));

    let a = m.apply(Event::Props {
        name: "d0".to_string(),
        label: String::new(),
        is_internal: false,
        max_brightness: 10000,
        brightness: -5,
    });
    assert!(a.changed);
    assert_eq!(m.displays()[0].percent, Some(0));
    assert_eq!(m.clamped(), 1);

    m.apply(Event::BrightnessChanged {
        name: "d0".to_string(),
        brightness: 10005,
        source_client: "x".to_string(),
        context: "c".to_string(),
    });
    assert_eq!(m.displays()[0].percent, Some(100));
    assert_eq!(m.clamped(), 2);

    // exact boundaries are not clamped
    m.apply(Event::BrightnessChanged {
        name: "d0".to_string(),
        brightness: 10000,
        source_client: "x".to_string(),
        context: "c".to_string(),
    });
    m.apply(Event::BrightnessChanged {
        name: "d0".to_string(),
        brightness: 0,
        source_client: "x".to_string(),
        context: "c".to_string(),
    });
    assert_eq!(m.clamped(), 2);

    // max <= 0 keeps the display but marks it unavailable
    let a = m.apply(Event::RangeChanged {
        name: "d0".to_string(),
        max_brightness: 0,
        brightness: 1,
    });
    assert!(a.changed);
    assert_eq!(m.displays().len(), 1);
    assert_eq!(m.displays()[0].percent, None);
    assert!(m.drag("d0", 50, 0).is_err());

    m.apply(Event::RangeChanged {
        name: "d0".to_string(),
        max_brightness: 10000,
        brightness: 2500,
    });
    assert_eq!(m.displays()[0].percent, Some(25));
}

// ---------------------------------------------------------------------------
// drag
// ---------------------------------------------------------------------------

#[test]
fn drag_errors() {
    let mut m = BrightnessModel::new();
    display_names(&mut m, &["d0"]);
    assert_eq!(m.drag("missing", 50, 0), Err(DragError::UnknownDisplay));
    assert_eq!(m.drag("d0", 50, 0), Err(DragError::UnavailableDisplay));
    props(&mut m, "d0", 10000, 2000);
    assert_eq!(m.drag("d0", 101, 0), Err(DragError::PercentOutOfRange));
    assert_eq!(m.drag("d0", 255, 0), Err(DragError::PercentOutOfRange));
    assert!(m.drag("d0", 100, 0).is_ok());
}

#[test]
fn throttle_boundaries_latest_wins() {
    let mut m = BrightnessModel::new();
    display_names(&mut m, &["d0"]);
    props(&mut m, "d0", 10000, 0);

    m.drag("d0", 50, 0).unwrap();
    let r = m.poll(0);
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].brightness, 5000);
    assert_eq!(r[0].path, "/org/kde/ScreenBrightness/d0");
    assert_eq!(r[0].flags, SUPPRESS_INDICATOR);
    assert_eq!(r[0].context, CONTEXT);

    m.drag("d0", 60, 10).unwrap();
    assert!(m.poll(49).is_empty());
    let r = m.poll(50);
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].brightness, 6000);

    m.drag("d0", 60, 60).unwrap();
    assert!(m.poll(100).is_empty()); // same value not resent
}

#[test]
fn release_flushes_and_ends_drag() {
    let mut m = BrightnessModel::new();
    display_names(&mut m, &["d0"]);
    props(&mut m, "d0", 10000, 0);

    // no poll: release flushes the pending target
    m.drag("d0", 80, 0).unwrap();
    let r = m.release("d0", 0).unwrap();
    assert_eq!(r.brightness, 8000);
    assert!(m.poll(100).is_empty());
    assert!(m.release("d0", 100).is_none()); // no drag anymore

    // nothing unsent -> None
    m.drag("d0", 50, 0).unwrap();
    m.poll(0);
    assert!(m.release("d0", 10).is_none());

    // flush overrides the throttle
    m.drag("d0", 70, 20).unwrap();
    m.poll(20);
    m.drag("d0", 90, 30).unwrap();
    let r = m.release("d0", 30).unwrap();
    assert_eq!(r.brightness, 9000);
    assert!(m.poll(40).is_empty());
}

#[test]
fn settle_window_499_vs_500() {
    let mut m = BrightnessModel::new();
    display_names(&mut m, &["d0"]);
    props(&mut m, "d0", 10000, 3000);

    m.drag("d0", 60, 0).unwrap();
    m.poll(0);
    m.release("d0", 0);
    // foreign change: stored becomes 40%
    m.apply(Event::BrightnessChanged {
        name: "d0".to_string(),
        brightness: 4000,
        source_client: "kde".to_string(),
        context: "powerdevil".to_string(),
    });

    m.poll(499);
    assert_eq!(m.displays()[0].percent, Some(60)); // settle shows target
    m.poll(500);
    assert_eq!(m.displays()[0].percent, Some(40)); // model value
}

#[test]
fn foreign_context_change_during_drag() {
    let mut m = BrightnessModel::new();
    display_names(&mut m, &["d0"]);
    props(&mut m, "d0", 10000, 3000);

    m.drag("d0", 50, 0).unwrap();
    m.poll(0);
    m.apply(Event::BrightnessChanged {
        name: "d0".to_string(),
        brightness: 2000,
        source_client: "kde".to_string(),
        context: "elsewhere".to_string(),
    });
    assert_eq!(m.displays()[0].percent, Some(50)); // drag target shown, not echo
    m.release("d0", 0);
    m.poll(499);
    assert_eq!(m.displays()[0].percent, Some(50));
    m.poll(500);
    assert_eq!(m.displays()[0].percent, Some(20)); // stored foreign value visible
}

#[test]
fn two_displays_throttled_independently() {
    let mut m = BrightnessModel::new();
    display_names(&mut m, &["d0", "d1"]);
    props(&mut m, "d0", 10000, 0);
    props(&mut m, "d1", 10000, 0);

    m.drag("d0", 50, 0).unwrap();
    m.drag("d1", 60, 0).unwrap();
    let r = m.poll(0);
    assert_eq!(r.len(), 2);

    m.drag("d0", 70, 10).unwrap();
    m.drag("d1", 80, 10).unwrap();
    assert!(m.poll(49).is_empty());
    let r = m.poll(50);
    assert_eq!(r.len(), 2);
    let mut b: Vec<i32> = r.iter().map(|q| q.brightness).collect();
    b.sort();
    assert_eq!(b, vec![7000, 8000]);
}

#[test]
fn removed_during_drag_ends_drag() {
    let mut m = BrightnessModel::new();
    display_names(&mut m, &["d0"]);
    props(&mut m, "d0", 10000, 0);
    m.drag("d0", 50, 0).unwrap();
    m.poll(0);
    m.apply(Event::DisplayRemoved("d0".to_string()));
    assert!(m.poll(100).is_empty());
    assert!(m.release("d0", 100).is_none());
    assert!(m.displays().is_empty());
}

#[test]
fn readrag_after_release_starts_fresh_session() {
    let mut m = BrightnessModel::new();
    display_names(&mut m, &["d0"]);
    props(&mut m, "d0", 10000, 0);
    m.drag("d0", 40, 0).unwrap();
    m.poll(0);
    m.release("d0", 0);
    // new drag within settle: fresh session sends immediately
    m.drag("d0", 70, 1).unwrap();
    let r = m.poll(1);
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].brightness, 7000);
}

#[test]
fn next_wakeup() {
    let mut m = BrightnessModel::new();
    display_names(&mut m, &["d0", "d1"]);
    props(&mut m, "d0", 10000, 0);
    props(&mut m, "d1", 10000, 0);

    assert_eq!(m.next_wakeup(0), None);

    // pending unsent -> now
    m.drag("d0", 50, 0).unwrap();
    assert_eq!(m.next_wakeup(0), Some(0));

    // after sending, throttle expiry
    m.poll(0);
    m.drag("d0", 60, 10).unwrap();
    assert_eq!(m.next_wakeup(49), Some(50));

    // settle expiry
    m.poll(50);
    m.release("d0", 60);
    assert_eq!(m.next_wakeup(60), Some(560));

    // combined: min across d0 settle and d1 immediate work
    m.drag("d1", 30, 61).unwrap();
    assert_eq!(m.next_wakeup(61), Some(61));
}

// ---------------------------------------------------------------------------
// step requests / serde
// ---------------------------------------------------------------------------

#[test]
fn step_mapping() {
    let cases = [
        (Step::Increase, 0u32),
        (Step::Decrease, 1u32),
        (Step::IncreaseSmall, 2u32),
        (Step::DecreaseSmall, 3u32),
    ];
    for (step, action) in cases {
        let r = step_request(step);
        assert_eq!(r.step_action, action);
        assert_eq!(r.flags, SUPPRESS_INDICATOR);
        assert_eq!(r.context, CONTEXT);
    }
}

#[test]
fn serde_camel_case_round_trips() {
    let d = Display {
        name: "display0".to_string(),
        label: "DELL".to_string(),
        is_internal: false,
        max_brightness: 10000,
        brightness: 5000,
    };
    let json = serde_json::to_string(&d).unwrap();
    assert!(json.contains("\"isInternal\""));
    assert!(json.contains("\"maxBrightness\""));
    assert!(!json.contains("max_brightness"));
    assert_eq!(serde_json::from_str::<Display>(&json).unwrap(), d);

    let sr = StepRequest {
        step_action: 2,
        flags: 1,
        context: CONTEXT.to_string(),
    };
    let json = serde_json::to_string(&sr).unwrap();
    assert!(json.contains("\"stepAction\""));
    assert!(!json.contains("step_action"));
    assert_eq!(serde_json::from_str::<StepRequest>(&json).unwrap(), sr);

    let sr = SetRequest {
        name: "display0".to_string(),
        path: "/org/kde/ScreenBrightness/display0".to_string(),
        brightness: 5000,
        flags: 1,
        context: CONTEXT.to_string(),
    };
    let json = serde_json::to_string(&sr).unwrap();
    assert_eq!(serde_json::from_str::<SetRequest>(&json).unwrap(), sr);

    let e = Event::BrightnessChanged {
        name: "display0".to_string(),
        brightness: 1234,
        source_client: "kde".to_string(),
        context: "powerdevil".to_string(),
    };
    let json = serde_json::to_string(&e).unwrap();
    assert!(json.contains("\"brightnessChanged\""));
    assert!(json.contains("\"sourceClient\""));
    assert!(!json.contains("source_client"));
    assert_eq!(serde_json::from_str::<Event>(&json).unwrap(), e);

    let e = Event::Props {
        name: "display0".to_string(),
        label: "L".to_string(),
        is_internal: false,
        max_brightness: 10000,
        brightness: 0,
    };
    let json = serde_json::to_string(&e).unwrap();
    assert!(json.contains("\"props\""));
    assert!(json.contains("\"isInternal\""));
    assert_eq!(serde_json::from_str::<Event>(&json).unwrap(), e);

    let e = Event::DisplayNames(vec!["display0".to_string()]);
    let json = serde_json::to_string(&e).unwrap();
    assert!(json.contains("\"displayNames\""));
    assert_eq!(serde_json::from_str::<Event>(&json).unwrap(), e);
}
