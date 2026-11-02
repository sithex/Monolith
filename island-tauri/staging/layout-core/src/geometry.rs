use crate::error::LayoutError;
use crate::model::{Layout, Side};

/// A rectangle in logical px (`f64`). Width/height are positive for geometry
/// inputs; values must be finite.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Layout {
    fn validate_island(&self, island: Rect) -> Result<(), LayoutError> {
        if !island.x.is_finite()
            || !island.y.is_finite()
            || !island.w.is_finite()
            || !island.h.is_finite()
        {
            return Err(LayoutError::NonFiniteGeometry);
        }
        if island.w <= 0.0 || island.h <= 0.0 {
            return Err(LayoutError::NegativeSizeGeometry);
        }
        if !self.button_diameter.is_finite() || !self.gap.is_finite() {
            return Err(LayoutError::NonFiniteGeometry);
        }
        Ok(())
    }

    /// Compute each side button's rect around `island`. Left buttons stack
    /// vertically from `island.y` downward, centred at `island.x - gap -
    /// d/2`, with consecutive buttons spaced by `gap`; Right mirrors at
    /// `island.x + island.w + gap + d/2` with the same spacing; Bottom
    /// buttons sit in a row centred under the island at `island.y +
    /// island.h + gap`, `count*d + (count-1)*gap` wide with consecutive
    /// buttons also spaced by `gap`. Within a side, occupied slots are
    /// packed in slot order with no holes.
    pub fn button_rects(&self, island: Rect) -> Result<Vec<(String, Rect)>, LayoutError> {
        self.validate_island(island)?;
        let d = self.button_diameter;
        let g = self.gap;
        let mut out = Vec::new();

        let mut left: Vec<_> = self
            .buttons
            .iter()
            .filter(|b| b.side == Side::Left)
            .collect();
        left.sort_by_key(|b| b.slot);
        let left_x = island.x - g - d;
        for (i, b) in left.into_iter().enumerate() {
            out.push((
                b.id.clone(),
                Rect {
                    x: left_x,
                    y: island.y + i as f64 * (d + g),
                    w: d,
                    h: d,
                },
            ));
        }

        let mut right: Vec<_> = self
            .buttons
            .iter()
            .filter(|b| b.side == Side::Right)
            .collect();
        right.sort_by_key(|b| b.slot);
        let right_x = island.x + island.w + g;
        for (i, b) in right.into_iter().enumerate() {
            out.push((
                b.id.clone(),
                Rect {
                    x: right_x,
                    y: island.y + i as f64 * (d + g),
                    w: d,
                    h: d,
                },
            ));
        }

        let mut bottom: Vec<_> = self
            .buttons
            .iter()
            .filter(|b| b.side == Side::Bottom)
            .collect();
        bottom.sort_by_key(|b| b.slot);
        let count = bottom.len() as f64;
        let row_width = count * d + (count - 1.0) * g;
        let row_left = island.x + island.w / 2.0 - row_width / 2.0;
        let row_top = island.y + island.h + g;
        for (i, b) in bottom.into_iter().enumerate() {
            out.push((
                b.id.clone(),
                Rect {
                    x: row_left + i as f64 * (d + g),
                    y: row_top,
                    w: d,
                    h: d,
                },
            ));
        }

        Ok(out)
    }

    /// The full input region as a set of rects: the island rect plus every
    /// button rect (one owner — the caller unions them, SPEC §2.4).
    pub fn input_region(&self, island: Rect) -> Result<Vec<Rect>, LayoutError> {
        let mut rects = Vec::with_capacity(1 + self.buttons.len());
        rects.push(island);
        for (_, r) in self.button_rects(island)? {
            rects.push(r);
        }
        Ok(rects)
    }

    /// The smallest rect covering the island and every button (for window
    /// sizing).
    pub fn bounding_box(&self, island: Rect) -> Result<Rect, LayoutError> {
        let rects = self.button_rects(island)?;
        let mut min_x = island.x;
        let mut min_y = island.y;
        let mut max_right = island.x + island.w;
        let mut max_bottom = island.y + island.h;
        for (_, r) in &rects {
            min_x = min_x.min(r.x);
            min_y = min_y.min(r.y);
            max_right = max_right.max(r.x + r.w);
            max_bottom = max_bottom.max(r.y + r.h);
        }
        Ok(Rect {
            x: min_x,
            y: min_y,
            w: max_right - min_x,
            h: max_bottom - min_y,
        })
    }
}
