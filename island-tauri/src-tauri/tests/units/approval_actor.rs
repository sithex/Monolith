use hermes_island_lib::{
    contract::{
        island::{HoverCause, IslandMode},
        settings::Settings,
    },
    state::machine::{Clock, Intent, Machine, Timer},
};
struct Now(u64);
impl Clock for Now {
    fn now_ms(&self) -> u64 {
        self.0
    }
}
fn shown(machine: &mut Machine, settings: &Settings, id: &str, at: u64) {
    machine
        .reduce(Intent::ApprovalShown(id.into()), settings, &Now(at))
        .unwrap();
}
#[test]
fn an_approval_pops_the_hover_notice_and_leaves_with_its_prompt() {
    let settings = Settings::default();
    let mut machine = Machine::new(&settings);
    shown(&mut machine, &settings, "ap1-1", 0);
    assert_eq!(machine.state.mode, IslandMode::Hover);
    assert_eq!(
        machine.state.hover_cause,
        Some(HoverCause::Alert { id: "ap1-1".into() })
    );
    // Answered anywhere: the notice goes away with the prompt.
    machine
        .reduce(Intent::ApprovalCleared("ap1-1".into()), &settings, &Now(500))
        .unwrap();
    assert_eq!(machine.state.mode, IslandMode::Still);
}
#[test]
fn clearing_a_different_prompt_keeps_the_notice() {
    let settings = Settings::default();
    let mut machine = Machine::new(&settings);
    shown(&mut machine, &settings, "ap1-2", 0);
    machine
        .reduce(Intent::ApprovalCleared("ap1-1".into()), &settings, &Now(10))
        .unwrap();
    assert_eq!(machine.state.mode, IslandMode::Hover);
}
#[test]
fn the_notice_times_out_by_itself_but_not_under_the_pointer() {
    let settings = Settings::default();
    let mut machine = Machine::new(&settings);
    shown(&mut machine, &settings, "ap1-1", 0);
    assert!(machine.timers().contains_key(&Timer::ToastTtl));
    machine.tick(&settings, &Now(60_000)).unwrap();
    assert_eq!(machine.state.mode, IslandMode::Still);
    shown(&mut machine, &settings, "ap1-2", 61_000);
    machine
        .reduce(Intent::PointerEnter, &settings, &Now(61_100))
        .unwrap();
    machine.tick(&settings, &Now(200_000)).unwrap();
    assert_eq!(machine.state.mode, IslandMode::Hover);
}
#[test]
fn the_setting_dnd_and_game_mode_keep_the_notch_quiet() {
    let off = Settings {
        approvals_in_notch: false,
        ..Settings::default()
    };
    let mut machine = Machine::new(&off);
    shown(&mut machine, &off, "ap1-1", 0);
    assert_eq!(machine.state.mode, IslandMode::Still);
    let on = Settings::default();
    let mut machine = Machine::new(&on);
    machine.reduce(Intent::SetDnd(true), &on, &Now(0)).unwrap();
    shown(&mut machine, &on, "ap1-1", 1);
    assert_eq!(machine.state.mode, IslandMode::Still);
    let game = Settings {
        game_mode: true,
        ..Settings::default()
    };
    let mut machine = Machine::new(&game);
    shown(&mut machine, &game, "ap1-1", 0);
    assert_eq!(machine.state.mode, IslandMode::Still);
}
#[test]
fn an_open_island_is_not_disturbed() {
    let settings = Settings::default();
    let mut machine = Machine::new(&settings);
    machine
        .reduce(Intent::PointerEnter, &settings, &Now(0))
        .unwrap();
    machine.tick(&settings, &Now(1_000)).unwrap();
    let before = machine.state.mode;
    shown(&mut machine, &settings, "ap1-1", 2_000);
    assert_eq!(machine.state.mode, before);
}
