//! Authoritative launcher edit sessions and private persistent side-button layouts.
//! Staging cores are copied unchanged except module paths; controller policy is enforced here.
pub mod arrange_core;
pub mod layout_core;

use crate::{
    contract::{
        island::IslandView,
        modules::{ModuleId, Registry},
        settings::SettingsPatch,
        workspace::*,
    },
    error::IslandError,
};
use arrange_core::{ArrangeCommit, Arrangement, GridItem};
use layout_core::{ButtonAction, Layout, Rect, Side, SideButton, SizePreset};
use std::{fs, path::PathBuf, sync::Mutex};

fn invalid(error: impl std::fmt::Display) -> IslandError {
    IslandError::invalid(error.to_string())
}
fn locked(id: &str) -> bool {
    matches!(id, "terminal" | "settings")
}

/// A production policy wrapper: both full-permutation and visible indices of locks are fixed.
/// Every tentative operation is applied to a clone; rejected indirect moves are atomic.
#[derive(Clone)]
pub struct LockedArrangement {
    core: Arrangement,
    base: Vec<GridItem>,
    items: Vec<GridItem>,
}
impl LockedArrangement {
    pub fn new(items: Vec<GridItem>) -> Result<Self, IslandError> {
        if items
            .iter()
            .any(|i| i.id.is_empty() || i.id.len() > arrange_core::MAX_ID_BYTES)
        {
            return Err(IslandError::invalid("Launcher IDs must be 1..=64 bytes"));
        }
        let core = Arrangement::new(items.clone()).map_err(invalid)?;
        let items = items
            .into_iter()
            .map(|mut item| {
                item.enabled |= item.locked;
                item
            })
            .collect::<Vec<_>>();
        Ok(Self {
            core,
            base: items.clone(),
            items,
        })
    }
    pub fn items(&self) -> &[GridItem] {
        &self.items
    }
    pub fn editing(&self) -> bool {
        self.core.is_editing()
    }
    /// Nick's rule (Nov 2): Hermes home stays the first tile; Settings can never be hidden or
    /// dragged directly, but other tiles may move or hide around it (its index floats). The
    /// old rule pinned Settings' full AND visible index, so hiding Monitor/AI or dropping a
    /// tile in front of it failed with "positions are fixed".
    fn check_locks(&self, next: &[GridItem]) -> Result<(), IslandError> {
        for (index, item) in self.base.iter().enumerate().filter(|(_, i)| i.locked) {
            let Some(next_index) = next.iter().position(|i| i.id == item.id) else {
                return Err(IslandError::invalid("Locked launcher tile is missing"));
            };
            if !next[next_index].enabled {
                return Err(IslandError::invalid(
                    "Hermes home and Settings cannot be hidden",
                ));
            }
            if item.id == "terminal" && next_index != index {
                return Err(IslandError::invalid("Hermes home stays the first tile"));
            }
        }
        Ok(())
    }
    pub fn apply(&mut self, op: &LauncherOp) -> Result<Option<ArrangeCommit>, IslandError> {
        let mut core = self.core.clone();
        match op {
            LauncherOp::Begin {} => core.begin_edit().map_err(invalid)?,
            LauncherOp::Move { id, to } => {
                if self.items.iter().any(|i| i.id == *id && i.locked) {
                    return Err(IslandError::invalid("Locked launcher tile cannot move"));
                }
                core.move_item(id, *to as usize).map_err(invalid)?;
            }
            LauncherOp::Disable { id } => core.disable(id).map_err(invalid)?,
            LauncherOp::Enable { id } => core.enable(id).map_err(invalid)?,
            LauncherOp::Cancel {} => {
                core.cancel().map_err(invalid)?;
                self.core = core;
                self.items = self.base.clone();
                return Ok(None);
            }
            LauncherOp::Commit {} => {
                self.check_locks(&self.items)?;
                let change = core.commit().map_err(invalid)?;
                self.core = core;
                self.base = self.items.clone();
                return Ok(change);
            }
        }
        let mut preview = core.clone();
        let commit = preview.commit().map_err(invalid)?;
        let mut items = self.base.clone();
        if let Some(commit) = commit {
            items = commit
                .order
                .iter()
                .map(|id| {
                    self.base
                        .iter()
                        .find(|i| i.id == *id)
                        .cloned()
                        .ok_or_else(|| IslandError::internal("Launcher permutation is incomplete"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            for change in commit.enabled_changes {
                if let Some(item) = items.iter_mut().find(|i| i.id == change.id) {
                    item.enabled = change.enabled;
                }
            }
        }
        self.check_locks(&items)?;
        self.core = core;
        self.items = items;
        Ok(None)
    }
}

fn registry_items(registry: &Registry) -> Vec<GridItem> {
    let mut modules = registry.modules().iter().collect::<Vec<_>>();
    modules.sort_by_key(|m| m.order);
    modules
        .into_iter()
        .map(|m| GridItem {
            id: m.id.as_str().to_owned(),
            enabled: m.is_active(),
            locked: locked(m.id.as_str()),
        })
        .collect()
}

struct LauncherState {
    registry: Registry,
    draft: Option<LockedArrangement>,
    rev: u32,
}
impl LauncherState {
    fn reconcile(&mut self, registry: &Registry) -> Result<(), IslandError> {
        if self.registry != *registry {
            // A changed enabled flag, order OR config invalidates the draft. Never merge stale edits.
            self.registry = registry.clone();
            self.draft = None;
            self.rev = self.rev.wrapping_add(1);
        }
        Ok(())
    }
    fn snapshot(&self) -> LauncherChanged {
        let items = self
            .draft
            .as_ref()
            .map_or_else(|| registry_items(&self.registry), |d| d.items.clone());
        LauncherChanged {
            rev: self.rev,
            editing: self.draft.as_ref().is_some_and(LockedArrangement::editing),
            items: items
                .into_iter()
                .map(|i| LauncherItem {
                    known: ModuleId::new(i.id.clone()).is_ok_and(|id| id.is_known()),
                    view: module_route(&i.id),
                    id: i.id,
                    enabled: i.enabled || i.locked,
                    locked: i.locked,
                })
                .collect(),
        }
    }
}
struct LayoutState {
    layout: Layout,
    unchanged_sizing: bool,
    rev: u32,
}

pub struct WorkspaceService {
    file: PathBuf,
    launcher: Mutex<LauncherState>,
    layout: Mutex<LayoutState>,
}
impl WorkspaceService {
    pub fn new(data_dir: PathBuf, registry: Registry) -> Result<Self, IslandError> {
        let file = data_dir.join("layout.json");
        let (layout, unchanged_sizing) = match fs::read(&file) {
            Ok(bytes) => {
                if bytes.len() > 64 * 1024 {
                    return Err(IslandError::invalid("Layout file is too large"));
                }
                let text = std::str::from_utf8(&bytes).map_err(invalid)?;
                let (layout, _) = Layout::from_json(text).map_err(invalid)?;
                let raw: serde_json::Value = serde_json::from_slice(&bytes).map_err(invalid)?;
                let unchanged = raw
                    .get("unchangedSizing")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                // Stored generic commands/unknown app routes never become executable actions.
                for button in &layout.buttons {
                    project_action(&button.action)?;
                }
                (layout, unchanged)
            }
            // A fresh install starts on the Bigger preset (Nick, Nov 2), not the old per-app sizes.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Layout::default(), false),
            Err(_) => return Err(IslandError::io("Cannot read layout")),
        };
        Ok(Self {
            file,
            launcher: Mutex::new(LauncherState {
                registry,
                draft: None,
                rev: 1,
            }),
            layout: Mutex::new(LayoutState {
                layout,
                unchanged_sizing,
                rev: 1,
            }),
        })
    }
    pub fn launcher_get(&self, registry: &Registry) -> Result<LauncherChanged, IslandError> {
        let mut state = self
            .launcher
            .lock()
            .map_err(|_| IslandError::internal("Launcher state poisoned"))?;
        state.reconcile(registry)?;
        Ok(state.snapshot())
    }
    pub fn launcher_command(
        &self,
        registry: &Registry,
        op: &LauncherOp,
    ) -> Result<LauncherChanged, IslandError> {
        let mut state = self
            .launcher
            .lock()
            .map_err(|_| IslandError::internal("Launcher state poisoned"))?;
        state.reconcile(registry)?;
        if matches!(op, LauncherOp::Commit {}) {
            return Err(IslandError::invalid(
                "Commit requires the settings transaction",
            ));
        }
        if let LauncherOp::Move { id, .. }
        | LauncherOp::Disable { id }
        | LauncherOp::Enable { id } = op
        {
            let id = ModuleId::new(id.clone()).map_err(invalid)?;
            if !id.is_known() {
                return Err(IslandError::invalid("Unknown modules are inert"));
            }
        }
        let before = state.snapshot();
        if matches!(op, LauncherOp::Begin {}) && state.draft.is_none() {
            state.draft = Some(LockedArrangement::new(registry_items(registry))?);
        }
        state
            .draft
            .as_mut()
            .ok_or_else(|| IslandError::invalid("No launcher edit session"))?
            .apply(op)?;
        if !state.draft.as_ref().is_some_and(LockedArrangement::editing) {
            state.draft = None;
        }
        if state.snapshot() != before {
            state.rev = state.rev.wrapping_add(1);
        }
        Ok(state.snapshot())
    }
    /// Prepare only; caller holds the shared settings command gate until persistence + effects finish.
    /// No std mutex guard survives this call or an await. Conflicts invalidate rather than overwrite.
    pub fn prepare_commit(
        &self,
        registry: &Registry,
    ) -> Result<Option<SettingsPatch>, IslandError> {
        let mut state = self
            .launcher
            .lock()
            .map_err(|_| IslandError::internal("Launcher state poisoned"))?;
        if state.registry != *registry {
            state.reconcile(registry)?;
            return Err(IslandError::new(
                crate::error::ErrorCode::Busy,
                "Registry changed; launcher draft was invalidated",
            ));
        }
        let draft = state
            .draft
            .as_ref()
            .ok_or_else(|| IslandError::invalid("No launcher edit session"))?;
        let mut candidate = draft.clone();
        let change = candidate.apply(&LauncherOp::Commit {})?;
        let Some(change) = change else {
            state.draft = None;
            state.rev = state.rev.wrapping_add(1);
            return Ok(None);
        };
        let mut next = registry.clone();
        let order = change
            .order
            .into_iter()
            .map(ModuleId::new)
            .collect::<Result<Vec<_>, _>>()
            .map_err(invalid)?;
        next.reorder(&order).map_err(invalid)?;
        for change in change.enabled_changes {
            let id = ModuleId::new(change.id).map_err(invalid)?;
            if !id.is_known() {
                return Err(IslandError::invalid("Unknown modules are inert"));
            }
            next.set_enabled(&id, change.enabled).map_err(invalid)?;
        }
        Ok(Some(SettingsPatch {
            module_registry: Some(next),
            ..Default::default()
        }))
    }
    pub fn committed(&self, registry: &Registry) -> Result<LauncherChanged, IslandError> {
        let mut state = self
            .launcher
            .lock()
            .map_err(|_| IslandError::internal("Launcher state poisoned"))?;
        state.registry = registry.clone();
        state.draft = None;
        state.rev = state.rev.wrapping_add(1);
        Ok(state.snapshot())
    }
    pub fn layout_get(&self) -> Result<LayoutChanged, IslandError> {
        let state = self
            .layout
            .lock()
            .map_err(|_| IslandError::internal("Layout state poisoned"))?;
        layout_snapshot(&state)
    }
    pub fn layout_command(&self, op: LayoutOp) -> Result<LayoutChanged, IslandError> {
        let mut state = self
            .layout
            .lock()
            .map_err(|_| IslandError::internal("Layout state poisoned"))?;
        let mut next = state.layout.clone();
        let mut unchanged = state.unchanged_sizing;
        match op {
            LayoutOp::Preset { preset } => {
                next.set_preset(core_preset(preset)).map_err(invalid)?;
                unchanged = false;
            }
            LayoutOp::Add { button } => next.add(core_button(button.0)?).map_err(invalid)?,
            LayoutOp::Remove { id } => {
                validate_button_id(&id)?;
                next.remove(&id).map_err(invalid)?;
            }
            LayoutOp::Edit { id, action, label } => {
                validate_button_id(&id)?;
                next.edit(&id, core_action(action)?, label)
                    .map_err(invalid)?;
            }
            LayoutOp::Move { id, side, slot } => {
                validate_button_id(&id)?;
                next.move_to(&id, core_side(side), slot_u8(slot)?)
                    .map_err(invalid)?;
            }
            LayoutOp::Diameter { diameter } => {
                next.set_button_diameter(diameter).map_err(invalid)?
            }
            LayoutOp::Gap { gap } => next.set_gap(gap).map_err(invalid)?,
        }
        if next == state.layout && unchanged == state.unchanged_sizing {
            return layout_snapshot(&state);
        }
        let mut raw = serde_json::to_value(&next).map_err(invalid)?;
        raw["unchangedSizing"] = serde_json::json!(unchanged);
        let bytes = serde_json::to_vec_pretty(&raw).map_err(invalid)?;
        super::autostart::atomic_write(&self.file, &bytes, "Cannot persist layout")?;
        state.layout = next;
        state.unchanged_sizing = unchanged;
        state.rev = state.rev.wrapping_add(1);
        layout_snapshot(&state)
    }
    pub fn layout_geometry(
        &self,
        query: LayoutGeometryQuery,
    ) -> Result<LayoutGeometry, IslandError> {
        let state = self
            .layout
            .lock()
            .map_err(|_| IslandError::internal("Layout state poisoned"))?;
        geometry(&state.layout, state.rev, query)
    }
}

fn validate_button_id(id: &str) -> Result<(), IslandError> {
    if id.is_empty()
        || id.len() > 64
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
    {
        return Err(IslandError::invalid(
            "Button ID must be 1..=64 ASCII slug bytes",
        ));
    }
    Ok(())
}
fn slot_u8(slot: u32) -> Result<u8, IslandError> {
    u8::try_from(slot).map_err(|_| IslandError::invalid("Slot out of range"))
}
fn core_side(side: LayoutSide) -> Side {
    match side {
        LayoutSide::Left => Side::Left,
        LayoutSide::Right => Side::Right,
        LayoutSide::Bottom => Side::Bottom,
    }
}
fn project_side(side: Side) -> LayoutSide {
    match side {
        Side::Left => LayoutSide::Left,
        Side::Right => LayoutSide::Right,
        Side::Bottom => LayoutSide::Bottom,
    }
}
fn core_preset(preset: LayoutPreset) -> SizePreset {
    match preset {
        LayoutPreset::Compact {} => SizePreset::Compact,
        LayoutPreset::Spacious {} => SizePreset::Spacious,
        LayoutPreset::Bigger {} => SizePreset::Bigger,
        LayoutPreset::Custom { width, max_height } => SizePreset::Custom { width, max_height },
    }
}
fn project_preset(preset: SizePreset) -> LayoutPreset {
    match preset {
        SizePreset::Compact => LayoutPreset::Compact {},
        SizePreset::Spacious => LayoutPreset::Spacious {},
        SizePreset::Bigger => LayoutPreset::Bigger {},
        SizePreset::Custom { width, max_height } => LayoutPreset::Custom { width, max_height },
    }
}
fn core_action(action: LayoutAction) -> Result<ButtonAction, IslandError> {
    Ok(match action {
        LayoutAction::OpenApp { view } => ButtonAction::OpenApp {
            app_id: serde_json::to_value(view)
                .map_err(invalid)?
                .as_str()
                .ok_or_else(|| IslandError::internal("Invalid app route"))?
                .to_owned(),
        },
        LayoutAction::AppGrid {} => ButtonAction::AppGrid,
        LayoutAction::Settings {} => ButtonAction::Settings,
        LayoutAction::Volume {} => ButtonAction::Volume,
        LayoutAction::NowPlaying {} => ButtonAction::NowPlaying,
        LayoutAction::Timer {} => ButtonAction::Timer,
        LayoutAction::PlayPause {} => ButtonAction::PlayPause,
        LayoutAction::NextTrack {} => ButtonAction::NextTrack,
        LayoutAction::PreviousTrack {} => ButtonAction::PreviousTrack,
        LayoutAction::ToggleDnd {} => ButtonAction::ToggleDnd,
        LayoutAction::MicMute {} => ButtonAction::MicMute,
        LayoutAction::CyclePowerProfile {} => ButtonAction::CyclePowerProfile,
        LayoutAction::Screenshot {} => ButtonAction::Screenshot,
        LayoutAction::PickColour {} => ButtonAction::PickColour,
        LayoutAction::KeepAwake {} => ButtonAction::KeepAwake,
        LayoutAction::Collapse {} => ButtonAction::Collapse,
    })
}
fn project_action(action: &ButtonAction) -> Result<LayoutAction, IslandError> {
    Ok(match action {
        ButtonAction::OpenApp { app_id } => LayoutAction::OpenApp {
            view: serde_json::from_value(serde_json::json!(app_id))
                .map_err(|_| IslandError::invalid("Unknown layout app route"))?,
        },
        ButtonAction::AppGrid => LayoutAction::AppGrid {},
        ButtonAction::Settings => LayoutAction::Settings {},
        ButtonAction::Volume => LayoutAction::Volume {},
        ButtonAction::NowPlaying => LayoutAction::NowPlaying {},
        ButtonAction::Timer => LayoutAction::Timer {},
        ButtonAction::PlayPause => LayoutAction::PlayPause {},
        ButtonAction::NextTrack => LayoutAction::NextTrack {},
        ButtonAction::PreviousTrack => LayoutAction::PreviousTrack {},
        ButtonAction::ToggleDnd => LayoutAction::ToggleDnd {},
        ButtonAction::MicMute => LayoutAction::MicMute {},
        ButtonAction::CyclePowerProfile => LayoutAction::CyclePowerProfile {},
        ButtonAction::Screenshot => LayoutAction::Screenshot {},
        ButtonAction::PickColour => LayoutAction::PickColour {},
        ButtonAction::KeepAwake => LayoutAction::KeepAwake {},
        ButtonAction::Collapse => LayoutAction::Collapse {},
        ButtonAction::Command { .. } => {
            return Err(IslandError::invalid(
                "Generic layout commands are not permitted",
            ))
        }
    })
}
fn core_button(button: LayoutButton) -> Result<SideButton, IslandError> {
    validate_button_id(&button.id)?;
    Ok(SideButton {
        id: button.id,
        side: core_side(button.side),
        slot: slot_u8(button.slot)?,
        action: core_action(button.action)?,
        label: button.label,
    })
}
fn layout_snapshot(state: &LayoutState) -> Result<LayoutChanged, IslandError> {
    Ok(LayoutChanged {
        rev: state.rev,
        unchanged_sizing: state.unchanged_sizing,
        layout: LayoutModel {
            version: state.layout.version,
            preset: project_preset(state.layout.preset),
            button_diameter: state.layout.button_diameter,
            gap: state.layout.gap,
            buttons: state
                .layout
                .buttons
                .iter()
                .map(|b| {
                    Ok(LayoutButton {
                        id: b.id.clone(),
                        side: project_side(b.side),
                        slot: b.slot.into(),
                        action: project_action(&b.action)?,
                        label: b.label.clone(),
                    })
                })
                .collect::<Result<_, IslandError>>()?,
        },
    })
}

pub fn module_route(id: &str) -> Option<IslandView> {
    Some(match id {
        "terminal" => IslandView::Terminal,
        "monitor" => IslandView::Monitor,
        "ai" => IslandView::Ai,
        "settings" => IslandView::Settings,
        "notifications" => IslandView::Notifications,
        "clipboard" => IslandView::Clipboard,
        "music" => IslandView::Music,
        "timer" => IslandView::Timer,
        "calendar" => IslandView::Calendar,
        "scratchpad" => IslandView::Scratchpad,
        "downloads" => IslandView::Downloads,
        "brightness" => IslandView::Brightness,
        "agent-tracker" => IslandView::Agents,
        "mixer" => IslandView::Mixer,
        "quick-toggles" | "system-tiles" => IslandView::Controls,
        "update-badge" => IslandView::Updates,
        _ => return None,
    })
}

/// Core occupied geometry + the ratified short-island rule. Miniature slots use the same
/// packed occupied rows; empty slots follow them. The bottom row clears BOTH column bottoms.
fn geometry(
    layout: &Layout,
    rev: u32,
    query: LayoutGeometryQuery,
) -> Result<LayoutGeometry, IslandError> {
    let mut island = query.island.0;
    let vw = query.viewport_width;
    let vh = query.viewport_height;
    if ![vw, vh, island.x, island.y, island.width, island.height]
        .iter()
        .all(|v| v.is_finite())
        || !(1.0..=16384.0).contains(&vw)
        || !(1.0..=16384.0).contains(&vh)
        || island.x < 0.0
        || island.y < 0.0
        || island.width <= 0.0
        || island.height <= 0.0
        || island.x + island.width > vw
        || island.y + island.height > vh
    {
        return Err(IslandError::invalid(
            "Layout geometry must be finite and inside the viewport",
        ));
    }
    let input = Rect {
        x: island.x,
        y: island.y,
        w: island.width,
        h: island.height,
    };
    let mut rects = layout.button_rects(input).map_err(invalid)?;
    let bottom_y = rects
        .iter()
        .filter(|(id, _)| {
            layout
                .buttons
                .iter()
                .any(|b| b.id == *id && b.side != Side::Bottom)
        })
        .fold(island.y + island.height, |bottom, (_, r)| {
            bottom.max(r.y + r.h)
        })
        + layout.gap;
    let mut buttons = Vec::new();
    for (id, r) in &mut rects {
        let b = layout
            .buttons
            .iter()
            .find(|b| b.id == *id)
            .ok_or_else(|| IslandError::internal("Missing geometry button"))?;
        if b.side == Side::Bottom {
            r.y = bottom_y;
        }
        buttons.push(LayoutSlotRect {
            id: Some(id.clone()),
            side: project_side(b.side),
            slot: b.slot.into(),
            rect: LayoutRect {
                x: r.x,
                y: r.y,
                width: r.w,
                height: r.h,
            },
        });
    }
    let mut empty_slots = Vec::new();
    if query.include_empty {
        let d = layout.button_diameter;
        let g = layout.gap;
        // Offer only the slots that fit this island (Nick, Nov 2): a side column holds as many
        // circles as the island is tall, the bottom row as many as it is wide. Occupied slots
        // beyond that (e.g. after shrinking the island) stay visible so they can be moved.
        let side_fit =
            (((island.height + g) / (d + g)).floor().max(1.0) as u8).min(Side::Left.capacity());
        let bottom_fit =
            (((island.width + g) / (d + g)).floor().max(1.0) as u8).min(Side::Bottom.capacity());
        let occupied = |side: Side| {
            layout
                .buttons
                .iter()
                .filter(|b| b.side == side)
                .map(|b| b.slot + 1)
                .max()
                .unwrap_or(0)
        };
        let column = occupied(Side::Left)
            .max(occupied(Side::Right))
            .max(side_fit);
        let row = occupied(Side::Bottom).max(bottom_fit);
        let full_column_bottom =
            island.y + f64::from(column) * d + f64::from(column.saturating_sub(1)) * g;
        let mini_bottom_y = (island.y + island.height).max(full_column_bottom) + g;
        let row_width = f64::from(row) * d + f64::from(row.saturating_sub(1)) * g;
        // For the editor, occupied + empty slots form a stable miniature.
        let mini_rect = |side: Side, slot: u8| LayoutRect {
            x: match side {
                Side::Left => island.x - g - d,
                Side::Right => island.x + island.width + g,
                Side::Bottom => {
                    island.x + island.width / 2.0 - row_width / 2.0 + f64::from(slot) * (d + g)
                }
            },
            y: if side == Side::Bottom {
                mini_bottom_y
            } else {
                island.y + f64::from(slot) * (d + g)
            },
            width: d,
            height: d,
        };
        for button in &mut buttons {
            button.rect = mini_rect(
                core_side(button.side),
                u8::try_from(button.slot).map_err(invalid)?,
            );
        }
        for (side, slot) in layout.empty_slots() {
            let fits = match side {
                Side::Left | Side::Right => slot < column,
                Side::Bottom => slot < row,
            };
            if !fits {
                continue;
            }
            empty_slots.push(LayoutSlotRect {
                id: None,
                side: project_side(side),
                slot: slot.into(),
                rect: mini_rect(side, slot),
            });
        }
    }
    if query.include_empty {
        // A miniature represents ALL slots, even at maximum diameter/gap. Fit that complete
        // union uniformly instead of failing or clipping its + controls. Live requests remain
        // strict: the frontend must first size/clamp its whole union before reporting it.
        let mut left = island.x;
        let mut top = island.y;
        let mut right = island.x + island.width;
        let mut bottom = island.y + island.height;
        for b in buttons.iter().chain(empty_slots.iter()) {
            left = left.min(b.rect.x);
            top = top.min(b.rect.y);
            right = right.max(b.rect.x + b.rect.width);
            bottom = bottom.max(b.rect.y + b.rect.height);
        }
        let scale = 1.0_f64.min(vw / (right - left)).min(vh / (bottom - top));
        let x = left.clamp(0.0, (vw - (right - left) * scale).max(0.0));
        let y = top.clamp(0.0, (vh - (bottom - top) * scale).max(0.0));
        let fit = |r: &mut LayoutRect| {
            r.x = x + (r.x - left) * scale;
            r.y = y + (r.y - top) * scale;
            r.width *= scale;
            r.height *= scale;
        };
        fit(&mut island);
        for b in buttons.iter_mut().chain(empty_slots.iter_mut()) {
            fit(&mut b.rect);
        }
    }
    if buttons.iter().chain(empty_slots.iter()).any(|b| {
        b.rect.x < 0.0
            || b.rect.y < 0.0
            || b.rect.x + b.rect.width > vw + 1e-9
            || b.rect.y + b.rect.height > vh + 1e-9
    }) {
        return Err(IslandError::invalid(
            "Side buttons do not fit inside the viewport",
        ));
    }
    Ok(LayoutGeometry {
        rev,
        island,
        buttons,
        empty_slots,
    })
}
