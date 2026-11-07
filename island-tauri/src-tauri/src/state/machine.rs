//! Pure reducer with caller-injected monotonic clock; no Tauri, I/O or sleeping.
use crate::contract::{
    island::{HoverCause, IslandMode, IslandState, IslandView, UiIntent},
    settings::Settings,
};
use crate::error::IslandError;
use std::collections::BTreeMap;

pub trait Clock {
    fn now_ms(&self) -> u64;
}
/// How long an approval notice stays up on its own (a blocking question outlives a toast);
/// the waiting pulse and a later hover bring it back.
const APPROVAL_NOTICE_MS: u64 = 45_000;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Timer {
    HoverDelay,
    LeaveGrace,
    ToastTtl,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Deadline {
    pub at_ms: u64,
    pub generation: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    SetClipboardPaused(bool),
    RequestFocus,
    RestartTerminal,
    Quit,
    PlayClick,
    StartTerminal,
    EmitState,
}
#[derive(Clone, Debug)]
pub enum Intent {
    PointerEnter,
    PointerLeave,
    PanelEnter,
    PanelLeave,
    Pin,
    PanelPin,
    Collapse,
    DismissPreview(u32),
    SelectView(IslandView),
    SetDnd(bool),
    SetClipboardPaused(bool),
    RestartTerminal,
    Quit,
    NotificationArrived(String),
    NotificationsSync(u32),
    /// A notification popup card appeared (hub, `popups.rs` queue): grow the closed notch
    /// into it. Its lifetime belongs to the popup queue, so no toast timer is armed here.
    PopupShown(String),
    /// The popup queue emptied: a notification-caused hover closes.
    PopupsEmpty,
    /// A Hermes approval/clarify request is waiting (its island prompt id): pop the hover
    /// notice. It stays until answered, cleared or dismissed — no toast timer.
    ApprovalShown(String),
    /// That prompt went away (answered anywhere, settled, cancelled, terminal gone).
    ApprovalCleared(String),
    TimerFired(Timer, u32),
    UpsertActivity(Activity),
    RemoveActivity(String),
    ExpireActivities,
    /// KWin reported whether the focused (non-island, non-panel) window is fullscreen.
    Fullscreen(bool),
    /// `gameMode` / `gameModeAuto` changed; the effective game mode is recomputed.
    SyncGameMode,
}
impl From<UiIntent> for Intent {
    fn from(intent: UiIntent) -> Self {
        match intent {
            UiIntent::PointerEnter => Self::PointerEnter,
            UiIntent::PointerLeave => Self::PointerLeave,
            UiIntent::Pin => Self::Pin,
            UiIntent::Collapse => Self::Collapse,
            UiIntent::DismissPreview { rev } => Self::DismissPreview(rev),
            UiIntent::SelectView { view } => Self::SelectView(view),
            UiIntent::SetDnd { on } => Self::SetDnd(on),
            UiIntent::SetClipboardPaused { paused } => Self::SetClipboardPaused(paused),
            UiIntent::RestartTerminal => Self::RestartTerminal,
            UiIntent::Quit => Self::Quit,
        }
    }
}

/// Additive model, separate from the frozen Phase 2 IslandState wire shape.
/// Higher numbers win; ties use oldest start then lexical id, never arrival order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Activity {
    pub id: String,
    pub source: String,
    pub priority: i32,
    pub compact: bool,
    pub expanded: bool,
    pub started_ms: u64,
    pub expires_ms: Option<u64>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Presentation {
    pub compact: Vec<String>,
    pub expanded: Option<String>,
}
impl Presentation {
    pub fn split(&self) -> bool {
        self.compact.len() == 2
    }
}
#[derive(Clone, Debug)]
pub struct Machine {
    pub state: IslandState,
    timers: BTreeMap<Timer, Deadline>,
    generation: u32,
    pending_hover: Option<HoverCause>,
    /// Hover sources are tracked separately (SPEC §8 Phase 3, revised after review): a
    /// leave from a source that never entered is a no-op, because a real panel-enter run
    /// received a DOM `mouseleave` (no prior enter) during the preview shape change.
    pointer_inside: bool,
    panel_inside: bool,
    activities: BTreeMap<String, Activity>,
    presentation: Presentation,
    seen_notifications: std::collections::VecDeque<String>,
    /// Newest popup card still queued (PopupShown … PopupsEmpty): a hover that ends while
    /// one is queued falls back to showing it instead of closing.
    popup: Option<String>,
    /// Last fullscreen report from the KWin focus script (false when unknown/unavailable).
    fullscreen: bool,
}
/// Effective game mode: manual, or automatic while a fullscreen window has focus.
pub fn game_mode_active(settings: &Settings, fullscreen: bool) -> bool {
    settings.game_mode || (settings.game_mode_auto && fullscreen)
}
impl Machine {
    pub fn new(settings: &Settings) -> Self {
        Self {
            state: IslandState {
                rev: 0,
                mode: IslandMode::Still,
                view: IslandView::Terminal,
                hover_cause: None,
                dnd: settings.dnd,
                unread: 0,
                terminal_started: false,
                game_mode: game_mode_active(settings, false),
            },
            timers: BTreeMap::new(),
            generation: 0,
            pending_hover: None,
            pointer_inside: false,
            panel_inside: false,
            activities: BTreeMap::new(),
            presentation: Presentation::default(),
            seen_notifications: Default::default(),
            popup: None,
            fullscreen: false,
        }
    }
    pub fn fullscreen(&self) -> bool {
        self.fullscreen
    }
    pub fn pointer_inside(&self) -> bool {
        self.pointer_inside
    }
    pub fn panel_inside(&self) -> bool {
        self.panel_inside
    }
    pub fn timers(&self) -> &BTreeMap<Timer, Deadline> {
        &self.timers
    }
    pub fn presentation(&self) -> &Presentation {
        &self.presentation
    }
    pub fn activities(&self) -> &BTreeMap<String, Activity> {
        &self.activities
    }
    pub fn next_deadline(&self) -> Option<u64> {
        self.timers
            .values()
            .map(|d| d.at_ms)
            .chain(self.activities.values().filter_map(|a| a.expires_ms))
            .min()
    }
    fn schedule(&mut self, timer: Timer, delay: u64, now: u64) {
        self.generation = self.generation.wrapping_add(1);
        self.timers.insert(
            timer,
            Deadline {
                at_ms: now.saturating_add(delay),
                generation: self.generation,
            },
        );
    }
    fn cancel_all(&mut self) {
        self.timers.clear();
        self.pending_hover = None;
    }
    fn still(&mut self) {
        self.state.mode = IslandMode::Still;
        self.state.hover_cause = None;
    }
    fn presentation_at(&self, now: u64, game: bool) -> Presentation {
        let mut live: Vec<_> = self
            .activities
            .values()
            .filter(|a| a.expires_ms.is_none_or(|expiry| expiry > now))
            .collect();
        live.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then_with(|| a.started_ms.cmp(&b.started_ms))
                .then_with(|| a.id.cmp(&b.id))
        });
        Presentation {
            compact: live
                .iter()
                .filter(|a| a.compact)
                .take(2)
                .map(|a| a.id.clone())
                .collect(),
            // Game mode: activities stay compact, nothing expands over the game.
            expanded: (!game)
                .then(|| live.iter().find(|a| a.expanded).map(|a| a.id.clone()))
                .flatten(),
        }
    }
    /// All timer callbacks must carry their scheduled generation. Canceled/early
    /// callbacks are inert, even if the same timer was rescheduled meanwhile.
    pub fn reduce(
        &mut self,
        intent: Intent,
        settings: &Settings,
        clock: &impl Clock,
    ) -> Result<Vec<Effect>, IslandError> {
        let now = clock.now_ms();
        if let Intent::UpsertActivity(a) = &intent {
            if a.id.is_empty()
                || a.id.len() > 256
                || a.source.is_empty()
                || a.source.len() > 256
                || a.id.chars().any(char::is_control)
                || a.source.chars().any(char::is_control)
                || a.started_ms > now
                || a.expires_ms.is_some_and(|end| end < a.started_ms)
                || (!self.activities.contains_key(&a.id) && self.activities.len() >= 128)
            {
                return Err(IslandError::invalid("Invalid or excess activity"));
            }
        }
        let before = self.state.clone();
        let previous = self.presentation.clone();
        let mut effects = Vec::new();
        // Game mode is decided before the intent so every suppression below sees it.
        let fullscreen_focus = matches!(intent, Intent::Fullscreen(true));
        if let Intent::Fullscreen(on) = &intent {
            let on = *on;
            self.fullscreen = on;
        }
        let game = game_mode_active(settings, self.fullscreen);
        let game_effects =
            crate::integrations::plasma::game_effects::GameEffects::plan(game, settings);
        if game && !self.state.game_mode {
            // Entering: drop a pending or showing preview/toast. An island the user opened
            // explicitly stays open (e.g. the side-control toggle itself).
            self.timers.remove(&Timer::HoverDelay);
            self.pending_hover = None;
            if self.state.mode == IslandMode::Hover
                && (game_effects.suppress_notifications
                    || !matches!(
                        self.state.hover_cause,
                        Some(HoverCause::Notification { .. })
                    ))
            {
                self.cancel_all();
                self.still();
            }
        }
        if game && fullscreen_focus && self.state.mode != IslandMode::Still {
            // A fullscreen window (a game) just took focus: get out of its way.
            self.cancel_all();
            self.still();
        }
        self.state.game_mode = game;
        // The game DND checkbox can change while the game is already active.
        if game_effects.suppress_notifications
            && matches!(
                self.state.hover_cause,
                Some(HoverCause::Notification { .. })
            )
        {
            self.cancel_all();
            self.still();
        }
        match intent {
            Intent::PointerEnter | Intent::PanelEnter => {
                let panel = matches!(intent, Intent::PanelEnter);
                if panel {
                    self.panel_inside = true;
                } else {
                    self.pointer_inside = true;
                }
                // Any enter during the grace period cancels it.
                self.timers.remove(&Timer::LeaveGrace);
                if self.state.mode == IslandMode::Still {
                    if settings.hover_delay > 0. && !game {
                        self.pending_hover = Some(if panel {
                            HoverCause::PanelPill
                        } else {
                            HoverCause::Pointer
                        });
                        self.schedule(Timer::HoverDelay, settings.hover_delay as u64, now);
                    } else {
                        self.timers.remove(&Timer::HoverDelay);
                        self.pending_hover = None;
                    }
                } else if self.state.mode == IslandMode::Hover {
                    self.timers.remove(&Timer::ToastTtl);
                    self.state.hover_cause = Some(if panel {
                        HoverCause::PanelPill
                    } else {
                        HoverCause::Pointer
                    });
                }
            }
            Intent::PointerLeave | Intent::PanelLeave => {
                let was_inside = if matches!(intent, Intent::PanelLeave) {
                    std::mem::take(&mut self.panel_inside)
                } else {
                    std::mem::take(&mut self.pointer_inside)
                };
                // A leave from a source that is not inside is a no-op: it must neither
                // cancel a pending hover nor start the leave grace.
                if was_inside && !self.pointer_inside && !self.panel_inside {
                    self.timers.remove(&Timer::HoverDelay);
                    self.pending_hover = None;
                    if self.state.mode == IslandMode::Hover {
                        self.schedule(Timer::LeaveGrace, 300, now);
                    }
                }
            }
            Intent::TimerFired(timer, generation) => {
                if self
                    .timers
                    .get(&timer)
                    .is_some_and(|d| d.generation == generation && d.at_ms <= now)
                {
                    self.timers.remove(&timer);
                    match timer {
                        Timer::HoverDelay if self.state.mode == IslandMode::Still && !game => {
                            self.state.mode = IslandMode::Hover;
                            self.state.hover_cause = self.pending_hover.take();
                            self.schedule(Timer::ToastTtl, 6000, now);
                        }
                        Timer::LeaveGrace if self.state.mode == IslandMode::Hover => {
                            // A queued popup card is still showing: hand the notch back to it.
                            let keep = self.popup.clone().filter(|_| {
                                !self.state.dnd
                                    && !game_effects.suppress_notifications
                                    && settings.notifications_enabled
                            });
                            match keep {
                                Some(id) => {
                                    self.timers.remove(&Timer::ToastTtl);
                                    self.state.hover_cause = Some(HoverCause::Notification { id });
                                }
                                None => self.still(),
                            }
                        }
                        Timer::ToastTtl if self.state.mode == IslandMode::Hover => {
                            if self.pointer_inside || self.panel_inside {
                                self.schedule(Timer::ToastTtl, 6000, now);
                            } else {
                                self.still();
                            }
                        }
                        _ => {}
                    }
                }
            }
            Intent::Pin | Intent::PanelPin => {
                let toggle =
                    matches!(intent, Intent::PanelPin) && self.state.mode == IslandMode::Large;
                self.cancel_all();
                if toggle {
                    self.still();
                } else if self.state.mode != IslandMode::Large {
                    self.state.mode = IslandMode::Large;
                    self.state.hover_cause = None;
                    effects.push(Effect::RequestFocus);
                    if !self.state.terminal_started {
                        self.state.terminal_started = true;
                        effects.push(Effect::StartTerminal);
                    }
                } else {
                    effects.push(Effect::RequestFocus);
                }
                if settings.click_sounds {
                    effects.push(Effect::PlayClick);
                }
            }
            Intent::DismissPreview(rev) => {
                if self.state.mode == IslandMode::Hover && self.state.rev == rev {
                    self.cancel_all();
                    self.still();
                }
            }
            Intent::Collapse => {
                self.cancel_all();
                self.still();
            }
            Intent::SelectView(view) => {
                self.cancel_all();
                self.state.view = view;
                if view == IslandView::Terminal && !self.state.terminal_started {
                    self.state.terminal_started = true;
                    effects.push(Effect::StartTerminal);
                }
                if self.state.mode != IslandMode::Large {
                    self.state.mode = IslandMode::Large;
                    effects.push(Effect::RequestFocus);
                }
                self.state.hover_cause = None;
            }
            Intent::SetDnd(on) => self.state.dnd = on,
            Intent::SetClipboardPaused(paused) => effects.push(Effect::SetClipboardPaused(paused)),
            Intent::RestartTerminal => effects.push(Effect::RestartTerminal),
            Intent::Quit => effects.push(Effect::Quit),
            Intent::NotificationsSync(unread) => self.state.unread = unread,
            Intent::ApprovalShown(id) => {
                // Like a toast, an approval never barges over DnD, a game or an open island;
                // hovering the notch (or the waiting pulse) still reveals it.
                if settings.approvals_in_notch && !self.state.dnd && !game {
                    match self.state.mode {
                        IslandMode::Still => {
                            self.cancel_all();
                            self.state.mode = IslandMode::Hover;
                            self.state.hover_cause = Some(HoverCause::Alert { id });
                            self.schedule(Timer::ToastTtl, APPROVAL_NOTICE_MS, now);
                        }
                        IslandMode::Hover if !self.pointer_inside && !self.panel_inside => {
                            self.state.hover_cause = Some(HoverCause::Alert { id });
                            self.schedule(Timer::ToastTtl, APPROVAL_NOTICE_MS, now);
                        }
                        _ => {}
                    }
                }
            }
            Intent::ApprovalCleared(id) => {
                if self.state.mode == IslandMode::Hover
                    && self.state.hover_cause == Some(HoverCause::Alert { id })
                {
                    self.cancel_all();
                    self.still();
                }
            }
            Intent::NotificationArrived(id) => {
                // Repeated service polling is not a new event. Bound retention; a new
                // notification id (including a new agent transition) can notify again.
                if self.seen_notifications.contains(&id) {
                    return Ok(effects);
                }
                if self.seen_notifications.len() == 128 {
                    self.seen_notifications.pop_front();
                }
                self.seen_notifications.push_back(id.clone());
                self.state.unread = self.state.unread.saturating_add(1);
                // Game mode keeps it quietly, like DnD: stored and counted, never toasted.
                if self.state.mode == IslandMode::Still
                    && !self.state.dnd
                    && !game_effects.suppress_notifications
                    && settings.notifications_enabled
                {
                    self.cancel_all();
                    self.state.mode = IslandMode::Hover;
                    self.state.hover_cause = Some(HoverCause::Notification { id });
                    self.schedule(Timer::ToastTtl, settings.toast_ttl_ms as u64, now);
                }
            }
            Intent::PopupShown(id) => {
                self.popup = Some(id.clone());
                if !self.state.dnd
                    && !game_effects.suppress_notifications
                    && settings.notifications_enabled
                {
                    match self.state.mode {
                        IslandMode::Still => {
                            self.cancel_all();
                            self.state.mode = IslandMode::Hover;
                            self.state.hover_cause = Some(HoverCause::Notification { id });
                        }
                        // An approval notice keeps the notch; the card waits in the queue.
                        IslandMode::Hover
                            if !matches!(
                                self.state.hover_cause,
                                Some(HoverCause::Alert { .. })
                            ) =>
                        {
                            self.timers.remove(&Timer::ToastTtl);
                            if !self.pointer_inside && !self.panel_inside {
                                self.state.hover_cause = Some(HoverCause::Notification { id });
                            }
                        }
                        _ => {}
                    }
                }
            }
            Intent::PopupsEmpty => {
                self.popup = None;
                if self.state.mode == IslandMode::Hover
                    && matches!(
                        self.state.hover_cause,
                        Some(HoverCause::Notification { .. })
                    )
                    && !self.pointer_inside
                    && !self.panel_inside
                {
                    self.cancel_all();
                    self.still();
                }
            }
            Intent::UpsertActivity(activity) => {
                self.activities.insert(activity.id.clone(), activity);
            }
            Intent::RemoveActivity(id) => {
                self.activities.remove(&id);
            }
            Intent::ExpireActivities | Intent::Fullscreen(_) | Intent::SyncGameMode => {}
        }
        self.activities
            .retain(|_, a| a.expires_ms.is_none_or(|expiry| expiry > now));
        self.presentation = self.presentation_at(now, game_effects.suppress_notifications);
        if self.state != before || self.presentation != previous {
            self.state.rev = self.state.rev.wrapping_add(1);
            effects.push(Effect::EmitState);
        }
        Ok(effects)
    }
    pub fn tick(
        &mut self,
        settings: &Settings,
        clock: &impl Clock,
    ) -> Result<Vec<Effect>, IslandError> {
        let now = clock.now_ms();
        let due: Vec<_> = self
            .timers
            .iter()
            .filter(|(_, d)| d.at_ms <= now)
            .map(|(t, d)| (*t, d.generation))
            .collect();
        let mut effects = Vec::new();
        for (timer, generation) in due {
            effects.extend(self.reduce(Intent::TimerFired(timer, generation), settings, clock)?);
        }
        effects.extend(self.reduce(Intent::ExpireActivities, settings, clock)?);
        Ok(effects)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compact_preview_dismiss_expiry_polling_and_terminal_lifetime() {
        let s = Settings::default();
        let mut m = Machine::new(&s);
        let effects = step(&mut m, Intent::Pin, &s, 0);
        assert!(effects.contains(&Effect::StartTerminal));
        step(&mut m, Intent::Collapse, &s, 1);
        step(
            &mut m,
            Intent::NotificationArrived("agent:run-1".into()),
            &s,
            2,
        );
        let rev = m.state.rev;
        let effects = step(&mut m, Intent::DismissPreview(rev), &s, 3);
        assert_eq!(m.state.mode, IslandMode::Still);
        assert!(m.state.terminal_started);
        assert!(!effects.contains(&Effect::StartTerminal));
        for now in 4..20 {
            step(
                &mut m,
                Intent::NotificationArrived("agent:run-1".into()),
                &s,
                now,
            );
        }
        assert_eq!(m.state.mode, IslandMode::Still);
        step(
            &mut m,
            Intent::NotificationArrived("agent:run-2".into()),
            &s,
            20,
        );
        assert_eq!(m.state.mode, IslandMode::Hover);
        step(&mut m, Intent::DismissPreview(rev), &s, 21); // stale click cannot dismiss a new event
        assert_eq!(m.state.mode, IslandMode::Hover);
        m.tick(&s, &Fake(6020)).unwrap();
        assert_eq!(m.state.mode, IslandMode::Still);
        step(
            &mut m,
            Intent::NotificationArrived("agent:run-2".into()),
            &s,
            6021,
        );
        assert_eq!(m.state.mode, IslandMode::Still);
        let effects = step(&mut m, Intent::Pin, &s, 6022);
        assert!(!effects.contains(&Effect::StartTerminal));
        let current = m.state.rev;
        step(&mut m, Intent::DismissPreview(current), &s, 6023);
        assert_eq!(m.state.mode, IslandMode::Large);
        for now in 6024..6030 {
            let effects = step(&mut m, Intent::Pin, &s, now); // transfer uses Pin, not toggle
            assert!(!effects.contains(&Effect::StartTerminal));
            assert_eq!(m.state.mode, IslandMode::Large);
        }
        step(&mut m, Intent::PanelPin, &s, 6030); // same monitor toggles
        assert_eq!(m.state.mode, IslandMode::Still);
    }

    struct Fake(u64);
    impl Clock for Fake {
        fn now_ms(&self) -> u64 {
            self.0
        }
    }
    fn step(m: &mut Machine, i: Intent, s: &Settings, now: u64) -> Vec<Effect> {
        m.reduce(i, s, &Fake(now)).unwrap()
    }
    fn hover(m: &mut Machine, s: &Settings) {
        step(m, Intent::PanelEnter, s, 0);
        m.tick(s, &Fake(180)).unwrap();
    }
    #[test]
    fn hover_delay_zero_disables_and_pointer_panel_causes_are_distinct() {
        let mut s = Settings::default();
        let mut m = Machine::new(&s);
        step(&mut m, Intent::PanelEnter, &s, 0);
        assert_eq!(m.state.mode, IslandMode::Still);
        m.tick(&s, &Fake(179)).unwrap();
        assert_eq!(m.state.mode, IslandMode::Still);
        m.tick(&s, &Fake(180)).unwrap();
        assert_eq!(m.state.hover_cause, Some(HoverCause::PanelPill));
        step(&mut m, Intent::Collapse, &s, 181);
        step(&mut m, Intent::PointerEnter, &s, 182);
        m.tick(&s, &Fake(362)).unwrap();
        assert_eq!(m.state.hover_cause, Some(HoverCause::Pointer));
        step(&mut m, Intent::Collapse, &s, 363);
        s.hover_delay = 0.;
        step(&mut m, Intent::PanelEnter, &s, 364);
        m.tick(&s, &Fake(9999)).unwrap();
        assert_eq!(m.state.mode, IslandMode::Still);
        assert!(m.timers.is_empty());
    }
    #[test]
    fn leave_grace_crossing_and_cancelled_generations_cannot_fire_late() {
        let s = Settings::default();
        let mut m = Machine::new(&s);
        hover(&mut m, &s);
        step(&mut m, Intent::PanelLeave, &s, 200);
        let old = m.timers[&Timer::LeaveGrace].generation;
        step(&mut m, Intent::PointerEnter, &s, 300);
        step(&mut m, Intent::TimerFired(Timer::LeaveGrace, old), &s, 600);
        assert_eq!(m.state.mode, IslandMode::Hover);
        step(&mut m, Intent::PointerLeave, &s, 601);
        m.tick(&s, &Fake(900)).unwrap();
        assert_eq!(m.state.mode, IslandMode::Hover);
        m.tick(&s, &Fake(901)).unwrap();
        assert_eq!(m.state.mode, IslandMode::Still);
        step(&mut m, Intent::PanelEnter, &s, 902);
        let generation = m.timers[&Timer::HoverDelay].generation;
        step(&mut m, Intent::PanelLeave, &s, 903);
        step(&mut m, Intent::PanelEnter, &s, 904);
        step(
            &mut m,
            Intent::TimerFired(Timer::HoverDelay, generation),
            &s,
            2000,
        );
        assert_eq!(m.state.mode, IslandMode::Still);
    }
    #[test]
    fn pin_panel_toggle_collapse_retain_terminal_and_focus_only_explicit_entry() {
        let s = Settings {
            click_sounds: true,
            ..Settings::default()
        };
        for prehover in [false, true] {
            let mut m = Machine::new(&s);
            if prehover {
                hover(&mut m, &s);
            }
            let e = step(&mut m, Intent::Pin, &s, 200);
            assert!(e.contains(&Effect::RequestFocus));
            assert!(e.contains(&Effect::StartTerminal));
            assert!(e.contains(&Effect::PlayClick));
            assert_eq!(m.state.mode, IslandMode::Large);
            assert!(m.timers.is_empty());
            // An explicit palette request must reclaim focus even when already open.
            assert!(step(&mut m, Intent::Pin, &s, 201).contains(&Effect::RequestFocus));
            step(&mut m, Intent::PanelPin, &s, 202);
            assert_eq!(m.state.mode, IslandMode::Still);
            assert!(m.state.terminal_started);
            let e = step(&mut m, Intent::PanelPin, &s, 203);
            assert!(e.contains(&Effect::RequestFocus));
            assert!(!e.contains(&Effect::StartTerminal));
            step(&mut m, Intent::Collapse, &s, 204);
            assert_eq!(m.state.mode, IslandMode::Still);
            assert!(m.state.terminal_started);
        }
    }
    #[test]
    fn select_view_in_every_mode_and_dnd_in_every_mode() {
        let s = Settings::default();
        for mode in [IslandMode::Still, IslandMode::Hover, IslandMode::Large] {
            let mut m = Machine::new(&s);
            m.state.mode = mode;
            let e = step(&mut m, Intent::SelectView(IslandView::Settings), &s, 0);
            assert_eq!(m.state.view, IslandView::Settings);
            assert_eq!(m.state.mode, IslandMode::Large);
            assert_eq!(e.contains(&Effect::RequestFocus), mode != IslandMode::Large);
            for on in [true, false] {
                step(&mut m, Intent::SetDnd(on), &s, 1);
                assert_eq!(m.state.dnd, on);
            }
        }
    }
    #[test]
    fn popup_cards_grow_the_notch_and_own_its_lifetime() {
        let mut s = Settings::default();
        let mut m = Machine::new(&s);
        step(&mut m, Intent::PopupShown("a".into()), &s, 0);
        assert_eq!(m.state.mode, IslandMode::Hover);
        assert_eq!(m.state.hover_cause, Some(HoverCause::Notification { id: "a".into() }));
        assert!(!m.timers.contains_key(&Timer::ToastTtl), "the popup queue owns the lifetime");
        // Hover the card, leave: the notch returns to the queued card instead of closing.
        step(&mut m, Intent::PointerEnter, &s, 10);
        assert_eq!(m.state.hover_cause, Some(HoverCause::Pointer));
        step(&mut m, Intent::PointerLeave, &s, 20);
        let generation = m.timers[&Timer::LeaveGrace].generation;
        step(&mut m, Intent::TimerFired(Timer::LeaveGrace, generation), &s, 400);
        assert_eq!(m.state.mode, IslandMode::Hover);
        assert_eq!(m.state.hover_cause, Some(HoverCause::Notification { id: "a".into() }));
        // A newer card takes over; the emptied queue closes the notch.
        step(&mut m, Intent::PopupShown("b".into()), &s, 500);
        assert_eq!(m.state.hover_cause, Some(HoverCause::Notification { id: "b".into() }));
        step(&mut m, Intent::PopupsEmpty, &s, 600);
        assert_eq!(m.state.mode, IslandMode::Still);
        // Leaving after the queue emptied closes as before.
        step(&mut m, Intent::PointerEnter, &s, 700);
        m.tick(&s, &Fake(900)).unwrap();
        step(&mut m, Intent::PointerLeave, &s, 950);
        let generation = m.timers[&Timer::LeaveGrace].generation;
        step(&mut m, Intent::TimerFired(Timer::LeaveGrace, generation), &s, 1300);
        assert_eq!(m.state.mode, IslandMode::Still);
        // DnD, disabled notifications and an open island never grow the notch.
        step(&mut m, Intent::SetDnd(true), &s, 1400);
        step(&mut m, Intent::PopupShown("c".into()), &s, 1401);
        assert_eq!(m.state.mode, IslandMode::Still);
        step(&mut m, Intent::SetDnd(false), &s, 1402);
        s.notifications_enabled = false;
        step(&mut m, Intent::PopupShown("d".into()), &s, 1403);
        assert_eq!(m.state.mode, IslandMode::Still);
        s.notifications_enabled = true;
        step(&mut m, Intent::Pin, &s, 1404);
        step(&mut m, Intent::PopupShown("e".into()), &s, 1405);
        assert_eq!(m.state.mode, IslandMode::Large);
        // An empty queue never closes an island the user opened.
        step(&mut m, Intent::PopupsEmpty, &s, 1406);
        assert_eq!(m.state.mode, IslandMode::Large);
    }

    #[test]
    fn notification_ttl_reading_dnd_disabled_and_large_unread_rows() {
        let mut s = Settings::default();
        let mut m = Machine::new(&s);
        step(&mut m, Intent::NotificationArrived("n".into()), &s, 0);
        assert_eq!(
            m.state.hover_cause,
            Some(HoverCause::Notification { id: "n".into() })
        );
        let generation = m.timers[&Timer::ToastTtl].generation;
        step(&mut m, Intent::PointerEnter, &s, 1);
        assert_eq!(m.state.hover_cause, Some(HoverCause::Pointer));
        step(
            &mut m,
            Intent::TimerFired(Timer::ToastTtl, generation),
            &s,
            10000,
        );
        assert_eq!(m.state.mode, IslandMode::Hover);
        step(&mut m, Intent::Collapse, &s, 10001);
        // Expiry pauses only while hovered; leave before the next notification.
        step(&mut m, Intent::PointerLeave, &s, 10001);
        step(
            &mut m,
            Intent::NotificationArrived("next".into()),
            &s,
            10002,
        );
        m.tick(&s, &Fake(16002)).unwrap();
        assert_eq!(m.state.mode, IslandMode::Still);
        step(&mut m, Intent::SetDnd(true), &s, 16003);
        step(
            &mut m,
            Intent::NotificationArrived("hidden".into()),
            &s,
            16004,
        );
        assert_eq!(m.state.mode, IslandMode::Still);
        step(&mut m, Intent::SetDnd(false), &s, 16005);
        s.notifications_enabled = false;
        step(
            &mut m,
            Intent::NotificationArrived("disabled".into()),
            &s,
            16006,
        );
        assert_eq!(m.state.mode, IslandMode::Still);
        step(&mut m, Intent::Pin, &s, 16007);
        let n = m.state.unread;
        let e = step(
            &mut m,
            Intent::NotificationArrived("large".into()),
            &s,
            16008,
        );
        assert_eq!(m.state.mode, IslandMode::Large);
        assert_eq!(m.state.unread, n + 1);
        assert!(!e.contains(&Effect::RequestFocus));
    }
    /// Controller audit (a): a callback that carries the CURRENT generation but arrives
    /// before the deadline must be inert; only the clock decides when a timer is due.
    #[test]
    fn current_generation_timer_before_its_deadline_is_ignored() {
        let s = Settings::default();
        let mut m = Machine::new(&s);
        step(&mut m, Intent::PanelEnter, &s, 0);
        let hover = m.timers[&Timer::HoverDelay].clone();
        assert_eq!(hover.at_ms, 180);
        step(
            &mut m,
            Intent::TimerFired(Timer::HoverDelay, hover.generation),
            &s,
            179,
        );
        assert_eq!(m.state.mode, IslandMode::Still);
        assert_eq!(
            m.timers.get(&Timer::HoverDelay),
            Some(&hover),
            "an early callback must not consume the timer"
        );
        step(
            &mut m,
            Intent::TimerFired(Timer::HoverDelay, hover.generation),
            &s,
            180,
        );
        assert_eq!(m.state.mode, IslandMode::Hover);
        // The same holds for the leave grace.
        step(&mut m, Intent::PanelLeave, &s, 200);
        let grace = m.timers[&Timer::LeaveGrace].clone();
        step(
            &mut m,
            Intent::TimerFired(Timer::LeaveGrace, grace.generation),
            &s,
            499,
        );
        assert_eq!(m.state.mode, IslandMode::Hover);
        step(
            &mut m,
            Intent::TimerFired(Timer::LeaveGrace, grace.generation),
            &s,
            500,
        );
        assert_eq!(m.state.mode, IslandMode::Still);
    }
    /// SPEC §8 Phase 3 (revised): the original failing sequence — panel enter, hover, then a
    /// DOM `mouseleave` the pointer never "entered" — must keep the preview open.
    #[test]
    fn spurious_leave_from_a_source_that_never_entered_is_a_no_op() {
        let s = Settings::default();
        let mut m = Machine::new(&s);
        step(&mut m, Intent::PanelEnter, &s, 0);
        m.tick(&s, &Fake(180)).unwrap();
        assert_eq!(m.state.mode, IslandMode::Hover);
        let rev = m.state.rev;
        let e = step(&mut m, Intent::PointerLeave, &s, 200);
        assert!(e.is_empty());
        assert!(
            !m.timers.contains_key(&Timer::LeaveGrace),
            "no LeaveGrace for a spurious leave"
        );
        assert_eq!(m.state.rev, rev, "a no-op leave must not bump the revision");
        m.tick(&s, &Fake(10_000)).unwrap();
        assert_eq!(m.state.mode, IslandMode::Hover);
        assert!(m.panel_inside() && !m.pointer_inside());
        // The real leave of the source that IS inside starts the grace.
        step(&mut m, Intent::PanelLeave, &s, 10_001);
        assert!(!m.panel_inside());
        // ... and a spurious leave INSIDE that grace must not restart (extend) it.
        let e = step(&mut m, Intent::PointerLeave, &s, 10_200);
        assert!(e.is_empty());
        m.tick(&s, &Fake(10_300)).unwrap();
        assert_eq!(m.state.mode, IslandMode::Hover);
        m.tick(&s, &Fake(10_301)).unwrap();
        assert_eq!(m.state.mode, IslandMode::Still);
        // A spurious leave while a hover is only pending must not cancel it either.
        step(&mut m, Intent::PanelLeave, &s, 11_000);
        step(&mut m, Intent::PointerEnter, &s, 11_001);
        step(&mut m, Intent::PanelLeave, &s, 11_002);
        m.tick(&s, &Fake(11_181)).unwrap();
        assert_eq!(m.state.mode, IslandMode::Hover);
        assert_eq!(m.state.hover_cause, Some(HoverCause::Pointer));
    }
    /// LeaveGrace is scheduled only when, after a leave, NEITHER source is inside; any
    /// enter during the grace cancels it.
    #[test]
    fn leave_grace_needs_both_sources_outside_and_any_enter_cancels_it() {
        let s = Settings::default();
        let mut m = Machine::new(&s);
        step(&mut m, Intent::PanelEnter, &s, 0);
        m.tick(&s, &Fake(180)).unwrap();
        step(&mut m, Intent::PointerEnter, &s, 200);
        assert!(m.panel_inside() && m.pointer_inside());
        // panel pill -> popup crossing: the panel leaves while the pointer is inside.
        step(&mut m, Intent::PanelLeave, &s, 210);
        assert!(m.timers.is_empty());
        m.tick(&s, &Fake(5_000)).unwrap();
        assert_eq!(m.state.mode, IslandMode::Hover);
        step(&mut m, Intent::PointerLeave, &s, 5_001);
        assert!(m.timers.contains_key(&Timer::LeaveGrace));
        // either source entering inside the grace cancels it
        for (round, enter) in [Intent::PanelEnter, Intent::PointerEnter]
            .into_iter()
            .enumerate()
        {
            let t = 6_000 + round as u64 * 10_000;
            step(&mut m, enter, &s, t);
            assert!(!m.timers.contains_key(&Timer::LeaveGrace));
            m.tick(&s, &Fake(t + 3_000)).unwrap();
            assert_eq!(m.state.mode, IslandMode::Hover);
            step(&mut m, Intent::PanelLeave, &s, t + 3_001);
            step(&mut m, Intent::PointerLeave, &s, t + 3_002);
            assert!(m.timers.contains_key(&Timer::LeaveGrace));
        }
        m.tick(&s, &Fake(16_000 + 3_302)).unwrap();
        assert_eq!(m.state.mode, IslandMode::Still);
    }
    fn activity(id: &str, priority: i32, start: u64, expiry: Option<u64>) -> Activity {
        Activity {
            id: id.into(),
            source: "fixture".into(),
            priority,
            compact: true,
            expanded: true,
            started_ms: start,
            expires_ms: expiry,
        }
    }
    #[test]
    fn activity_priority_ties_expiry_split_merge_and_availability() {
        let s = Settings::default();
        let mut m = Machine::new(&s);
        step(
            &mut m,
            Intent::UpsertActivity(activity("b", 1, 0, None)),
            &s,
            10,
        );
        assert_eq!(m.presentation.compact, ["b"]);
        assert!(!m.presentation.split());
        step(
            &mut m,
            Intent::UpsertActivity(activity("a", 1, 0, None)),
            &s,
            10,
        );
        assert_eq!(m.presentation.compact, ["a", "b"]);
        step(
            &mut m,
            Intent::UpsertActivity(activity("high", 9, 5, Some(20))),
            &s,
            10,
        );
        assert_eq!(m.presentation.compact, ["high", "a"]);
        assert_eq!(m.presentation.expanded.as_deref(), Some("high"));
        m.tick(&s, &Fake(20)).unwrap();
        assert_eq!(m.presentation.compact, ["a", "b"]);
        let mut only_expanded = activity("expanded", 10, 0, None);
        only_expanded.compact = false;
        step(&mut m, Intent::UpsertActivity(only_expanded), &s, 21);
        assert_eq!(m.presentation.compact, ["a", "b"]);
        assert_eq!(m.presentation.expanded.as_deref(), Some("expanded"));
        step(&mut m, Intent::RemoveActivity("a".into()), &s, 22);
        assert_eq!(m.presentation.compact, ["b"]);
        assert!(!m.presentation.split());
        step(
            &mut m,
            Intent::UpsertActivity(activity("later", 1, 2, None)),
            &s,
            23,
        );
        assert_eq!(m.presentation.compact, ["b", "later"]);
        step(&mut m, Intent::RemoveActivity("expanded".into()), &s, 24);
        step(&mut m, Intent::RemoveActivity("later".into()), &s, 25);
        step(&mut m, Intent::RemoveActivity("b".into()), &s, 26);
        assert_eq!(m.presentation, Presentation::default());
        assert!(m.activities.is_empty());
    }
    #[test]
    fn invalid_activity_is_atomic_and_revision_wraps() {
        let s = Settings::default();
        let mut m = Machine::new(&s);
        let before = m.state.clone();
        assert!(m
            .reduce(
                Intent::UpsertActivity(activity("", 1, 0, None)),
                &s,
                &Fake(0)
            )
            .is_err());
        assert_eq!(m.state, before);
        assert!(m.activities.is_empty());
        m.state.rev = u32::MAX;
        step(&mut m, Intent::SetDnd(true), &s, 0);
        assert_eq!(m.state.rev, 0);
        let rev = m.state.rev;
        step(&mut m, Intent::SetDnd(true), &s, 1);
        assert_eq!(m.state.rev, rev);
    }

    #[test]
    fn game_effects_dnd_checkbox_is_effective_only_and_resumes_without_replaying() {
        let mut s = Settings {
            game_mode: true,
            game_mode_dnd: false,
            ..Settings::default()
        };
        let mut m = Machine::new(&s);
        step(&mut m, Intent::NotificationArrived("allowed".into()), &s, 0);
        assert_eq!(m.state.mode, IslandMode::Hover);
        assert!(!m.state.dnd && !s.dnd);
        s.game_mode_dnd = true;
        step(&mut m, Intent::SyncGameMode, &s, 1);
        assert_eq!(m.state.mode, IslandMode::Still);
        step(&mut m, Intent::NotificationArrived("quiet".into()), &s, 2);
        assert_eq!(m.state.mode, IslandMode::Still);
        assert_eq!(m.state.unread, 2);
        assert!(!m.state.dnd && !s.dnd);
        s.game_mode = false;
        step(&mut m, Intent::SyncGameMode, &s, 3);
        assert_eq!(
            m.state.mode,
            IslandMode::Still,
            "no suppressed toast replay"
        );
        step(&mut m, Intent::NotificationArrived("resumed".into()), &s, 4);
        assert_eq!(m.state.mode, IslandMode::Hover);
        step(&mut m, Intent::SetDnd(true), &s, 5);
        s.game_mode = true;
        step(&mut m, Intent::SyncGameMode, &s, 6);
        s.game_mode = false;
        step(&mut m, Intent::SyncGameMode, &s, 7);
        assert!(m.state.dnd, "manual DND survives game exit");
    }

    #[test]
    fn game_mode_is_manual_or_auto_fullscreen_and_suppresses_previews_and_toasts() {
        let mut s = Settings::default();
        assert!(s.game_mode_auto && !s.game_mode);
        let mut m = Machine::new(&s);
        assert!(!m.state.game_mode);
        // Auto: a fullscreen focus turns it on, any other focus turns it off.
        step(&mut m, Intent::Fullscreen(true), &s, 0);
        assert!(m.state.game_mode && m.fullscreen());
        // No hover preview from the pointer or the panel pill, not even after the delay.
        step(&mut m, Intent::PointerEnter, &s, 1);
        step(&mut m, Intent::PanelEnter, &s, 2);
        m.tick(&s, &Fake(10_000)).unwrap();
        assert_eq!(m.state.mode, IslandMode::Still);
        assert!(!m.timers.contains_key(&Timer::HoverDelay));
        step(&mut m, Intent::PointerLeave, &s, 10_001);
        step(&mut m, Intent::PanelLeave, &s, 10_002);
        // A notification is kept quietly: counted, never toasted.
        let unread = m.state.unread;
        step(
            &mut m,
            Intent::NotificationArrived("n-1".into()),
            &s,
            10_003,
        );
        assert_eq!(m.state.unread, unread + 1);
        assert_eq!(m.state.mode, IslandMode::Still);
        assert!(m.state.hover_cause.is_none());
        // Activities stay compact; nothing auto-expands.
        step(
            &mut m,
            Intent::UpsertActivity(activity("timer", 5, 0, None)),
            &s,
            10_004,
        );
        assert_eq!(m.presentation().compact, vec!["timer".to_owned()]);
        assert_eq!(m.presentation().expanded, None);
        // Explicit opens still work: a notch click (or the shortcut) toggles the island.
        let effects = step(&mut m, Intent::PanelPin, &s, 10_005);
        assert_eq!(m.state.mode, IslandMode::Large);
        assert!(effects.contains(&Effect::RequestFocus));
        step(&mut m, Intent::PanelPin, &s, 10_006);
        assert_eq!(m.state.mode, IslandMode::Still);
        // Leaving fullscreen ends auto game mode; previews and expansion come back.
        step(&mut m, Intent::Fullscreen(false), &s, 10_007);
        assert!(!m.state.game_mode);
        assert_eq!(m.presentation().expanded, Some("timer".to_owned()));
        step(
            &mut m,
            Intent::NotificationArrived("n-2".into()),
            &s,
            10_008,
        );
        assert_eq!(m.state.mode, IslandMode::Hover);
        // Auto off: fullscreen is ignored. Manual on: active regardless of focus.
        step(&mut m, Intent::Collapse, &s, 10_009);
        s.game_mode_auto = false;
        step(&mut m, Intent::SyncGameMode, &s, 10_010);
        step(&mut m, Intent::Fullscreen(true), &s, 10_011);
        assert!(!m.state.game_mode);
        s.game_mode = true;
        let rev = m.state.rev;
        let effects = step(&mut m, Intent::SyncGameMode, &s, 10_012);
        assert!(m.state.game_mode);
        assert!(
            effects.contains(&Effect::EmitState),
            "the UI must learn the new state"
        );
        assert_ne!(m.state.rev, rev);
    }

    #[test]
    fn entering_game_mode_drops_previews_but_keeps_an_explicitly_opened_island() {
        let mut s = Settings::default();
        let mut m = Machine::new(&s);
        // A pending hover delay is cancelled.
        step(&mut m, Intent::PointerEnter, &s, 0);
        step(&mut m, Intent::Fullscreen(true), &s, 1);
        m.tick(&s, &Fake(5_000)).unwrap();
        assert_eq!(m.state.mode, IslandMode::Still);
        step(&mut m, Intent::PointerLeave, &s, 5_001);
        step(&mut m, Intent::Fullscreen(false), &s, 5_002);
        // A showing toast/preview is dismissed.
        step(&mut m, Intent::NotificationArrived("n".into()), &s, 5_003);
        assert_eq!(m.state.mode, IslandMode::Hover);
        step(&mut m, Intent::Fullscreen(true), &s, 5_004);
        assert_eq!(m.state.mode, IslandMode::Still);
        assert!(m.timers.is_empty());
        // Manual toggle from the open island (side control) keeps it open.
        step(&mut m, Intent::Fullscreen(false), &s, 5_005);
        step(&mut m, Intent::Pin, &s, 5_006);
        s.game_mode = true;
        step(&mut m, Intent::SyncGameMode, &s, 5_007);
        assert!(m.state.game_mode);
        assert_eq!(m.state.mode, IslandMode::Large);
        // ...but a fullscreen window taking focus closes it again.
        step(&mut m, Intent::Fullscreen(true), &s, 5_008);
        assert_eq!(m.state.mode, IslandMode::Still);
        // Re-opening over the game is allowed and stays until the game is focused again.
        step(&mut m, Intent::PanelPin, &s, 5_009);
        assert_eq!(m.state.mode, IslandMode::Large);
        step(&mut m, Intent::Fullscreen(true), &s, 5_010);
        assert_eq!(m.state.mode, IslandMode::Still);
        // Without game mode a fullscreen focus never closes the island.
        s.game_mode = false;
        s.game_mode_auto = false;
        step(&mut m, Intent::SyncGameMode, &s, 5_011);
        step(&mut m, Intent::Pin, &s, 5_012);
        step(&mut m, Intent::Fullscreen(true), &s, 5_013);
        assert_eq!(m.state.mode, IslandMode::Large);
    }
}
