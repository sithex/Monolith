use hermes_island_lib::integrations::notifications::dbus::*;
use hermes_island_lib::integrations::notifications::*;
use std::collections::BTreeMap;

fn args() -> NotifyArgs {
    NotifyArgs {
        app_name: "Mail".into(),
        replaces_id: 0,
        app_icon: "mail-unread".into(),
        summary: "New mail".into(),
        body: "body".into(),
        actions: vec!["open".into(), "Open".into()],
        hints: BTreeMap::new(),
        expire_timeout: -1,
    }
}
fn image() -> ImageData {
    ImageData {
        width: 2,
        height: 2,
        rowstride: 8,
        has_alpha: true,
        bits_per_sample: 8,
        channels: 4,
        data: vec![0; 16],
    }
}

#[test]
fn image_width_bound_is_independent_of_stride_height_and_byte_bounds() {
    let mut notify = args();
    // RGB permits 129*3 bytes within the independent <=512 stride bound.
    let candidate = ImageData {
        width: 129,
        height: 1,
        rowstride: 387,
        has_alpha: false,
        bits_per_sample: 8,
        channels: 3,
        data: vec![0; 387],
    };
    notify
        .hints
        .insert("image-data".into(), HintValue::ImageData(candidate));
    assert!(
        notify.parse(1).is_err(),
        "129px width accepted despite otherwise valid RGB layout"
    );
}

#[test]
fn lru_evicts_least_recent_serial_and_touch_refreshes() {
    let mut correlator = NotifyCorrelator::default();
    for serial in 1..=256 {
        correlator.observe_call(":1.5", serial, args()).unwrap();
    }
    assert!(correlator.touch(":1.5", 1));
    assert!(!correlator.touch(":missing", 1));
    correlator.observe_call(":1.5", 257, args()).unwrap();
    assert_eq!(correlator.len(), 256);
    assert!(correlator.method_return(":1.5", 2, 22).unwrap().is_none());
    let returned = correlator.method_return(":1.5", 1, 21).unwrap().unwrap();
    assert_eq!(returned.input.server_id, Some(21));
    assert!(correlator.method_return(":1.5", 1, 21).unwrap().is_none());
    assert!(correlator.method_return(":1.5", 257, 23).unwrap().is_some());
}

#[test]
fn correlation_is_per_sender_and_unmatched_returns_never_create_items() {
    let mut correlator = NotifyCorrelator::default();
    let mut second = args();
    second.summary = "second caller".into();
    correlator.observe_call(":first", 1, args()).unwrap();
    correlator.observe_call(":second", 1, second).unwrap();
    assert!(correlator.method_return(":other", 1, 3).unwrap().is_none());
    assert!(correlator.method_return(":first", 2, 3).unwrap().is_none());
    assert_eq!(
        correlator
            .method_return(":second", 1, 4)
            .unwrap()
            .unwrap()
            .input
            .title,
        "second caller"
    );
    assert_eq!(
        correlator
            .method_return(":first", 1, 5)
            .unwrap()
            .unwrap()
            .input
            .title,
        "New mail"
    );
    assert!(correlator.is_empty());
}

#[test]
fn duplicate_serial_refreshes_payload_and_reset_forgets_previous_owner() {
    let mut correlator = NotifyCorrelator::default();
    correlator.observe_call(":caller", 1, args()).unwrap();
    let mut updated = args();
    updated.summary = "updated".into();
    correlator.observe_call(":caller", 1, updated).unwrap();
    assert_eq!(correlator.len(), 1);
    assert_eq!(
        correlator
            .method_return(":caller", 1, 10)
            .unwrap()
            .unwrap()
            .input
            .title,
        "updated"
    );
    correlator.observe_call(":caller", 2, args()).unwrap();
    correlator.reset();
    assert!(correlator
        .method_return(":caller", 2, 11)
        .unwrap()
        .is_none());
}

#[test]
fn invalid_calls_and_zero_server_id_do_not_destroy_pending_match() {
    let mut correlator = NotifyCorrelator::default();
    correlator.observe_call(":caller", 1, args()).unwrap();
    let mut invalid = args();
    invalid.summary = "x".repeat(MAX_TITLE_BYTES + 1);
    assert!(correlator.observe_call(":caller", 1, invalid).is_err());
    assert!(correlator.observe_call(":caller", 0, args()).is_err());
    assert!(correlator.observe_call("", 3, args()).is_err());
    assert!(correlator.method_return(":caller", 1, 0).is_err());
    assert_eq!(correlator.len(), 1);
    assert_eq!(
        correlator
            .method_return(":caller", 1, 42)
            .unwrap()
            .unwrap()
            .input
            .title,
        "New mail"
    );
}

#[test]
fn notify_tuple_parses_every_listed_hint_actions_and_markup() {
    let mut notify = args();
    notify.replaces_id = 77;
    notify.expire_timeout = 1500;
    notify.body = "<b>Bold</b> <i>italic</i> <u>under</u><br/><a href='bad'>link</a>&amp; &#65; &#x42; &lt;b&gt;".into();
    notify.hints = BTreeMap::from([
        ("urgency".into(), HintValue::Byte(2)),
        ("desktop-entry".into(), HintValue::String("org.mail".into())),
        ("category".into(), HintValue::String("email.arrived".into())),
        ("transient".into(), HintValue::Bool(true)),
        ("image-path".into(), HintValue::String("/image/path".into())),
        ("image-data".into(), HintValue::ImageData(image())),
        (
            "x-kde-origin-name".into(),
            HintValue::String("KDE Mail".into()),
        ),
        ("unknown".into(), HintValue::Other),
    ]);
    let parsed = notify.parse(77).unwrap();
    assert_eq!(parsed.input.title, "New mail");
    assert_eq!(
        parsed.input.body.as_deref(),
        Some("Bold italic under\nlink& A B <b>")
    );
    assert_eq!(
        parsed.formatting,
        Formatting {
            bold: true,
            italic: true,
            underline: true
        }
    );
    assert_eq!(parsed.input.source, NotificationSource::System);
    assert_eq!(parsed.input.urgency, Urgency::Critical);
    assert_eq!(parsed.input.server_id, Some(77));
    assert_eq!(parsed.input.replaces_id, 77);
    assert_eq!(parsed.input.ttl_ms, Some(1500.0));
    assert!(parsed.input.transient);
    assert_eq!(parsed.desktop_entry.as_deref(), Some("org.mail"));
    assert_eq!(parsed.category.as_deref(), Some("email.arrived"));
    assert_eq!(parsed.image_path.as_deref(), Some("/image/path"));
    assert_eq!(parsed.image_data, Some(image()));
    assert_eq!(parsed.origin_name.as_deref(), Some("KDE Mail"));
    assert_eq!(parsed.app_icon, "mail-unread");
    assert_eq!(
        parsed.input.actions,
        vec![NotificationAction {
            key: "open".into(),
            label: "Open".into()
        }]
    );
    let mut store = NotificationStore::default();
    store.insert(parsed.input, 10.0).unwrap();
    assert!(!store.list()[0].can_invoke_actions);
}

#[test]
fn legacy_hint_names_and_precedence_are_explicit_not_serde_aliases() {
    let mut notify = args();
    notify
        .hints
        .insert("image_path".into(), HintValue::String("legacy".into()));
    notify
        .hints
        .insert("icon_data".into(), HintValue::ImageData(image()));
    assert_eq!(
        notify.parse(1).unwrap().image_path.as_deref(),
        Some("legacy")
    );
    assert_eq!(notify.parse(1).unwrap().image_data, Some(image()));
    notify
        .hints
        .insert("image-path".into(), HintValue::String("modern".into()));
    assert_eq!(
        notify.parse(1).unwrap().image_path.as_deref(),
        Some("modern")
    );
    // A malformed shadowed alias is still rejected rather than silently bypassed.
    notify
        .hints
        .insert("image_path".into(), HintValue::Bool(false));
    assert!(notify.parse(1).is_err());
}

#[test]
fn notify_string_caps_count_utf8_bytes_before_markup_stripping() {
    let mut notify = args();
    notify.summary = "é".repeat(MAX_TITLE_BYTES / 2);
    notify.body = "é".repeat(MAX_BODY_BYTES / 2);
    notify.hints.insert(
        "category".into(),
        HintValue::String("é".repeat(MAX_HINT_STRING_BYTES / 2)),
    );
    assert!(notify.parse(1).is_ok());
    let mut too_long = notify.clone();
    too_long.summary.push('x');
    assert!(too_long.parse(1).is_err());
    let mut too_long = notify.clone();
    too_long.body.push('x');
    assert!(too_long.parse(1).is_err());
    let mut too_long = notify.clone();
    too_long.hints.insert(
        "unused".into(),
        HintValue::String("x".repeat(MAX_HINT_STRING_BYTES + 1)),
    );
    assert!(too_long.parse(1).is_err());
    let mut too_long = notify.clone();
    too_long.app_icon = "x".repeat(MAX_HINT_STRING_BYTES + 1);
    assert!(too_long.parse(1).is_err());
    let mut too_long = notify.clone();
    too_long.app_name = "x".repeat(MAX_NAME_BYTES + 1);
    assert!(too_long.parse(1).is_err());
    let mut too_long = notify.clone();
    too_long.actions[1] = "x".repeat(MAX_NAME_BYTES + 1);
    assert!(too_long.parse(1).is_err());
    let mut too_long = notify.clone();
    too_long.body = "<b>".repeat(MAX_BODY_BYTES);
    assert!(too_long.parse(1).is_err());
}

#[test]
fn notify_rejects_wrong_hint_types_invalid_urgency_actions_timeouts_and_hint_count() {
    for (name, value) in [
        ("urgency", HintValue::Byte(3)),
        ("urgency", HintValue::String("critical".into())),
        ("transient", HintValue::Byte(1)),
        ("desktop-entry", HintValue::Bool(true)),
        ("category", HintValue::Bool(true)),
        ("image-data", HintValue::String("x".into())),
        ("x-kde-origin-name", HintValue::Byte(1)),
    ] {
        let mut notify = args();
        notify.hints.insert(name.into(), value);
        assert!(notify.parse(1).is_err(), "{name}");
    }
    let mut notify = args();
    notify.actions.push("unpaired".into());
    assert!(notify.parse(1).is_err());
    notify.actions = vec!["action".into(); MAX_ACTIONS * 2 + 2];
    assert!(notify.parse(1).is_err());
    let mut notify = args();
    notify.expire_timeout = -2;
    assert!(notify.parse(1).is_err());
    notify.expire_timeout = i32::MAX;
    assert!(notify.parse(1).is_err());
    notify.expire_timeout = 0;
    assert_eq!(notify.parse(1).unwrap().input.ttl_ms, Some(0.0));
    for i in 0..MAX_HINTS {
        notify
            .hints
            .insert(format!("unknown-{i}"), HintValue::Other);
    }
    assert!(notify.parse(1).is_ok());
    notify.hints.insert("overflow".into(), HintValue::Other);
    assert!(notify.parse(1).is_err());
}

#[test]
fn image_hint_geometry_and_bytes_are_bounded_without_encoding_or_io() {
    let mut notify = args();
    let exact = ImageData {
        width: 128,
        height: 128,
        rowstride: 512,
        data: vec![0; 65536],
        ..image()
    };
    notify
        .hints
        .insert("image-data".into(), HintValue::ImageData(exact));
    assert!(notify.parse(1).is_ok());
    let mut invalid = vec![];
    let mut x = image();
    x.width = 129;
    invalid.push(x);
    let mut x = image();
    x.height = 0;
    invalid.push(x);
    let mut x = image();
    x.rowstride = u32::MAX;
    invalid.push(x);
    let mut x = image();
    x.rowstride = 1;
    invalid.push(x);
    let mut x = image();
    x.data.pop();
    invalid.push(x);
    let mut x = image();
    x.bits_per_sample = 16;
    invalid.push(x);
    let mut x = image();
    x.channels = 3;
    invalid.push(x);
    for candidate in invalid {
        notify
            .hints
            .insert("image-data".into(), HintValue::ImageData(candidate));
        assert!(notify.parse(1).is_err());
    }
}

#[test]
fn body_is_plain_text_including_malformed_unicode_and_once_decoded_entities() {
    let mut notify = args();
    notify.body = "<script>text</script>é &amp;lt; &unknown; &#x1f600; <unfinished".into();
    assert_eq!(
        notify.parse(1).unwrap().input.body.as_deref(),
        Some("texté &lt; &unknown; 😀 <unfinished")
    );
    notify.body = "<B>bold</B>".into();
    assert!(notify.parse(1).unwrap().formatting.bold);
    for (byte, expected) in [
        (0, Urgency::Low),
        (1, Urgency::Normal),
        (2, Urgency::Critical),
    ] {
        notify.hints.insert("urgency".into(), HintValue::Byte(byte));
        assert_eq!(notify.parse(1).unwrap().input.urgency, expected);
    }
}
