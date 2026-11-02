use std::collections::HashMap;

use crate::model::{visible_in, AppEntry};

/// Merges entries from multiple XDG dirs by desktop id. Lower `dir_rank`
/// wins (earlier dir = higher priority); a `Hidden` entry in a
/// higher-priority dir masks the same id from lower dirs.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    entries: HashMap<String, (u32, AppEntry)>,
}

impl Catalog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, desktop_id: &str) -> Option<&AppEntry> {
        self.entries.get(desktop_id).map(|(_, e)| e)
    }

    pub fn add_dir_entries(&mut self, dir_rank: u32, entries: Vec<AppEntry>) {
        for entry in entries {
            let id = entry.desktop_id.clone();
            match self.entries.get(&id) {
                Some((old_rank, _)) if *old_rank <= dir_rank => {}
                _ => {
                    self.entries.insert(id, (dir_rank, entry));
                }
            }
        }
    }

    /// Case-insensitive substring search over name, genericName, keywords and
    /// desktopId; only visible entries. Ranking: name prefix > name substring
    /// > other fields; ties by name then desktopId.
    pub fn search(&self, query: &str, current_desktops: &[&str]) -> Vec<&AppEntry> {
        let q = query.to_lowercase();
        let mut hits: Vec<&AppEntry> = self
            .entries
            .values()
            .map(|(_, e)| e)
            .filter(|e| visible_in(e, current_desktops))
            .filter(|e| matches_query(e, &q))
            .collect();
        hits.sort_by_key(|a| rank_key(a, &q));
        hits
    }
}

fn matches_query(e: &AppEntry, q: &str) -> bool {
    e.name.to_lowercase().contains(q)
        || e.generic_name.to_lowercase().contains(q)
        || e.desktop_id.to_lowercase().contains(q)
        || e.keywords.iter().any(|k| k.to_lowercase().contains(q))
}

/// (category, name, desktop id) sort key: name prefix < name substring <
/// other fields; ties by name then desktop id.
fn rank_key(e: &AppEntry, q: &str) -> (u8, String, String) {
    let name = e.name.to_lowercase();
    let category = if name.starts_with(q) {
        0
    } else if name.contains(q) {
        1
    } else {
        2
    };
    (category, name, e.desktop_id.to_lowercase())
}
