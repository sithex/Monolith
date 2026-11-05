use hermes_island_lib::{
    contract::{island::IslandMode, settings::Settings},
    state::machine::{Clock, Intent, Machine, Timer},
};
struct Now(u64);
impl Clock for Now {
    fn now_ms(&self) -> u64 {
        self.0
    }
}
#[test]
fn pointer_enter_removes_toast_timer_not_just_its_hover_cause() {
    let settings = Settings::default();
    let mut machine = Machine::new(&settings);
    machine
        .reduce(
            Intent::NotificationArrived("fixture".into()),
            &settings,
            &Now(0),
        )
        .unwrap();
    assert!(machine.timers().contains_key(&Timer::ToastTtl));
    machine
        .reduce(Intent::PointerEnter, &settings, &Now(100))
        .unwrap();
    assert!(
        !machine.timers().contains_key(&Timer::ToastTtl),
        "a stale scheduled timer is not cancellation"
    );
    machine.tick(&settings, &Now(7000)).unwrap();
    assert_eq!(machine.state.mode, IslandMode::Hover);
}
#[test]
fn actor_dnd_prevents_notification_hover_but_counts_arrivals() {
    let settings = Settings::default();
    let mut machine = Machine::new(&settings);
    machine
        .reduce(Intent::SetDnd(true), &settings, &Now(0))
        .unwrap();
    machine
        .reduce(
            Intent::NotificationArrived("silent".into()),
            &settings,
            &Now(1),
        )
        .unwrap();
    assert_eq!(machine.state.mode, IslandMode::Still);
    assert_eq!(machine.state.unread, 1);
    assert!(machine.timers().is_empty());
}
