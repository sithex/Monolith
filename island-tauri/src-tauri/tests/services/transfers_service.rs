//! File-copy progress end to end on a private, scratch-only bus: a stand-in Plasma job server,
//! a KDE-style client, the island's passive monitor, and the notch signal on `/Companion`.
//! Never the user's session bus; the job server is a fake built from KDE's interface XML.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)]
use crate::private_bus::{wait_until, PrivateBus};
use hermes_island_lib::{
    error::IslandError,
    integrations::{downloads::Opener, jobs::TransfersService, plasma::companion::CompanionBus},
};
use tauri::async_runtime::block_on;
use std::{
    collections::HashMap,
    ffi::OsString,
    num::NonZeroUsize,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use zbus::{MessageStream, OwnedObjectPath, OwnedVariant, Variant};

const T: Duration = Duration::from_secs(10);
const VIEW: &str = "/org/kde/notificationmanager/jobs/JobView_1";

struct Server;
#[zbus::interface(name = "org.kde.JobViewServerV2")]
impl Server {
    #[zbus(name = "requestView")]
    fn request_view(
        &self,
        _app: String,
        _caps: i32,
        _hints: HashMap<String, OwnedVariant>,
    ) -> OwnedObjectPath {
        OwnedObjectPath::try_from(VIEW).unwrap()
    }
}

struct View {
    updates: Arc<AtomicU32>,
}
#[zbus::interface(name = "org.kde.JobViewV3")]
impl View {
    #[zbus(name = "update")]
    fn update(&self, _properties: HashMap<String, OwnedVariant>) {
        self.updates.fetch_add(1, Ordering::SeqCst);
    }
    #[zbus(name = "terminate")]
    fn terminate(&self, _code: u32, _message: String, _hints: HashMap<String, OwnedVariant>) {}
}

#[derive(Default)]
struct Recorder(Mutex<Vec<Vec<OsString>>>);
impl Opener for Recorder {
    fn open(&self, argv: &[OsString]) -> Result<(), IslandError> {
        self.0.lock().unwrap().push(argv.to_vec());
        Ok(())
    }
}

fn hints(entries: &[(&str, Variant<'static>)]) -> HashMap<String, OwnedVariant> {
    entries
        .iter()
        .map(|(k, v)| (k.to_string(), OwnedVariant::try_from(v.clone()).unwrap()))
        .collect()
}

#[test]
fn a_kde_copy_reaches_the_card_and_the_notch_in_steps_and_the_job_is_undisturbed() {
    let bus = PrivateBus::start("transfers");
    let address = bus.address();
    let dest = bus.root.join("home");
    let updates = Arc::new(AtomicU32::new(0));
    let server = block_on(
        zbus::connection::Builder::address(address.as_str())
            .name("org.kde.JobViewServer")
            .serve_at("/JobViewServer", Server)
            .serve_at(
                VIEW,
                View {
                    updates: updates.clone(),
                },
            )
            .build(),
    )
    .unwrap();

    let opener = Arc::new(Recorder::default());
    let service = TransfersService::start(Some(address.clone()), true, opener.clone());
    wait_until("monitor armed", T, || {
        (service.current().detail == "monitoring").then_some(())
    });
    let companion = block_on(CompanionBus::start_at(&address, None)).unwrap();
    companion.relay_copy_progress(service.progress());
    let listener = block_on(zbus::connection::Builder::address(address.as_str()).build())
        .unwrap();
    let mut signals = block_on(MessageStream::for_match_rule(
        "type='signal',interface='local.hermes.Monolith.Transfers',member='CopyProgress'",
        &listener,
        NonZeroUsize::new(64),
    ))
    .unwrap();
    let next_signal = |signals: &mut MessageStream| -> (i32, Instant) {
        use hermes_island_lib::integrations::notifications::linux_monitor::next;
        let message = block_on(async {
            tokio::time::timeout(T, next(signals))
                .await
                .unwrap()
                .unwrap()
                .unwrap()
        });
        (message.body().deserialize::<i32>().unwrap(), Instant::now())
    };
    let progress_now = || -> i32 {
        block_on(listener.call_method(
            Some("local.hermes.Monolith"),
            "/Companion",
            Some("local.hermes.Monolith.Transfers"),
            "Progress",
            &(),
        ))
        .unwrap()
        .body()
        .deserialize::<i32>()
        .unwrap()
    };
    assert_eq!(progress_now(), -1, "no copy yet");

    // The client: Dolphin-like requestView, then updates on the returned path.
    let client = block_on(zbus::connection::Builder::address(address.as_str()).build())
        .unwrap();
    let dest_url = format!("file://{}/movie.mkv", dest.display());
    let reply = block_on(client.call_method(
        Some("org.kde.JobViewServer"),
        "/JobViewServer",
        Some("org.kde.JobViewServerV2"),
        "requestView",
        &(
            "org.kde.dolphin",
            3i32,
            hints(&[
                ("title", Variant::from("Copying")),
                ("percent", Variant::from(5u32)),
                ("totalBytes", Variant::from(1000u64)),
                ("destUrl", Variant::from(dest_url.clone())),
            ]),
        ),
    ))
    .unwrap();
    let path: OwnedObjectPath = reply.body().deserialize().unwrap();
    assert_eq!(path.as_str(), VIEW);
    let card = wait_until("job on the card", T, || {
        service.current().items.first().cloned()
    });
    assert_eq!(card.app, "org.kde.dolphin");
    assert_eq!(card.percent, 5.);
    assert_eq!(card.destination, format!("{}/movie.mkv", dest.display()));
    let (start, started_at) = next_signal(&mut signals);
    assert_eq!(start, 0, "a copy shows at once, rounded down to its step");

    let update = |percent: u32| {
        block_on(client.call_method(
            Some("org.kde.JobViewServer"),
            VIEW,
            Some("org.kde.JobViewV3"),
            "update",
            &(hints(&[("percent", Variant::from(percent))]),),
        ))
        .unwrap();
    };
    for percent in [12, 23, 47] {
        update(percent);
    }
    wait_until("card follows", T, || {
        (service.current().items.first()?.percent == 47.).then_some(())
    });
    let (step, stepped_at) = next_signal(&mut signals);
    assert_eq!(step, 40, "one signal for three updates, in whole tens");
    assert!(
        stepped_at.duration_since(started_at) >= Duration::from_millis(2900),
        "at most one step per 3 s: {:?}",
        stepped_at.duration_since(started_at)
    );
    assert_eq!(progress_now(), 40, "a late notch can ask for the current step");

    // Show in folder: the path comes from the job (the file does not exist yet: its folder).
    service.reveal(VIEW).unwrap();
    assert_eq!(
        opener.0.lock().unwrap().last().unwrap(),
        &vec![OsString::from("xdg-open"), dest.clone().into_os_string()]
    );
    assert!(service.reveal("/not/a/job").is_err());

    block_on(client.call_method(
        Some("org.kde.JobViewServer"),
        VIEW,
        Some("org.kde.JobViewV3"),
        "terminate",
        &(0u32, "", hints(&[])),
    ))
    .unwrap();
    wait_until("card cleared", T, || {
        service.current().items.is_empty().then_some(())
    });
    let (end, ended_at) = next_signal(&mut signals);
    assert_eq!(end, -1, "the end shows at once");
    assert!(ended_at.duration_since(stepped_at) < Duration::from_millis(2900));
    assert_eq!(
        updates.load(Ordering::SeqCst),
        3,
        "the real server got every update: the monitor only watches"
    );

    // A client that dies mid-copy leaves no stuck row.
    let crashing = block_on(zbus::connection::Builder::address(address.as_str()).build())
        .unwrap();
    block_on(crashing.call_method(
        Some("org.kde.JobViewServer"),
        "/JobViewServer",
        Some("org.kde.JobViewServerV2"),
        "requestView",
        &("org.kde.dolphin", 3i32, hints(&[])),
    ))
    .unwrap();
    wait_until("second job", T, || {
        (!service.current().items.is_empty()).then_some(())
    });
    drop(crashing);
    wait_until("crashed job removed", T, || {
        service.current().items.is_empty().then_some(())
    });

    // The off switch drops the monitor and clears everything.
    service.configure(false);
    wait_until("disabled", T, || {
        (service.current().detail == "disabled" && !service.current().available).then_some(())
    });
    service.shutdown();
    drop(companion);
    drop(server);
}
