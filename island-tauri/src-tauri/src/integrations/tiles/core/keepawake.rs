//! The Keep-awake state machine (SPEC §0.5 item 1). Models the session-bus
//! `org.freedesktop.ScreenSaver` Inhibit/UnInhibit lifecycle: `turn_on`
//! emits an [`Action::Inhibit`] and enters [`KeepAwakeState::Requesting`];
//! the bus reply ([`KeepAwake::inhibited`]) is tracked as
//! [`KeepAwakeState::On`] with its cookie; `turn_off` releases the cookie. A
//! *pending cancel* (`turn_off` while still `Requesting`) guarantees a late
//! `inhibited` reply is immediately released — a cookie is never leaked. The
//! clock is injectable: every time-dependent method takes `now_ms`. This
//! module never talks to any bus.

use serde::{Deserialize, Serialize};

/// The application name passed to `Inhibit`.
pub const INHIBIT_APP: &str = "Hermes Island";

/// 30 minutes in milliseconds (timed keep-awake option).
pub const AUTO_OFF_30M: u64 = 30 * 60 * 1000;

/// 1 hour in milliseconds.
pub const AUTO_OFF_1H: u64 = 60 * 60 * 1000;

/// 2 hours in milliseconds.
pub const AUTO_OFF_2H: u64 = 2 * 60 * 60 * 1000;

/// An action the owning actor must perform (a D-Bus call).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    /// Call `Inhibit(application_name, reason)` on
    /// `org.freedesktop.ScreenSaver`.
    Inhibit { app: String, reason: String },
    /// Call `UnInhibit(cookie)`.
    UnInhibit { cookie: u32 },
}

/// Observable state of the keep-awake machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeepAwakeState {
    /// Not inhibiting.
    Off,
    /// Inhibit sent, reply not yet received. `cancel_pending` records a
    /// `turn_off` that arrived before the reply: the reply's cookie must be
    /// released immediately (never leaked).
    Requesting { cancel_pending: bool, since_ms: u64 },
    /// Inhibit confirmed; `cookie` is held until released.
    On { cookie: u32, since_ms: u64 },
}

/// The keep-awake state machine. Pure: no IO, no clock of its own — every
/// time-dependent call takes `now_ms`. It never panics.
#[derive(Debug, Clone)]
pub struct KeepAwake {
    state: KeepAwakeState,
    last_error: Option<String>,
    auto_off_at_ms: Option<u64>,
}

impl KeepAwake {
    /// A fresh machine in [`KeepAwakeState::Off`].
    pub fn new() -> Self {
        Self {
            state: KeepAwakeState::Off,
            last_error: None,
            auto_off_at_ms: None,
        }
    }

    /// The current state.
    pub fn state(&self) -> KeepAwakeState {
        self.state.clone()
    }

    /// The last failure message (from [`Self::failed`]); cleared on the next
    /// [`Self::turn_on`].
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// The absolute auto-off deadline, if a timed keep-awake is configured.
    pub fn auto_off_at_ms(&self) -> Option<u64> {
        self.auto_off_at_ms
    }

    /// Set the auto-off deadline as an absolute millisecond timestamp;
    /// `None` means "until off".
    pub fn set_auto_off_at_ms(&mut self, at_ms: Option<u64>) {
        self.auto_off_at_ms = at_ms;
    }

    /// Set the auto-off as a duration from `now_ms`; `None` means "until
    /// off".
    pub fn set_auto_off(&mut self, duration_ms: Option<u64>, now_ms: u64) {
        self.auto_off_at_ms = duration_ms.map(|d| now_ms.saturating_add(d));
    }

    /// Request keep-awake. From [`KeepAwakeState::Off`] emits an
    /// [`Action::Inhibit`] and enters [`KeepAwakeState::Requesting`];
    /// otherwise a no-op (`None`).
    pub fn turn_on(&mut self, now_ms: u64, reason: impl Into<String>) -> Option<Action> {
        match &self.state {
            KeepAwakeState::Off => {
                self.state = KeepAwakeState::Requesting {
                    cancel_pending: false,
                    since_ms: now_ms,
                };
                self.last_error = None;
                Some(Action::Inhibit {
                    app: INHIBIT_APP.to_string(),
                    reason: reason.into(),
                })
            }
            _ => None,
        }
    }

    /// The bus confirmed an inhibit with `cookie`. From a clean `Requesting`
    /// enters [`KeepAwakeState::On`]. When a cancel is pending (or in any
    /// other state, defensively) the cookie is released immediately so it can
    /// never leak.
    pub fn inhibited(&mut self, cookie: u32) -> Option<Action> {
        match &self.state {
            KeepAwakeState::Requesting {
                cancel_pending: true,
                ..
            } => {
                self.state = KeepAwakeState::Off;
                Some(Action::UnInhibit { cookie })
            }
            KeepAwakeState::Requesting {
                cancel_pending: false,
                since_ms,
            } => {
                self.state = KeepAwakeState::On {
                    cookie,
                    since_ms: *since_ms,
                };
                None
            }
            KeepAwakeState::On { .. } | KeepAwakeState::Off => {
                // An unexpected/spurious reply: release it (never hold a
                // cookie we did not ask to track).
                Some(Action::UnInhibit { cookie })
            }
        }
    }

    /// The Inhibit call failed with `msg`. From `Requesting` this returns to
    /// [`KeepAwakeState::Off`] and records the error; otherwise it is
    /// ignored.
    pub fn failed(&mut self, msg: impl Into<String>) {
        if matches!(&self.state, KeepAwakeState::Requesting { .. }) {
            self.state = KeepAwakeState::Off;
            self.last_error = Some(msg.into());
        }
    }

    /// Stop keep-awake. From [`KeepAwakeState::On`] releases the cookie and
    /// returns to [`KeepAwakeState::Off`]; from `Requesting` marks a pending
    /// cancel (no action — the late reply will be released); otherwise a
    /// no-op. Any configured auto-off deadline is cleared.
    pub fn turn_off(&mut self) -> Option<Action> {
        let action = match &self.state {
            KeepAwakeState::On { cookie, .. } => {
                let cookie = *cookie;
                self.state = KeepAwakeState::Off;
                Some(Action::UnInhibit { cookie })
            }
            KeepAwakeState::Requesting {
                cancel_pending: false,
                since_ms,
            } => {
                let since_ms = *since_ms;
                self.state = KeepAwakeState::Requesting {
                    cancel_pending: true,
                    since_ms,
                };
                None
            }
            _ => None,
        };
        self.auto_off_at_ms = None;
        action
    }

    /// Advance the clock. When a timed auto-off is configured and `now_ms`
    /// has reached it, a live [`KeepAwakeState::On`] releases its cookie (and
    /// the deadline is consumed); a still-`Requesting` state marks a pending
    /// cancel instead (there is no cookie yet to release).
    pub fn tick(&mut self, now_ms: u64) -> Option<Action> {
        let due = self.auto_off_at_ms.is_some_and(|t| now_ms >= t);
        if !due {
            return None;
        }
        match &self.state {
            KeepAwakeState::On { cookie, .. } => {
                let cookie = *cookie;
                self.state = KeepAwakeState::Off;
                self.auto_off_at_ms = None;
                Some(Action::UnInhibit { cookie })
            }
            KeepAwakeState::Requesting {
                cancel_pending: false,
                since_ms,
            } => {
                let since_ms = *since_ms;
                self.state = KeepAwakeState::Requesting {
                    cancel_pending: true,
                    since_ms,
                };
                self.auto_off_at_ms = None;
                None
            }
            _ => None,
        }
    }

    /// The session bus was lost: all cookies die with the connection. Returns
    /// to [`KeepAwakeState::Off`] and clears the auto-off deadline. No
    /// [`Action::UnInhibit`] is emitted (the connection is gone).
    pub fn on_bus_lost(&mut self) {
        self.state = KeepAwakeState::Off;
        self.auto_off_at_ms = None;
    }
}

impl Default for KeepAwake {
    fn default() -> Self {
        Self::new()
    }
}
