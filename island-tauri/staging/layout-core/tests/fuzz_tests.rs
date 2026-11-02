#![allow(clippy::float_cmp)]

use layout_core::{ButtonAction, Layout, LayoutError, Rect, Side, SideButton, SizePreset};
use std::collections::HashMap;

/// Hand-rolled LCG (MMIX constants), fixed seed, no external deps.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            0
        } else {
            self.next() % n
        }
    }

    fn coin(&mut self) -> bool {
        self.below(2) == 0
    }

    fn pick(&mut self, chars: &[char]) -> char {
        chars[self.below(chars.len() as u64) as usize]
    }
}

fn random_f64(lcg: &mut Lcg) -> f64 {
    match lcg.below(25) {
        0 => f64::NAN,
        1 => f64::INFINITY,
        2 => f64::NEG_INFINITY,
        _ => {
            let base = lcg.below(2000) as f64 - 500.0;
            let frac = lcg.below(100) as f64 / 97.0;
            base + frac
        }
    }
}

fn random_side(lcg: &mut Lcg) -> Side {
    [Side::Left, Side::Right, Side::Bottom][lcg.below(3) as usize]
}

fn random_slot(lcg: &mut Lcg, side: Side) -> u8 {
    let cap = side.capacity();
    if lcg.coin() {
        lcg.below(cap as u64 + 3) as u8
    } else {
        lcg.below(cap as u64) as u8
    }
}

fn random_id(lcg: &mut Lcg) -> String {
    let good = ['a', 'b', 'c', '0', '1', '-', '_', 'Z', '.'];
    let bad = [' ', '/', 'é', '#', '\n'];
    let len = lcg.below(70);
    let mut s = String::new();
    for _ in 0..len {
        let c = if lcg.coin() {
            lcg.pick(&good)
        } else {
            lcg.pick(&bad)
        };
        s.push(c);
    }
    s
}

fn random_label(lcg: &mut Lcg) -> Option<String> {
    if lcg.coin() {
        return None;
    }
    let len = lcg.below(40);
    let mut s = String::new();
    for _ in 0..len {
        s.push(lcg.pick(&['a', 'b', ' ', 'é', '中', '_', '-']));
    }
    Some(s)
}

fn random_action(lcg: &mut Lcg) -> ButtonAction {
    match lcg.below(9) {
        0 => ButtonAction::AppGrid,
        1 => ButtonAction::Settings,
        2 => ButtonAction::Volume,
        3 => ButtonAction::NowPlaying,
        4 => ButtonAction::Timer,
        5 => ButtonAction::OpenApp {
            app_id: random_id(lcg),
        },
        6 => ButtonAction::Command { id: random_id(lcg) },
        7 => ButtonAction::OpenApp {
            app_id: "has space".to_string(),
        },
        _ => ButtonAction::Command { id: String::new() },
    }
}

fn random_button(lcg: &mut Lcg) -> SideButton {
    let side = random_side(lcg);
    SideButton {
        id: random_id(lcg),
        side,
        slot: random_slot(lcg, side),
        action: random_action(lcg),
        label: random_label(lcg),
    }
}

fn random_island(lcg: &mut Lcg) -> Rect {
    if lcg.below(6) == 0 {
        Rect {
            x: random_f64(lcg),
            y: random_f64(lcg),
            w: random_f64(lcg),
            h: random_f64(lcg),
        }
    } else {
        Rect {
            x: lcg.below(400) as f64,
            y: lcg.below(400) as f64,
            w: (100 + lcg.below(600)) as f64,
            h: (60 + lcg.below(400)) as f64,
        }
    }
}

fn id_ok(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

fn check_invariants(l: &Layout) {
    let mut ids = std::collections::HashSet::new();
    let mut pairs = std::collections::HashSet::new();
    for b in &l.buttons {
        assert!(id_ok(&b.id), "invalid id loaded: {:?}", b.id);
        assert!(ids.insert(b.id.clone()), "duplicate id {}", b.id);
        assert!(
            pairs.insert((b.side, b.slot)),
            "duplicate pair {:?} {}",
            b.side,
            b.slot
        );
        assert!(b.slot < b.side.capacity(), "slot out of range");
        if let Some(label) = &b.label {
            assert!(label.chars().count() <= 32, "label too long");
        }
    }
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

fn contains(outer: Rect, inner: Rect) -> bool {
    // The bounding box stores w/h as (max - min); rebuilding the far edge
    // reintroduces float roundoff (~1e-14 here), so allow a tiny tolerance.
    const EPS: f64 = 1e-9;
    outer.x - EPS <= inner.x
        && outer.y - EPS <= inner.y
        && inner.x + inner.w <= outer.x + outer.w + EPS
        && inner.y + inner.h <= outer.y + outer.h + EPS
}

fn random_action_json(lcg: &mut Lcg) -> String {
    match lcg.below(8) {
        0 => "\"appGrid\"".to_string(),
        1 => "\"settings\"".to_string(),
        2 => "\"volume\"".to_string(),
        3 => "\"nowPlaying\"".to_string(),
        4 => "\"timer\"".to_string(),
        5 => format!("{{\"openApp\":{{\"appId\":\"{}\"}}}}", random_id(lcg)),
        6 => format!("{{\"command\":{{\"id\":\"{}\"}}}}", random_id(lcg)),
        _ => "99".to_string(),
    }
}

fn random_button_json(lcg: &mut Lcg, i: u64) -> String {
    match lcg.below(6) {
        0 => return format!("{}", lcg.below(5)),
        1 => return format!("{{\"id\":{}}}", lcg.below(3)),
        _ => {}
    }
    let id = if lcg.coin() {
        format!("btn{i}")
    } else {
        random_id(lcg)
    };
    let side = if lcg.coin() {
        ["left", "right", "bottom"][lcg.below(3) as usize]
    } else {
        "middle"
    };
    let slot = lcg.below(8);
    let action = random_action_json(lcg);
    let label = match lcg.below(4) {
        0 => String::from("null"),
        1 => format!("\"{}\"", random_short_label(lcg)),
        2 => format!("\"{}\"", random_long_label(lcg)),
        _ => String::from("42"),
    };
    format!(
        "{{\"id\":\"{id}\",\"side\":\"{side}\",\"slot\":{slot},\"action\":{action},\"label\":{label}}}"
    )
}

fn random_short_label(lcg: &mut Lcg) -> String {
    let len = lcg.below(33);
    let mut s = String::new();
    for _ in 0..len {
        s.push(lcg.pick(&['a', 'b', ' ']));
    }
    s
}

fn random_long_label(lcg: &mut Lcg) -> String {
    let len = 33 + lcg.below(20);
    "z".repeat(len as usize)
}

fn random_json(lcg: &mut Lcg) -> String {
    match lcg.below(12) {
        0 => return String::from("not json at all {{"),
        1 => return format!("[1, 2, {}]", lcg.next()),
        2 => return format!("{{\"version\": {}}}", lcg.below(5)),
        _ => {}
    }
    let mut s = String::from("{\"version\":1");
    match lcg.below(4) {
        0 => s.push_str(",\"preset\":\"compact\""),
        1 => s.push_str(",\"preset\":\"spacious\""),
        2 => {
            s.push_str(",\"preset\":{\"custom\":{\"width\":");
            s.push_str(&format!("{}", random_f64(lcg)));
            s.push_str(",\"maxHeight\":");
            s.push_str(&format!("{}", random_f64(lcg)));
            s.push_str("}}");
        }
        _ => {}
    }
    if lcg.coin() {
        s.push_str(&format!(",\"buttonDiameter\":{}", random_f64(lcg)));
    }
    if lcg.coin() {
        s.push_str(&format!(",\"gap\":{}", random_f64(lcg)));
    }
    let n = lcg.below(8);
    if lcg.coin() {
        s.push_str(",\"buttons\":[");
        for i in 0..n {
            if i > 0 {
                s.push(',');
            }
            s.push_str(&random_button_json(lcg, i));
        }
        s.push(']');
    }
    s.push('}');
    s
}

#[test]
fn fuzz_layout_never_panics_and_keeps_invariants() {
    let mut lcg = Lcg(0xC0FFEE_DEADBEEF);
    let mut layout = Layout::default();
    let mut iteration = 0u32;
    loop {
        iteration += 1;
        if iteration > 20_000 {
            break;
        }
        match lcg.below(10) {
            0 => {
                let b = random_button(&mut lcg);
                let _ = layout.add(b);
            }
            1 => {
                let id = random_id(&mut lcg);
                let _ = layout.remove(&id);
            }
            2 => {
                let id = if lcg.coin() && !layout.buttons.is_empty() {
                    layout.buttons[lcg.below(layout.buttons.len() as u64) as usize]
                        .id
                        .clone()
                } else {
                    random_id(&mut lcg)
                };
                let side = random_side(&mut lcg);
                let slot = random_slot(&mut lcg, side);
                let _ = layout.move_to(&id, side, slot);
            }
            3 => {
                let id = if lcg.coin() && !layout.buttons.is_empty() {
                    layout.buttons[lcg.below(layout.buttons.len() as u64) as usize]
                        .id
                        .clone()
                } else {
                    random_id(&mut lcg)
                };
                let action = random_action(&mut lcg);
                let label = random_label(&mut lcg);
                let _ = layout.edit(&id, action, label);
            }
            4 => {
                let p = match lcg.below(4) {
                    0 => SizePreset::Compact,
                    1 => SizePreset::Spacious,
                    2 => SizePreset::Custom {
                        width: random_f64(&mut lcg),
                        max_height: random_f64(&mut lcg),
                    },
                    _ => SizePreset::Custom {
                        width: 300.0 + lcg.below(601) as f64,
                        max_height: 240.0 + lcg.below(661) as f64,
                    },
                };
                let _ = layout.set_preset(p);
            }
            5 => {
                let _ = layout.set_button_diameter(random_f64(&mut lcg));
            }
            6 => {
                let _ = layout.set_gap(random_f64(&mut lcg));
            }
            7 => {
                let j = random_json(&mut lcg);
                if let Ok((loaded, _)) = Layout::from_json(&j) {
                    check_invariants(&loaded);
                }
            }
            _ => {}
        }
        check_invariants(&layout);

        let island = random_island(&mut lcg);
        match layout.button_rects(island) {
            Ok(rects) => {
                for (_, r) in &rects {
                    assert!(r.x.is_finite() && r.y.is_finite());
                    assert!(r.w.is_finite() && r.h.is_finite());
                }
                // Controller decision: same-side buttons never touch. Every
                // pair on a side is separated by at least `gap` on that
                // side's stacking axis (vertical for left/right, horizontal
                // for bottom), which also guarantees same-side disjointness.
                // Cross-side rects may legitimately overlap for
                // extreme-but-valid parameter combos (a wide bottom row over
                // a narrow island reaches the left/right columns while tall
                // stacks reach down to row height), so the disjointness
                // guarantee is per-side, not global.
                let side_of: HashMap<&str, Side> = layout
                    .buttons
                    .iter()
                    .map(|b| (b.id.as_str(), b.side))
                    .collect();
                let mut by_side: HashMap<Side, Vec<Rect>> = HashMap::new();
                for (id, r) in &rects {
                    by_side.entry(side_of[id.as_str()]).or_default().push(*r);
                }
                for (side, rs) in &by_side {
                    for i in 0..rs.len() {
                        for j in i + 1..rs.len() {
                            let (a, b) = (&rs[i], &rs[j]);
                            assert!(
                                !overlaps(*a, *b),
                                "{side:?} buttons overlap (a={a:?} b={b:?})"
                            );
                            let sep = match side {
                                Side::Left | Side::Right => b.y - (a.y + a.h),
                                Side::Bottom => b.x - (a.x + a.w),
                            };
                            assert!(
                                sep >= layout.gap - 1e-9,
                                "{side:?} pair separation {sep} < gap {} (a={a:?} b={b:?})",
                                layout.gap
                            );
                        }
                    }
                }
                for (_, r) in &rects {
                    assert!(!overlaps(island, *r), "button overlaps island");
                }
                let bb = layout.bounding_box(island).unwrap();
                assert!(contains(bb, island), "bounding box misses the island");
                for (_, r) in &rects {
                    assert!(
                        contains(bb, *r),
                        "bounding box misses a button: island={island:?} bb={bb:?} r={r:?}"
                    );
                }
            }
            Err(e) => {
                assert!(
                    matches!(
                        e,
                        LayoutError::NonFiniteGeometry | LayoutError::NegativeSizeGeometry
                    ),
                    "unexpected geometry error: {e}"
                );
            }
        }
    }
}

#[test]
fn fuzz_from_json_never_panics() {
    let mut lcg = Lcg(0xD00D_F00D_1234_5678);
    for _ in 0..10_000 {
        let j = random_json(&mut lcg);
        if let Ok((loaded, _)) = Layout::from_json(&j) {
            check_invariants(&loaded);
        }
    }
}
