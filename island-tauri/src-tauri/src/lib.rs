//! Hermes Island on Tauri v2 — builder wiring only (SPEC §1.1). Logic lives in the modules.

pub mod bus;
pub mod cli;
pub mod cli_notify;
pub mod commands;
pub mod contract;
pub mod error;
pub mod identity;
pub mod integrations;
pub mod keyseq;
pub mod paths;
pub mod services;
pub mod singleton;
pub mod state;
#[cfg(target_os = "linux")]
mod status_temps;
pub mod testhooks;
pub mod tray;
pub mod ui_assets;
pub mod window;

use anyhow::Context;
use std::process::ExitCode;

/// The typed IPC surface. Keep `collect_commands!` and `build.rs::COMMANDS` in sync
/// (`command_lists_match` fails otherwise).
pub fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        // SPEC §8 Phase 2 owns these wire shapes even before their service commands
        // are implemented. Explicit roots keep otherwise-unreferenced types exported.
        .typ::<contract::island::UiIntent>()
        .typ::<contract::clipboard::ClipSummary>()
        .typ::<contract::clipboard::ClipContent>()
        .typ::<contract::system::DisplayInfo>()
        .commands(tauri_specta::collect_commands![
            commands::workspace::launcher_get,
            commands::workspace::launcher_command,
            commands::workspace::layout_get,
            commands::workspace::layout_command,
            commands::workspace::layout_geometry,
            commands::frontend::visual_get,
            commands::frontend::appearance_set,
            commands::island::island_bootstrap,
            commands::island::island_dispatch,
            commands::island::island_report_geometry,
            commands::settings::settings_get,
            commands::settings::settings_patch,
            commands::settings::palette_get,
            commands::settings::hermes_theme_sync,
            commands::settings::notch_scheme_set,
            commands::settings::settings_defaults,
            commands::settings::display_outputs,
            commands::fonts::font_families,
            commands::terminal::terminal_attach,
            commands::terminal::terminal_input,
            commands::terminal::terminal_copy,
            commands::terminal::terminal_resize,
            commands::terminal::terminal_restart,
            commands::terminal::terminal_ack,
            commands::system::system_stats,
            commands::ai::ai_usage,
            commands::ai::ai_connect,
            commands::voice::voice_command,
            commands::voice::voice_panel,
            commands::notifications::notifications_list,
            commands::notifications::notifications_dismiss,
            commands::notifications::notifications_clear,
            integrations::notifications::popup_commands::notification_popup,
            commands::clipboard::clipboard_list,
            commands::clipboard::clipboard_get,
            commands::clipboard::clipboard_copy,
            commands::clipboard::clipboard_pin,
            commands::clipboard::clipboard_delete,
            commands::clipboard::clipboard_clear,
            commands::clipboard::clipboard_set_paused,
            commands::system::weather_refresh,
            commands::system::weather_geocode,
            commands::media::media_control,
            commands::media::media_set_visible,
            commands::apps::timer_get,
            commands::apps::timer_command,
            commands::apps::scratchpad_get,
            commands::apps::scratchpad_command,
            commands::apps::downloads_get,
            commands::apps::downloads_open,
            commands::apps::calendar_get,
            commands::apps::calendar_command,
            commands::apps::calendar_refresh,
            commands::apps::calendar_set_subscription,
            commands::controls::brightness_get,
            commands::controls::brightness_command,
            commands::controls::agents_get,
            commands::controls::mixer_get,
            commands::controls::mixer_command,
            commands::controls::toggles_get,
            commands::controls::toggles_command,
            commands::controls::updates_get,
            commands::controls::updates_refresh,
            commands::controls::tiles_get,
            commands::controls::tiles_command,
            commands::probe::test_probe_reply,
        ])
        .events(tauri_specta::collect_events![
            contract::workspace::LauncherChanged,
            contract::workspace::LayoutChanged,
            contract::frontend::VisualChanged,
            contract::events::SettingsChanged,
            contract::events::PaletteChanged,
            contract::events::AnchorSignal,
            contract::events::IslandStateChanged,
            contract::events::IslandCue,
            contract::notify::NotificationReceived,
            contract::notify::NotificationClosed,
            contract::notify::NotificationSnapshot,
            integrations::notifications::popups::NotificationPopups,
            contract::clipboard::ClipboardAdded,
            contract::clipboard::ClipboardStatusChanged,
            contract::events::PowerChanged,
            contract::events::WeatherChanged,
            contract::system::BluetoothChanged,
            contract::system::MediaChanged,
            contract::system::LyricsChanged,
            contract::system::EqualizerChanged,
            contract::system::ServiceStatus,
            contract::apps::TimerChanged,
            contract::apps::ScratchpadChanged,
            contract::apps::DownloadsChanged,
            contract::apps::CalendarChanged,
            contract::controls::BrightnessChanged,
            contract::controls::AgentsChanged,
            contract::controls::MixerChanged,
            contract::controls::TogglesChanged,
            contract::controls::UpdatesChanged,
            contract::tiles::TilesChanged,
        ])
        // Revised after Phase 1: one TS type per Rust type (no `_Serialize/_Deserialize`) and
        // `f64` is `number`. Non-finite floats cannot cross the JSON IPC (serde_json writes
        // them as `null`, which Rust then rejects for an `f64`), so `number` is the honest type.
        .disable_serde_phases()
        .semantic_types(
            specta_typescript::semantic::Configuration::empty().enable_lossless_floats(),
        )
}

/// Generated TypeScript bindings header (never hand-edit the output).
pub fn typescript_exporter() -> specta_typescript::Typescript {
    specta_typescript::Typescript::default()
        .header("// Generated by tauri-specta from src-tauri/src/contract. Do not edit.\n/* eslint-disable */")
}

/// Linux identity (SPEC §2.3 step 1): GDK must run on X11 (XWayland) for ShapeInput, and the
/// program name / application name are part of the frozen identity contract.
#[cfg(target_os = "linux")]
fn prepare_identity() -> anyhow::Result<()> {
    // Unset stays X11 (every test and gate assumes it). `run.sh` exports the user's
    // `displayBackend` choice; `wayland` selects the native layer-shell path (Nov 2c).
    match std::env::var("GDK_BACKEND") {
        Err(_) => std::env::set_var("GDK_BACKEND", "x11"),
        Ok(value) if matches!(value.split(',').next(), Some("x11" | "wayland")) => {}
        Ok(value) => anyhow::bail!(
            "GDK_BACKEND={value}: the island supports x11 (XWayland) or wayland; \
             unset it or pick one of those"
        ),
    }
    glib::set_prgname(Some(identity::PRGNAME));
    glib::set_application_name(identity::APPLICATION_NAME);
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn prepare_identity() -> anyhow::Result<()> {
    Ok(())
}

fn run_app(cli: cli::Cli, force_embedded_ui: bool) -> anyhow::Result<()> {
    use std::sync::Arc;
    use tauri::Manager;
    let repo_root = paths::repo_root();
    let paths = paths::resolve(&cli, &repo_root)?;
    // Ownership precedes GTK, services, test sockets and status publication.
    let instance = singleton::acquire(&paths.data_dir)?;
    if cli.overlay && !cli.test_instance {
        if let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) {
            let state = std::env::var_os("XDG_STATE_HOME").map(std::path::PathBuf::from);
            let link = paths::anchor_client_link(state.as_deref(), &home);
            if let Err(e) = paths::publish_anchor_client(&link, &repo_root) {
                tracing::warn!(error = %e, "Could not publish the notch click client link");
            }
        }
    }
    let anchor = if cli.overlay
        && matches!(
            cli.ui,
            contract::island::UiKind::Legacy | contract::island::UiKind::Island
        ) {
        match tauri::async_runtime::block_on(integrations::plasma::anchor::discover()) {
            Ok(anchor) => Some(anchor),
            Err(error) => {
                tracing::info!(%error, "No Plasma anchor; retaining generic below-work-area placement");
                None
            }
        }
    } else {
        None
    };
    prepare_identity()?;
    let hooks_on = testhooks::hooks_enabled(&cli, std::env::var(cli::TEST_ENV).ok().as_deref());
    let config =
        services::ServiceConfig::from_cli(&cli, repo_root, paths.data_dir.clone(), hooks_on);
    let specta = specta_builder();
    let app = tauri::Builder::default()
        .invoke_handler(specta.invoke_handler())
        .setup(move |app| {
            specta.mount_events(app);
            tracing::info!(data_dir = %paths.data_dir.display(), overlay = cli.overlay, ui = ?cli.ui, test_hooks = hooks_on, "starting island");
            app.manage(paths.clone());
            // Services first: the page may call `island_bootstrap` the moment it loads.
            let mut service = services::Services::new(&config);
            service.runtime.panel_anchor = anchor.as_ref().is_some_and(|a| a.native);
            app.manage(service);
            app.manage(services::frontend::VisualService::new(paths.data_dir.clone())?);
            app.manage(Arc::new(services::workspace::WorkspaceService::new(
                paths.data_dir.clone(),
                app.state::<services::Services>().settings.get()?.module_registry,
            )?));
            app.manage(integrations::plasma::anchor::PlasmaAnchor(anchor.clone()));
            let services = app.state::<services::Services>();
            services.start_event_forwarders(app.handle());
            // The state actor is the single IslandState authority (SPEC §8 Phase 3). It must
            // exist before the window: the page calls `island_bootstrap` as soon as it loads.
            app.manage(state::host::IslandActor::start(
                app.handle().clone(),
                services.settings.get()?,
            ));
            let clipboard = integrations::clipboard::Clipboard::new(paths.data_dir.clone(), services.settings.get()?)?;
            clipboard.start_events(app.handle().clone())?;
            app.manage(clipboard);
            app.manage(integrations::notifications::hub::Notifications::start(
                app.handle().clone(),
                app.state::<state::host::IslandActor>().handle.clone(),
                services.settings.clone(),
                &paths.data_dir,
            )?);
            let window = window::create_island_window(app.handle(), &cli)?;
            // Phase 6a power/weather/Bluetooth. Battery alerts feed the notification hub, and a
            // test instance never reaches the real system bus (see `system_bus_address`).
            let hub = app
                .state::<integrations::notifications::hub::Notifications>()
                .hub
                .clone();
            app.manage(Arc::new(
                integrations::system_services::SystemServices::start(
                    app.handle(),
                    &services.settings.get()?,
                    cli.test_instance,
                    hub,
                    &paths.data_dir,
                )?,
            ));
            if matches!(cli.ui, contract::island::UiKind::Legacy | contract::island::UiKind::Island) {
                app.manage(tray::Tray::start(app.handle(), app.state::<state::host::IslandActor>().handle.clone())?);
            }
            if cli.overlay && services.settings.get()?.show_in_taskbar {
                window
                    .set_skip_taskbar(false)
                    .map_err(|e| error::IslandError::internal(format!("taskbar update failed: {e}")))?;
            }
            services.start_palette_watcher(services::profile_dir(
                std::env::var_os("HERMES_HOME"),
                &config.home,
            ));
            let host = Arc::new(testhooks::AppProbeHost {
                app: app.handle().clone(),
                terminal: Arc::clone(&services.terminal),
            });
            let hooks = testhooks::TestHooks::maybe_start(hooks_on, &paths.data_dir, host)?;
            *services
                .test_hooks
                .lock()
                .map_err(|_| error::IslandError::internal("test hooks slot is poisoned"))? =
                hooks.map(Arc::new);
            app.manage(integrations::plasma::runtime::NativeHost::start(app.handle(), instance, anchor)?);
            // Game mode fullscreen detection + the global toggle shortcut (product island only).
            integrations::plasma::gamemode::start_for_app(app.handle());
            for signal in [libc::SIGTERM, libc::SIGINT] {
                let handle = app.handle().clone();
                glib::source::unix_signal_add_local(signal, move || {
                    handle.exit(0);
                    glib::ControlFlow::Continue
                });
            }
            Ok(())
        })
        .build({
            let mut context = tauri::generate_context!();
            let env = std::env::var(ui_assets::FORCE_EMBEDDED_ENV).ok();
            ui_assets::install(
                &mut context,
                &paths::repo_root(),
                ui_assets::forced_reason(force_embedded_ui, env.as_deref()),
            );
            context
        })
        .context("tauri runtime failed")?;
    app.run(|handle, event| {
        // Kill the PTY's owned process tree, stop watchers and remove the test socket.
        if let tauri::RunEvent::Exit = event {
            if let Some(clipboard) = tauri::Manager::try_state::<
                std::sync::Arc<integrations::clipboard::Clipboard>,
            >(handle)
            {
                if let Err(error) = clipboard.shutdown() {
                    tracing::error!(%error, "Clipboard shutdown failed");
                }
            }
            if let Some(system) = tauri::Manager::try_state::<
                std::sync::Arc<integrations::system_services::SystemServices>,
            >(handle)
            {
                system.shutdown();
            }
            if let Some(tray) = tauri::Manager::try_state::<tray::Tray>(handle) {
                if let Err(error) = tray.shutdown() {
                    tracing::error!(%error, "Tray shutdown failed");
                }
            }
            if let Some(notifications) =
                tauri::Manager::try_state::<integrations::notifications::hub::Notifications>(handle)
            {
                if let Err(error) = notifications.shutdown() {
                    tracing::error!(%error, "Notification shutdown failed");
                }
            }
            integrations::plasma::gamemode::shutdown_for_app(handle);
            // Stop the authority first so no new effect reaches a half-torn-down host.
            if let Some(actor) = tauri::Manager::try_state::<state::host::IslandActor>(handle) {
                if let Err(error) = actor.shutdown() {
                    tracing::error!(%error, "State actor shutdown failed");
                }
            }
            if let Some(host) =
                tauri::Manager::try_state::<integrations::plasma::runtime::NativeHost>(handle)
            {
                if let Err(error) = host.shutdown() {
                    tracing::error!(%error, "Native ownership cleanup failed");
                }
            }
            if let Some(services) = tauri::Manager::try_state::<services::Services>(handle) {
                services.shutdown();
            }
        }
    });
    Ok(())
}

pub fn run() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // `scripts/deploy-ui.sh` asks a binary which UI contract it was built with.
    if args == [ui_assets::PRINT_CONTRACT_FLAG] {
        println!("{}", ui_assets::contract_hash());
        return ExitCode::SUCCESS;
    }
    let force_embedded_ui = ui_assets::take_force_flag(&mut args);
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();
    if let Some(request) = cli_notify::parse(&args) {
        return match request.and_then(cli_notify::send) {
            Ok(reply) => {
                println!("{reply}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("hermes-island notify: {error}");
                ExitCode::FAILURE
            }
        };
    }
    let cli = match cli::Cli::parse_from(args) {
        Ok(cli) => cli,
        Err(error) => {
            eprintln!("hermes-island: {error}");
            return ExitCode::from(2);
        }
    };
    match run_app(cli, force_embedded_ui) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("hermes-island: {error:#}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn bindings_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../ui/ipc/bindings.ts")
    }

    fn scratch_file(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("hi-tauri-{}-{name}", std::process::id()))
    }

    /// `cargo test export_bindings -- --ignored` regenerates `ui/ipc/bindings.ts`.
    #[test]
    #[ignore = "writes ui/ipc/bindings.ts; run explicitly to regenerate"]
    fn export_bindings() {
        specta_builder()
            .export(typescript_exporter(), bindings_path())
            .unwrap();
    }

    #[test]
    fn companion_bindings_only_expose_canonical_choices() {
        let generated = std::fs::read_to_string(bindings_path()).unwrap();
        let companion = generated
            .split("export type CompanionCharacter =")
            .nth(1)
            .unwrap()
            .split(';')
            .next()
            .unwrap();
        for id in [
            "blob",
            "isaac",
            "notchmimic",
            "companioncube",
            "nurgling",
            "hollowknight",
            "frog",
        ] {
            assert!(companion.contains(&format!("\"{id}\"")));
        }
        assert!(
            !companion.contains("obelisk"),
            "legacy input must not leak into supported public choices"
        );
    }

    #[test]
    fn bindings_are_current() {
        let fresh = scratch_file("bindings.ts");
        specta_builder()
            .export(typescript_exporter(), &fresh)
            .unwrap();
        let generated = std::fs::read_to_string(&fresh).unwrap();
        std::fs::remove_file(&fresh).unwrap();
        let committed = std::fs::read_to_string(bindings_path())
            .expect("ui/ipc/bindings.ts missing: run `cargo test export_bindings -- --ignored`");
        assert_eq!(
            committed, generated,
            "ui/ipc/bindings.ts is stale: run `cargo test export_bindings -- --ignored`"
        );
    }

    /// Revised after Phase 1: the generator config must not regress to split or nullable types.
    #[test]
    fn generated_types_are_clean() {
        let generated = std::fs::read_to_string(bindings_path()).unwrap();
        for bad in ["_Serialize", "_Deserialize"] {
            assert!(
                !generated.contains(bad),
                "bindings.ts contains `{bad}` (specta config regressed)"
            );
        }
        // SPEC §5.1 / §8 Phase 2: scope exceptions to TYPE + FIELD, not lines (enums can
        // contain several variants on one line). Plain numeric fields must stay non-null.
        let allowed = [
            // SPEC §5.2: unavailable SystemStats metrics preserve the salvaged JSON nulls.
            ("SystemStats", "cpu"),
            ("SystemStats", "memPct"),
            ("SystemStats", "memUsed"),
            ("SystemStats", "memTotal"),
            ("SystemStats", "uptime"),
            ("SystemStats", "cpuCount"),
            // SPEC §5.2 TerminalSnapshot pid and TerminalEvent::Exit code.
            ("TerminalSnapshot", "pid"),
            ("TerminalEvent", "code"),
            // SPEC §3.5 / §5.2: unavailable battery telemetry and duration estimates.
            ("PowerState", "percent"),
            ("PowerState", "timeToEmpty"),
            ("PowerState", "timeToFull"),
            // SPEC §5.2 WeatherState tempC/code, BtDevice batteryPercent.
            ("WeatherState", "tempC"),
            ("WeatherState", "code"),
            // SPEC §3.5 / Phase 6a: weather value in the user's unit and the success time are
            // `null` until a fetch succeeded, never 0.
            ("WeatherState", "temp"),
            ("WeatherState", "updatedAt"),
            ("BtDevice", "batteryPercent"),
            // SPEC §3.5 / §5.2: optional player timeline, not fabricated zero.
            ("NowPlaying", "positionMs"),
            ("NowPlaying", "durationMs"),
            ("NowPlaying", "volumePercent"),
            ("NowPlaying", "pid"),
            // Phase 6m: the active synced line is absent before the first line / unknown position.
            ("LyricsChanged", "activeIndex"),
            // SPEC §0.5 Calendar: no fabricated last success or age on a cold failure.
            ("CalendarChanged", "ageMs"),
            ("CalendarChanged", "updatedAt"),
            // SPEC §8 Phase 6b2: unsupported/unavailable display brightness is null.
            ("BrightnessDisplay", "percent"),
            // Phase 6b2 agent metadata: unknown API call count is null, never zero.
            ("AgentChip", "apiCallCount"),
            // SPEC Phase 6b2: unknown channels/group routing and process metadata stay null.
            ("MixerSink", "volumePercent"),
            ("MixerStream", "volumePercent"),
            ("MixerStream", "pid"),
            ("MixerApp", "volumePercent"),
            ("MixerApp", "sinkIndex"),
            // SPEC §0.5 update badge: unknown counts/age are never fabricated zeros.
            ("UpdatesChanged", "count"),
            ("UpdatesChanged", "repoCount"),
            ("UpdatesChanged", "aurCount"),
            ("UpdatesChanged", "ageSecs"),
            // Untimed keep-awake and unavailable Tailscale observations are explicitly null.
            ("KeepAwakeTile", "remainingMs"),
            ("TailscaleTile", "onlineCount"),
            ("TilesOp", "durationMs"),
            // SPEC §5.2: notification progress is absent for non-progress notifications.
            ("IslandNotification", "progress"),
            // Critical notification cards deliberately have no expiry deadline.
            ("Popup", "remainingMs"),
        ];
        for declaration in generated.split("export type ").skip(1) {
            let (name, body) = declaration.split_once(" = ").unwrap();
            for (index, _) in body.match_indices("number | null") {
                let prefix = &body[..index];
                let (before_colon, between) = prefix.rsplit_once(':').expect("nullable property");
                assert!(
                    between.trim().is_empty(),
                    "unrecognized nullable shape in {name}"
                );
                let field = before_colon
                    .trim_end()
                    .trim_end_matches('?')
                    .rsplit(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                    .next()
                    .unwrap();
                assert!(
                    allowed.contains(&(name, field)),
                    "unexpected nullable numeric field {name}.{field}"
                );
            }
        }
        assert!(
            !generated.contains("bigint"),
            "64-bit integers must not cross the IPC"
        );
        assert!(generated.contains("scale: number,"), "f64 must be `number`");
        assert!(generated.contains("export type IslandMode = \"still\" | \"hover\" | \"large\";"));
    }

    /// Every command must appear in all four places: `collect_commands!` (via the generated
    /// bindings), `build.rs::COMMANDS`, and `permissions/island.toml` (+ the capability set).
    #[test]
    fn command_lists_match() {
        use std::collections::BTreeSet;
        // build.rs cannot depend on this crate, so its literal list is parsed and compared with
        // what tauri-specta actually generated.
        let build_rs = include_str!("../build.rs");
        let list = build_rs
            .split("COMMANDS: &[&str] = &[")
            .nth(1)
            .and_then(|s| s.split("];").next())
            .unwrap();
        let from_build: BTreeSet<String> = list
            .split('"')
            .skip(1)
            .step_by(2)
            .map(str::to_owned)
            .collect();
        assert!(from_build.len() >= 15, "{from_build:?}");
        let generated = std::fs::read_to_string(bindings_path()).unwrap();
        let from_bindings: BTreeSet<String> = generated
            .lines()
            .filter_map(|line| {
                let rest = line.split("__TAURI_INVOKE(\"").nth(1)?;
                Some(rest.split('"').next()?.to_owned())
            })
            .collect();
        assert_eq!(
            from_build, from_bindings,
            "build.rs::COMMANDS and the generated commands differ"
        );
        let permissions = include_str!("../permissions/island.toml");
        let granted: BTreeSet<String> = permissions
            .split('"')
            .filter_map(|s| s.strip_prefix("allow-"))
            .map(|s| s.replace('-', "_"))
            .collect();
        assert_eq!(
            from_build, granted,
            "permissions/island.toml island-commands set differs from build.rs::COMMANDS"
        );
        let capability = include_str!("../capabilities/island.json");
        assert!(capability.contains("island-commands"));
    }

    #[test]
    fn events_are_registered_in_the_bindings() {
        let generated = std::fs::read_to_string(bindings_path()).unwrap();
        for event in ["settingsChanged", "paletteChanged", "anchorSignal"] {
            assert!(
                generated.contains(event),
                "{event} missing from bindings.ts"
            );
        }
    }
}
