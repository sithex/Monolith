use std::collections::{HashSet, VecDeque};

use super::config::HistoryConfig;
use super::error::Error;
use super::model::{
    ClipContent, ClipInput, ClipKind, ClipPayload, ClipSummary, OfferOutcome, SkipReason,
};
use super::persist;
use super::preview::preview;
use regex::Regex;
use xxhash_rust::xxh3::xxh3_64;

/// Self-echo guard window (SPEC §3.3): an `expect_echo` registration lives for
/// exactly this many milliseconds.
pub const ECHO_WINDOW_MS: u64 = 2000;

/// MIME types (normalized) that mark an offer as sensitive. The
/// `x-kde-passwordManagerHint=secret` case is delivered via `password_hint`
/// and is handled separately. `CanIncludeInClipboardHistory` is the caller's
/// job, not ours.
const SENSITIVE_MIME: [&str; 3] = [
    "org.nspasteboard.concealedtype",
    "org.nspasteboard.transienttype",
    "excludeclipboardcontentfrommonitorprocessing",
];

/// The pure clipboard history store.
///
/// Entries live in a `VecDeque` whose front is the newest entry. Identifiers
/// are allocated from a monotonic counter and are never reused within a given
/// store instance (including across a persisted round-trip).
#[derive(Debug)]
pub struct HistoryStore {
    config: HistoryConfig,
    entries: VecDeque<Entry>,
    next_id: u64,
    paused: bool,
    /// Pending self-echo expectations: `(hash, expires_at_ms)`.
    echoes: Vec<(u64, u64)>,
    deny_regexes: Vec<Regex>,
}

#[derive(Debug)]
struct Entry {
    id: String,
    payload: ClipPayload,
    pinned: bool,
    copied_at_ms: f64,
    source_app: Option<String>,
    hash: u64,
}

impl HistoryStore {
    /// Create an empty store. `config` is validated (capacity clamped, deny
    /// patterns checked) — an invalid config is an error.
    pub fn new(config: HistoryConfig) -> Result<Self, Error> {
        let config = config.validated()?;
        let deny_regexes = config
            .deny_patterns
            .iter()
            .filter_map(|p| Regex::new(p).ok())
            .collect();
        Ok(Self {
            config,
            entries: VecDeque::new(),
            next_id: 1,
            paused: false,
            echoes: Vec::new(),
            deny_regexes,
        })
    }

    /// Restore a store from its serialized form. Unknown format versions are
    /// rejected; every entry is re-validated and capped against `config`
    /// (deny-matched / empty / oversize-image entries are dropped, oversize
    /// text is truncated at a char boundary, and the result is capped to
    /// capacity — oldest unpinned first, then oldest overall). Entries with
    /// identical content hashes are de-duplicated: the newest occurrence is
    /// kept and later duplicates are dropped, but a dropped pinned duplicate's
    /// pin transfers to the kept entry.
    pub fn from_persisted(config: HistoryConfig, json: &str) -> Result<Self, Error> {
        let config = config.validated()?;
        let deny_regexes = config
            .deny_patterns
            .iter()
            .filter_map(|p| Regex::new(p).ok())
            .collect();

        let file: persist::StoredFile =
            serde_json::from_str(json).map_err(|e| Error::Corrupt(format!("parse failed: {e}")))?;
        if file.version != persist::PERSIST_VERSION {
            return Err(Error::UnknownVersion(file.version));
        }

        let mut store = Self {
            config,
            entries: VecDeque::new(),
            next_id: file.next_id.max(1),
            paused: false,
            echoes: Vec::new(),
            deny_regexes,
        };

        let mut seen_ids: HashSet<String> = HashSet::new();
        // Load-time dedup by content hash (SPEC §3.3). Persisted entries are
        // processed newest-first, so the first occurrence of a hash is kept
        // and later duplicates are dropped; a dropped duplicate's pin is
        // carried onto the kept entry.
        let mut seen_hashes: HashSet<u64> = HashSet::new();
        for stored in file.entries {
            let payload = ClipPayload::from(stored.payload);

            // Re-validate: deny regex (text only).
            if let ClipPayload::Text(text) = &payload {
                if store.deny_regexes.iter().any(|re| re.is_match(text)) {
                    continue;
                }
            }
            // Re-validate / cap against current size limits.
            let Some(payload) = cap_payload(payload, &store.config) else {
                continue;
            };
            // Re-validate: empty / whitespace-only text, or zero files.
            match &payload {
                ClipPayload::Text(text) if text.trim().is_empty() => continue,
                ClipPayload::Files(uris) if uris.is_empty() => continue,
                _ => {}
            }

            let mut id = stored.id;
            if id.is_empty() || !seen_ids.insert(id.clone()) {
                id = store.alloc_unique_id(&mut seen_ids);
            }

            let hash = hash_payload(&payload);
            if !seen_hashes.insert(hash) {
                // Duplicate content: drop this (older) entry. If it was
                // pinned, the kept entry takes on the pin.
                if stored.pinned {
                    if let Some(kept) = store.entries.iter_mut().find(|e| e.hash == hash) {
                        kept.pinned = true;
                    }
                }
                continue;
            }
            store.entries.push_back(Entry {
                id,
                payload,
                pinned: stored.pinned,
                copied_at_ms: stored.copied_at_ms,
                source_app: stored.source_app,
                hash,
            });
        }

        // Cap to capacity: drop oldest unpinned first, then oldest overall.
        while store.entries.len() > store.config.capacity {
            let pos = store
                .entries
                .iter()
                .rposition(|e| !e.pinned)
                .unwrap_or(store.entries.len() - 1);
            store.entries.remove(pos);
        }

        // Guarantee id uniqueness: never resume below an existing suffix.
        let max_suffix = store
            .entries
            .iter()
            .filter_map(|e| id_suffix(&e.id))
            .max()
            .unwrap_or(0);
        if store.next_id <= max_suffix {
            store.next_id = max_suffix + 1;
        }

        Ok(store)
    }

    /// Serialize the store (version + entries + next id) as a JSON string.
    ///
    /// Per SPEC §3.3 the persisted snapshot defaults to pinned entries only:
    /// when `include_unpinned` is `false`, unpinned entries are excluded from
    /// the output entirely. The internal id counter is always preserved so a
    /// subsequent load resumes without reusing ids.
    pub fn to_persisted(&self, include_unpinned: bool) -> Result<String, Error> {
        let file = persist::StoredFile {
            version: persist::PERSIST_VERSION,
            next_id: self.next_id,
            entries: self
                .entries
                .iter()
                .filter(|e| include_unpinned || e.pinned)
                .map(|e| persist::StoredEntry {
                    id: e.id.clone(),
                    payload: persist::StoredPayload::from(&e.payload),
                    pinned: e.pinned,
                    copied_at_ms: e.copied_at_ms,
                    source_app: e.source_app.clone(),
                })
                .collect(),
        };
        serde_json::to_string(&file).map_err(|e| Error::Corrupt(format!("serialize: {e}")))
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Hash of an offer's content: `xxh3_64(kind_byte || content_bytes)`.
    /// Text kind byte `0`, image `1` (raw PNG bytes), files `2` (URIs joined
    /// with NUL separators so `["ab","c"]` != `["a","bc"]`).
    pub fn hash_of(input: &ClipInput) -> u64 {
        hash_payload(&input.content)
    }

    /// Register a self-echo expectation for `hash`, live from `now_ms` for
    /// exactly `ECHO_WINDOW_MS`.
    pub fn expect_echo(&mut self, hash: u64, now_ms: u64) {
        self.echoes.retain(|&(_, expires)| now_ms < expires);
        self.echoes
            .push((hash, now_ms.saturating_add(ECHO_WINDOW_MS)));
    }

    /// Offer a clipboard change. Returns `OfferOutcome` per SPEC §3.3 rules.
    pub fn offer(&mut self, input: ClipInput, now_ms: u64) -> OfferOutcome {
        if self.paused {
            return OfferOutcome::Skipped(SkipReason::Paused);
        }
        if is_sensitive(&input) {
            return OfferOutcome::Skipped(SkipReason::Sensitive);
        }
        // Size limits run before the deny-regex check: an oversize offer is
        // TooLarge even when its text would also match a deny pattern.
        match &input.content {
            ClipPayload::Text(text) if text.len() > self.config.max_text_bytes => {
                return OfferOutcome::Skipped(SkipReason::TooLarge);
            }
            ClipPayload::Image { png, width, height }
                if png.len() > self.config.max_image_bytes
                    || *width > self.config.max_image_side
                    || *height > self.config.max_image_side =>
            {
                return OfferOutcome::Skipped(SkipReason::TooLarge);
            }
            _ => {}
        }
        if let ClipPayload::Text(text) = &input.content {
            if self.deny_regexes.iter().any(|re| re.is_match(text)) {
                return OfferOutcome::Skipped(SkipReason::Denied);
            }
        }
        match &input.content {
            ClipPayload::Text(text) if text.trim().is_empty() => {
                return OfferOutcome::Skipped(SkipReason::Empty);
            }
            ClipPayload::Files(uris) if uris.is_empty() => {
                return OfferOutcome::Skipped(SkipReason::Empty);
            }
            _ => {}
        }

        let hash = Self::hash_of(&input);

        // Self-echo window: consume the expectation; only reorder.
        if self.take_echo(hash, now_ms) {
            if let Some(pos) = self.entries.iter().position(|e| e.hash == hash) {
                self.bump_to_front(pos);
            }
            return OfferOutcome::Skipped(SkipReason::Echo);
        }

        // Dedupe: promote existing entry (preserves id and pinned state).
        if let Some(pos) = self.entries.iter().position(|e| e.hash == hash) {
            let id = self.entries[pos].id.clone();
            self.bump_to_front(pos);
            return OfferOutcome::Reordered { id };
        }

        // Capacity: evict the oldest unpinned entry; if all are pinned, full.
        if self.entries.len() >= self.config.capacity {
            match self.entries.iter().rposition(|e| !e.pinned) {
                Some(pos) => {
                    self.entries.remove(pos);
                }
                None => return OfferOutcome::Skipped(SkipReason::Full),
            }
        }

        let id = self.alloc_id();
        self.entries.push_front(Entry {
            id: id.clone(),
            payload: input.content,
            pinned: false,
            copied_at_ms: now_ms as f64,
            source_app: input.source_app,
            hash,
        });
        OfferOutcome::Added { id }
    }

    /// Set or clear the pinned flag.
    pub fn pin(&mut self, id: &str, pinned: bool) -> Result<(), Error> {
        let entry = self
            .entries
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or(Error::NotFound)?;
        entry.pinned = pinned;
        Ok(())
    }

    /// Delete an entry by id (pinned entries can be deleted explicitly).
    pub fn delete(&mut self, id: &str) -> Result<(), Error> {
        let pos = self
            .entries
            .iter()
            .position(|e| e.id == id)
            .ok_or(Error::NotFound)?;
        self.entries.remove(pos);
        Ok(())
    }

    /// Remove all unpinned entries; pinned entries survive. Returns the number
    /// of removed entries.
    pub fn clear(&mut self) -> usize {
        let before = self.entries.len();
        self.entries.retain(|e| e.pinned);
        before - self.entries.len()
    }

    /// List summaries, newest first. When `query` is `Some`, only entries whose
    /// preview or source app contains it (case-insensitive substring) match.
    /// The result is capped at `limit` entries.
    pub fn list(&self, query: Option<&str>, limit: u16) -> Vec<ClipSummary> {
        let needle = query.map(|q| q.to_lowercase());
        let limit = limit as usize;
        let mut out = Vec::new();
        for entry in &self.entries {
            if out.len() >= limit {
                break;
            }
            let summary = self.summary(entry);
            if let Some(needle) = &needle {
                let in_preview = summary.preview.to_lowercase().contains(needle.as_str());
                let in_source = summary
                    .source_app
                    .as_deref()
                    .map(|s| s.to_lowercase().contains(needle.as_str()))
                    .unwrap_or(false);
                if !in_preview && !in_source {
                    continue;
                }
            }
            out.push(summary);
        }
        out
    }

    /// Fetch the full content for an id.
    pub fn get(&self, id: &str) -> Option<ClipContent> {
        let entry = self.entries.iter().find(|e| e.id == id)?;
        Some(match &entry.payload {
            ClipPayload::Text(text) => ClipContent::Text { text: text.clone() },
            ClipPayload::Image { png, width, height } => ClipContent::Image {
                png_data_url: png_data_url(png),
                width: *width,
                height: *height,
            },
            ClipPayload::Files(uris) => ClipContent::Files { uris: uris.clone() },
        })
    }

    fn summary(&self, entry: &Entry) -> ClipSummary {
        ClipSummary {
            id: entry.id.clone(),
            kind: kind_of(&entry.payload),
            preview: preview(&entry.payload, self.config.preview_chars),
            bytes: saturating_bytes(&entry.payload),
            pinned: entry.pinned,
            copied_at: entry.copied_at_ms,
            source_app: entry.source_app.clone(),
        }
    }

    fn alloc_id(&mut self) -> String {
        let id = format!("c-{}", self.next_id);
        self.next_id += 1;
        id
    }

    /// Allocate an id that is not already present in `seen` (used when
    /// reassigning ids from persisted data).
    fn alloc_unique_id(&mut self, seen: &mut HashSet<String>) -> String {
        loop {
            let id = self.alloc_id();
            if seen.insert(id.clone()) {
                return id;
            }
        }
    }

    fn bump_to_front(&mut self, pos: usize) {
        if let Some(entry) = self.entries.remove(pos) {
            self.entries.push_front(entry);
        }
    }

    fn take_echo(&mut self, hash: u64, now_ms: u64) -> bool {
        self.echoes.retain(|&(_, expires)| now_ms < expires);
        if let Some(pos) = self.echoes.iter().position(|&(h, _)| h == hash) {
            self.echoes.swap_remove(pos);
            true
        } else {
            false
        }
    }
}

fn hash_payload(payload: &ClipPayload) -> u64 {
    let mut buf = Vec::new();
    match payload {
        ClipPayload::Text(text) => {
            buf.push(0);
            buf.extend_from_slice(text.as_bytes());
        }
        ClipPayload::Image { png, .. } => {
            buf.push(1);
            buf.extend_from_slice(png);
        }
        ClipPayload::Files(uris) => {
            buf.push(2);
            for (i, uri) in uris.iter().enumerate() {
                if i > 0 {
                    buf.push(0);
                }
                buf.extend_from_slice(uri.as_bytes());
            }
        }
    }
    xxh3_64(&buf)
}

fn is_sensitive(input: &ClipInput) -> bool {
    if input.password_hint.as_deref() == Some("secret") {
        return true;
    }
    input.mime_types.iter().any(|mime| {
        let normalized = mime.trim().to_ascii_lowercase();
        SENSITIVE_MIME.contains(&normalized.as_str())
    })
}

fn kind_of(payload: &ClipPayload) -> ClipKind {
    match payload {
        ClipPayload::Text(_) => ClipKind::Text,
        ClipPayload::Image { .. } => ClipKind::Image,
        ClipPayload::Files(_) => ClipKind::Files,
    }
}

fn saturating_bytes(payload: &ClipPayload) -> u32 {
    let total: u64 = match payload {
        ClipPayload::Text(text) => text.len() as u64,
        ClipPayload::Image { png, .. } => png.len() as u64,
        ClipPayload::Files(uris) => uris.iter().map(|u| u.len() as u64).sum(),
    };
    total.min(u32::MAX as u64) as u32
}

fn png_data_url(png: &[u8]) -> String {
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(png);
    format!("data:image/png;base64,{encoded}")
}

/// Truncate oversize text to `max_text_bytes` at a char boundary; drop
/// oversize images (raw PNG bytes cannot be safely truncated) and empty file
/// lists. Returns `None` when the payload must be dropped.
fn cap_payload(payload: ClipPayload, config: &HistoryConfig) -> Option<ClipPayload> {
    match payload {
        ClipPayload::Text(text) => {
            if text.len() > config.max_text_bytes {
                let mut end = config.max_text_bytes.min(text.len());
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                Some(ClipPayload::Text(text[..end].to_string()))
            } else {
                Some(ClipPayload::Text(text))
            }
        }
        ClipPayload::Image { png, width, height } => {
            if png.len() > config.max_image_bytes
                || width > config.max_image_side
                || height > config.max_image_side
            {
                None
            } else {
                Some(ClipPayload::Image { png, width, height })
            }
        }
        ClipPayload::Files(uris) => {
            if uris.is_empty() {
                None
            } else {
                Some(ClipPayload::Files(uris))
            }
        }
    }
}

fn id_suffix(id: &str) -> Option<u64> {
    id.strip_prefix("c-")?.parse().ok()
}
