//! Calendar subscription and local ICS store. The secret URL is provisioned through a
//! write-only setup IPC or private file, never returned. HTTP errors and parser warnings
//! are deliberately never formatted.
pub mod core;
use crate::{
    bus::Latest,
    contract::apps::{AgendaRow, CalendarChanged, CalendarOp, CalendarSource},
    error::IslandError,
    integrations::private_file,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::watch;
pub const BODY_CAP: usize = 1024 * 1024;
pub const LOCAL_CAP: usize = 1024 * 1024;
pub const URL_CAP: usize = 8192;
pub const REFRESH_MS: u64 = 15 * 60 * 1000;
pub const MAX_BACKOFF_MS: u64 = 60 * 60 * 1000;
#[derive(Clone)]
pub struct CalendarConfig {
    pub data: PathBuf,
    pub loopback_test: bool,
    pub timeout: Duration,
    pub tz: TzConfig,
}
/// Time-zone source. `Default` = the system zone (`TZ`/`/etc/localtime`) and the system tz
/// database; tests inject an IANA name and a zoneinfo directory and never depend on the host zone.
#[derive(Clone, Default)]
pub struct TzConfig {
    pub local: Option<String>,
    pub dir: Option<PathBuf>,
}
struct Zones {
    db: Option<jiff::tz::TimeZoneDatabase>,
    local: jiff::tz::TimeZone,
}
fn offset_in(tz: &jiff::tz::TimeZone, n: core::NaiveDateTime) -> Option<i32> {
    let dt = jiff::civil::DateTime::new(
        i16::try_from(n.year).ok()?,
        i8::try_from(n.month).ok()?,
        i8::try_from(n.day).ok()?,
        i8::try_from(n.hour).ok()?,
        i8::try_from(n.minute).ok()?,
        i8::try_from(n.second).ok()?,
        0,
    )
    .ok()?;
    let ts = tz.to_ambiguous_timestamp(dt).compatible().ok()?;
    // An effective offset also represents compatible gap disambiguation: 02:30 in a spring
    // gap advances to 03:30. Subtracting the *post*-transition offset would move it backwards.
    i32::try_from(core::naive_to_unix(n) - ts.as_second()).ok()
}
impl Zones {
    fn new(config: &TzConfig) -> Result<Self, IslandError> {
        let db = config
            .dir
            .as_ref()
            .map(|d| {
                jiff::tz::TimeZoneDatabase::from_dir(d).map_err(|_| {
                    IslandError::unavailable("Calendar time-zone database unavailable")
                })
            })
            .transpose()?;
        let mut zones = Self {
            db,
            local: jiff::tz::TimeZone::UTC,
        };
        zones.local = match &config.local {
            Some(name) => zones
                .named(name)
                .ok_or_else(|| IslandError::unavailable("Calendar local time zone unavailable"))?,
            None => jiff::tz::TimeZone::try_system()
                .map_err(|_| IslandError::unavailable("Calendar system time zone unavailable"))?,
        };
        Ok(zones)
    }
    fn named(&self, name: &str) -> Option<jiff::tz::TimeZone> {
        match &self.db {
            Some(db) => db.get(name).ok(),
            None => jiff::tz::db().get(name).ok(),
        }
    }
    fn local_offset(&self, n: core::NaiveDateTime) -> Option<i32> {
        offset_in(&self.local, n)
    }
    fn tzid_offset(&self, tzid: &str, n: core::NaiveDateTime) -> Option<i32> {
        offset_in(&self.named(tzid)?, n)
    }
    fn validate(&self, events: &[core::Event]) -> Result<(), IslandError> {
        for event in events {
            for value in std::iter::once(&event.dtstart)
                .chain(event.dtend.iter())
                .chain(event.exdates.iter())
                .chain(event.recurrence_id.iter())
            {
                if let core::DateTimeKind::Zoned { tzid } = &value.kind {
                    if self.named(tzid).is_none() {
                        return Err(IslandError::unsupported("Calendar time zone unavailable"));
                    }
                }
            }
        }
        Ok(())
    }
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Cached {
    body: String,
    fetched_at: f64,
}
struct State {
    local: BTreeMap<String, String>,
    remote: Vec<core::Event>,
    rev: u32,
    configured: bool,
    stale: bool,
    updated: Option<f64>,
    detail: String,
    failures: u32,
    readable: bool,
}
pub struct CalendarService {
    game_pause: crate::integrations::plasma::game_effects::PollPause,
    config: CalendarConfig,
    zones: Zones,
    client: reqwest::Client,
    state: Mutex<State>,
    pub topic: Latest<CalendarChanged>,
    stop: watch::Sender<bool>,
    worker: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    gate: tokio::sync::Mutex<()>,
}
fn epoch_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64() * 1000.)
        .unwrap_or(0.)
}
fn parse(text: &str) -> Result<Vec<core::Event>, IslandError> {
    // The staged parser slices fixed ASCII date fields. Refuse non-ASCII date property values
    // at the adapter boundary (Unicode TEXT remains valid).
    for line in text.lines() {
        if ["DTSTART", "DTEND", "EXDATE", "RECURRENCE-ID", "RRULE"]
            .iter()
            .any(|p| line.to_ascii_uppercase().starts_with(p))
            && !line.is_ascii()
        {
            return Err(IslandError::invalid("Invalid calendar date encoding"));
        }
    }
    core::parse_calendar(text)
        .map(|p| p.0)
        .map_err(|_| IslandError::invalid("Invalid calendar document"))
}
fn local_blocks(text: &str) -> Result<BTreeMap<String, String>, IslandError> {
    let normalized = text.replace("\r\n", "\n");
    let mut blocks = BTreeMap::new();
    let mut buffer = None::<String>;
    parse(text)?;
    for line in normalized.lines() {
        if line.eq_ignore_ascii_case("BEGIN:VEVENT") {
            if buffer.is_some() {
                return Err(IslandError::invalid("Nested local event"));
            }
            buffer = Some("BEGIN:VEVENT\r\n".into());
        } else if line.eq_ignore_ascii_case("END:VEVENT") {
            let Some(mut body) = buffer.take() else {
                return Err(IslandError::invalid("Unmatched local event"));
            };
            body.push_str("END:VEVENT\r\n");
            let (mut events, warnings) =
                core::parse_calendar(&format!("BEGIN:VCALENDAR\r\n{body}END:VCALENDAR\r\n"))
                    .map_err(|_| IslandError::invalid("Invalid local event"))?;
            if events.len() != 1 || !warnings.is_empty() {
                return Err(IslandError::invalid("Invalid or unsupported local event"));
            }
            let Some(event) = events.pop() else {
                return Err(IslandError::invalid("Missing local event"));
            };
            if blocks.insert(event.uid, body).is_some() {
                return Err(IslandError::invalid("Duplicate local calendar id"));
            }
        } else if let Some(body) = buffer.as_mut() {
            body.push_str(line);
            body.push_str("\r\n");
        }
    }
    if buffer.is_some() {
        return Err(IslandError::invalid("Unclosed local event"));
    }
    Ok(blocks)
}
fn document(blocks: &BTreeMap<String, String>) -> String {
    let mut text =
        String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Hermes Island//Calendar//EN\r\n");
    for block in blocks.values() {
        text.push_str(block);
    }
    text.push_str("END:VCALENDAR\r\n");
    text
}
impl CalendarService {
    pub fn new(config: CalendarConfig) -> Result<Arc<Self>, IslandError> {
        Self::new_gated(config, Default::default())
    }
    pub fn new_gated(
        config: CalendarConfig,
        game_pause: crate::integrations::plasma::game_effects::PollPause,
    ) -> Result<Arc<Self>, IslandError> {
        let client = reqwest::Client::builder()
            .timeout(config.timeout)
            .connect_timeout(config.timeout)
            .redirect(reqwest::redirect::Policy::none())
            .https_only(!config.loopback_test)
            .build()
            .map_err(|_| IslandError::unavailable("Calendar HTTP unavailable"))?;
        let loaded = private_file::read(&config.data.join("calendar-local.ics"), LOCAL_CAP)
            .and_then(|data| match data {
                None => Ok(BTreeMap::new()),
                Some(data) => std::str::from_utf8(&data)
                    .map_err(|_| IslandError::invalid("Local calendar encoding invalid"))
                    .and_then(local_blocks),
            });
        let readable = loaded.is_ok();
        let cache = private_file::read(
            &config.data.join("calendar-cache.json"),
            BODY_CAP * 6 + 1024,
        )
        .ok()
        .flatten()
        .and_then(|b| serde_json::from_slice::<Cached>(&b).ok())
        .filter(|c| c.body.len() <= BODY_CAP && c.fetched_at.is_finite() && c.fetched_at >= 0.);
        let (remote, updated) = match cache {
            Some(c) => match parse(&c.body) {
                Ok(e) => (e, Some(c.fetched_at)),
                Err(_) => (vec![], None),
            },
            None => (vec![], None),
        };
        let service = Arc::new(Self {
            game_pause,
            zones: Zones::new(&config.tz)?,
            config,
            client,
            state: Mutex::new(State {
                local: loaded.unwrap_or_default(),
                remote,
                rev: 0,
                configured: false,
                stale: updated.is_some(),
                updated,
                detail: if readable {
                    "not-configured"
                } else {
                    "local-load-failed-file-preserved"
                }
                .into(),
                failures: 0,
                readable,
            }),
            topic: Latest::new(),
            stop: watch::channel(false).0,
            worker: Mutex::new(None),
            gate: tokio::sync::Mutex::new(()),
        });
        service.publish()?;
        Ok(service)
    }
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, State>, IslandError> {
        self.state
            .lock()
            .map_err(|_| IslandError::internal("Calendar state poisoned"))
    }
    fn secret(&self) -> Result<Option<String>, IslandError> {
        let bytes = private_file::read(&self.config.data.join("calendar-secret-url"), URL_CAP)?;
        let Some(bytes) = bytes else {
            return Ok(None);
        };
        let text = String::from_utf8(bytes)
            .map_err(|_| IslandError::invalid("Calendar URL encoding invalid"))?;
        self.validate_url(text.trim()).map(Some)
    }
    /// Shared by the private file reader and the write-only `set_subscription` IPC.
    fn validate_url(&self, text: &str) -> Result<String, IslandError> {
        let text = text.to_owned();
        let url =
            reqwest::Url::parse(&text).map_err(|_| IslandError::invalid("Calendar URL invalid"))?;
        let loopback = self.config.loopback_test
            && url.scheme() == "http"
            && matches!(url.host_str(), Some("127.0.0.1" | "[::1]"));
        if (if self.config.loopback_test {
            !loopback
        } else {
            url.scheme() != "https"
        }) || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return Err(IslandError::invalid("Calendar requires HTTPS"));
        }
        Ok(text)
    }
    /// Write-only subscription setup from the island UI. The URL is validated, stored in the
    /// private 0600 file and fetched once; it is never returned over IPC. A link that cannot be
    /// fetched is rolled back to the previous file so a typo never replaces a working feed.
    /// `None` disconnects: removes the link and the cached subscription events.
    pub async fn set_subscription(
        self: &Arc<Self>,
        url: Option<String>,
        now: f64,
    ) -> Result<(), IslandError> {
        let path = self.config.data.join("calendar-secret-url");
        let Some(url) = url else {
            for name in ["calendar-secret-url", "calendar-cache.json"] {
                if let Err(e) = std::fs::remove_file(self.config.data.join(name)) {
                    if e.kind() != std::io::ErrorKind::NotFound {
                        return Err(IslandError::io("Calendar subscription removal failed"));
                    }
                }
            }
            {
                let mut s = self.lock()?;
                s.remote = vec![];
                s.updated = None;
                s.failures = 0;
            }
            return self.refresh(now).await;
        };
        let text = url.trim();
        // Google/Apple hand out webcal:// links; they are the same feed over HTTPS.
        let text = match text.get(..9) {
            Some(scheme) if scheme.eq_ignore_ascii_case("webcal://") => {
                format!("https://{}", &text[9..])
            }
            _ => text.to_owned(),
        };
        let text = self.validate_url(&text)?;
        let previous = private_file::read(&path, URL_CAP).ok().flatten();
        private_file::write(&path, text.as_bytes(), URL_CAP)?;
        let result = self.refresh(now).await;
        if let Err(error) = result {
            match previous {
                Some(bytes) => private_file::write(&path, &bytes, URL_CAP)?,
                None => {
                    if let Err(e) = std::fs::remove_file(&path) {
                        if e.kind() != std::io::ErrorKind::NotFound {
                            return Err(IslandError::io("Calendar subscription rollback failed"));
                        }
                    }
                }
            }
            // Republish the restored state (configured flag/detail) without hiding the cause.
            if self.refresh(now).await.is_err() {
                tracing::warn!("Calendar state republish after rollback failed");
            }
            return Err(error);
        }
        Ok(())
    }
    fn snapshot(
        &self,
        s: &State,
        now: f64,
        start: i64,
        end: i64,
        fallback_offset: i32,
        secret: Option<&str>,
    ) -> Result<CalendarChanged, IslandError> {
        let local = parse(&document(&s.local))?;
        let zones_valid = self
            .zones
            .validate(&local)
            .and_then(|()| self.zones.validate(&s.remote))
            .is_ok();
        let mut agenda = Vec::new();
        for (events, source) in [
            (&local, CalendarSource::Local),
            (&s.remote, CalendarSource::Subscription),
        ] {
            if !zones_valid {
                break;
            }
            let local = |n| self.zones.local_offset(n);
            let (rows, _warnings) = core::agenda_zoned(
                events,
                start,
                end,
                fallback_offset,
                Some(&local),
                &|tzid, n| self.zones.tzid_offset(tzid, n),
            )
            .map_err(|_| IslandError::invalid("Calendar window exceeds 400 days"))?;
            for i in rows {
                let scrub = |text: String| match secret {
                    Some(url) => text.replace(url, "[redacted]"),
                    None => text,
                };
                agenda.push(AgendaRow {
                    uid: scrub(i.uid),
                    summary: scrub(i.summary),
                    location: i.location.map(scrub),
                    start_s: i.start_s as f64,
                    end_s: i.end_s as f64,
                    all_day: i.all_day,
                    source,
                    read_only: source == CalendarSource::Subscription,
                });
            }
        }
        agenda.sort_by(|a, b| {
            a.start_s
                .total_cmp(&b.start_s)
                .then_with(|| b.all_day.cmp(&a.all_day))
                .then_with(|| a.uid.cmp(&b.uid))
        });
        agenda.truncate(2000);
        Ok(CalendarChanged {
            rev: s.rev,
            available: zones_valid && s.readable && s.detail != "fetch-failed",
            detail: if zones_valid {
                s.detail.clone()
            } else {
                "unsupported-time-zone".into()
            },
            configured: s.configured,
            stale: s.stale,
            age_ms: s.updated.map(|at| (now - at).max(0.)),
            updated_at: s.updated,
            agenda,
        })
    }
    pub fn current(
        &self,
        now: f64,
        start: i64,
        end: i64,
        offset: i32,
    ) -> Result<CalendarChanged, IslandError> {
        let secret = self.secret().ok().flatten();
        self.snapshot(&*self.lock()?, now, start, end, offset, secret.as_deref())
    }
    fn publish(&self) -> Result<(), IslandError> {
        let now = epoch_ms();
        let start = now as i64 / 1000 - 86400;
        let secret = self.secret().ok().flatten();
        let mut s = self.lock()?;
        s.rev = s.rev.wrapping_add(1);
        let value = self.snapshot(&s, now, start, start + 32 * 86400, 0, secret.as_deref())?;
        self.topic.publish(value);
        Ok(())
    }
    pub fn apply(&self, op: CalendarOp) -> Result<(), IslandError> {
        let mut s = self.lock()?;
        if !s.readable {
            return Err(IslandError::unavailable("Local calendar file preserved"));
        }
        let mut candidate = s.local.clone();
        match op {
            CalendarOp::Delete { source, uid } => {
                if source != CalendarSource::Local {
                    return Err(IslandError::invalid("Subscribed events are read-only"));
                }
                if candidate.remove(&uid).is_none() {
                    return Err(IslandError::invalid("Local event not found"));
                }
            }
            CalendarOp::Create { ics } => {
                if ics.len() > LOCAL_CAP {
                    return Err(IslandError::invalid("Local event too large"));
                }
                let mut blocks = local_blocks(&ics)?;
                if blocks.len() != 1 {
                    return Err(IslandError::invalid("Exactly one local event required"));
                }
                let Some((uid, body)) = blocks.pop_first() else {
                    return Err(IslandError::invalid("Missing local event"));
                };
                if candidate.contains_key(&uid) {
                    return Err(IslandError::invalid("Local event already exists"));
                }
                candidate.insert(uid, body);
            }
            CalendarOp::Edit {
                source: CalendarSource::Local,
                uid,
                ics,
            } => {
                if ics.len() > LOCAL_CAP {
                    return Err(IslandError::invalid("Local event too large"));
                }
                let mut blocks = local_blocks(&ics)?;
                if blocks.len() != 1 || !blocks.contains_key(&uid) || !candidate.contains_key(&uid)
                {
                    return Err(IslandError::invalid(
                        "Edit must retain an existing local event id",
                    ));
                }
                let Some(body) = blocks.remove(&uid) else {
                    return Err(IslandError::invalid("Missing local event"));
                };
                candidate.insert(uid, body);
            }
            CalendarOp::Edit { .. } => {
                return Err(IslandError::invalid("Subscribed events are read-only"))
            }
        }
        self.zones.validate(&parse(&document(&candidate))?)?;
        private_file::write(
            &self.config.data.join("calendar-local.ics"),
            document(&candidate).as_bytes(),
            LOCAL_CAP,
        )?;
        s.local = candidate;
        drop(s);
        self.publish()
    }
    pub async fn refresh(self: &Arc<Self>, now: f64) -> Result<(), IslandError> {
        if !now.is_finite() || now < 0. {
            return Err(IslandError::invalid("Invalid calendar clock"));
        }
        let _gate = self.gate.lock().await;
        let service = Arc::clone(self);
        let secret = tauri::async_runtime::spawn_blocking(move || service.secret())
            .await
            .map_err(|_| IslandError::internal("Calendar file worker failed"))?;
        let result = match secret {
            Ok(Some(url)) => {
                self.lock()?.configured = true;
                let mut stop = self.stop.subscribe();
                let result = if *stop.borrow() {
                    Err(IslandError::unavailable("Calendar stopped"))
                } else {
                    tokio::select! {_=stop.changed()=>Err(IslandError::unavailable("Calendar stopped")),result=self.fetch(&url)=>result}
                };
                result.map(|body| (url, body))
            }
            Ok(None) => {
                {
                    let mut s = self.lock()?;
                    s.configured = false;
                    s.stale = s.updated.is_some();
                    s.detail = "not-configured".into();
                }
                let service = Arc::clone(self);
                return tauri::async_runtime::spawn_blocking(move || service.publish())
                    .await
                    .map_err(|_| IslandError::internal("Calendar file worker failed"))?;
            }
            Err(e) => Err(e),
        };
        let service = Arc::clone(self);
        tauri::async_runtime::spawn_blocking(move || service.finish_refresh(now, result))
            .await
            .map_err(|_| IslandError::internal("Calendar file worker failed"))?
    }
    fn finish_refresh(
        &self,
        now: f64,
        result: Result<(String, String), IslandError>,
    ) -> Result<(), IslandError> {
        let result = match result {
            Ok((url, body)) => {
                if body.contains(&url) {
                    Err(IslandError::invalid(
                        "Calendar response contains secret URL",
                    ))
                } else {
                    parse(&body).and_then(|events| {
                        self.zones.validate(&events)?;
                        let cache = serde_json::to_vec(&Cached {
                            body,
                            fetched_at: now,
                        })
                        .map_err(|_| IslandError::internal("Calendar cache encoding failed"))?;
                        private_file::write(
                            &self.config.data.join("calendar-cache.json"),
                            &cache,
                            BODY_CAP * 6 + 1024,
                        )?;
                        let mut s = self.lock()?;
                        s.remote = events;
                        s.updated = Some(now);
                        s.stale = false;
                        s.detail = "fresh".into();
                        s.failures = 0;
                        Ok(())
                    })
                }
            }
            Err(e) => Err(e),
        };
        if result.is_err() {
            let mut s = self.lock()?;
            s.stale = s.updated.is_some();
            s.detail = "fetch-failed".into();
            s.failures = s.failures.saturating_add(1);
        }
        self.publish()?;
        result
    }
    async fn fetch(&self, url: &str) -> Result<String, IslandError> {
        let mut response = self
            .client
            .get(url)
            .header("accept", "text/calendar")
            .send()
            .await
            .map_err(|_| IslandError::unavailable("Calendar request failed"))?;
        if !response.status().is_success() {
            return Err(IslandError::unavailable("Calendar HTTP failure"));
        }
        if response
            .content_length()
            .is_some_and(|n| n > BODY_CAP as u64)
        {
            return Err(IslandError::invalid("Calendar body too large"));
        }
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| IslandError::unavailable("Calendar body failed"))?
        {
            if chunk.len() > BODY_CAP.saturating_sub(body.len()) {
                return Err(IslandError::invalid("Calendar body too large"));
            }
            body.extend_from_slice(&chunk);
        }
        String::from_utf8(body).map_err(|_| IslandError::invalid("Calendar encoding invalid"))
    }
    pub fn next_refresh_delay(&self) -> Result<Duration, IslandError> {
        let s = self.lock()?;
        Ok(Duration::from_millis(if s.failures == 0 {
            REFRESH_MS
        } else {
            30_000u64
                .saturating_mul(1u64 << s.failures.saturating_sub(1).min(10))
                .min(MAX_BACKOFF_MS)
        }))
    }
    pub fn start(self: &Arc<Self>) -> Result<(), IslandError> {
        let mut slot = self
            .worker
            .lock()
            .map_err(|_| IslandError::internal("Calendar worker poisoned"))?;
        if slot.is_some() {
            return Err(IslandError::invalid("Calendar worker already running"));
        }
        let service = Arc::clone(self);
        let mut stop = self.stop.subscribe();
        *slot = Some(tauri::async_runtime::spawn(async move {
            let mut pause = service.game_pause.subscribe();
            loop {
                if *stop.borrow() {
                    return;
                }
                let paused = *pause.borrow_and_update();
                if paused {
                    tokio::select! { _ = stop.changed() => return, _ = pause.changed() => {} }
                    continue;
                }
                tokio::select! {
                    _ = stop.changed() => return,
                    _ = pause.changed() => continue,
                    result = service.refresh(epoch_ms()) => {
                        if result.is_err() { tracing::warn!("Calendar refresh failed"); }
                    }
                }
                let delay = match service.next_refresh_delay() {
                    Ok(d) => d,
                    Err(_) => return,
                };
                tokio::select! {_=stop.changed()=>return,_=pause.changed()=>{},()=tokio::time::sleep(delay)=>{}}
            }
        }));
        Ok(())
    }
    pub fn shutdown(&self) -> Result<(), IslandError> {
        self.stop.send_replace(true);
        let task = self
            .worker
            .lock()
            .map_err(|_| IslandError::internal("Calendar worker poisoned"))?
            .take();
        if let Some(task) = task {
            tauri::async_runtime::block_on(async {
                tokio::time::timeout(Duration::from_secs(2), task)
                    .await
                    .map_err(|_| IslandError::timeout("Calendar shutdown"))?
                    .map_err(|_| IslandError::internal("Calendar worker panicked"))
            })?;
        }
        Ok(())
    }
}
