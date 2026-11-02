//! Power-profile tile logic (power-profiles-daemon).

use crate::model::{DndTile, PowerTile, PpdSnapshot, Profile};

/// `power_tile` result when no snapshot is available.
pub const UNAVAILABLE_REASON: &str = "power-profiles-daemon not running";

/// Build the power-profile tile from a daemon snapshot. A `None`
/// snapshot means the daemon is not running (`available: false`).
/// Unknown profile strings in `profiles` are ignored and counted in
/// `ignored`; they never fail the snapshot. An unknown `active` yields
/// `active: None` but the tile stays available.
pub fn power_tile(snapshot: Option<&PpdSnapshot>) -> PowerTile {
    let Some(snap) = snapshot else {
        return PowerTile {
            available: false,
            unavailable_reason: Some(UNAVAILABLE_REASON.to_string()),
            active: None,
            choices: Vec::new(),
            degraded: None,
            held_by: Vec::new(),
            ignored: 0,
        };
    };

    let mut seen = [false; 3];
    let mut ignored = 0usize;
    for p in &snap.profiles {
        match p.profile.parse() {
            Ok(Profile::PowerSaver) => seen[0] = true,
            Ok(Profile::Balanced) => seen[1] = true,
            Ok(Profile::Performance) => seen[2] = true,
            Err(_) => ignored += 1,
        }
    }

    let mut choices = Vec::new();
    for (profile, &present) in Profile::ALL.iter().zip(seen.iter()) {
        if present {
            choices.push(*profile);
        }
    }

    let active = snap.active.parse().ok();
    let degraded = if snap.degraded.is_empty() {
        None
    } else {
        Some(snap.degraded.clone())
    };
    let held_by = snap
        .holds
        .iter()
        .map(|h| h.reason.clone())
        .filter(|r| !r.is_empty())
        .collect();

    PowerTile {
        available: true,
        unavailable_reason: None,
        active,
        choices,
        degraded,
        held_by,
        ignored,
    }
}

/// Build the Do Not Disturb tile (trivial; the settings module owns the
/// underlying state).
pub fn dnd_tile(enabled: bool) -> DndTile {
    DndTile { on: enabled }
}

/// The next profile when toggling: cycles through `choices` in canonical
/// order and wraps. `None` when there are fewer than two choices or the
/// active profile is unknown (or absent from `choices`).
pub fn next_profile(tile: &PowerTile) -> Option<Profile> {
    let active = tile.active?;
    if tile.choices.len() < 2 {
        return None;
    }
    let pos = tile.choices.iter().position(|&c| c == active)?;
    Some(tile.choices[(pos + 1) % tile.choices.len()])
}
