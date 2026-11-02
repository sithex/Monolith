use crate::ArrangeError;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Maximum number of grid items.
pub const MAX_ITEMS: usize = 256;
/// Maximum id length in bytes.
pub const MAX_ID_BYTES: usize = 64;

/// One tile in the app grid. `locked` items (Hermes terminal home, Settings)
/// can be moved but never disabled; the core always treats a locked item as
/// enabled even if the input claims otherwise (see Decisions).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GridItem {
    pub id: String,
    pub enabled: bool,
    pub locked: bool,
}

/// One enabled-flag difference between the snapshot and the committed state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnabledChange {
    pub id: String,
    pub enabled: bool,
}

/// Result of `Arrangement::commit`. `order` is a full permutation of all ids
/// (visible and hidden); `enabled_changes` lists only ids whose enabled flag
/// differs from the snapshot, in `order` order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArrangeCommit {
    pub order: Vec<String>,
    pub enabled_changes: Vec<EnabledChange>,
}

/// The launcher grid arrangement. Internal order is the single source of
/// truth: `visible` = enabled items in that order, `hidden` = disabled items
/// in that order. While editing, `snapshot` holds the pre-edit state for
/// cancel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arrangement {
    items: Vec<GridItem>,
    snapshot: Option<Vec<GridItem>>,
}

impl Arrangement {
    /// Build an arrangement from the module list. Validates: non-empty ids,
    /// each id ≤ 64 bytes, unique ids, ≤ 256 items. Error precedence: empty →
    /// first too-long id → first duplicate id → too many items. A locked item
    /// is forced enabled (locked dominates).
    pub fn new(items: Vec<GridItem>) -> Result<Self, ArrangeError> {
        if items.is_empty() {
            return Err(ArrangeError::EmptyIds);
        }
        let mut seen = HashSet::with_capacity(items.len());
        let mut normalized = Vec::with_capacity(items.len());
        for it in &items {
            if it.id.len() > MAX_ID_BYTES {
                return Err(ArrangeError::IdTooLong);
            }
            if !seen.insert(&it.id) {
                return Err(ArrangeError::DuplicateId);
            }
            normalized.push(GridItem {
                id: it.id.clone(),
                enabled: it.enabled || it.locked,
                locked: it.locked,
            });
        }
        if normalized.len() > MAX_ITEMS {
            return Err(ArrangeError::TooManyItems);
        }
        Ok(Arrangement {
            items: normalized,
            snapshot: None,
        })
    }

    /// The enabled items, in internal order.
    pub fn visible(&self) -> Vec<&GridItem> {
        self.items.iter().filter(|i| i.enabled).collect()
    }

    /// The disabled items, in internal order.
    pub fn hidden(&self) -> Vec<&GridItem> {
        self.items.iter().filter(|i| !i.enabled).collect()
    }

    /// Whether an edit session is currently open.
    pub fn is_editing(&self) -> bool {
        self.snapshot.is_some()
    }

    /// Open an edit session, snapshotting the current state. Errors with
    /// `AlreadyEditing` if a session is already open.
    pub fn begin_edit(&mut self) -> Result<(), ArrangeError> {
        if self.snapshot.is_some() {
            return Err(ArrangeError::AlreadyEditing);
        }
        self.snapshot = Some(self.items.clone());
        Ok(())
    }

    /// Move a visible item to a VISIBLE-grid index. `to` is clamped to the
    /// last visible slot; moving an item onto its own slot is a no-op; hidden
    /// items keep their relative order (and positions) among all items.
    pub fn move_item(&mut self, id: &str, to_visible_index: usize) -> Result<(), ArrangeError> {
        if self.snapshot.is_none() {
            return Err(ArrangeError::NotEditing);
        }
        let pos = self
            .items
            .iter()
            .position(|i| i.id == id)
            .ok_or(ArrangeError::UnknownId)?;
        if !self.items[pos].enabled {
            return Err(ArrangeError::NotVisible);
        }
        let current_visible = self.items[..pos].iter().filter(|i| i.enabled).count();
        let visible_count = self.items.iter().filter(|i| i.enabled).count();
        let last = visible_count - 1;
        let target = to_visible_index.min(last);
        if target == current_visible {
            return Ok(());
        }
        let item = self.items.remove(pos);
        let mut seen_visible = 0usize;
        let mut insert_at = self.items.len();
        for (i, it) in self.items.iter().enumerate() {
            if it.enabled {
                if seen_visible == target {
                    insert_at = i;
                    break;
                }
                seen_visible += 1;
            }
        }
        self.items.insert(insert_at, item);
        Ok(())
    }

    /// Disable an item. Locked items are rejected (`Locked`); disabling an
    /// already-disabled item is a no-op.
    pub fn disable(&mut self, id: &str) -> Result<(), ArrangeError> {
        if self.snapshot.is_none() {
            return Err(ArrangeError::NotEditing);
        }
        let pos = self
            .items
            .iter()
            .position(|i| i.id == id)
            .ok_or(ArrangeError::UnknownId)?;
        if self.items[pos].locked {
            return Err(ArrangeError::Locked);
        }
        if !self.items[pos].enabled {
            return Ok(());
        }
        self.items[pos].enabled = false;
        Ok(())
    }

    /// Re-enable a hidden item, placing it at the end of the visible grid
    /// (just after the last enabled item; hidden items' positions are
    /// otherwise untouched). Enabling an already-visible item is a no-op.
    pub fn enable(&mut self, id: &str) -> Result<(), ArrangeError> {
        if self.snapshot.is_none() {
            return Err(ArrangeError::NotEditing);
        }
        let pos = self
            .items
            .iter()
            .position(|i| i.id == id)
            .ok_or(ArrangeError::UnknownId)?;
        if self.items[pos].enabled {
            return Ok(());
        }
        let mut item = self.items.remove(pos);
        item.enabled = true;
        let insert_at = match self.items.iter().rposition(|i| i.enabled) {
            Some(last_enabled) => last_enabled + 1,
            None => 0,
        };
        self.items.insert(insert_at, item);
        Ok(())
    }

    /// Restore the exact snapshot and end the session.
    pub fn cancel(&mut self) -> Result<(), ArrangeError> {
        let snap = self.snapshot.take().ok_or(ArrangeError::NotEditing)?;
        self.items = snap;
        Ok(())
    }

    /// End the session. Returns `None` if nothing changed vs the snapshot,
    /// else the commit. The session always ends.
    pub fn commit(&mut self) -> Result<Option<ArrangeCommit>, ArrangeError> {
        let snap = self.snapshot.take().ok_or(ArrangeError::NotEditing)?;
        if snap == self.items {
            return Ok(None);
        }
        let order = self.items.iter().map(|i| i.id.clone()).collect();
        let enabled_changes = self
            .items
            .iter()
            .filter_map(|it| {
                let original = snap.iter().find(|s| s.id == it.id)?;
                (original.enabled != it.enabled).then(|| EnabledChange {
                    id: it.id.clone(),
                    enabled: it.enabled,
                })
            })
            .collect();
        Ok(Some(ArrangeCommit {
            order,
            enabled_changes,
        }))
    }

    /// Merge a newer module list. Surviving ids keep their relative order and
    /// their current enabled flag; `locked` is always taken from `current_ids`
    /// (locking a disabled survivor forces it enabled). Missing ids are
    /// dropped; new ids are appended with their given flags. During editing
    /// the same transform is applied to the snapshot, so cancel never
    /// resurrects a removed id nor drops a new one.
    pub fn reconcile(&mut self, current_ids: &[GridItem]) {
        self.items = Self::reconcile_list(&self.items, current_ids);
        if let Some(snap) = &mut self.snapshot {
            *snap = Self::reconcile_list(snap, current_ids);
        }
    }

    fn reconcile_list(base: &[GridItem], current_ids: &[GridItem]) -> Vec<GridItem> {
        let mut deduped: Vec<GridItem> = Vec::new();
        let mut seen = HashSet::new();
        for it in current_ids {
            if seen.insert(&it.id) {
                deduped.push(GridItem {
                    id: it.id.clone(),
                    enabled: it.enabled || it.locked,
                    locked: it.locked,
                });
            }
        }
        let mut out = Vec::with_capacity(deduped.len());
        for it in base {
            if let Some(new) = deduped.iter().find(|d| d.id == it.id) {
                out.push(GridItem {
                    id: it.id.clone(),
                    enabled: it.enabled || new.locked,
                    locked: new.locked,
                });
            }
        }
        for new in &deduped {
            if !out.iter().any(|o| o.id == new.id) {
                out.push(new.clone());
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, enabled: bool, locked: bool) -> GridItem {
        GridItem {
            id: id.to_string(),
            enabled,
            locked,
        }
    }

    fn must<T>(r: Result<T, ArrangeError>, what: &str) -> T {
        match r {
            Ok(v) => v,
            Err(e) => panic!("{what} failed: {e}"),
        }
    }

    #[test]
    fn cancel_restores_exact_internal_snapshot() {
        let mut a = must(
            Arrangement::new(vec![
                item("a", true, false),
                item("b", true, false),
                item("c", false, false),
            ]),
            "new",
        );
        must(a.begin_edit(), "begin");
        let snapshot = match a.snapshot.clone() {
            Some(s) => s,
            None => panic!("snapshot missing"),
        };
        must(a.move_item("a", 1), "move");
        must(a.disable("b"), "disable");
        must(a.cancel(), "cancel");
        assert_eq!(a.items, snapshot);
        assert!(!a.is_editing());
    }

    #[test]
    fn reconcile_during_editing_keeps_snapshot_parallel() {
        let mut a = must(
            Arrangement::new(vec![item("a", true, false), item("b", false, false)]),
            "new",
        );
        must(a.begin_edit(), "begin");
        must(a.disable("a"), "disable"); // work: a hidden; snapshot: a visible
        a.reconcile(&[item("a", false, false), item("c", true, false)]);
        // a survives (enabled kept per list: work=false, snapshot=true); c
        // appended to both.
        assert!(!a.items[0].enabled);
        assert!(a.items[1].enabled);
        let snap = match a.snapshot.as_ref() {
            Some(s) => s,
            None => panic!("snapshot missing"),
        };
        assert!(snap[0].enabled);
        assert!(snap[1].enabled);
        // cancel restores the reconciled snapshot: a visible, c appended, b gone
        must(a.cancel(), "cancel");
        assert_eq!(a.visible().len(), 2);
        assert!(a.hidden().is_empty());
    }
}
