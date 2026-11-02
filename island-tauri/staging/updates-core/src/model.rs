//! The update model: [`Update`] records, the [`Source`] enum, and the
//! [`Badge`] computed by [`Badge::compute`]. Kernel/reboot classification
//! lives in [`is_kernel`].

use crate::error::UpdatesError;
use serde::{Deserialize, Serialize};

/// Seconds that make the badge (and the schedule's battery override) stale.
pub const STALE_AFTER_SECS: u64 = 6 * 60 * 60;

/// Where an update comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    /// `checkupdates` (repo packages).
    Repo,
    /// `paru -Qua` (AUR packages).
    Aur,
}

/// One `<pkgname> <oldver> -> <newver>` update line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Update {
    pub name: String,
    pub old_version: String,
    pub new_version: String,
    /// True when the line carried a trailing ` [ignored]` marker (pacman
    /// IgnorePkg). Ignored updates are excluded from counts.
    pub ignored: bool,
    pub source: Source,
}

impl Update {
    /// Whether updating this package likely needs a reboot.
    pub fn needs_reboot(&self) -> bool {
        is_kernel(&self.name)
    }
}

/// Reboot-needing package names: the kernel (`linux`, `linux-*`, incl.
/// `linux-cachyos` and headers) and graphics drivers (`nvidia*`, `mesa`).
/// The exact list is documented in VERIFICATION.md under Decisions.
pub fn is_kernel(name: &str) -> bool {
    name == "linux" || name.starts_with("linux-") || name.starts_with("nvidia") || name == "mesa"
}

/// The badge state shown by the island's package-updates tile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Badge {
    /// Actionable (non-ignored) update count. `None` = unknown (a source
    /// failed and no previous OK result existed); never fabricated as 0.
    pub count: Option<u32>,
    /// Non-ignored repo update count.
    pub repo_count: Option<u32>,
    /// Non-ignored AUR update count.
    pub aur_count: Option<u32>,
    /// True when any shown item needs a reboot.
    pub needs_reboot: bool,
    /// True when no successful repo check happened within the last 6 h.
    pub stale: bool,
    /// Current check error(s), when any source failed this round.
    pub error: Option<String>,
    /// All known updates (ignored included), sorted needsReboot first,
    /// then by name.
    pub items: Vec<Update>,
}

fn non_ignored(updates: &[Update]) -> u32 {
    updates.iter().filter(|u| !u.ignored).count() as u32
}

impl Badge {
    /// Compute the badge from the latest check results.
    ///
    /// `repo` / `aur` are the current outcomes. `aur = None` means the
    /// AUR check was not run and is *known empty*. `last_ok_secs` is the
    /// timestamp of the last successful repo check (`None` = never).
    /// `prev_ok` is the last successfully parsed full update list (`None`
    /// = never had one): when a source fails, its counts/items fall back
    /// to `prev_ok`'s entries for that source when available; otherwise
    /// the counts stay `None` (unknown, never fabricated as 0).
    pub fn compute(
        repo: Result<Vec<Update>, UpdatesError>,
        aur: Option<Result<Vec<Update>, UpdatesError>>,
        now_secs: u64,
        last_ok_secs: Option<u64>,
        prev_ok: Option<&[Update]>,
    ) -> Badge {
        let mut errors: Vec<String> = Vec::new();

        let repo_known = match repo {
            Ok(updates) => Some(updates),
            Err(e) => {
                errors.push(format!("repo: {e}"));
                prev_ok.map(|prev| {
                    prev.iter()
                        .filter(|u| u.source == Source::Repo)
                        .cloned()
                        .collect::<Vec<_>>()
                })
            }
        };

        let aur_known = match aur {
            None => Some(Vec::new()),
            Some(Ok(updates)) => Some(updates),
            Some(Err(e)) => {
                errors.push(format!("aur: {e}"));
                prev_ok.map(|prev| {
                    prev.iter()
                        .filter(|u| u.source == Source::Aur)
                        .cloned()
                        .collect::<Vec<_>>()
                })
            }
        };

        let (count, repo_count, aur_count) = match (&repo_known, &aur_known) {
            (Some(repo_items), Some(aur_items)) => {
                let repo_n = non_ignored(repo_items);
                let aur_n = non_ignored(aur_items);
                (Some(repo_n + aur_n), Some(repo_n), Some(aur_n))
            }
            _ => (None, None, None),
        };

        let mut items = Vec::new();
        if let Some(repo_items) = &repo_known {
            items.extend_from_slice(repo_items);
        }
        if let Some(aur_items) = &aur_known {
            items.extend_from_slice(aur_items);
        }
        items.sort_by(|a, b| {
            b.needs_reboot()
                .cmp(&a.needs_reboot())
                .then_with(|| a.name.cmp(&b.name))
        });
        let needs_reboot = items.iter().any(Update::needs_reboot);

        let stale = match last_ok_secs {
            None => true,
            Some(ok) => now_secs.saturating_sub(ok) > STALE_AFTER_SECS,
        };

        let error = if errors.is_empty() {
            None
        } else {
            Some(errors.join("; "))
        };

        Badge {
            count,
            repo_count,
            aur_count,
            needs_reboot,
            stale,
            error,
            items,
        }
    }
}
