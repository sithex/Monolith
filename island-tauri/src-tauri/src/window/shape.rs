//! Rounded-rect → X11 input-region rows. Exact port of `island-rust/src/host.rs::shape_rows`
//! (same math, same bounds validation, same 0.5–4.0 scale range) plus the pure helpers the
//! controller needs (`normalize_rows`, `union_rows`). No platform code here.

use crate::contract::island::{IslandMode, RoundedRect};
use crate::error::IslandError;
use std::collections::BTreeMap;

/// Logical island window size (tauri.conf.json). The 1 px slack in `shape_rows` is inherited.
pub const WINDOW_WIDTH: f64 = 1180.;
pub const WINDOW_HEIGHT: f64 = 820.;
/// Largest window any page uses: the island overlay (`window::ISLAND_OVERLAY_*`, clamped to
/// the screen). Region rects are validated against this rather than the 1180×820 default.
pub const MAX_WINDOW_WIDTH: f64 = 1960.;
pub const MAX_WINDOW_HEIGHT: f64 = 1360.;

/// One input-region rectangle in physical, window-relative pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ShapeRow {
    pub x: i16,
    pub y: i16,
    pub width: u16,
    pub height: u16,
}

pub fn shape_rows(
    rect: RoundedRect,
    mode: IslandMode,
    scale: f64,
    native: bool,
) -> Result<Vec<ShapeRow>, IslandError> {
    if ![rect.x, rect.y, rect.width, rect.height, rect.radius, scale]
        .iter()
        .all(|n| n.is_finite())
        || rect.x < 0.
        || rect.y < 0.
        || rect.width < 1.
        || rect.height < 1.
        || rect.x + rect.width > MAX_WINDOW_WIDTH + 1.
        || rect.y + rect.height > MAX_WINDOW_HEIGHT + 1.
        || rect.radius < 0.
        || rect.radius > 100.
        || !(0.5..=4.).contains(&scale)
    {
        return Err(IslandError::invalid("Invalid overlay layout"));
    }
    if native && mode == IslandMode::Still {
        return Ok(vec![]);
    }
    let x = (rect.x * scale).floor() as i16;
    let y = (rect.y * scale).floor() as i16;
    let w = (rect.width * scale).ceil() as u16;
    let h = (rect.height * scale).ceil() as u16;
    let radius = (rect.radius * scale)
        .round()
        .min((h / 2) as f64)
        .min((w / 2) as f64);
    Ok((0..h)
        .map(|row| {
            let d = if (row as f64) < radius {
                radius - row as f64 - 0.5
            } else if row as f64 >= h as f64 - radius {
                row as f64 - (h as f64 - radius) + 0.5
            } else {
                0.
            };
            let inset = if d > 0. {
                (radius - (radius * radius - d * d).sqrt()).ceil() as u16
            } else {
                0
            };
            ShapeRow {
                x: x + inset as i16,
                y: y + row as i16,
                width: (w - 2 * inset).max(1),
                height: 1,
            }
        })
        .collect())
}

/// Rows for every rect of a report, as one canonical region.
pub fn region_rows(
    rects: &[RoundedRect],
    mode: IslandMode,
    scale: f64,
    native: bool,
) -> Result<Vec<ShapeRow>, IslandError> {
    let mut all = Vec::new();
    for rect in rects {
        all.extend(shape_rows(*rect, mode, scale, native)?);
    }
    Ok(normalize_rows(&all))
}

fn saturate_i16(value: i32) -> i16 {
    i16::try_from(value).unwrap_or(if value < 0 { i16::MIN } else { i16::MAX })
}

/// Canonical pixel coverage: height-1 rows, sorted by `y` then `x`, touching/overlapping
/// intervals merged. The X server coalesces adjacent equal bands, so a readback is only
/// comparable to what was applied after both sides are normalised.
pub fn normalize_rows(rows: &[ShapeRow]) -> Vec<ShapeRow> {
    let mut by_y: BTreeMap<i32, Vec<(i32, i32)>> = BTreeMap::new();
    for row in rows.iter().filter(|r| r.width > 0 && r.height > 0) {
        let (x, w) = (i32::from(row.x), i32::from(row.width));
        for dy in 0..i32::from(row.height) {
            by_y.entry(i32::from(row.y) + dy)
                .or_default()
                .push((x, x + w));
        }
    }
    let mut out = Vec::new();
    for (y, mut spans) in by_y {
        spans.sort_unstable();
        let mut merged: Vec<(i32, i32)> = Vec::new();
        for (start, end) in spans {
            match merged.last_mut() {
                Some(last) if start <= last.1 => last.1 = last.1.max(end),
                _ => merged.push((start, end)),
            }
        }
        for (start, end) in merged {
            out.push(ShapeRow {
                x: saturate_i16(start),
                y: saturate_i16(y),
                width: u16::try_from(end - start).unwrap_or(u16::MAX),
                height: 1,
            });
        }
    }
    out
}

pub fn union_rows(a: &[ShapeRow], b: &[ShapeRow]) -> Vec<ShapeRow> {
    let mut all = a.to_vec();
    all.extend_from_slice(b);
    normalize_rows(&all)
}

/// Exclude the native QML MouseArea's rectangular bounds, including rounded corners.
/// Outward physical rounding keeps fractional-scale edge pixels owned by the panel too.
pub fn subtract_rect(
    rows: &[ShapeRow],
    rect: RoundedRect,
    scale: f64,
) -> Result<Vec<ShapeRow>, IslandError> {
    shape_rows(rect, IslandMode::Large, scale, false)?;
    let left = (rect.x * scale).floor() as i32;
    let right = ((rect.x + rect.width) * scale).ceil() as i32;
    let top = (rect.y * scale).floor() as i32;
    let bottom = ((rect.y + rect.height) * scale).ceil() as i32;
    let mut result = Vec::new();
    for row in normalize_rows(rows) {
        let x = i32::from(row.x);
        let end = x + i32::from(row.width);
        let y = i32::from(row.y);
        if y < top || y >= bottom || end <= left || x >= right {
            result.push(row);
        } else {
            for (start, stop) in [(x, end.min(left)), (x.max(right), end)] {
                if stop > start {
                    result.push(ShapeRow {
                        x: start as i16,
                        y: row.y,
                        width: (stop - start) as u16,
                        height: 1,
                    });
                }
            }
        }
    }
    Ok(result)
}

/// Does the region cover pixel `(px, py)`?
pub fn covers(rows: &[ShapeRow], px: i32, py: i32) -> bool {
    rows.iter().any(|r| {
        let (x, y) = (i32::from(r.x), i32::from(r.y));
        px >= x && px < x + i32::from(r.width) && py >= y && py < y + i32::from(r.height)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- ported from island-rust/src/host.rs (legacy mode names mapped to canonical) ----
    #[test]
    fn native_still_has_no_input_but_hover_and_scaled_geometry_do() {
        let rect = RoundedRect {
            x: 10.25,
            y: 20.25,
            width: 100.1,
            height: 40.1,
            radius: 0.,
        };
        assert!(shape_rows(rect, IslandMode::Still, 2., true)
            .unwrap()
            .is_empty());
        let rows = shape_rows(rect, IslandMode::Hover, 2., true).unwrap();
        assert_eq!(rows.len(), 81);
        assert_eq!(
            rows[0],
            ShapeRow {
                x: 20,
                y: 40,
                width: 201,
                height: 1
            }
        );
        for scale in [f64::NAN, f64::INFINITY, 0., 5.] {
            assert!(shape_rows(rect, IslandMode::Large, scale, false).is_err());
        }
        for bad in [
            RoundedRect { x: -1., ..rect },
            RoundedRect {
                width: 2000.,
                ..rect
            },
            RoundedRect {
                radius: 101.,
                ..rect
            },
        ] {
            assert!(shape_rows(bad, IslandMode::Large, 1., false).is_err());
        }
    }

    #[test]
    fn rounded_shape_covers_only_measured_surface() {
        let rows = shape_rows(
            RoundedRect {
                x: 10.,
                y: 20.,
                width: 100.,
                height: 40.,
                radius: 10.,
            },
            IslandMode::Large,
            1.,
            false,
        )
        .unwrap();
        assert_eq!(rows.len(), 40);
        assert_eq!(
            rows[20],
            ShapeRow {
                x: 10,
                y: 40,
                width: 100,
                height: 1
            }
        );
        assert!(rows[0].x > 10);
        assert!(rows
            .iter()
            .all(|r| r.x >= 10 && r.x as u16 + r.width <= 110));
    }

    // ---- new helpers ----
    fn rect(x: f64, y: f64, w: f64, h: f64, r: f64) -> RoundedRect {
        RoundedRect {
            x,
            y,
            width: w,
            height: h,
            radius: r,
        }
    }

    #[test]
    fn corners_are_excluded_and_edges_included() {
        let rows =
            shape_rows(rect(10., 20., 100., 40., 10.), IslandMode::Hover, 1., false).unwrap();
        assert!(
            !covers(&rows, 10, 20),
            "top-left corner pixel must be outside"
        );
        assert!(
            !covers(&rows, 109, 59),
            "bottom-right corner pixel must be outside"
        );
        assert!(covers(&rows, 60, 20) && covers(&rows, 10, 40) && covers(&rows, 109, 40));
        assert!(!covers(&rows, 9, 40) && !covers(&rows, 110, 40) && !covers(&rows, 60, 19));
    }

    #[test]
    fn normalize_merges_bands_the_way_the_x_server_does() {
        let rows = shape_rows(rect(5., 5., 30., 12., 4.), IslandMode::Large, 1., false).unwrap();
        let merged = vec![
            // a tall rect covering the straight middle band, as an X server would report it
            ShapeRow {
                x: 5,
                y: 9,
                width: 30,
                height: 4,
            },
        ];
        let from_rows = normalize_rows(&rows);
        let tall = normalize_rows(&merged);
        assert!(tall.iter().all(|t| from_rows.contains(t)));
        // idempotent, order independent
        assert_eq!(normalize_rows(&from_rows), from_rows);
        let mut reversed = rows.clone();
        reversed.reverse();
        assert_eq!(normalize_rows(&reversed), from_rows);
    }

    #[test]
    fn union_covers_both_and_adjacent_spans_merge() {
        let a = [ShapeRow {
            x: 0,
            y: 0,
            width: 10,
            height: 1,
        }];
        let b = [
            ShapeRow {
                x: 10,
                y: 0,
                width: 5,
                height: 1,
            },
            ShapeRow {
                x: 0,
                y: 1,
                width: 3,
                height: 1,
            },
        ];
        let u = union_rows(&a, &b);
        assert_eq!(
            u,
            vec![
                ShapeRow {
                    x: 0,
                    y: 0,
                    width: 15,
                    height: 1
                },
                ShapeRow {
                    x: 0,
                    y: 1,
                    width: 3,
                    height: 1
                },
            ]
        );
    }

    #[test]
    fn region_rows_unions_multiple_rects_and_validates_all() {
        let rows = region_rows(
            &[rect(0., 0., 10., 10., 0.), rect(20., 0., 10., 10., 0.)],
            IslandMode::Hover,
            1.,
            false,
        )
        .unwrap();
        assert!(covers(&rows, 5, 5) && covers(&rows, 25, 5) && !covers(&rows, 15, 5));
        assert!(region_rows(
            &[rect(0., 0., 10., 10., 0.), rect(-1., 0., 5., 5., 0.)],
            IslandMode::Hover,
            1.,
            false
        )
        .is_err());
    }
}
