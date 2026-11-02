#![allow(clippy::unwrap_used)]
#[path = "helpers/controls_fixture.rs"]
mod controls_fixture;
#[path = "helpers/private_bus.rs"]
mod private_bus;
use controls_fixture::ControlsMock;
use hermes_island_lib::{
    contract::controls::{BrightnessOp, BrightnessStep},
    integrations::brightness::{
        core::{BrightnessModel, Event},
        BrightnessService,
    },
};
use private_bus::{wait_until, PrivateBus};
use serde_json::json;
use std::{sync::Arc, time::Duration};
struct Service(Arc<BrightnessService>);
impl Drop for Service {
    fn drop(&mut self) {
        self.0.shutdown().unwrap();
    }
}
fn ready(service: &BrightnessService) {
    wait_until(
        "private PowerDevil snapshot",
        Duration::from_secs(5),
        || service.current().available.then_some(()),
    );
}
fn value(service: &BrightnessService) -> Option<u8> {
    service.current().displays.first().and_then(|d| d.percent)
}
fn initial(tag: &str) -> (PrivateBus, ControlsMock, Service) {
    let bus = PrivateBus::start(tag);
    let mock = ControlsMock::start(&bus);
    let service = Service(BrightnessService::start(Some(bus.address())));
    ready(&service.0);
    assert_eq!(value(&service.0), Some(50));
    (bus, mock, service)
}

#[tokio::test]
async fn brightness_private_methods_events_loss_recovery_and_shutdown() {
    let (bus, mock, service) = initial("6b2-brightness");
    let mut changes = service.0.topic.subscribe();
    service
        .0
        .request(BrightnessOp::Drag {
            name: "display0".into(),
            percent: 61,
        })
        .await
        .unwrap();
    service
        .0
        .request(BrightnessOp::Release {
            name: "display0".into(),
        })
        .await
        .unwrap();
    service
        .0
        .request(BrightnessOp::Step {
            action: BrightnessStep::DecreaseSmall,
        })
        .await
        .unwrap();
    assert_eq!(
        mock.calls(),
        vec![
            json!([
                "/org/kde/ScreenBrightness/display0",
                "SetBrightnessWithContext",
                [6100, 1, "hermes-island"]
            ]),
            json!([
                "/org/kde/ScreenBrightness",
                "AdjustBrightnessStepWithContext",
                [3, 1, "hermes-island"]
            ]),
        ]
    );
    changes.changed().await.unwrap();
    assert!(changes.borrow_and_update().as_ref().unwrap().rev > 0);
    drop(mock);
    wait_until(
        "PowerDevil unavailable after name loss",
        Duration::from_secs(5),
        || (!service.0.current().available).then_some(()),
    );
    assert!(service.0.current().displays.is_empty());
    let _replacement = ControlsMock::start(&bus);
    ready(&service.0);
    assert_eq!(value(&service.0), Some(50));
}

#[tokio::test]
async fn brightness_settle_deadline_polls_without_any_followup_signal_or_command() {
    let (_bus, mut mock, service) = initial("6b2-settle");
    mock.send(json!({"op":"echo","on":false}));
    service
        .0
        .request(BrightnessOp::Drag {
            name: "display0".into(),
            percent: 80,
        })
        .await
        .unwrap();
    service
        .0
        .request(BrightnessOp::Release {
            name: "display0".into(),
        })
        .await
        .unwrap();
    assert_eq!(value(&service.0), Some(80));
    // The confirmed value is still 50. Only the scheduled core poll can end the optimistic
    // settle display; this test deliberately sends no D-Bus signal or second command.
    wait_until(
        "500ms brightness settle deadline",
        Duration::from_secs(2),
        || (value(&service.0) == Some(50)).then_some(()),
    );
}

async fn rogue(
    tag: &str,
    member: &str,
    signature: &str,
    args: serde_json::Value,
    changed: impl Fn(&BrightnessService) -> bool,
) {
    let (_bus, mut mock, service) = initial(tag);
    let before = service.0.current();
    let mut topic = service.0.topic.subscribe();
    topic.borrow_and_update();
    mock.send(json!({"op":"emit","rogue":true,"member":member,"signature":signature,"args":args}));
    assert!(
        tokio::time::timeout(Duration::from_millis(250), topic.changed())
            .await
            .is_err(),
        "rogue {member} emitted an event"
    );
    assert_eq!(service.0.current(), before);
    // Positive control uses exactly the same body from the actual owner.
    mock.send(json!({"op":"emit","member":member,"signature":signature,"args":args}));
    wait_until(member, Duration::from_secs(3), || {
        changed(&service.0).then_some(())
    });
}
#[tokio::test]
async fn brightness_rogue_brightness_changed_is_ignored() {
    rogue(
        "6b2-rogue-value",
        "BrightnessChanged",
        "(siss)",
        json!(["display0", 9900, ":fake", "foreign"]),
        |s| value(s) == Some(99),
    )
    .await;
}
#[tokio::test]
async fn brightness_rogue_range_changed_is_ignored() {
    rogue(
        "6b2-rogue-range",
        "BrightnessRangeChanged",
        "(sii)",
        json!(["display0", 20000, 5000]),
        |s| value(s) == Some(25),
    )
    .await;
}
#[tokio::test]
async fn brightness_rogue_display_added_is_ignored() {
    rogue(
        "6b2-rogue-add",
        "DisplayAdded",
        "(s)",
        json!(["display1"]),
        |s| s.current().displays.len() == 2,
    )
    .await;
}
#[tokio::test]
async fn brightness_rogue_display_removed_is_ignored() {
    rogue(
        "6b2-rogue-remove",
        "DisplayRemoved",
        "(s)",
        json!(["display0"]),
        |s| s.current().displays.is_empty(),
    )
    .await;
}
#[tokio::test]
async fn brightness_rogue_name_owner_changed_is_ignored() {
    let (_bus, mut mock, service) = initial("6b2-rogue-owner");
    let before = service.0.current();
    let mut topic = service.0.topic.subscribe();
    topic.borrow_and_update();
    mock.send(json!({"op":"rogue_noc"}));
    assert!(
        tokio::time::timeout(Duration::from_millis(250), topic.changed())
            .await
            .is_err()
    );
    assert_eq!(service.0.current(), before);
    drop(mock);
    wait_until("legitimate owner loss", Duration::from_secs(3), || {
        (!service.0.current().available).then_some(())
    });
}
#[tokio::test]
async fn brightness_failed_write_is_unavailable_and_recovers_without_optimistic_stale_value() {
    let (_bus, mut mock, service) = initial("6b2-write-failure");
    mock.send(json!({"op":"fail","on":true}));
    assert!(service
        .0
        .request(BrightnessOp::Drag {
            name: "display0".into(),
            percent: 70
        })
        .await
        .is_err());
    assert!(!service.0.current().available);
    assert!(service.0.current().displays.is_empty());
    mock.send(json!({"op":"fail","on":false}));
    ready(&service.0);
    assert_eq!(value(&service.0), Some(50));
    service
        .0
        .request(BrightnessOp::Drag {
            name: "display0".into(),
            percent: 70,
        })
        .await
        .unwrap();
    assert_eq!(
        mock.calls().len(),
        2,
        "same target must be sent again after the failed write"
    );
}
#[test]
fn brightness_port_metadata_and_unavailable_range_do_not_fabricate_values() {
    let mut m = BrightnessModel::new();
    m.apply(Event::DisplayNames(vec!["display0".into()]));
    let props = |label: &str, internal| Event::Props {
        name: "display0".into(),
        label: label.into(),
        is_internal: internal,
        max_brightness: 100,
        brightness: 50,
    };
    assert!(m.apply(props("one", false)).changed);
    assert!(m.apply(props("two", false)).changed);
    assert!(m.apply(props("two", true)).changed);
    assert!(!m.apply(props("two", true)).changed);
    m.drag("display0", 80, 0).unwrap();
    m.apply(Event::RangeChanged {
        name: "display0".into(),
        max_brightness: 0,
        brightness: 0,
    });
    assert_eq!(m.displays()[0].percent, None);
    m.release("display0", 10);
    assert_eq!(m.displays()[0].percent, None);
}
#[test]
fn brightness_port_failed_write_clears_drag_and_settle_for_a_same_target_retry() {
    let mut m = BrightnessModel::new();
    m.apply(Event::DisplayNames(vec!["display0".into()]));
    m.apply(Event::Props {
        name: "display0".into(),
        label: "one".into(),
        is_internal: false,
        max_brightness: 100,
        brightness: 50,
    });
    m.drag("display0", 80, 0).unwrap();
    assert_eq!(m.poll(0).len(), 1);
    m.write_failed("display0");
    assert_eq!(m.displays()[0].percent, Some(50));
    m.drag("display0", 80, 1).unwrap();
    assert_eq!(m.poll(1).len(), 1);
    m.release("display0", 2);
    m.write_failed("display0");
    assert_eq!(m.displays()[0].percent, Some(50));
}
#[tokio::test]
async fn brightness_missing_explicit_test_bus_never_uses_the_real_bus() {
    let service = Service(BrightnessService::start(None));
    wait_until(
        "unconfigured brightness bus",
        Duration::from_secs(2),
        || (service.0.current().detail == "no-session-bus-configured").then_some(()),
    );
    assert!(!service.0.current().available);
    assert!(service.0.current().displays.is_empty());
    assert!(service
        .0
        .request(BrightnessOp::Step {
            action: BrightnessStep::Increase
        })
        .await
        .is_err());
}
