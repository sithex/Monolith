//! Generic placement (SPEC §2.3 step 3, non-Plasma branch). The Plasma anchor branch
//! (`integrations::plasma::anchor`) is Phase 3.

use crate::window::shape::WINDOW_WIDTH;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogicalOrigin {
    pub x: f64,
    pub y: f64,
}

/// Top-centre of the work area, clamped so the window stays inside it when it fits.
/// All inputs are physical pixels except the logical window size; output is logical.
pub fn generic_origin(work_area: PhysRect, scale: f64) -> LogicalOrigin {
    let scale = if scale.is_finite() && scale > 0. {
        scale
    } else {
        1.
    };
    let free_w = f64::from(work_area.width) - WINDOW_WIDTH * scale;
    let x = f64::from(work_area.x) + (free_w / 2.).max(0.);
    // Flush with the top edge of the work area (below any panel); never vertically centred.
    let y = f64::from(work_area.y);
    LogicalOrigin {
        x: (x / scale).round(),
        y: (y / scale).round(),
    }
}

/// Island-page-only overlap placement. Earlier pages continue to use `below_panel` unchanged.
pub fn over_panel(
    anchor: &crate::integrations::plasma::anchor::Anchor,
    width: u32,
    height: u32,
) -> Result<crate::integrations::plasma::anchor::Placement, crate::error::IslandError> {
    use crate::error::IslandError;
    let screen = anchor.screen.validate()?;
    let pill = anchor.pill.validate()?;
    if width == 0 || height == 0 || !anchor.top.is_finite() || !anchor.center.is_finite() {
        return Err(IslandError::invalid("Invalid overlap placement"));
    }
    let left = screen.x.ceil();
    let right = (screen.x + screen.width).floor();
    let top = anchor.top.floor().max(screen.y.ceil());
    let available = (screen.y + screen.height).floor() - top;
    if available < 1.0 || right - left < 1.0 {
        return Err(IslandError::invalid("No overlap viewport available"));
    }
    let width = width.min((right - left) as u32);
    let height = height.min(available as u32);
    let x = (anchor.center - f64::from(width) / 2.0)
        .round()
        .clamp(left, right - f64::from(width));
    if pill.x < x
        || pill.y < top
        || pill.x + pill.width > x + f64::from(width)
        || pill.y + pill.height > top + f64::from(height)
    {
        return Err(IslandError::invalid(
            "Native pill does not fit overlap viewport",
        ));
    }
    Ok(crate::integrations::plasma::anchor::Placement {
        x: x as i32,
        y: top as i32,
        width,
        height,
    })
}

/// Origin for the island window on the primary monitor.
pub fn resolve(app: &tauri::AppHandle) -> LogicalOrigin {
    let monitor = app.primary_monitor().ok().flatten().or_else(|| {
        app.available_monitors()
            .ok()
            .and_then(|m| m.into_iter().next())
    });
    match monitor {
        Some(monitor) => {
            let area = monitor.work_area();
            generic_origin(
                PhysRect {
                    x: area.position.x,
                    y: area.position.y,
                    width: area.size.width,
                    height: area.size.height,
                },
                monitor.scale_factor(),
            )
        }
        None => {
            tracing::warn!("no monitor reported; placing the island at (0, 0)");
            LogicalOrigin { x: 0., y: 0. }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centres_horizontally_at_the_top_of_the_work_area() {
        let o = generic_origin(
            PhysRect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            },
            1.,
        );
        assert_eq!(o, LogicalOrigin { x: 370., y: 0. });
    }

    #[test]
    fn respects_work_area_offset_of_a_secondary_monitor_and_panel() {
        let o = generic_origin(
            PhysRect {
                x: 1920,
                y: 32,
                width: 2560,
                height: 1408,
            },
            1.,
        );
        assert_eq!(
            o,
            LogicalOrigin {
                x: 1920. + 690.,
                y: 32.
            }
        );
    }

    #[test]
    fn scales_to_logical_coordinates() {
        let o = generic_origin(
            PhysRect {
                x: 0,
                y: 0,
                width: 3840,
                height: 2160,
            },
            2.,
        );
        // physical: (3840 - 1180*2) / 2 = 740 px  →  logical 370
        assert_eq!(o, LogicalOrigin { x: 370., y: 0. });
    }

    #[test]
    fn never_goes_left_of_the_work_area_on_small_screens() {
        let o = generic_origin(
            PhysRect {
                x: 100,
                y: 0,
                width: 800,
                height: 600,
            },
            1.,
        );
        assert_eq!(o.x, 100.);
    }

    #[test]
    fn bad_scale_falls_back_to_one() {
        let a = generic_origin(
            PhysRect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            },
            f64::NAN,
        );
        assert_eq!(a.x, 370.);
    }
}
