//! Caller-decoded D-Bus values. These are not JSON/IPC contract types.
//! The adapter must authenticate the notification server and reset on owner change.
use super::types::{bounded, nonempty};
use super::*;
use std::collections::{BTreeMap, VecDeque};

pub const CORRELATION_CAPACITY: usize = 256;
pub const MAX_HINTS: usize = 32;
pub const MAX_HINT_STRING_BYTES: usize = 1024;
pub const MAX_IMAGE_SIDE: u32 = 128;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageData {
    pub width: u32,
    pub height: u32,
    pub rowstride: u32,
    pub has_alpha: bool,
    pub bits_per_sample: u32,
    pub channels: u32,
    pub data: Vec<u8>,
}
impl ImageData {
    fn validate(&self) -> Result<(), Error> {
        if self.width == 0
            || self.height == 0
            || self.width > MAX_IMAGE_SIDE
            || self.height > MAX_IMAGE_SIDE
            || self.bits_per_sample != 8
            || self.channels != if self.has_alpha { 4 } else { 3 }
            || self.rowstride < self.width * self.channels
            || self.rowstride > MAX_IMAGE_SIDE * 4
            || self.data.len() != (self.rowstride * self.height) as usize
        {
            return Err(Error::invalid("invalid or oversized image hint"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HintValue {
    Byte(u8),
    String(String),
    Bool(bool),
    ImageData(ImageData),
    /// Unknown D-Bus types are discarded by the caller, not retained as blobs.
    Other,
}

/// Notify(s,u,s,s,s,as,a{sv},i); negative timeouts other than -1 are invalid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotifyArgs {
    pub app_name: String,
    pub replaces_id: u32,
    pub app_icon: String,
    pub summary: String,
    pub body: String,
    pub actions: Vec<String>,
    pub hints: BTreeMap<String, HintValue>,
    pub expire_timeout: i32,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Formatting {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct DbusNotification {
    pub input: NotificationInput,
    pub app_icon: String,
    pub desktop_entry: Option<String>,
    pub category: Option<String>,
    pub image_path: Option<String>,
    /// Caller encodes a bounded PNG off-thread; this crate performs no encoding/IO.
    pub image_data: Option<ImageData>,
    pub origin_name: Option<String>,
    pub formatting: Formatting,
}

impl NotifyArgs {
    pub fn parse(&self, server_id: u32) -> Result<DbusNotification, Error> {
        bounded(&self.app_icon, MAX_HINT_STRING_BYTES)?;
        bounded(&self.body, MAX_BODY_BYTES)?;
        if self.actions.len() > MAX_ACTIONS * 2 || self.actions.len() % 2 != 0 {
            return Err(Error::invalid("actions must be bounded key/label pairs"));
        }
        if self.hints.len() > MAX_HINTS {
            return Err(Error::invalid("too many hints"));
        }
        for (key, value) in &self.hints {
            bounded(key, MAX_NAME_BYTES)?;
            if let HintValue::String(s) = value {
                bounded(s, MAX_HINT_STRING_BYTES)?;
            }
            match (key.as_str(), value) {
                ("urgency", HintValue::Byte(0..=2)) | ("transient", HintValue::Bool(_)) => {}
                (
                    "desktop-entry" | "category" | "image-path" | "image_path"
                    | "x-kde-origin-name",
                    HintValue::String(_),
                ) => {}
                ("image-data" | "icon_data", HintValue::ImageData(image)) => image.validate()?,
                (
                    "urgency" | "transient" | "desktop-entry" | "category" | "image-path"
                    | "image_path" | "x-kde-origin-name" | "image-data" | "icon_data",
                    _,
                ) => return Err(Error::invalid("wrong notification hint type or value")),
                _ => {}
            }
        }
        let string = |key: &str| match self.hints.get(key) {
            Some(HintValue::String(value)) => Some(value.clone()),
            _ => None,
        };
        let image = |key: &str| match self.hints.get(key) {
            Some(HintValue::ImageData(value)) => Some(value.clone()),
            _ => None,
        };
        let urgency = match self.hints.get("urgency") {
            Some(HintValue::Byte(0)) => Urgency::Low,
            Some(HintValue::Byte(2)) => Urgency::Critical,
            _ => Urgency::Normal,
        };
        let ttl_ms = match self.expire_timeout {
            -1 => None,
            value if value >= 0 => Some(f64::from(value)),
            _ => return Err(Error::invalid("invalid expire timeout")),
        };
        let (body, formatting) = plain_body(&self.body);
        let input = NotificationInput {
            source: NotificationSource::System,
            app_name: self.app_name.clone(),
            title: self.summary.clone(),
            body: Some(body),
            icon_png: None,
            urgency,
            actions: self
                .actions
                .chunks_exact(2)
                .map(|p| NotificationAction {
                    key: p[0].clone(),
                    label: p[1].clone(),
                })
                .collect(),
            progress: None,
            dedupe_key: None,
            server_id: Some(server_id),
            replaces_id: self.replaces_id,
            transient: matches!(self.hints.get("transient"), Some(HintValue::Bool(true))),
            ttl_ms,
        };
        input.validate()?;
        Ok(DbusNotification {
            input,
            app_icon: self.app_icon.clone(),
            desktop_entry: string("desktop-entry"),
            category: string("category"),
            image_path: string("image-path").or_else(|| string("image_path")),
            image_data: image("image-data").or_else(|| image("icon_data")),
            origin_name: string("x-kde-origin-name"),
            formatting,
        })
    }
}

// A conservative tag scanner, not an HTML renderer. Preserve comparison signs,
// and do not mistake a quoted '>' in an attribute for the end of a tag.
fn tag_end(text: &str) -> Option<(usize, String)> {
    if text.starts_with("<!--") {
        return text.find("-->").map(|end| (end + 2, String::new()));
    }
    let inner = text.strip_prefix('<')?;
    let name_start = if inner.starts_with('/') { 2 } else { 1 };
    let bytes = text.as_bytes();
    if !bytes.get(name_start)?.is_ascii_alphabetic() {
        return None;
    }
    let mut name_end = name_start;
    while bytes
        .get(name_end)
        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'-' || *b == b':')
    {
        name_end += 1;
    }
    if !bytes
        .get(name_end)
        .is_some_and(|b| b.is_ascii_whitespace() || *b == b'/' || *b == b'>')
    {
        return None;
    }
    let mut quote = None;
    for (index, byte) in bytes.iter().copied().enumerate().skip(name_end) {
        if let Some(open) = quote {
            if byte == open {
                quote = None;
            }
        } else if byte == b'\'' || byte == b'"' {
            quote = Some(byte);
        } else if byte == b'>' {
            let name = text[1..name_end].to_ascii_lowercase();
            return Some((index, name));
        }
    }
    None
}

fn plain_body(body: &str) -> (String, Formatting) {
    let mut text = String::new();
    let mut flags = Formatting::default();
    let mut rest = body;
    while let Some(start) = rest.find('<') {
        text.push_str(&rest[..start]);
        let Some((end, tag)) = tag_end(&rest[start..]) else {
            text.push('<');
            rest = &rest[start + 1..];
            continue;
        };
        match tag.as_str() {
            "b" => flags.bold = true,
            "i" => flags.italic = true,
            "u" => flags.underline = true,
            "br" => text.push('\n'),
            _ => {}
        }
        rest = &rest[start + end + 1..];
    }
    text.push_str(rest);
    // Decode once, AFTER removing tags. Entity-escaped tags remain literal text.
    let mut decoded = String::new();
    let mut rest = text.as_str();
    while let Some(start) = rest.find('&') {
        decoded.push_str(&rest[..start]);
        let tail = &rest[start + 1..];
        let entity = tail.find(';').filter(|end| *end <= 12);
        let replacement = entity.and_then(|end| {
            let name = &tail[..end];
            match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ => {
                    let number = if let Some(hex) = name.strip_prefix("#x") {
                        u32::from_str_radix(hex, 16).ok()
                    } else {
                        name.strip_prefix('#').and_then(|n| n.parse::<u32>().ok())
                    };
                    number.and_then(char::from_u32)
                }
            }
        });
        if let (Some(end), Some(ch)) = (entity, replacement) {
            decoded.push(ch);
            rest = &tail[end + 1..];
        } else {
            decoded.push('&');
            rest = tail;
        }
    }
    decoded.push_str(rest);
    (decoded, flags)
}

#[derive(Clone, Debug)]
struct Pending {
    sender: String,
    serial: u32,
    parsed: DbusNotification,
}
/// Serial numbers are per calling connection, NOT globally unique on the bus.
#[derive(Clone, Debug, Default)]
pub struct NotifyCorrelator {
    pending: VecDeque<Pending>,
}
impl NotifyCorrelator {
    pub fn len(&self) -> usize {
        self.pending.len()
    }
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
    pub fn observe_call(
        &mut self,
        sender: &str,
        serial: u32,
        args: NotifyArgs,
    ) -> Result<(), Error> {
        nonempty(sender, MAX_NAME_BYTES)?;
        if serial == 0 {
            return Err(Error::invalid("serial must be nonzero"));
        }
        // Validate BEFORE modifying the LRU; server id is supplied by the return later.
        let parsed = args.parse(1)?;
        if let Some(index) = self
            .pending
            .iter()
            .position(|p| p.sender == sender && p.serial == serial)
        {
            self.pending.remove(index);
        }
        if self.pending.len() == CORRELATION_CAPACITY {
            self.pending.pop_back();
        }
        self.pending.push_front(Pending {
            sender: sender.into(),
            serial,
            parsed,
        });
        Ok(())
    }
    pub fn touch(&mut self, sender: &str, serial: u32) -> bool {
        let index = self
            .pending
            .iter()
            .position(|p| p.sender == sender && p.serial == serial);
        if let Some(entry) = index.and_then(|i| self.pending.remove(i)) {
            self.pending.push_front(entry);
            true
        } else {
            false
        }
    }
    /// destination + reply_serial identify the original caller. Consumes a match once.
    pub fn method_return(
        &mut self,
        destination: &str,
        reply_serial: u32,
        server_id: u32,
    ) -> Result<Option<DbusNotification>, Error> {
        let Some(index) = self
            .pending
            .iter()
            .position(|p| p.sender == destination && p.serial == reply_serial)
        else {
            return Ok(None);
        };
        if server_id == 0 {
            return Err(Error::invalid("server id must be nonzero"));
        }
        if let Some(mut entry) = self.pending.remove(index) {
            entry.parsed.input.server_id = Some(server_id);
            return Ok(Some(entry.parsed));
        }
        Ok(None)
    }
    /// Required when the server owner changes; no stale return may bind to a new server.
    pub fn reset(&mut self) {
        self.pending.clear();
    }
    /// Adapter timeout eviction (the staged core intentionally has no clock).
    pub fn forget(&mut self, sender: &str, serial: u32) {
        self.pending
            .retain(|p| p.sender != sender || p.serial != serial);
    }
}
