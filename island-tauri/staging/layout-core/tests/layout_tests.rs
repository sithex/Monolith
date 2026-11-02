#![allow(clippy::float_cmp)]

use layout_core::{
    ButtonAction, Layout, LayoutError, LoadReport, Rect, Side, SideButton, SizePreset,
};
use std::collections::HashMap;

const ISLAND: Rect = Rect {
    x: 100.0,
    y: 200.0,
    w: 400.0,
    h: 80.0,
};

fn button(id: &str, side: Side, slot: u8, action: ButtonAction) -> SideButton {
    SideButton {
        id: id.to_string(),
        side,
        slot,
        action,
        label: None,
    }
}

fn parse(j: &str) -> (Layout, LoadReport) {
    match Layout::from_json(j) {
        Ok(x) => x,
        Err(e) => panic!("unexpected parse error: {e}"),
    }
}

fn rects(l: &Layout, island: Rect) -> Vec<(String, Rect)> {
    match l.button_rects(island) {
        Ok(v) => v,
        Err(e) => panic!("unexpected geometry error: {e}"),
    }
}

fn rect_of<'a>(rs: &'a [(String, Rect)], id: &str) -> &'a Rect {
    rs.iter()
        .find(|(i, _)| i == id)
        .map(|(_, r)| r)
        .unwrap_or_else(|| panic!("missing rect for {id}"))
}

fn ids(l: &Layout) -> Vec<String> {
    l.buttons.iter().map(|b| b.id.clone()).collect()
}

#[test]
fn default_layout_exact_contents() {
    let l = Layout::default();
    assert_eq!(l.version, 1);
    assert_eq!(l.preset, SizePreset::Compact);
    assert_eq!(l.button_diameter, 36.0);
    assert_eq!(l.gap, 10.0);
    assert_eq!(l.buttons.len(), 5);
    let expected: Vec<(&str, Side, u8, ButtonAction, Option<&str>)> = vec![
        ("apps", Side::Left, 0, ButtonAction::AppGrid, None),
        ("timer", Side::Left, 1, ButtonAction::Timer, None),
        ("settings", Side::Right, 0, ButtonAction::Settings, None),
        ("volume", Side::Right, 1, ButtonAction::Volume, None),
        (
            "nowplaying",
            Side::Bottom,
            0,
            ButtonAction::NowPlaying,
            None,
        ),
    ];
    for (b, (id, side, slot, action, label)) in l.buttons.iter().zip(expected) {
        assert_eq!(b.id, id);
        assert_eq!(b.side, side);
        assert_eq!(b.slot, slot);
        assert_eq!(b.action, action);
        assert_eq!(b.label.as_deref(), label);
    }
}

#[test]
fn empty_slots_order_with_defaults() {
    let l = Layout::default();
    assert_eq!(
        l.empty_slots(),
        vec![
            (Side::Left, 2),
            (Side::Right, 2),
            (Side::Bottom, 1),
            (Side::Bottom, 2),
            (Side::Bottom, 3),
            (Side::Bottom, 4),
        ]
    );
}

#[test]
fn left_slot_2_accepted_slot_3_rejected() {
    let mut l = Layout::default();
    assert!(l
        .add(button("x", Side::Left, 2, ButtonAction::Settings))
        .is_ok());
    assert_eq!(
        l.add(button("y", Side::Left, 3, ButtonAction::Settings)),
        Err(LayoutError::SlotOutOfRange)
    );
}

#[test]
fn bottom_slot_4_accepted_slot_5_rejected() {
    let mut l = Layout::default();
    assert!(l
        .add(button("x", Side::Bottom, 4, ButtonAction::Settings))
        .is_ok());
    assert_eq!(
        l.add(button("y", Side::Bottom, 5, ButtonAction::Settings)),
        Err(LayoutError::SlotOutOfRange)
    );
}

#[test]
fn add_rejects_invalid_ids() {
    let mut l = Layout::default();
    let bad = vec![
        String::new(),
        "a".repeat(65),
        "has space".to_string(),
        "slash/".to_string(),
        "é".to_string(),
        "#".to_string(),
        "a\n".to_string(),
    ];
    for id in bad {
        assert_eq!(
            l.add(SideButton {
                id,
                side: Side::Left,
                slot: 2,
                action: ButtonAction::AppGrid,
                label: None,
            }),
            Err(LayoutError::InvalidId)
        );
    }
    // dot is allowed; 64 chars is the max.
    assert!(l
        .add(button("a.b_c-d", Side::Left, 2, ButtonAction::AppGrid))
        .is_ok());
    assert!(l
        .add(button(
            &"a".repeat(64),
            Side::Right,
            2,
            ButtonAction::AppGrid
        ))
        .is_ok());
}

#[test]
fn add_rejects_invalid_action_and_label() {
    let mut l = Layout::default();
    assert_eq!(
        l.add(SideButton {
            id: "x".into(),
            side: Side::Left,
            slot: 2,
            action: ButtonAction::OpenApp { app_id: "".into() },
            label: None,
        }),
        Err(LayoutError::InvalidAction)
    );
    assert_eq!(
        l.add(SideButton {
            id: "y".into(),
            side: Side::Left,
            slot: 2,
            action: ButtonAction::AppGrid,
            label: Some("z".repeat(33)),
        }),
        Err(LayoutError::LabelTooLong)
    );
    assert!(l
        .add(SideButton {
            id: "z".into(),
            side: Side::Left,
            slot: 2,
            action: ButtonAction::AppGrid,
            label: Some("z".repeat(32)),
        })
        .is_ok());
}

#[test]
fn add_rejects_duplicate_id_and_occupied_slot_and_leaves_state() {
    let mut l = Layout::default();
    let before = l.to_json();
    assert_eq!(
        l.add(button("apps", Side::Bottom, 1, ButtonAction::AppGrid)),
        Err(LayoutError::DuplicateId)
    );
    assert_eq!(
        l.add(button("newname", Side::Left, 0, ButtonAction::AppGrid)),
        Err(LayoutError::OccupiedSlot)
    );
    assert_eq!(l.to_json(), before);
}

#[test]
fn remove_and_not_found() {
    let mut l = Layout::default();
    l.remove("timer").unwrap();
    assert_eq!(ids(&l), vec!["apps", "settings", "volume", "nowplaying"]);
    assert_eq!(l.remove("timer"), Err(LayoutError::NotFound));
    assert!(l.empty_slots().contains(&(Side::Left, 1)));
}

#[test]
fn move_into_occupied_slot_swaps_both_buttons() {
    let mut l = Layout::default();
    // default: apps left0, timer left1, settings right0, volume right1,
    // nowplaying bottom0. Move timer -> bottom slot 0 (occupied by nowplaying).
    l.move_to("timer", Side::Bottom, 0).unwrap();
    assert_eq!(
        l.buttons
            .iter()
            .find(|b| b.id == "timer")
            .map(|b| (b.side, b.slot)),
        Some((Side::Bottom, 0))
    );
    assert_eq!(
        l.buttons
            .iter()
            .find(|b| b.id == "nowplaying")
            .map(|b| (b.side, b.slot)),
        Some((Side::Left, 1))
    );
    // and a cross-side swap into a free slot, then back into an occupied one
    l.move_to("settings", Side::Bottom, 1).unwrap();
    l.move_to("volume", Side::Bottom, 1).unwrap();
    assert_eq!(
        l.buttons
            .iter()
            .find(|b| b.id == "settings")
            .map(|b| (b.side, b.slot)),
        Some((Side::Right, 1))
    );
    assert_eq!(
        l.buttons
            .iter()
            .find(|b| b.id == "volume")
            .map(|b| (b.side, b.slot)),
        Some((Side::Bottom, 1))
    );
}

#[test]
fn move_to_same_slot_noop() {
    let mut l = Layout::default();
    let before = l.to_json();
    l.move_to("apps", Side::Left, 0).unwrap();
    assert_eq!(l.to_json(), before);
}

#[test]
fn move_to_out_of_range_slot_rejected_and_missing_id_not_found() {
    let mut l = Layout::default();
    assert_eq!(
        l.move_to("apps", Side::Left, 3),
        Err(LayoutError::SlotOutOfRange)
    );
    assert_eq!(l.move_to("nope", Side::Left, 0), Err(LayoutError::NotFound));
    let before = l.to_json();
    assert_eq!(l.to_json(), before);
}

#[test]
fn edit_updates_action_and_label() {
    let mut l = Layout::default();
    l.edit("apps", ButtonAction::Volume, Some("Vol".into()))
        .unwrap();
    let b = l.buttons.iter().find(|b| b.id == "apps").unwrap();
    assert_eq!(b.action, ButtonAction::Volume);
    assert_eq!(b.label.as_deref(), Some("Vol"));
    l.edit("apps", ButtonAction::AppGrid, None).unwrap();
    let b = l.buttons.iter().find(|b| b.id == "apps").unwrap();
    assert_eq!(b.action, ButtonAction::AppGrid);
    assert_eq!(b.label, None);
}

#[test]
fn edit_rejects_bad_action_long_label_and_missing_id() {
    let mut l = Layout::default();
    assert_eq!(
        l.edit(
            "apps",
            ButtonAction::OpenApp {
                app_id: "bad id".into()
            },
            None
        ),
        Err(LayoutError::InvalidAction)
    );
    assert_eq!(
        l.edit("apps", ButtonAction::AppGrid, Some("z".repeat(33))),
        Err(LayoutError::LabelTooLong)
    );
    assert_eq!(
        l.edit("nope", ButtonAction::AppGrid, None),
        Err(LayoutError::NotFound)
    );
    let before = l.to_json();
    assert_eq!(l.to_json(), before);
}

#[test]
fn custom_preset_bounds_accepted() {
    let mut l = Layout::default();
    l.set_preset(SizePreset::Custom {
        width: 300.0,
        max_height: 240.0,
    })
    .unwrap();
    l.set_preset(SizePreset::Custom {
        width: 900.0,
        max_height: 900.0,
    })
    .unwrap();
    assert_eq!(l.preset.width(), 900.0);
    assert_eq!(l.preset.max_height(), 900.0);
}

#[test]
fn custom_preset_bounds_rejected() {
    let mut l = Layout::default();
    let bad = [
        SizePreset::Custom {
            width: 299.9,
            max_height: 300.0,
        },
        SizePreset::Custom {
            width: 900.1,
            max_height: 300.0,
        },
        SizePreset::Custom {
            width: 500.0,
            max_height: 239.9,
        },
        SizePreset::Custom {
            width: 500.0,
            max_height: 900.1,
        },
        SizePreset::Custom {
            width: f64::NAN,
            max_height: 300.0,
        },
        SizePreset::Custom {
            width: 500.0,
            max_height: f64::NAN,
        },
        SizePreset::Custom {
            width: f64::INFINITY,
            max_height: 300.0,
        },
        SizePreset::Custom {
            width: 500.0,
            max_height: f64::NEG_INFINITY,
        },
    ];
    for p in bad {
        assert_eq!(l.set_preset(p), Err(LayoutError::InvalidPreset));
    }
    assert_eq!(l.preset, SizePreset::Compact);
}

#[test]
fn set_preset_compact_spacious_ok() {
    let mut l = Layout::default();
    l.set_preset(SizePreset::Spacious).unwrap();
    assert_eq!(l.preset, SizePreset::Spacious);
    assert_eq!(l.preset.width(), 520.0);
    assert_eq!(l.preset.max_height(), 640.0);
    l.set_preset(SizePreset::Compact).unwrap();
    assert_eq!(l.preset, SizePreset::Compact);
}

#[test]
fn diameter_boundaries() {
    let mut l = Layout::default();
    l.set_button_diameter(28.0).unwrap();
    l.set_button_diameter(56.0).unwrap();
    assert_eq!(l.button_diameter, 56.0);
    for d in [27.0, 57.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(l.set_button_diameter(d), Err(LayoutError::InvalidDiameter));
    }
    assert_eq!(l.button_diameter, 56.0);
}

#[test]
fn gap_boundaries() {
    let mut l = Layout::default();
    l.set_gap(4.0).unwrap();
    l.set_gap(24.0).unwrap();
    assert_eq!(l.gap, 24.0);
    for g in [3.0, 25.0, f64::NAN, f64::INFINITY] {
        assert_eq!(l.set_gap(g), Err(LayoutError::InvalidGap));
    }
    assert_eq!(l.gap, 24.0);
}

#[test]
fn left_stack_positions_exact() {
    let mut l = Layout::default();
    l.buttons.clear();
    l.add(button("a", Side::Left, 0, ButtonAction::AppGrid))
        .unwrap();
    l.add(button("b", Side::Left, 1, ButtonAction::Timer))
        .unwrap();
    l.add(button("c", Side::Left, 2, ButtonAction::Settings))
        .unwrap();
    // d=36, g=10: x = island.x - g - d = 54; y pitch = d + g = 46.
    let rs = rects(&l, ISLAND);
    assert_eq!(
        rect_of(&rs, "a"),
        &Rect {
            x: 54.0,
            y: 200.0,
            w: 36.0,
            h: 36.0
        }
    );
    assert_eq!(
        rect_of(&rs, "b"),
        &Rect {
            x: 54.0,
            y: 246.0,
            w: 36.0,
            h: 36.0
        }
    );
    assert_eq!(
        rect_of(&rs, "c"),
        &Rect {
            x: 54.0,
            y: 292.0,
            w: 36.0,
            h: 36.0
        }
    );
}

#[test]
fn right_side_positions_exact() {
    let mut l = Layout::default();
    l.buttons.clear();
    l.add(button("r0", Side::Right, 0, ButtonAction::Settings))
        .unwrap();
    l.add(button("r1", Side::Right, 1, ButtonAction::Volume))
        .unwrap();
    // x = island.x + island.w + g = 510; y pitch = d + g = 46.
    let rs = rects(&l, ISLAND);
    assert_eq!(
        rect_of(&rs, "r0"),
        &Rect {
            x: 510.0,
            y: 200.0,
            w: 36.0,
            h: 36.0
        }
    );
    assert_eq!(
        rect_of(&rs, "r1"),
        &Rect {
            x: 510.0,
            y: 246.0,
            w: 36.0,
            h: 36.0
        }
    );
}

#[test]
fn bottom_row_packs_slots_zero_and_three() {
    let mut l = Layout::default();
    l.buttons.clear();
    l.add(button("b0", Side::Bottom, 0, ButtonAction::NowPlaying))
        .unwrap();
    l.add(button("b3", Side::Bottom, 3, ButtonAction::Timer))
        .unwrap();
    let rs = rects(&l, ISLAND);
    // two buttons, d=36 g=10: row width 2*36 + 1*10 = 82, centre = island
    // centre = 300 -> left edge 259; button pitch = d + g = 46.
    assert_eq!(
        rect_of(&rs, "b0"),
        &Rect {
            x: 259.0,
            y: 290.0,
            w: 36.0,
            h: 36.0
        }
    );
    assert_eq!(
        rect_of(&rs, "b3"),
        &Rect {
            x: 305.0,
            y: 290.0,
            w: 36.0,
            h: 36.0
        }
    );
    assert_eq!(
        rect_of(&rs, "b3").x - (rect_of(&rs, "b0").x + rect_of(&rs, "b0").w),
        10.0
    );
    assert_eq!(
        (rect_of(&rs, "b0").x + rect_of(&rs, "b3").x + 36.0) / 2.0,
        300.0
    );
}

#[test]
fn bottom_row_no_holes_for_gap_slots() {
    let mut l = Layout::default();
    l.buttons.clear();
    l.add(button("b0", Side::Bottom, 0, ButtonAction::NowPlaying))
        .unwrap();
    l.add(button("b2", Side::Bottom, 2, ButtonAction::Timer))
        .unwrap();
    // same packed positions as {0,3} -> slot 1 hole ignored
    let rs = rects(&l, ISLAND);
    assert_eq!(
        rect_of(&rs, "b0"),
        &Rect {
            x: 259.0,
            y: 290.0,
            w: 36.0,
            h: 36.0
        }
    );
    assert_eq!(
        rect_of(&rs, "b2"),
        &Rect {
            x: 305.0,
            y: 290.0,
            w: 36.0,
            h: 36.0
        }
    );
}

#[test]
fn bottom_row_centred_on_island_centre() {
    let mut l = Layout::default();
    l.buttons.clear();
    for (i, id) in ["b0", "b1", "b2"].iter().enumerate() {
        l.add(button(id, Side::Bottom, i as u8, ButtonAction::Timer))
            .unwrap();
    }
    let rs = rects(&l, ISLAND);
    // 3 buttons: row width 3*36 + 2*10 = 128, left edge 300 - 64 = 236
    assert_eq!(
        rect_of(&rs, "b0"),
        &Rect {
            x: 236.0,
            y: 290.0,
            w: 36.0,
            h: 36.0
        }
    );
    assert_eq!(
        rect_of(&rs, "b2"),
        &Rect {
            x: 328.0,
            y: 290.0,
            w: 36.0,
            h: 36.0
        }
    );
    assert_eq!(
        (rect_of(&rs, "b0").x + rect_of(&rs, "b2").x + 36.0) / 2.0,
        300.0
    );
}

/// Assert that every pair of buttons on the same side is separated by exactly
/// `expected_gap` on that side's stacking axis (vertical for left/right,
/// horizontal for bottom). `rs` must be in `button_rects` output order,
/// which is slot-ascending within each side.
fn assert_same_side_gaps(l: &Layout, island: Rect, expected_gap: f64) {
    let rs = rects(l, island);
    let side_of: HashMap<&str, Side> = l.buttons.iter().map(|b| (b.id.as_str(), b.side)).collect();
    let mut by_side: HashMap<Side, Vec<Rect>> = HashMap::new();
    for (id, r) in &rs {
        by_side.entry(side_of[id.as_str()]).or_default().push(*r);
    }
    for (side, rs) in &by_side {
        for pair in rs.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            let sep = match side {
                Side::Left | Side::Right => b.y - (a.y + a.h),
                Side::Bottom => b.x - (a.x + a.w),
            };
            assert!(
                (sep - expected_gap).abs() < 1e-9,
                "{side:?} adjacent gap {sep} != {expected_gap} (a={a:?} b={b:?})"
            );
        }
    }
}

#[test]
fn adjacent_button_gaps_equal_gap() {
    // Controller decision: consecutive buttons on the same side never touch;
    // the clear gap between every adjacent pair is exactly `gap`. Exercise
    // several diameter/gap combos across all three sides.
    for (d, g) in [(36.0, 10.0), (44.0, 8.0), (28.0, 24.0)] {
        let mut l = Layout::default();
        l.buttons.clear();
        l.set_button_diameter(d).unwrap();
        l.set_gap(g).unwrap();
        for i in 0..3u8 {
            l.add(button(
                &format!("l{i}"),
                Side::Left,
                i,
                ButtonAction::AppGrid,
            ))
            .unwrap();
            l.add(button(
                &format!("r{i}"),
                Side::Right,
                i,
                ButtonAction::Settings,
            ))
            .unwrap();
            l.add(button(
                &format!("b{i}"),
                Side::Bottom,
                i,
                ButtonAction::Timer,
            ))
            .unwrap();
        }
        assert_same_side_gaps(&l, ISLAND, g);
    }
}

#[test]
fn input_region_count_is_one_plus_buttons() {
    let l = Layout::default();
    let region = l.input_region(ISLAND).unwrap();
    assert_eq!(region.len(), 1 + l.buttons.len());
    assert_eq!(region[0], ISLAND);
    let rs = rects(&l, ISLAND);
    for (_, r) in &rs {
        assert!(region.contains(r));
    }
}

#[test]
fn bounding_box_contains_every_rect_and_island() {
    let l = Layout::default();
    let bb = l.bounding_box(ISLAND).unwrap();
    // default: left 54..90, right 510..546, bottom 282..318 (2? no — one bottom)
    // bottom row of one button: row_width 36 -> left = 300 - 18 = 282, y=290
    assert_eq!(
        bb,
        Rect {
            x: 54.0,
            y: 200.0,
            w: 492.0,
            h: 126.0
        }
    );
    let all: Vec<Rect> = std::iter::once(ISLAND)
        .chain(rects(&l, ISLAND).into_iter().map(|(_, r)| r))
        .collect();
    for r in all {
        assert!(bb.x <= r.x && bb.y <= r.y);
        assert!(r.x + r.w <= bb.x + bb.w && r.y + r.h <= bb.y + bb.h);
    }
}

#[test]
fn geometry_rejects_non_finite_island() {
    let l = Layout::default();
    let bad = [
        Rect {
            x: f64::NAN,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        },
        Rect {
            x: 0.0,
            y: f64::INFINITY,
            w: 10.0,
            h: 10.0,
        },
        Rect {
            x: 0.0,
            y: 0.0,
            w: f64::NEG_INFINITY,
            h: 10.0,
        },
        Rect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: f64::NAN,
        },
    ];
    for r in bad {
        assert!(matches!(
            l.button_rects(r),
            Err(LayoutError::NonFiniteGeometry)
        ));
        assert!(matches!(
            l.input_region(r),
            Err(LayoutError::NonFiniteGeometry)
        ));
        assert!(matches!(
            l.bounding_box(r),
            Err(LayoutError::NonFiniteGeometry)
        ));
    }
}

#[test]
fn geometry_rejects_negative_or_zero_size_island() {
    let l = Layout::default();
    let bad = [
        Rect {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 10.0,
        },
        Rect {
            x: 0.0,
            y: 0.0,
            w: -5.0,
            h: 10.0,
        },
        Rect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: -5.0,
        },
    ];
    for r in bad {
        assert!(matches!(
            l.button_rects(r),
            Err(LayoutError::NegativeSizeGeometry)
        ));
        assert!(matches!(
            l.input_region(r),
            Err(LayoutError::NegativeSizeGeometry)
        ));
        assert!(matches!(
            l.bounding_box(r),
            Err(LayoutError::NegativeSizeGeometry)
        ));
    }
}

#[test]
fn round_trip_default_layout_equal() {
    let l = Layout::default();
    let j = l.to_json();
    let (back, report) = parse(&j);
    assert_eq!(back, l);
    assert_eq!(report, LoadReport::default());
}

#[test]
fn round_trip_after_ops_equal() {
    let mut l = Layout::default();
    l.set_preset(SizePreset::Custom {
        width: 480.0,
        max_height: 560.0,
    })
    .unwrap();
    l.set_button_diameter(44.0).unwrap();
    l.set_gap(8.0).unwrap();
    l.move_to("timer", Side::Bottom, 2).unwrap();
    l.add(SideButton {
        id: "calc".into(),
        side: Side::Left,
        slot: 2,
        action: ButtonAction::OpenApp {
            app_id: "org.kde.calculator".into(),
        },
        label: Some("Calc".into()),
    })
    .unwrap();
    let j = l.to_json();
    let (back, report) = parse(&j);
    assert_eq!(back, l);
    assert_eq!(report, LoadReport::default());
}

#[test]
fn serde_camelcase_wire_shape() {
    let mut l = Layout::default();
    l.buttons.clear();
    l.add(SideButton {
        id: "a".into(),
        side: Side::Left,
        slot: 0,
        action: ButtonAction::OpenApp {
            app_id: "spotify".into(),
        },
        label: Some("S".into()),
    })
    .unwrap();
    l.add(SideButton {
        id: "b".into(),
        side: Side::Right,
        slot: 0,
        action: ButtonAction::Command { id: "kbd".into() },
        label: None,
    })
    .unwrap();
    l.add(SideButton {
        id: "c".into(),
        side: Side::Bottom,
        slot: 0,
        action: ButtonAction::NowPlaying,
        label: None,
    })
    .unwrap();
    l.add(SideButton {
        id: "d".into(),
        side: Side::Bottom,
        slot: 1,
        action: ButtonAction::AppGrid,
        label: None,
    })
    .unwrap();
    l.set_preset(SizePreset::Custom {
        width: 400.0,
        max_height: 500.0,
    })
    .unwrap();
    let j = l.to_json();
    let v: serde_json::Value = serde_json::from_str(&j).unwrap();
    assert_eq!(v["version"], 1);
    assert_eq!(v["preset"]["custom"]["width"], 400.0);
    assert_eq!(v["preset"]["custom"]["maxHeight"], 500.0);
    assert_eq!(v["buttonDiameter"], 36.0);
    assert_eq!(v["gap"], 10.0);
    assert_eq!(v["buttons"][0]["id"], "a");
    assert_eq!(v["buttons"][0]["side"], "left");
    assert_eq!(v["buttons"][0]["slot"], 0);
    assert_eq!(v["buttons"][0]["label"], "S");
    assert_eq!(v["buttons"][0]["action"]["openApp"]["appId"], "spotify");
    assert_eq!(v["buttons"][1]["action"]["command"]["id"], "kbd");
    assert_eq!(v["buttons"][2]["action"], "nowPlaying");
    assert_eq!(v["buttons"][3]["action"], "appGrid");
    assert!(v.get("preset").unwrap().get("custom").is_some());
}

#[test]
fn to_json_preset_unit_variants() {
    let mut l = Layout::default();
    assert!(l.to_json().contains("\"preset\":\"compact\""));
    l.set_preset(SizePreset::Spacious).unwrap();
    assert!(l.to_json().contains("\"preset\":\"spacious\""));
}

#[test]
fn load_skips_each_invalid_kind_and_counts() {
    let j = r#"{
      "version": 1,
      "buttons": [
        42,
        {"id": 5},
        {"id": "", "side": "left", "slot": 0, "action": "appGrid"},
        {"id": "bad id", "side": "left", "slot": 0, "action": "appGrid"},
        {"id": "dup", "side": "left", "slot": 0, "action": "appGrid"},
        {"id": "dup", "side": "left", "slot": 1, "action": "appGrid"},
        {"id": "os", "side": "left", "slot": 3, "action": "appGrid"},
        {"id": "occ", "side": "left", "slot": 0, "action": "appGrid"},
        {"id": "act", "side": "right", "slot": 0, "action": {"openApp":{"appId":"bad id"}}},
        {"id": "lab", "side": "right", "slot": 1, "action": "appGrid", "label": "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"},
        {"id": "lbltype", "side": "right", "slot": 1, "action": "appGrid", "label": 7},
        {"id": "sidebad", "side": "middle", "slot": 0, "action": "appGrid"},
        {"id": "slotfloat", "side": "bottom", "slot": 2.0, "action": "appGrid"},
        {"id": "ok", "side": "bottom", "slot": 0, "action": "nowPlaying"},
        {"id": "ok2", "side": "bottom", "slot": 1, "action": "timer", "label": "x"}
      ]
    }"#;
    let (l, report) = parse(j);
    assert_eq!(report.skipped, 12);
    assert_eq!(l.buttons.len(), 3);
    assert_eq!(ids(&l), vec!["dup", "ok", "ok2"]);
    assert_eq!(l.buttons[0].slot, 0);
    assert_eq!(l.buttons[1].action, ButtonAction::NowPlaying);
    assert_eq!(l.buttons[2].label.as_deref(), Some("x"));
}

#[test]
fn load_first_occurrence_wins() {
    let j = r#"{
      "version": 1,
      "buttons": [
        {"id": "same", "side": "left", "slot": 0, "action": "appGrid"},
        {"id": "same", "side": "bottom", "slot": 2, "action": "timer"}
      ]
    }"#;
    let (l, report) = parse(j);
    assert_eq!(report.skipped, 1);
    assert_eq!(l.buttons.len(), 1);
    assert_eq!(l.buttons[0].side, Side::Left);
    assert_eq!(l.buttons[0].slot, 0);
    assert_eq!(l.buttons[0].action, ButtonAction::AppGrid);
}

#[test]
fn load_invalid_preset_diameter_gap_fall_back_and_count() {
    let j = r#"{
      "version": 1,
      "preset": {"custom": {"width": 1, "maxHeight": 2}},
      "buttonDiameter": 99,
      "gap": 0
    }"#;
    let (l, report) = parse(j);
    assert_eq!(l.preset, SizePreset::Compact);
    assert_eq!(l.button_diameter, 36.0);
    assert_eq!(l.gap, 10.0);
    assert!(report.preset_fallback);
    assert!(report.diameter_fallback);
    assert!(report.gap_fallback);

    let j2 = r#"{"version":1,"preset":"huge","buttonDiameter":"big","gap":[1]}"#;
    let (l2, report2) = parse(j2);
    assert_eq!(l2.preset, SizePreset::Compact);
    assert_eq!(l2.button_diameter, 36.0);
    assert_eq!(l2.gap, 10.0);
    assert!(report2.preset_fallback);
    assert!(report2.diameter_fallback);
    assert!(report2.gap_fallback);
}

#[test]
fn load_fatal_errors() {
    let cases = [
        ("this is not json", LayoutError::InvalidJson),
        ("{", LayoutError::InvalidJson),
        ("[1,2]", LayoutError::NotAnObject),
        ("\"hello\"", LayoutError::NotAnObject),
        ("42", LayoutError::NotAnObject),
        ("{\"version\":2}", LayoutError::InvalidVersion),
        ("{\"version\":\"1\"}", LayoutError::InvalidVersion),
        ("{\"version\":1.0}", LayoutError::InvalidVersion),
        ("{}", LayoutError::InvalidVersion),
    ];
    for (j, expected) in cases {
        assert_eq!(Layout::from_json(j), Err(expected), "case: {j}");
    }
}

#[test]
fn load_unknown_fields_ignored() {
    let j = r#"{
      "version": 1,
      "whatever": {"nested": true},
      "buttonDiameter": 40,
      "buttons": [
        {"id": "a", "side": "left", "slot": 0, "action": "appGrid", "zzz": 1}
      ]
    }"#;
    let (l, report) = parse(j);
    assert_eq!(l.button_diameter, 40.0);
    assert_eq!(l.buttons.len(), 1);
    assert_eq!(report, LoadReport::default());
}

#[test]
fn load_missing_fields_default_without_counts() {
    let (l, report) = parse("{\"version\":1}");
    assert_eq!(l.preset, SizePreset::Compact);
    assert_eq!(l.button_diameter, 36.0);
    assert_eq!(l.gap, 10.0);
    assert!(l.buttons.is_empty());
    assert_eq!(report, LoadReport::default());

    let (l2, report2) = parse("{\"version\":1,\"buttons\":[]}");
    assert!(l2.buttons.is_empty());
    assert_eq!(report2, LoadReport::default());
}

#[test]
fn load_buttons_non_array_null_tolerated() {
    let (l, report) = parse("{\"version\":1,\"buttons\":null}");
    assert!(l.buttons.is_empty());
    assert_eq!(report.skipped, 0);

    let (l2, report2) = parse("{\"version\":1,\"buttons\":42}");
    assert!(l2.buttons.is_empty());
    assert_eq!(report2.skipped, 1);

    let (l3, report3) = parse("{\"version\":1,\"buttons\":\"x\"}");
    assert!(l3.buttons.is_empty());
    assert_eq!(report3.skipped, 1);
}

#[test]
fn load_custom_preset_valid_dimensions_kept() {
    let j = r#"{"version":1,"preset":{"custom":{"width":300,"maxHeight":900}}}"#;
    let (l, report) = parse(j);
    assert_eq!(
        l.preset,
        SizePreset::Custom {
            width: 300.0,
            max_height: 900.0
        }
    );
    assert!(!report.preset_fallback);
}
