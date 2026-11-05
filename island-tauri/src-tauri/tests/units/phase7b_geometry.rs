//! Focused, display-free Phase 7b geometry tests. No Plasma, GTK window, or live data access.
use hermes_island_lib::{
    contract::{
        island::{GeometryReport, IslandMode, RoundedRect},
        modules::Registry,
        settings::ObjectOnly,
        workspace::*,
    },
    error::IslandError,
    integrations::plasma::anchor::{Anchor, Rect},
    services::workspace::WorkspaceService,
    window::{
        input_region::{InputRegionBackend, InputRegionController},
        placement::over_panel,
        shape::{covers, region_rows, ShapeRow},
    },
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU32, Ordering},
};

#[derive(Default)]
struct Fake {
    input: Vec<ShapeRow>,
    paint: Vec<ShapeRow>,
    paints: usize,
}
impl InputRegionBackend for Fake {
    fn apply(&mut self, rows: &[ShapeRow]) -> Result<(), IslandError> {
        self.input = rows.to_vec();
        Ok(())
    }
    fn readback(&self) -> Result<Vec<ShapeRow>, IslandError> {
        Ok(self.input.clone())
    }
    fn paint(&mut self, rows: &[ShapeRow]) -> Result<(), IslandError> {
        self.paint = rows.to_vec();
        self.paints += 1;
        Ok(())
    }
}
fn rect(x: f64, y: f64, w: f64, h: f64, r: f64) -> RoundedRect {
    RoundedRect {
        x,
        y,
        width: w,
        height: h,
        radius: r,
    }
}
fn report(
    rev: u32,
    mode: IslandMode,
    scale: f64,
    settled: bool,
    rects: Vec<RoundedRect>,
) -> GeometryReport {
    GeometryReport {
        state_rev: rev,
        mode,
        rects,
        scale,
        settled,
    }
}

#[test]
fn native_pill_mousearea_is_excluded_but_blur_keeps_exact_painted_shape_at_every_scale() {
    let pill = rect(115.25, 4.5, 150.0, 32.0, 16.0);
    let surface = rect(10.0, 0.0, 420.0, 220.0, 28.0);
    for scale in [0.75, 1.0, 1.25, 2.0] {
        let mut owner =
            InputRegionController::new(Fake::default(), true).with_island_geometry(Some(pill));
        owner.install().unwrap();
        owner
            .report(&report(1, IslandMode::Hover, scale, true, vec![surface]))
            .unwrap();
        let backend = owner.backend();
        assert_eq!(
            backend.paint,
            region_rows(&[surface], IslandMode::Hover, scale, false).unwrap()
        );
        // Independent pixel oracle: exclusion is the whole QML MouseArea, not a rounded hole.
        for y in 0..(220.0 * scale) as i32 {
            for x in 0..(430.0 * scale) as i32 {
                let excluded = x >= (pill.x * scale).floor() as i32
                    && x < ((pill.x + pill.width) * scale).ceil() as i32
                    && y >= (pill.y * scale).floor() as i32
                    && y < ((pill.y + pill.height) * scale).ceil() as i32;
                assert_eq!(
                    covers(&backend.input, x, y),
                    covers(&backend.paint, x, y) && !excluded,
                    "scale={scale} ({x},{y})"
                );
            }
        }
        assert!(covers(
            &backend.paint,
            (pill.x * scale) as i32,
            (pill.y * scale) as i32
        ));
        assert!(!covers(
            &backend.input,
            (pill.x * scale) as i32,
            (pill.y * scale) as i32
        ));
    }
}
#[test]
fn island_transition_never_leaves_swept_input_or_fills_the_gap_between_circles() {
    let shell = rect(100.0, 20.0, 300.0, 200.0, 24.0);
    let old = rect(20.0, 20.0, 36.0, 36.0, 18.0);
    let new = rect(20.0, 100.0, 36.0, 36.0, 18.0);
    let mut owner = InputRegionController::new(Fake::default(), false).with_island_geometry(None);
    owner.install().unwrap();
    owner
        .report(&report(1, IslandMode::Large, 1.0, true, vec![shell, old]))
        .unwrap();
    owner
        .report(&report(2, IslandMode::Large, 1.0, false, vec![shell, new]))
        .unwrap();
    assert!(
        !covers(&owner.backend().input, 38, 38),
        "old circle must not leave a hit trail"
    );
    assert!(covers(&owner.backend().input, 38, 118));
    assert!(
        !covers(&owner.backend().input, 80, 118),
        "gap is not the union's bounding box"
    );
    assert_eq!(owner.backend().input, owner.backend().paint);
}
#[test]
fn closing_native_island_immediately_releases_input_but_paints_until_settled_then_deletes_blur() {
    let mut owner = InputRegionController::new(Fake::default(), true)
        .with_island_geometry(Some(rect(100.0, 4.0, 150.0, 32.0, 16.0)));
    owner.install().unwrap();
    owner
        .report(&report(
            1,
            IslandMode::Large,
            1.0,
            true,
            vec![rect(0.0, 0.0, 500.0, 300.0, 24.0)],
        ))
        .unwrap();
    let closing = vec![rect(80.0, 4.0, 200.0, 60.0, 20.0)];
    owner
        .report(&report(2, IslandMode::Still, 1.0, false, closing.clone()))
        .unwrap();
    assert!(owner.backend().input.is_empty());
    assert!(!owner.backend().paint.is_empty());
    owner
        .report(&report(2, IslandMode::Still, 1.0, true, closing))
        .unwrap();
    assert!(owner.backend().input.is_empty());
    assert!(owner.backend().paint.is_empty());
    let paints = owner.backend().paints;
    assert!(
        !owner
            .report(&report(
                1,
                IslandMode::Large,
                1.0,
                false,
                vec![rect(0.0, 0.0, 500.0, 300.0, 24.0)]
            ))
            .unwrap()
            .applied
    );
    assert_eq!(
        owner.backend().paints,
        paints,
        "stale frame cannot reopen blur"
    );
}
#[test]
fn painted_changes_are_not_deduplicated_against_identical_empty_input() {
    let pill = rect(100.0, 4.0, 150.0, 32.0, 16.0);
    let mut owner =
        InputRegionController::new(Fake::default(), true).with_island_geometry(Some(pill));
    owner.install().unwrap();
    owner
        .report(&report(1, IslandMode::Hover, 1.0, false, vec![pill]))
        .unwrap();
    let before = owner.backend().paints;
    owner
        .report(&report(
            1,
            IslandMode::Hover,
            1.0,
            false,
            vec![rect(110.0, 5.0, 130.0, 30.0, 15.0)],
        ))
        .unwrap();
    assert!(owner.backend().input.is_empty());
    assert_eq!(owner.backend().paints, before + 1);
}
#[test]
fn earlier_controller_mode_preserves_union_until_settle_and_does_not_drive_painted_blur() {
    let mut owner = InputRegionController::new(Fake::default(), false);
    owner.install().unwrap();
    owner
        .report(&report(
            1,
            IslandMode::Large,
            1.0,
            true,
            vec![rect(20.0, 20.0, 36.0, 36.0, 18.0)],
        ))
        .unwrap();
    owner
        .report(&report(
            2,
            IslandMode::Large,
            1.0,
            false,
            vec![rect(20.0, 100.0, 36.0, 36.0, 18.0)],
        ))
        .unwrap();
    assert!(covers(&owner.backend().input, 38, 38));
    assert!(covers(&owner.backend().input, 38, 118));
    assert_eq!(owner.backend().paints, 0);
}
#[test]
fn settled_native_still_reports_are_validated_even_when_nothing_is_painted() {
    let mut owner = InputRegionController::new(Fake::default(), true).with_island_geometry(None);
    owner.install().unwrap();
    assert!(owner
        .report(&report(
            1,
            IslandMode::Still,
            1.0,
            true,
            vec![rect(-1.0, 0.0, 150.0, 32.0, 16.0)]
        ))
        .is_err());
}
fn anchor() -> Anchor {
    Anchor {
        output: "fixture".into(),
        panel: 1,
        widget: 2,
        native: true,
        center: 800.0,
        top: 0.0,
        panel_height: 40.0,
        left: 0.0,
        screen_width: 1600.0,
        screen_height: 1000.0,
        screen: Rect {
            x: 0.0,
            y: 0.0,
            width: 1600.0,
            height: 1000.0,
        },
        pill: Rect {
            x: 725.0,
            y: 4.0,
            width: 150.0,
            height: 32.0,
        },
        pill_radius: 16.0,
    }
}
#[test]
fn overlap_window_starts_at_panel_top_while_historical_below_panel_placement_is_unchanged() {
    let a = anchor();
    let over = over_panel(&a, 1180, 820).unwrap();
    assert_eq!(
        (over.x, over.y, over.width, over.height),
        (210, 0, 1180, 820)
    );
    let below =
        hermes_island_lib::integrations::plasma::anchor::below_panel(&a, 1180, 820).unwrap();
    assert_eq!((below.x, below.y), (210, 40));
}
#[test]
fn overlap_placement_clamps_small_negative_origin_monitor_and_keeps_exact_pill_coordinates() {
    let mut a = anchor();
    a.screen = Rect {
        x: -400.0,
        y: -100.0,
        width: 600.0,
        height: 400.0,
    };
    a.top = -100.0;
    a.center = -300.0;
    a.pill = Rect {
        x: -375.0,
        y: -96.0,
        width: 150.0,
        height: 32.0,
    };
    let p = over_panel(&a, 1180, 820).unwrap();
    assert_eq!((p.x, p.y, p.width, p.height), (-400, -100, 600, 400));
    assert_eq!(
        (a.pill.x - f64::from(p.x), a.pill.y - f64::from(p.y)),
        (25.0, 4.0)
    );
    a.pill.x = -401.0;
    assert!(over_panel(&a, 1180, 820).is_err());
}
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let path = PathBuf::from(
            std::env::var_os("TMPDIR").unwrap_or_else(|| panic!("private TMPDIR required")),
        )
        .join(format!(
            "phase7b-geometry-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap_or_else(|e| panic!("create geometry scratch: {e}"));
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap_or_else(|e| panic!("remove geometry scratch: {e}"));
    }
}
#[test]
fn max_diameter_gap_editor_fits_every_slot_and_preserves_short_island_gap_after_uniform_scaling() {
    let dir = Scratch::new();
    let service = WorkspaceService::new(dir.0.clone(), Registry::defaults()).unwrap();
    service
        .layout_command(LayoutOp::Diameter { diameter: 56.0 })
        .unwrap();
    service.layout_command(LayoutOp::Gap { gap: 24.0 }).unwrap();
    for (width, height) in [(400.0, 280.0), (220.0, 180.0)] {
        let g = service
            .layout_geometry(LayoutGeometryQuery {
                island: ObjectOnly(LayoutRect {
                    x: 70.0,
                    y: 20.0,
                    width: 100.0,
                    height: 20.0,
                }),
                viewport_width: width,
                viewport_height: height,
                include_empty: true,
            })
            .unwrap();
        let all = g
            .buttons
            .iter()
            .chain(g.empty_slots.iter())
            .collect::<Vec<_>>();
        // Only slots that fit this 100x20 island are offered (one per side column, one bottom),
        // plus the occupied defaults; the editor no longer lists every slot regardless of size.
        assert_eq!(all.len(), 5);
        for b in &all {
            assert!(
                b.rect.x >= 0.0
                    && b.rect.y >= 0.0
                    && b.rect.x + b.rect.width <= width + 1e-9
                    && b.rect.y + b.rect.height <= height + 1e-9
            );
        }
        let d = all[0].rect.width;
        let scale = d / 56.0;
        let bottom = all
            .iter()
            .filter(|b| b.side != LayoutSide::Bottom)
            .map(|b| b.rect.y + b.rect.height)
            .fold(g.island.y + g.island.height, f64::max);
        for b in all.iter().filter(|b| b.side == LayoutSide::Bottom) {
            assert!((b.rect.y - bottom - 24.0 * scale).abs() < 1e-8);
        }
    }
}
