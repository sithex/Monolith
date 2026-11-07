//! Read-only update checks. Full successes hourly, accepted core backoff after failure.
//! The injected monotonic clock drives age/schedule; no install/update argv exists here.
//!
//! The backend (pacman, dnf or none) is chosen once at start from os-release
//! ([`core::select_backend`]). An unsupported system runs no command at all: it publishes one
//! calm "unsupported-system" snapshot and only answers requests, with no schedule or backoff.
use super::core::{self, Backend, Badge, Schedule, UpdatesError};
use crate::{
    contract::{
        controls::{PackageUpdate, UpdatesBackend, UpdatesChanged, UpdatesOp},
        system::PowerState,
    },
    integrations::{
        control_actor::{respond, ControlActor, Request, State},
        control_process::Runner,
    },
    platform::os_release::Distro,
    state::machine::Clock,
};
use std::{path::Path, sync::Arc, time::Duration};
use tokio::sync::{mpsc, watch};
impl State for UpdatesChanged {
    fn rev(&mut self) -> &mut u32 {
        &mut self.rev
    }
}
pub type UpdatesService = ControlActor<UpdatesChanged, UpdatesOp>;
/// pacman checks are local and fast; dnf may have to refresh expired metadata over the network.
const PACMAN_TIMEOUT: Duration = Duration::from_secs(10);
const DNF_TIMEOUT: Duration = Duration::from_secs(60);
const RPM_TIMEOUT: Duration = Duration::from_secs(10);
/// Message of the runner's spawn failure (executable missing or not runnable).
const SPAWN_FAILED: &str = "Controls helper could not start";
fn wire_backend(backend: Backend) -> UpdatesBackend {
    match backend {
        Backend::Pacman => UpdatesBackend::Pacman,
        Backend::Dnf => UpdatesBackend::Dnf,
        Backend::Unsupported(_) => UpdatesBackend::Unsupported,
    }
}
fn wire(
    backend: Backend,
    badge: &Badge,
    now: u64,
    last_ok: Option<u64>,
    next: u64,
    available: bool,
) -> UpdatesChanged {
    UpdatesChanged {
        rev: 0,
        backend: wire_backend(backend),
        available,
        detail: if available {
            "read-only-checks"
        } else {
            "checks-unavailable"
        }
        .into(),
        count: badge.count,
        repo_count: badge.repo_count,
        aur_count: badge.aur_count,
        needs_reboot: badge.needs_reboot,
        stale: badge.stale,
        error: badge.error.clone(),
        items: badge
            .items
            .iter()
            .map(|p| PackageUpdate {
                name: p.name.clone(),
                old_version: p.old_version.clone(),
                new_version: p.new_version.clone(),
                ignored: p.ignored,
                source: match p.source {
                    core::Source::Repo => "repo",
                    core::Source::Aur => "aur",
                    core::Source::Dnf => "dnf",
                }
                .into(),
                arch: p.arch.clone(),
            })
            .collect(),
        age_secs: last_ok.map(|t| now.saturating_sub(t) as f64),
        next_check_in_secs: next.saturating_sub(now) as f64,
    }
}
/// The one snapshot an unsupported system ever shows: nothing unknown is called an error.
fn unsupported_wire() -> UpdatesChanged {
    UpdatesChanged {
        rev: 0,
        backend: UpdatesBackend::Unsupported,
        available: false,
        detail: "unsupported-system".into(),
        count: None,
        repo_count: None,
        aur_count: None,
        needs_reboot: false,
        stale: false,
        error: None,
        items: Vec::new(),
        age_secs: None,
        next_check_in_secs: 0.,
    }
}
/// Badge before the first check: every source of this backend unknown.
fn initial_badge(backend: Backend) -> Badge {
    match backend {
        Backend::Dnf => {
            Badge::compute_repo_only(Err(UpdatesError::CommandFailed), "dnf", 0, None, None)
        }
        Backend::Pacman | Backend::Unsupported(_) => Badge::compute(
            Err(UpdatesError::CommandFailed),
            Some(Err(UpdatesError::CommandFailed)),
            0,
            None,
            None,
        ),
    }
}
/// True when an executable file named `name` is on `path` (a `PATH`-style list).
fn resolves(name: &str, path: Option<&std::ffi::OsStr>) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.is_some_and(|path| {
        std::env::split_paths(path).any(|dir| {
            dir.is_absolute()
                && std::fs::metadata(dir.join(name))
                    .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        })
    })
}
impl UpdatesService {
    /// The backend for this machine: `/etc/os-release` (else `/usr/lib/os-release`) and, for
    /// the Fedora family, whether `dnf` is on PATH.
    pub fn host_backend() -> Backend {
        let distro = Distro::read(Path::new("/"));
        let backend = core::select_backend(
            distro.as_ref(),
            resolves("dnf", std::env::var_os("PATH").as_deref()),
        );
        tracing::info!(
            id = distro.as_ref().map(|d| d.id.as_str()),
            ?backend,
            "updates backend"
        );
        backend
    }
    pub fn start(
        runner: Runner,
        clock: Arc<dyn Clock + Send + Sync>,
        power: watch::Receiver<Option<PowerState>>,
    ) -> Arc<Self> {
        Self::start_gated(runner, clock, power, Default::default())
    }
    pub fn start_gated(
        runner: Runner,
        clock: Arc<dyn Clock + Send + Sync>,
        power: watch::Receiver<Option<PowerState>>,
        game_pause: crate::integrations::plasma::game_effects::PollPause,
    ) -> Arc<Self> {
        Self::start_with_backend(Self::host_backend(), runner, clock, power, game_pause)
    }
    /// Start with an explicit backend (tests; the host's comes from [`Self::host_backend`]).
    pub fn start_with_backend(
        backend: Backend,
        runner: Runner,
        clock: Arc<dyn Clock + Send + Sync>,
        power: watch::Receiver<Option<PowerState>>,
        game_pause: crate::integrations::plasma::game_effects::PollPause,
    ) -> Arc<Self> {
        let initial = if let Backend::Unsupported(_) = backend {
            unsupported_wire()
        } else {
            wire(backend, &initial_badge(backend), 0, None, 0, false)
        };
        let (service, rx) = Self::new(initial);
        let owned = service.clone();
        service.own(tauri::async_runtime::spawn(async move {
            if let Backend::Unsupported(_) = backend {
                idle(owned, rx).await
            } else {
                worker(owned, backend, runner, clock, power, rx, game_pause).await
            }
        }));
        service
    }
}
struct Model {
    backend: Backend,
    schedule: Schedule,
    last_attempt: Option<u64>,
    last_ok: Option<u64>,
    previous: Option<Vec<core::Update>>,
    badge: Badge,
    available: bool,
}
fn argv(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| (*s).into()).collect()
}
impl Model {
    fn next(&self, now: u64) -> u64 {
        if self.schedule.due(now, self.last_attempt, self.last_ok) {
            return now;
        }
        let failed = self
            .last_ok
            .is_none_or(|ok| self.last_attempt.is_some_and(|last| last > ok));
        let delay = if failed {
            core::backoff_minutes(self.schedule.consecutive_failures) * 60
        } else {
            3600
        };
        let due = self.last_attempt.unwrap_or(now).saturating_add(delay);
        if self.schedule.on_battery {
            self.last_ok.map_or(due, |ok| {
                due.max(ok.saturating_add(core::STALE_AFTER_SECS).saturating_add(1))
            })
        } else {
            due
        }
    }
    /// `cache_only`: known to be on battery and not asked by the user (dnf only).
    async fn check(&mut self, runner: &Runner, now: u64, cache_only: bool) {
        self.last_attempt = Some(now);
        let (available, badge) = match self.backend {
            Backend::Dnf => {
                let repo = check_dnf(runner, cache_only).await;
                let available = repo.is_ok();
                let last_ok = if available { Some(now) } else { self.last_ok };
                let badge =
                    Badge::compute_repo_only(repo, "dnf", now, last_ok, self.previous.as_deref());
                (available, badge)
            }
            Backend::Pacman | Backend::Unsupported(_) => self.check_pacman(runner, now).await,
        };
        self.available = available;
        if self.available {
            self.last_ok = Some(now);
            self.schedule.note_success();
        } else {
            self.schedule.note_failure();
        }
        self.badge = badge;
        if self.available {
            self.previous = Some(self.badge.items.clone());
        }
    }
    async fn check_pacman(&self, runner: &Runner, now: u64) -> (bool, Badge) {
        let repo = match runner
            .run(
                &argv(&core::CHECKUPDATES_ARGV),
                PACMAN_TIMEOUT,
                core::MAX_STDOUT_BYTES,
            )
            .await
        {
            Ok(output) => core::parse_checkupdates(output.code, &output.stdout, &output.stderr),
            Err(_) => Err(UpdatesError::CommandFailed),
        };
        // No `paru` (not installed) only takes the AUR half away; `checkupdates` still counts.
        let aur = match runner
            .run(
                &argv(&core::PARU_QUA_ARGV),
                PACMAN_TIMEOUT,
                core::MAX_STDOUT_BYTES,
            )
            .await
        {
            Ok(output) => Some(core::parse_paru_qua(
                output.code,
                &output.stdout,
                &output.stderr,
            )),
            Err(e) if e.message == SPAWN_FAILED => None,
            Err(_) => Some(Err(UpdatesError::CommandFailed)),
        };
        let available = repo.is_ok() && aur.as_ref().is_none_or(Result::is_ok);
        let last_ok = if available { Some(now) } else { self.last_ok };
        let badge = match aur {
            Some(aur) => Badge::compute(repo, Some(aur), now, last_ok, self.previous.as_deref()),
            None => Badge::compute_repo_only(repo, "repo", now, last_ok, self.previous.as_deref()),
        };
        (available, badge)
    }
}
/// `dnf check-upgrade --json`, then installed versions from batched `rpm -q` calls. An rpm
/// failure only leaves old versions empty; it never fails the check.
async fn check_dnf(runner: &Runner, cache_only: bool) -> Result<Vec<core::Update>, UpdatesError> {
    let shape: &[&str] = if cache_only {
        &core::DNF_CHECK_UPGRADE_CACHED_ARGV
    } else {
        &core::DNF_CHECK_UPGRADE_ARGV
    };
    let upgrades = match runner
        .run(&argv(shape), DNF_TIMEOUT, core::DNF_MAX_STDOUT_BYTES)
        .await
    {
        Ok(output) => core::parse_dnf_check_upgrade(output.code, &output.stdout, &output.stderr)?,
        Err(_) => return Err(UpdatesError::CommandFailed),
    };
    let keys: Vec<String> = upgrades.iter().map(core::DnfUpgrade::key).collect();
    let mut installed = std::collections::HashMap::new();
    for batch in keys.chunks(core::RPM_BATCH) {
        match runner
            .run(
                &core::rpm_query_argv(batch),
                RPM_TIMEOUT,
                core::DNF_MAX_STDOUT_BYTES,
            )
            .await
        {
            Ok(output) => installed.extend(core::parse_rpm_installed(&output.stdout)),
            Err(e) => tracing::debug!(error = %e.message, "rpm -q failed; old versions unknown"),
        }
    }
    Ok(core::dnf_updates(upgrades, &installed))
}
/// Unsupported system: answer requests with the one snapshot until stopped. No checks, no timer.
async fn idle(
    service: Arc<UpdatesService>,
    mut rx: mpsc::Receiver<Request<UpdatesChanged, UpdatesOp>>,
) {
    let mut stop = service.stop.subscribe();
    loop {
        if *stop.borrow() {
            break;
        }
        tokio::select! {
            _ = stop.changed() => break,
            request = rx.recv() => {
                let Some(request) = request else { break; };
                respond(request.reply, Ok(service.current()));
            }
        }
    }
    let mut value = service.current();
    value.detail = "stopped".into();
    service.publish(value);
}
async fn worker(
    service: Arc<UpdatesService>,
    backend: Backend,
    runner: Runner,
    clock: Arc<dyn Clock + Send + Sync>,
    mut power: watch::Receiver<Option<PowerState>>,
    mut rx: mpsc::Receiver<Request<UpdatesChanged, UpdatesOp>>,
    game_pause: crate::integrations::plasma::game_effects::PollPause,
) {
    let mut pause = game_pause.subscribe();
    let mut was_paused = false;
    let mut stop = service.stop.subscribe();
    let mut model = Model {
        backend,
        schedule: Schedule::new(true),
        last_attempt: None,
        last_ok: None,
        previous: None,
        badge: initial_badge(backend),
        available: false,
    };
    // A user Refresh answers at once (the request has an 8 s deadline, a check can take
    // 20 s) and then runs a real read-only check; the result arrives as a topic event.
    let mut forced = false;
    loop {
        if *stop.borrow() {
            break;
        }
        let paused = *pause.borrow_and_update();
        if paused {
            was_paused = true;
            tokio::select! {
                _ = stop.changed() => break,
                _ = pause.changed() => {},
                request = rx.recv() => {
                    let Some(request) = request else { break; };
                    respond(request.reply, Ok(service.current()));
                }
            }
            continue;
        }
        let resumed = std::mem::take(&mut was_paused);
        let now = clock.now_ms() / 1000;
        let known_battery = power
            .borrow()
            .as_ref()
            .and_then(|p| p.available.then_some(p.on_battery).flatten());
        model.schedule.on_battery = known_battery.unwrap_or(true);
        let user_asked = std::mem::take(&mut forced);
        let force = user_asked
            && model
                .last_attempt
                .is_none_or(|last| now.saturating_sub(last) >= 15);
        if resumed || force || model.schedule.due(now, model.last_attempt, model.last_ok) {
            let cache_only = known_battery == Some(true) && !force;
            tokio::select! {_=stop.changed()=>break,_=pause.changed()=>{was_paused=true;continue;},()=model.check(&runner,now,cache_only)=>{}}
        }
        let now = clock.now_ms() / 1000;
        let next = model.next(now);
        service.publish(wire(
            backend,
            &model.badge,
            now,
            model.last_ok,
            next,
            model.available,
        ));
        tokio::select! {
            _=stop.changed()=>break,
            _=pause.changed()=>{was_paused=true;},
            changed=power.changed()=>{if changed.is_err(){break;}},
            ()=tokio::time::sleep(Duration::from_secs(next.saturating_sub(now).max(1)))=>{},
            request=rx.recv()=>{let Some(request)=request else{break;};forced=true;if !request.reply.is_closed(){respond(request.reply,Ok(service.current()));}},
        }
    }
    let mut value = service.current();
    value.available = false;
    value.detail = "stopped".into();
    service.publish(value);
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn updates_dnf_resolution_needs_an_executable_file_on_path() {
        let root = std::env::temp_dir().join(format!("updates-resolve-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (a, b) = (root.join("a"), root.join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(b.join("dnf")).unwrap(); // a directory named dnf
        let path = std::env::join_paths([&a, &b]).unwrap();
        assert!(!resolves("dnf", Some(&path)));
        std::fs::write(a.join("dnf"), "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(a.join("dnf"), std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(!resolves("dnf", Some(&path)), "not executable");
        std::fs::set_permissions(a.join("dnf"), std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(resolves("dnf", Some(&path)));
        assert!(!resolves("dnf", None));
        // Relative PATH entries are never trusted.
        assert!(!resolves("dnf", Some(std::ffi::OsStr::new("relative:"))));
        let _ = std::fs::remove_dir_all(&root);
    }
}
