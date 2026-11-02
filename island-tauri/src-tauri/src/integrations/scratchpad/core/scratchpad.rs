use crate::integrations::scratchpad::core::error::ScratchpadError;
use crate::integrations::scratchpad::core::note::{
    derive_title, is_valid_id, Note, NoteSummary, MAX_BODY_BYTES,
};
use crate::integrations::scratchpad::core::save_scheduler::SaveScheduler;
use std::cmp::Ordering;
use std::collections::HashSet;

/// Maximum number of notes in a scratchpad.
pub const MAX_NOTES: usize = 200;

/// Result of loading a scratchpad file: per-note problems are tolerated and
/// counted, never fatal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadReport {
    /// Number of invalid notes skipped (bad id, duplicate id, oversize body,
    /// wrong types, non-object element).
    pub skipped: u32,
    /// True when more than [`MAX_NOTES`] valid notes were present and only
    /// the newest 200 were kept.
    pub truncated_to_max: bool,
}

/// The pure scratchpad store: at most [`MAX_NOTES`] notes, kept sorted as
/// [`Scratchpad::list`] orders them (pinned first, then `updated_ms`
/// descending, ties by id ascending). Owns a [`SaveScheduler`]; every
/// mutating operation that changes state reports its `now` to the scheduler.
#[derive(Debug, Clone)]
pub struct Scratchpad {
    notes: Vec<Note>,
    scheduler: SaveScheduler,
}

impl Scratchpad {
    /// An empty scratchpad with a clean (not dirty) scheduler.
    pub fn new() -> Self {
        Self {
            notes: Vec::new(),
            scheduler: SaveScheduler::new(),
        }
    }

    /// Number of notes.
    pub fn len(&self) -> usize {
        self.notes.len()
    }

    /// Whether there are no notes.
    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }

    /// Borrow the save scheduler (read-only).
    pub fn scheduler(&self) -> &SaveScheduler {
        &self.scheduler
    }

    /// Mutably borrow the save scheduler (to drive saves).
    pub fn scheduler_mut(&mut self) -> &mut SaveScheduler {
        &mut self.scheduler
    }

    /// Whether there are unsaved changes.
    pub fn is_dirty(&self) -> bool {
        self.scheduler.is_dirty()
    }

    /// Borrow a note by id.
    pub fn get(&self, id: &str) -> Option<&Note> {
        self.notes.iter().find(|n| n.id == id)
    }

    /// Create a new empty note (id supplied by the caller). The id must be
    /// 1..=64 chars of [A-Za-z0-9_-], unique, and the scratchpad must hold
    /// fewer than [`MAX_NOTES`] notes. Marks dirty.
    pub fn new_note(&mut self, now: u64, id: String) -> Result<(), ScratchpadError> {
        if !is_valid_id(&id) {
            return Err(ScratchpadError::InvalidId);
        }
        if self.notes.iter().any(|n| n.id == id) {
            return Err(ScratchpadError::DuplicateId);
        }
        if self.notes.len() >= MAX_NOTES {
            return Err(ScratchpadError::TooManyNotes);
        }
        let note = Note {
            id,
            title: "Untitled".to_string(),
            body: String::new(),
            created_ms: now,
            updated_ms: now,
            pinned: false,
        };
        let pos = self.insert_position(&note);
        self.notes.insert(pos, note);
        self.scheduler.mark_dirty(now);
        Ok(())
    }

    /// Replace a note's body. A body over [`MAX_BODY_BYTES`] bytes is a
    /// typed error and leaves the note unchanged. Editing with the identical
    /// body changes nothing (no `updated_ms` bump, no dirty). Marks dirty on
    /// a real change.
    pub fn edit(&mut self, now: u64, id: &str, body: String) -> Result<(), ScratchpadError> {
        let idx = self.find_index(id).ok_or(ScratchpadError::NotFound)?;
        if body.len() > MAX_BODY_BYTES {
            return Err(ScratchpadError::BodyTooLarge);
        }
        if body == self.notes[idx].body {
            return Ok(());
        }
        let mut note = self.notes.remove(idx);
        note.body = body;
        note.title = derive_title(&note.body);
        note.updated_ms = now;
        let pos = self.insert_position(&note);
        self.notes.insert(pos, note);
        self.scheduler.mark_dirty(now);
        Ok(())
    }

    /// Delete a note. Marks dirty.
    pub fn delete(&mut self, now: u64, id: &str) -> Result<(), ScratchpadError> {
        let idx = self.find_index(id).ok_or(ScratchpadError::NotFound)?;
        self.notes.remove(idx);
        self.scheduler.mark_dirty(now);
        Ok(())
    }

    /// Set a note's pinned flag. Setting the same value changes nothing (no
    /// dirty). Marks dirty on a real change.
    pub fn set_pinned(&mut self, now: u64, id: &str, pinned: bool) -> Result<(), ScratchpadError> {
        let idx = self.find_index(id).ok_or(ScratchpadError::NotFound)?;
        if self.notes[idx].pinned == pinned {
            return Ok(());
        }
        let mut note = self.notes.remove(idx);
        note.pinned = pinned;
        let pos = self.insert_position(&note);
        self.notes.insert(pos, note);
        self.scheduler.mark_dirty(now);
        Ok(())
    }

    /// Notes in display order: pinned first, then `updated_ms` descending,
    /// ties by id ascending. Summaries carry no body.
    pub fn list(&self) -> Vec<NoteSummary> {
        self.notes
            .iter()
            .map(|n| NoteSummary {
                id: n.id.clone(),
                title: n.title.clone(),
                created_ms: n.created_ms,
                updated_ms: n.updated_ms,
                pinned: n.pinned,
            })
            .collect()
    }

    /// Serialize to the persistence format: `{"version":1,"notes":[…]}`.
    /// Field names are camelCase; the emitted order is the canonical display
    /// order.
    pub fn to_json(&self) -> String {
        #[derive(serde::Serialize)]
        struct Envelope<'a> {
            version: u32,
            notes: Vec<&'a Note>,
        }
        let envelope = Envelope {
            version: 1,
            notes: self.notes.iter().collect(),
        };
        serde_json::to_string(&envelope)
            .unwrap_or_else(|_| unreachable!("serializing a plain struct cannot fail"))
    }

    /// Parse the persistence format. Fatal (typed error) conditions: the
    /// input is not valid JSON, the top level is not an object, `version` is
    /// missing or not 1, or `notes` is present but not an array. On any of
    /// these the caller should keep the file aside and start empty.
    ///
    /// Per-note problems are tolerated: invalid id, duplicate id, oversize
    /// body, wrong-typed fields, or a non-object element skip that note and
    /// increment [`LoadReport::skipped`]. More than [`MAX_NOTES`] valid notes
    /// keep only the newest 200 (`truncated_to_max`). Titles are recomputed,
    /// never trusted; unknown extra fields are ignored.
    pub fn from_json(input: &str) -> Result<(Scratchpad, LoadReport), ScratchpadError> {
        let value: serde_json::Value =
            serde_json::from_str(input).map_err(|_| ScratchpadError::InvalidJson)?;
        let obj = value.as_object().ok_or(ScratchpadError::NotAnObject)?;

        match obj.get("version").and_then(|v| v.as_u64()) {
            Some(1) => {}
            _ => return Err(ScratchpadError::InvalidVersion),
        }

        let reported_notes = match obj.get("notes") {
            None => None,
            Some(v) => Some(v.as_array().ok_or(ScratchpadError::NotesNotArray)?),
        };

        let mut skipped: u32 = 0;
        let mut seen_ids: HashSet<String> = HashSet::new();
        let mut valid: Vec<Note> = Vec::new();

        if let Some(array) = reported_notes {
            for item in array {
                if let Some(note) = parse_note(item, &mut seen_ids, &mut skipped) {
                    valid.push(note);
                }
            }
        }

        let truncated_to_max = valid.len() > MAX_NOTES;
        if truncated_to_max {
            // keep the 200 most recently updated (ties broken by id asc)
            valid.sort_by(|a, b| {
                b.updated_ms
                    .cmp(&a.updated_ms)
                    .then_with(|| a.id.cmp(&b.id))
            });
            valid.truncate(MAX_NOTES);
        }
        valid.sort_by(cmp_notes);

        let mut scratchpad = Scratchpad::new();
        scratchpad.notes = valid;
        Ok((
            scratchpad,
            LoadReport {
                skipped,
                truncated_to_max,
            },
        ))
    }

    fn find_index(&self, id: &str) -> Option<usize> {
        self.notes.iter().position(|n| n.id == id)
    }

    fn insert_position(&self, note: &Note) -> usize {
        self.notes
            .partition_point(|n| cmp_notes(n, note) == Ordering::Less)
    }
}

impl PartialEq for Scratchpad {
    fn eq(&self, other: &Self) -> bool {
        self.notes == other.notes
    }
}

impl Eq for Scratchpad {}

impl Default for Scratchpad {
    fn default() -> Self {
        Self::new()
    }
}

fn cmp_notes(a: &Note, b: &Note) -> Ordering {
    b.pinned
        .cmp(&a.pinned)
        .then_with(|| b.updated_ms.cmp(&a.updated_ms))
        .then_with(|| a.id.cmp(&b.id))
}

/// Parse one note array element tolerantly. Returns `None` (after bumping
/// `skipped`) for anything invalid.
fn parse_note(
    item: &serde_json::Value,
    seen_ids: &mut HashSet<String>,
    skipped: &mut u32,
) -> Option<Note> {
    let note_obj = match item.as_object() {
        Some(o) => o,
        None => {
            *skipped = skipped.saturating_add(1);
            return None;
        }
    };
    let id = match note_obj.get("id").and_then(|v| v.as_str()) {
        Some(id) if is_valid_id(id) && !seen_ids.contains(id) => id,
        _ => {
            *skipped = skipped.saturating_add(1);
            return None;
        }
    };
    let created_ms = match note_obj.get("createdMs") {
        None => 0,
        Some(v) => match v.as_u64() {
            Some(n) => n,
            None => {
                *skipped = skipped.saturating_add(1);
                return None;
            }
        },
    };
    let updated_ms = match note_obj.get("updatedMs") {
        None => 0,
        Some(v) => match v.as_u64() {
            Some(n) => n,
            None => {
                *skipped = skipped.saturating_add(1);
                return None;
            }
        },
    };
    let pinned = match note_obj.get("pinned") {
        None => false,
        Some(v) => match v.as_bool() {
            Some(b) => b,
            None => {
                *skipped = skipped.saturating_add(1);
                return None;
            }
        },
    };
    let body = match note_obj.get("body") {
        None => String::new(),
        Some(v) => match v.as_str() {
            Some(s) => s.to_string(),
            None => {
                *skipped = skipped.saturating_add(1);
                return None;
            }
        },
    };
    if body.len() > MAX_BODY_BYTES {
        *skipped = skipped.saturating_add(1);
        return None;
    }
    seen_ids.insert(id.to_string());
    Some(Note {
        id: id.to_string(),
        title: derive_title(&body),
        body,
        created_ms,
        updated_ms,
        pinned,
    })
}
