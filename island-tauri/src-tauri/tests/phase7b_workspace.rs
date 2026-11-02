#![allow(clippy::unwrap_used)]
use hermes_island_lib::{
    commands::workspace::serialized_settings,
    contract::{
        island::IslandView,
        modules::{
            ModuleConfig, ModuleEntry, ModuleId, ModuleJsonValue, Registry, CURRENT_VERSION,
        },
        settings::{ObjectOnly, SettingsPatch},
        workspace::*,
    },
    error::ErrorCode,
    services::{
        autostart::Autostart,
        settings::SettingsService,
        workspace::{arrange_core::GridItem, LockedArrangement, WorkspaceService},
    },
};
use serde_json::json;
use std::{collections::BTreeMap, fs, path::PathBuf, sync::Arc};

struct Scratch(PathBuf);
impl Scratch {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("hi-7b-{tag}-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
    fn settings(&self) -> SettingsService {
        SettingsService::new(
            self.0.clone(),
            Autostart::with_default_launcher(self.0.join("home"), self.0.join("repo"), true),
        )
    }
    fn workspace(&self, registry: Registry) -> WorkspaceService {
        WorkspaceService::new(self.0.clone(), registry).unwrap()
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn item(id: &str, enabled: bool, locked: bool) -> GridItem {
    GridItem {
        id: id.into(),
        enabled,
        locked,
    }
}
fn id(id: &str) -> ModuleId {
    ModuleId::new(id).unwrap()
}
fn begin(workspace: &WorkspaceService, registry: &Registry) {
    assert!(
        workspace
            .launcher_command(registry, &LauncherOp::Begin {})
            .unwrap()
            .editing
    );
}

#[test]
fn locked_direct_and_indirect_moves_disables_enables_and_commit_are_atomic() {
    let input = vec![
        item("terminal", true, true),
        item("a", true, false),
        item("settings", true, true),
        item("hidden", false, false),
        item("b", true, false),
        item("c", true, false),
    ];
    let mut draft = LockedArrangement::new(input.clone()).unwrap();
    draft.apply(&LauncherOp::Begin {}).unwrap();
    for op in [
        LauncherOp::Move {
            id: "terminal".into(),
            to: 0,
        },
        LauncherOp::Move {
            id: "settings".into(),
            to: 2,
        },
        LauncherOp::Move {
            id: "b".into(),
            to: 0,
        },
        LauncherOp::Disable {
            id: "terminal".into(),
        },
        LauncherOp::Disable {
            id: "settings".into(),
        },
    ] {
        assert!(draft.apply(&op).is_err(), "{op:?}");
        assert_eq!(draft.items(), input);
        assert!(draft.editing());
    }
    // Settings floats (Nick, Nov 2): hiding a tile in front of it or enabling a hidden one
    // shifts Settings' index and is legal; Settings itself can still never be hidden or dragged.
    let hidden_before = vec![
        item("terminal", true, true),
        item("hidden", false, false),
        item("settings", true, true),
        item("a", true, false),
    ];
    let mut hidden_draft = LockedArrangement::new(hidden_before.clone()).unwrap();
    hidden_draft.apply(&LauncherOp::Begin {}).unwrap();
    hidden_draft
        .apply(&LauncherOp::Enable {
            id: "hidden".into(),
        })
        .unwrap();
    hidden_draft
        .apply(&LauncherOp::Disable { id: "a".into() })
        .unwrap();
    assert!(hidden_draft
        .apply(&LauncherOp::Commit {})
        .unwrap()
        .is_some());
    let mut floating = LockedArrangement::new(input.clone()).unwrap();
    floating.apply(&LauncherOp::Begin {}).unwrap();
    floating
        .apply(&LauncherOp::Move {
            id: "c".into(),
            to: 1,
        })
        .unwrap();
    floating
        .apply(&LauncherOp::Disable { id: "a".into() })
        .unwrap();
    let order = floating
        .items()
        .iter()
        .map(|i| i.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(order[0], "terminal");
    assert!(floating
        .items()
        .iter()
        .any(|i| i.id == "settings" && i.enabled));
    floating.apply(&LauncherOp::Commit {}).unwrap();
    // Legal rearrangement wholly after the last lock; oversized index clamps in the pure core.
    draft
        .apply(&LauncherOp::Move {
            id: "b".into(),
            to: u32::MAX,
        })
        .unwrap();
    assert_eq!(
        draft
            .items()
            .iter()
            .map(|i| i.id.as_str())
            .collect::<Vec<_>>(),
        ["terminal", "a", "settings", "hidden", "c", "b"]
    );
    let commit = draft.apply(&LauncherOp::Commit {}).unwrap().unwrap();
    assert_eq!(
        commit.order,
        ["terminal", "a", "settings", "hidden", "c", "b"]
    );
    assert!(!draft.editing());
}

#[test]
fn full_hidden_permutation_clamp_and_utf8_64_65_byte_boundaries() {
    for good in ["a".repeat(64), "é".repeat(32)] {
        assert!(LockedArrangement::new(vec![item(&good, true, false)]).is_ok());
    }
    for bad in [
        String::new(),
        "a".repeat(65),
        format!("{}a", "é".repeat(32)),
    ] {
        assert!(LockedArrangement::new(vec![item(&bad, true, false)]).is_err());
    }
    let mut draft = LockedArrangement::new(vec![
        item("terminal", true, true),
        item("settings", true, true),
        item("a", true, false),
        item("hidden", false, false),
        item("b", true, false),
    ])
    .unwrap();
    draft.apply(&LauncherOp::Begin {}).unwrap();
    draft
        .apply(&LauncherOp::Move {
            id: "a".into(),
            to: u32::MAX,
        })
        .unwrap();
    let commit = draft.apply(&LauncherOp::Commit {}).unwrap().unwrap();
    assert_eq!(commit.order, ["terminal", "settings", "hidden", "b", "a"]);
    assert!(commit.enabled_changes.is_empty());
}

#[test]
fn last_visible_move_clamp_preserves_trailing_hidden_items_and_is_a_noop() {
    let mut draft = LockedArrangement::new(vec![
        item("terminal", true, true),
        item("settings", true, true),
        item("a", true, false),
        item("b", true, false),
        item("trailing-hidden", false, false),
    ])
    .unwrap();
    draft.apply(&LauncherOp::Begin {}).unwrap();
    draft
        .apply(&LauncherOp::Move {
            id: "b".into(),
            to: u32::MAX,
        })
        .unwrap();
    assert!(draft.apply(&LauncherOp::Commit {}).unwrap().is_none());
    assert_eq!(
        draft
            .items()
            .iter()
            .map(|i| i.id.as_str())
            .collect::<Vec<_>>(),
        ["terminal", "settings", "a", "b", "trailing-hidden"]
    );
}
#[test]
fn launcher_cancel_and_unchanged_commit_never_persist() {
    let dir = Scratch::new("cancel");
    let settings = dir.settings();
    let registry = settings.get().unwrap().module_registry;
    let workspace = dir.workspace(registry.clone());
    let original = workspace.launcher_get(&registry).unwrap();
    begin(&workspace, &registry);
    workspace
        .launcher_command(
            &registry,
            &LauncherOp::Move {
                id: "timer".into(),
                to: u32::MAX,
            },
        )
        .unwrap();
    workspace
        .launcher_command(
            &registry,
            &LauncherOp::Disable {
                id: "downloads".into(),
            },
        )
        .unwrap();
    let cancelled = workspace
        .launcher_command(&registry, &LauncherOp::Cancel {})
        .unwrap();
    assert_eq!(cancelled.items, original.items);
    assert!(!cancelled.editing);
    assert!(!dir.0.join("settings.json").exists());
    begin(&workspace, &registry);
    assert!(workspace.prepare_commit(&registry).unwrap().is_none());
    assert!(!dir.0.join("settings.json").exists());
    assert!(!dir.0.join("layout.json").exists());
}

#[test]
fn launcher_commit_uses_one_existing_settings_patch_and_preserves_future_configs() {
    let dir = Scratch::new("commit");
    let settings = dir.settings();
    let mut modules = Registry::defaults().modules().to_vec();
    let unknown = ModuleEntry {
        id: id("future-app"),
        enabled: true,
        order: 99,
        config: ModuleConfig::Unknown(ModuleJsonValue::Object(BTreeMap::from([(
            "nested".into(),
            ModuleJsonValue::String("keep exactly".into()),
        )]))),
    };
    modules.push(unknown.clone());
    let registry = Registry::new(CURRENT_VERSION, modules).unwrap();
    settings
        .patch(&SettingsPatch {
            module_registry: Some(registry.clone()),
            ..Default::default()
        })
        .unwrap();
    let workspace = dir.workspace(registry.clone());
    let snapshot = workspace.launcher_get(&registry).unwrap();
    let future = snapshot
        .items
        .iter()
        .find(|i| i.id == "future-app")
        .unwrap();
    assert!(!future.enabled && !future.known && future.view.is_none());
    begin(&workspace, &registry);
    for op in [
        LauncherOp::Enable {
            id: "future-app".into(),
        },
        LauncherOp::Disable {
            id: "future-app".into(),
        },
        LauncherOp::Move {
            id: "future-app".into(),
            to: 1,
        },
    ] {
        assert!(workspace.launcher_command(&registry, &op).is_err());
    }
    workspace
        .launcher_command(
            &registry,
            &LauncherOp::Move {
                id: "timer".into(),
                to: u32::MAX,
            },
        )
        .unwrap();
    workspace
        .launcher_command(
            &registry,
            &LauncherOp::Disable {
                id: "downloads".into(),
            },
        )
        .unwrap();
    let draft = workspace.launcher_get(&registry).unwrap();
    let patch = workspace.prepare_commit(&registry).unwrap().unwrap();
    // Existing atomic service, not a second launcher-specific persistence path.
    let saved = settings.patch(&patch).unwrap();
    let result = workspace.committed(&saved.module_registry).unwrap();
    assert_eq!(result.items, draft.items);
    assert!(!result.editing);
    assert_eq!(
        saved.module_registry.get(&id("future-app")).unwrap().config,
        unknown.config
    );
    assert!(
        saved
            .module_registry
            .get(&id("future-app"))
            .unwrap()
            .enabled
    );
    assert!(!saved.module_registry.get(&id("downloads")).unwrap().enabled);
    for original in registry.modules() {
        assert_eq!(
            saved.module_registry.get(&original.id).unwrap().config,
            original.config
        );
    }
    assert_eq!(dir.settings().get().unwrap(), saved);
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        fs::metadata(dir.0.join("settings.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}

#[test]
fn external_registry_config_order_and_enable_changes_invalidate_draft() {
    for kind in ["config", "order", "enable"] {
        let dir = Scratch::new(kind);
        let registry = Registry::defaults();
        let workspace = dir.workspace(registry.clone());
        begin(&workspace, &registry);
        workspace
            .launcher_command(
                &registry,
                &LauncherOp::Move {
                    id: "timer".into(),
                    to: u32::MAX,
                },
            )
            .unwrap();
        let mut external = registry.clone();
        match kind {
            "config" => external
                .set_config(&id("terminal"), ModuleConfig::Terminal { font_size: 15.0 })
                .unwrap(),
            "enable" => external.set_enabled(&id("music"), true).unwrap(),
            _ => {
                let mut order = registry
                    .modules()
                    .iter()
                    .map(|m| m.id.clone())
                    .collect::<Vec<_>>();
                order.swap(1, 2);
                external.reorder(&order).unwrap();
            }
        }
        // Direct commit detects conflict, preserves the external registry, discards stale draft.
        assert_eq!(
            workspace.prepare_commit(&external).unwrap_err().code,
            ErrorCode::Busy
        );
        let new = workspace.launcher_get(&external).unwrap();
        assert!(!new.editing);
        assert!(workspace.prepare_commit(&external).is_err());
        // Reconciliation on get is authoritative, including externally changed enabled flags.
        begin(&workspace, &external);
        let new = workspace.launcher_get(&registry).unwrap();
        assert!(!new.editing);
        assert_eq!(
            new.items,
            dir.workspace(registry.clone())
                .launcher_get(&registry)
                .unwrap()
                .items
        );
    }
}

#[test]
fn layout_private_atomic_reload_defaults_unchanged_sizing_and_all_edit_operations() {
    let dir = Scratch::new("layout");
    let workspace = dir.workspace(Registry::defaults());
    let initial = workspace.layout_get().unwrap();
    // Nov 2 (Nick): a fresh install starts on the Bigger preset, not the old per-app sizes.
    assert!(!initial.unchanged_sizing);
    assert_eq!(initial.layout.preset, LayoutPreset::Bigger {});
    assert_eq!(initial.layout.buttons.len(), 5);
    workspace
        .layout_command(LayoutOp::Gap { gap: 12.0 })
        .unwrap();
    assert!(
        !dir.workspace(Registry::defaults())
            .layout_get()
            .unwrap()
            .unchanged_sizing
    );
    workspace
        .layout_command(LayoutOp::Preset {
            preset: LayoutPreset::Custom {
                width: 800.0,
                max_height: 620.0,
            },
        })
        .unwrap();
    workspace
        .layout_command(LayoutOp::Diameter { diameter: 44.0 })
        .unwrap();
    let long = "a".repeat(64);
    workspace
        .layout_command(LayoutOp::Add {
            button: ObjectOnly(LayoutButton {
                id: long.clone(),
                side: LayoutSide::Left,
                slot: 2,
                action: LayoutAction::OpenApp {
                    view: IslandView::Calendar,
                },
                label: Some("Calendar".into()),
            }),
        })
        .unwrap();
    workspace
        .layout_command(LayoutOp::Edit {
            id: long.clone(),
            action: LayoutAction::OpenApp {
                view: IslandView::Controls,
            },
            label: None,
        })
        .unwrap();
    workspace
        .layout_command(LayoutOp::Move {
            id: long.clone(),
            side: LayoutSide::Right,
            slot: 0,
        })
        .unwrap();
    let moved = workspace.layout_get().unwrap();
    assert!(moved
        .layout
        .buttons
        .iter()
        .any(|b| b.id == "settings" && b.side == LayoutSide::Left && b.slot == 2));
    workspace
        .layout_command(LayoutOp::Remove { id: long })
        .unwrap();
    let saved = workspace.layout_get().unwrap();
    let reloaded = dir.workspace(Registry::defaults()).layout_get().unwrap();
    assert_eq!(reloaded.layout, saved.layout);
    assert!(!reloaded.unchanged_sizing);
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        fs::metadata(dir.0.join("layout.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let bytes = fs::read(dir.0.join("layout.json")).unwrap();
    for op in [
        LayoutOp::Gap { gap: f64::NAN },
        LayoutOp::Diameter { diameter: 900.0 },
        LayoutOp::Preset {
            preset: LayoutPreset::Custom {
                width: f64::INFINITY,
                max_height: 500.0,
            },
        },
        LayoutOp::Move {
            id: "apps".into(),
            side: LayoutSide::Left,
            slot: u32::MAX,
        },
        LayoutOp::Add {
            button: ObjectOnly(LayoutButton {
                id: "a".repeat(65),
                side: LayoutSide::Bottom,
                slot: 2,
                action: LayoutAction::AppGrid {},
                label: None,
            }),
        },
    ] {
        assert!(workspace.layout_command(op).is_err());
        assert_eq!(workspace.layout_get().unwrap(), saved);
        assert_eq!(fs::read(dir.0.join("layout.json")).unwrap(), bytes);
    }
    // A genuine rename failure leaves memory/revision unchanged and no owned temp file behind.
    fs::remove_file(dir.0.join("layout.json")).unwrap();
    fs::create_dir(dir.0.join("layout.json")).unwrap();
    assert!(workspace
        .layout_command(LayoutOp::Gap { gap: 14.0 })
        .is_err());
    assert_eq!(workspace.layout_get().unwrap(), saved);
    assert!(!fs::read_dir(&dir.0).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .ends_with(".new")));
}

fn query(include_empty: bool) -> LayoutGeometryQuery {
    LayoutGeometryQuery {
        island: ObjectOnly(LayoutRect {
            x: 100.0,
            y: 25.0,
            width: 300.0,
            height: 20.0,
        }),
        viewport_width: 600.0,
        viewport_height: 400.0,
        include_empty,
    }
}
#[test]
fn short_island_bottom_clears_side_columns_and_miniature_empty_slots_do_not_overlap() {
    let dir = Scratch::new("geometry");
    let workspace = dir.workspace(Registry::defaults());
    for include_empty in [false, true] {
        let geometry = workspace.layout_geometry(query(include_empty)).unwrap();
        let all = geometry
            .buttons
            .iter()
            .chain(geometry.empty_slots.iter())
            .collect::<Vec<_>>();
        // A 300×20 island only offers the slots that fit it (Nick, Nov 2): occupied side slots
        // stay, no extra empty side slots below a 20 px island, and a 6-wide bottom row.
        if !include_empty {
            assert_eq!(all.len(), 5);
        } else {
            let empty = &geometry.empty_slots;
            let column = geometry
                .buttons
                .iter()
                .filter(|b| b.side != LayoutSide::Bottom)
                .map(|b| b.slot + 1)
                .max()
                .unwrap_or(1)
                .max(1);
            assert!(
                empty.iter().all(|e| if e.side == LayoutSide::Bottom {
                    e.slot < 6
                } else {
                    e.slot < column
                }),
                "{empty:?}"
            );
            assert_eq!(
                empty
                    .iter()
                    .filter(|e| e.side == LayoutSide::Bottom)
                    .count()
                    + geometry
                        .buttons
                        .iter()
                        .filter(|b| b.side == LayoutSide::Bottom)
                        .count(),
                6
            );
        }
        let sides_bottom = all
            .iter()
            .filter(|b| b.side != LayoutSide::Bottom)
            .map(|b| b.rect.y + b.rect.height)
            .fold(45.0, f64::max);
        for b in all.iter().filter(|b| b.side == LayoutSide::Bottom) {
            assert_eq!(b.rect.y, sides_bottom + 10.0);
        }
        for (index, a) in all.iter().enumerate() {
            for b in all.iter().skip(index + 1) {
                assert!(
                    a.rect.x + a.rect.width <= b.rect.x
                        || b.rect.x + b.rect.width <= a.rect.x
                        || a.rect.y + a.rect.height <= b.rect.y
                        || b.rect.y + b.rect.height <= a.rect.y,
                    "overlap: {a:?} {b:?}"
                );
            }
        }
    }
    let mut too_wide = query(false);
    too_wide.island.0.x = 0.0;
    assert!(workspace.layout_geometry(too_wide).is_err());
    let mut nan = query(false);
    nan.viewport_width = f64::NAN;
    assert!(workspace.layout_geometry(nan).is_err());
    let mut infinity = query(false);
    infinity.island.0.height = f64::INFINITY;
    assert!(workspace.layout_geometry(infinity).is_err());
}

#[test]
fn wire_inputs_reject_arrays_unknown_keys_arbitrary_actions_and_noncanonical_views() {
    for bad in [
        json!([]),
        json!({"type":"begin","unexpected":true}),
        json!({"type":"move","id":"timer","to":-1}),
        json!({"type":"move","id":"timer","to":4294967296_u64}),
    ] {
        assert!(serde_json::from_value::<ObjectOnly<LauncherOp>>(bad).is_err());
    }
    for bad in [
        json!([]),
        json!({"type":"add","button":[]}),
        json!({"type":"edit","id":"apps","action":{"type":"command","id":"rm -rf"},"label":null}),
        json!({"type":"edit","id":"apps","action":{"type":"openApp","view":"idle"},"label":null}),
        json!({"type":"gap","gap":10,"extra":0}),
    ] {
        assert!(serde_json::from_value::<ObjectOnly<LayoutOp>>(bad).is_err());
    }
    assert!(serde_json::from_value::<ObjectOnly<LayoutGeometryQuery>>(
        json!({"island":[],"viewportWidth":600,"viewportHeight":400,"includeEmpty":true})
    )
    .is_err());
    assert_eq!(
        serde_json::to_value(IslandView::QuickInfo).unwrap(),
        "quickInfo"
    );
    for route in [
        "launcher",
        "music",
        "quickInfo",
        "timer",
        "calendar",
        "scratchpad",
        "downloads",
        "controls",
        "mixer",
        "brightness",
        "agents",
        "updates",
    ] {
        assert!(serde_json::from_value::<IslandView>(json!(route)).is_ok());
    }
}

#[tokio::test]
async fn launcher_settings_transactions_are_serialized_across_awaits() {
    let order = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    let one_order = order.clone();
    let one = tokio::spawn(async move {
        serialized_settings(async move {
            one_order.lock().await.push(1);
            started_tx.send(()).unwrap();
            release_rx.await.unwrap();
            one_order.lock().await.push(2);
        })
        .await
    });
    started_rx.await.unwrap();
    let two_order = order.clone();
    let two = tokio::spawn(async move {
        serialized_settings(async move {
            two_order.lock().await.push(3);
        })
        .await
    });
    tokio::task::yield_now().await;
    assert_eq!(*order.lock().await, [1]);
    release_tx.send(()).unwrap();
    one.await.unwrap();
    two.await.unwrap();
    assert_eq!(*order.lock().await, [1, 2, 3]);
}

#[test]
fn tall_wide_island_offers_five_per_side_and_seven_along_the_bottom() {
    let dir = Scratch::new("capacity");
    let workspace = dir.workspace(Registry::defaults());
    let geometry = workspace
        .layout_geometry(LayoutGeometryQuery {
            island: ObjectOnly(LayoutRect {
                x: 200.0,
                y: 30.0,
                width: 760.0,
                height: 700.0,
            }),
            viewport_width: 1400.0,
            viewport_height: 1000.0,
            include_empty: true,
        })
        .unwrap();
    let count = |side: LayoutSide| {
        geometry
            .buttons
            .iter()
            .chain(geometry.empty_slots.iter())
            .filter(|b| b.side == side)
            .count()
    };
    assert_eq!(
        (
            count(LayoutSide::Left),
            count(LayoutSide::Right),
            count(LayoutSide::Bottom)
        ),
        (5, 5, 7)
    );
}
