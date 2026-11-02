//! Read-only update checks. Full successes hourly, accepted core backoff after failure.
//! The injected monotonic clock drives age/schedule; no install/update argv exists here.
use super::core::{self, Badge, Schedule, UpdatesError};
use crate::{
    contract::{
        controls::{PackageUpdate, UpdatesChanged, UpdatesOp},
        system::PowerState,
    },
    integrations::{
        control_actor::{respond, ControlActor, Request, State},
        control_process::Runner,
    },
    state::machine::Clock,
};
use std::{sync::Arc, time::Duration};
use tokio::sync::{mpsc, watch};
impl State for UpdatesChanged {
    fn rev(&mut self) -> &mut u32 {
        &mut self.rev
    }
}
pub type UpdatesService = ControlActor<UpdatesChanged, UpdatesOp>;
fn wire(
    badge: &Badge,
    now: u64,
    last_ok: Option<u64>,
    next: u64,
    available: bool,
) -> UpdatesChanged {
    UpdatesChanged {
        rev: 0,
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
                }
                .into(),
            })
            .collect(),
        age_secs: last_ok.map(|t| now.saturating_sub(t) as f64),
        next_check_in_secs: next.saturating_sub(now) as f64,
    }
}
impl UpdatesService {
    pub fn start(
        runner: Runner,
        clock: Arc<dyn Clock + Send + Sync>,
        power: watch::Receiver<Option<PowerState>>,
    ) -> Arc<Self> {
        let badge = Badge::compute(
            Err(UpdatesError::CommandFailed),
            Some(Err(UpdatesError::CommandFailed)),
            0,
            None,
            None,
        );
        let (service, rx) = Self::new(wire(&badge, 0, None, 0, false));
        let owned = service.clone();
        service.own(tauri::async_runtime::spawn(async move {
            worker(owned, runner, clock, power, rx).await
        }));
        service
    }
}
struct Model {
    schedule: Schedule,
    last_attempt: Option<u64>,
    last_ok: Option<u64>,
    previous: Option<Vec<core::Update>>,
    badge: Badge,
    available: bool,
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
    async fn check(&mut self, runner: &Runner, now: u64) {
        self.last_attempt = Some(now);
        let repo = match runner
            .run(
                &core::CHECKUPDATES_ARGV
                    .iter()
                    .map(|s| (*s).into())
                    .collect::<Vec<_>>(),
                Duration::from_secs(10),
                core::MAX_STDOUT_BYTES,
            )
            .await
        {
            Ok(output) => core::parse_checkupdates(output.code, &output.stdout, &output.stderr),
            Err(_) => Err(UpdatesError::CommandFailed),
        };
        let aur = match runner
            .run(
                &core::PARU_QUA_ARGV
                    .iter()
                    .map(|s| (*s).into())
                    .collect::<Vec<_>>(),
                Duration::from_secs(10),
                core::MAX_STDOUT_BYTES,
            )
            .await
        {
            Ok(output) => core::parse_paru_qua(output.code, &output.stdout, &output.stderr),
            Err(_) => Err(UpdatesError::CommandFailed),
        };
        self.available = repo.is_ok() && aur.is_ok();
        if self.available {
            self.last_ok = Some(now);
            self.schedule.note_success();
        } else {
            self.schedule.note_failure();
        }
        self.badge = Badge::compute(repo, Some(aur), now, self.last_ok, self.previous.as_deref());
        if self.available {
            self.previous = Some(self.badge.items.clone());
        }
    }
}
async fn worker(
    service: Arc<UpdatesService>,
    runner: Runner,
    clock: Arc<dyn Clock + Send + Sync>,
    mut power: watch::Receiver<Option<PowerState>>,
    mut rx: mpsc::Receiver<Request<UpdatesChanged, UpdatesOp>>,
) {
    let mut stop = service.stop.subscribe();
    let mut model = Model {
        schedule: Schedule::new(true),
        last_attempt: None,
        last_ok: None,
        previous: None,
        badge: Badge::compute(
            Err(UpdatesError::CommandFailed),
            Some(Err(UpdatesError::CommandFailed)),
            0,
            None,
            None,
        ),
        available: false,
    };
    // A user Refresh answers at once (the request has an 8 s deadline, a check can take
    // 20 s) and then runs a real read-only check; the result arrives as a topic event.
    let mut forced = false;
    loop {
        if *stop.borrow() {
            break;
        }
        let now = clock.now_ms() / 1000;
        model.schedule.on_battery = power
            .borrow()
            .as_ref()
            .and_then(|p| p.available.then_some(p.on_battery).flatten())
            .unwrap_or(true);
        let force = std::mem::take(&mut forced)
            && model
                .last_attempt
                .is_none_or(|last| now.saturating_sub(last) >= 15);
        if force || model.schedule.due(now, model.last_attempt, model.last_ok) {
            tokio::select! {_=stop.changed()=>break,()=model.check(&runner,now)=>{}}
        }
        let now = clock.now_ms() / 1000;
        let next = model.next(now);
        service.publish(wire(
            &model.badge,
            now,
            model.last_ok,
            next,
            model.available,
        ));
        tokio::select! {
            _=stop.changed()=>break,
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
