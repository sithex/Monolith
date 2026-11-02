//! Subscription event mapping: [`parse_event`] turns a `pactl subscribe`
//! line into a [`PaEvent`]; [`needs_refresh`] decides which part of the
//! mixer model a given event invalidates.

/// The kind of a pactl subscription event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaKind {
    New,
    Change,
    Remove,
}

/// The facility a pactl subscription event refers to. Unknown facilities
/// map to [`PaFacility::Other`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaFacility {
    Sink,
    Source,
    SinkInput,
    SourceOutput,
    Client,
    Module,
    Card,
    Server,
    Other(String),
}

/// A parsed `pactl subscribe` event line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaEvent {
    pub kind: PaKind,
    pub facility: PaFacility,
    pub index: u32,
}

/// Which part of the mixer model a [`PaEvent`] invalidates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshScope {
    /// Sink list changed (list `sinks` again).
    Sinks,
    /// Stream (sink-input) list changed (list `sink-inputs` again).
    Streams,
    /// Both lists may have changed.
    Both,
    /// Nothing this model cares about.
    None,
}

fn parse_kind(raw: &str) -> Option<PaKind> {
    let inner = raw.strip_prefix('\'')?.strip_suffix('\'')?;
    match inner {
        "new" => Some(PaKind::New),
        "change" => Some(PaKind::Change),
        "remove" => Some(PaKind::Remove),
        _ => None,
    }
}

fn parse_facility(raw: &str) -> PaFacility {
    match raw {
        "sink" => PaFacility::Sink,
        "source" => PaFacility::Source,
        "sink-input" => PaFacility::SinkInput,
        "source-output" => PaFacility::SourceOutput,
        "client" => PaFacility::Client,
        "module" => PaFacility::Module,
        "card" => PaFacility::Card,
        "server" => PaFacility::Server,
        other => PaFacility::Other(other.to_string()),
    }
}

/// Parse one `pactl subscribe` line of the shape
/// `Event '<kind>' on <facility> #<index>`. Malformed lines yield `None`.
pub fn parse_event(line: &str) -> Option<PaEvent> {
    let mut parts = line.trim().split(' ');
    if parts.next()? != "Event" {
        return None;
    }
    let kind_raw = parts.next()?;
    if parts.next()? != "on" {
        return None;
    }
    let facility_raw = parts.next()?;
    let index_raw = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    let kind = parse_kind(kind_raw)?;
    let index = index_raw.strip_prefix('#')?.parse::<u32>().ok()?;
    Some(PaEvent {
        kind,
        facility: parse_facility(facility_raw),
        index,
    })
}

/// Map an event to the refresh scope it demands.
pub fn needs_refresh(event: &PaEvent) -> RefreshScope {
    match &event.facility {
        PaFacility::Sink => RefreshScope::Sinks,
        PaFacility::SinkInput | PaFacility::Client => RefreshScope::Streams,
        PaFacility::Server => RefreshScope::Both,
        _ => RefreshScope::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_kind_requires_quotes() {
        assert_eq!(parse_kind("'new'"), Some(PaKind::New));
        assert_eq!(parse_kind("'change'"), Some(PaKind::Change));
        assert_eq!(parse_kind("'remove'"), Some(PaKind::Remove));
        assert_eq!(parse_kind("new"), None);
        assert_eq!(parse_kind("'novel'"), None);
        assert_eq!(parse_kind("\"new\""), None);
    }

    #[test]
    fn parse_facility_known_and_other() {
        assert_eq!(parse_facility("sink"), PaFacility::Sink);
        assert_eq!(parse_facility("source"), PaFacility::Source);
        assert_eq!(parse_facility("sink-input"), PaFacility::SinkInput);
        assert_eq!(parse_facility("source-output"), PaFacility::SourceOutput);
        assert_eq!(parse_facility("client"), PaFacility::Client);
        assert_eq!(parse_facility("module"), PaFacility::Module);
        assert_eq!(parse_facility("card"), PaFacility::Card);
        assert_eq!(parse_facility("server"), PaFacility::Server);
        assert_eq!(
            parse_facility("garbage"),
            PaFacility::Other("garbage".to_string())
        );
    }
}
