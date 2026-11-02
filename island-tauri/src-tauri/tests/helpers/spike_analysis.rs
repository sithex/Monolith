//! Pure frame/pacing analysis for the Phase 2.5 spike (no X, no KWin). Unit-tested below so a
//! broken analysis cannot silently pass the GUI scenario.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]
use std::path::Path;

pub type Rgb = [u8; 3];

/// Flat colours of the sandbox (screen coordinates; the island window origin is (210, 0)).
pub const PILL_TEAL: Rgb = [0, 204, 204];
pub const PANEL_BLUE: Rgb = [25, 51, 153];
pub const ISLAND_YELLOW: Rgb = [255, 200, 0];
/// Stand-in pill and the hover target in screen px: (x, y, w, h, radius).
pub const PILL: (f64, f64, f64, f64, f64) = (725., 4., 150., 36., 18.);
pub const HOVER: (f64, f64, f64, f64, f64) = (590., 4., 420., 150., 28.);

pub struct Capture {
    pub w: usize,
    pub h: usize,
    pub px: Vec<Rgb>,
}

impl Capture {
    /// KWin's QImage format codes: 4..=6 are BGRx-ish (B at byte 0), 16..=18 are RGBx.
    pub fn from_raw(raw: &[u8], stride: usize, format: u64, w: usize, h: usize) -> Self {
        let order = match format {
            4..=6 => [2, 1, 0],
            16..=18 => [0, 1, 2],
            other => panic!("unsupported capture format {other}"),
        };
        let mut px = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                let i = y * stride + x * 4;
                px.push([raw[i + order[0]], raw[i + order[1]], raw[i + order[2]]]);
            }
        }
        Self { w, h, px }
    }
    pub fn solid(w: usize, h: usize, colour: Rgb) -> Self {
        Self {
            w,
            h,
            px: vec![colour; w * h],
        }
    }
    pub fn at(&self, x: usize, y: usize) -> Rgb {
        self.px[y * self.w + x]
    }
    pub fn set(&mut self, x: usize, y: usize, colour: Rgb) {
        self.px[y * self.w + x] = colour;
    }
    pub fn crop(&self, x: usize, y: usize, w: usize, h: usize) -> Capture {
        let mut px = Vec::with_capacity(w * h);
        for yy in y..y + h {
            px.extend_from_slice(&self.px[yy * self.w + x..yy * self.w + x + w]);
        }
        Capture { w, h, px }
    }
    pub fn save_png(&self, path: &Path) {
        let mut rgba = Vec::with_capacity(self.w * self.h * 4);
        for p in &self.px {
            rgba.extend_from_slice(&[p[0], p[1], p[2], 255]);
        }
        image::RgbaImage::from_raw(self.w as u32, self.h as u32, rgba)
            .unwrap()
            .save(path)
            .unwrap();
    }
}

/// "Island-ish": yellow over anything at coverage >= ~0.4 (pure teal, blue, black, white and
/// grey do not qualify). A soft mask so anti-aliased edge pixels do not count as holes.
pub fn soft_island(p: Rgb) -> bool {
    p[0] >= 120 && p[1] >= 90 && p[2] <= 110 && p[0] > p[2].saturating_add(40)
}

pub fn near(a: Rgb, b: Rgb, tol: u8) -> bool {
    a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= tol)
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

#[derive(Debug, Clone)]
pub struct Measured {
    /// Inclusive screen-pixel bounding box of the soft island mask.
    pub bbox: (i32, i32, i32, i32),
    pub w: i32,
    pub h: i32,
    /// Morph progress derived from the width alone; the other dimensions must agree with it.
    pub t: f64,
    pub teal: usize,
    pub island_px: usize,
}

fn runs(flags: impl Iterator<Item = bool>) -> usize {
    let mut count = 0;
    let mut inside = false;
    for f in flags {
        if f && !inside {
            count += 1;
        }
        inside = f;
    }
    count
}

fn inside_rounded(px: f64, py: f64, x0: f64, y0: f64, w: f64, h: f64, r: f64) -> bool {
    let r = r.min(w / 2.).min(h / 2.); // a radius can never exceed half the shorter side
    let cx = px.clamp(x0 + r, x0 + w - r);
    let cy = py.clamp(y0 + r, y0 + h - r);
    (px - cx).powi(2) + (py - cy).powi(2) <= r * r
}

/// Measure one capture whose top-left is the screen point `origin` and check that it is ONE
/// coherent, solid, rounded rectangle whose x/y/width/height/radius all belong to the same morph
/// progress (a torn frame mixing two times, a gap/stripe at the panel edge, a hole: all fail).
pub fn measure(cap: &Capture, origin: (i32, i32)) -> Result<Measured, String> {
    let mut island_px = 0usize;
    let mut teal = 0usize;
    let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for y in 0..cap.h {
        for x in 0..cap.w {
            let p = cap.at(x, y);
            if near(p, PILL_TEAL, 12) {
                teal += 1;
            }
            if soft_island(p) {
                island_px += 1;
                x0 = x0.min(x as i32);
                x1 = x1.max(x as i32);
                y0 = y0.min(y as i32);
                y1 = y1.max(y as i32);
            }
        }
    }
    if island_px == 0 {
        return Err("no island pixels in the frame".into());
    }
    let (w, h) = (x1 - x0 + 1, y1 - y0 + 1);
    let t = ((f64::from(w) - PILL.2) / (HOVER.2 - PILL.2)).clamp(0., 1.);
    let (sx, sy) = (x0 + origin.0, y0 + origin.1);
    let want_x = lerp(PILL.0, HOVER.0, t);
    let want_h = lerp(PILL.3, HOVER.3, t);
    if (f64::from(sx) - want_x).abs() > 3. || (f64::from(sy) - PILL.1).abs() > 2. {
        return Err(format!(
            "origin ({sx},{sy}) does not belong to width {w} (want x {want_x:.1}, y {})",
            PILL.1
        ));
    }
    if (f64::from(h) - want_h).abs() > 3. {
        return Err(format!(
            "height {h} does not belong to width {w} (want {want_h:.1}): torn frame?"
        ));
    }
    let r = lerp(PILL.4, HOVER.4, t);
    let band = r.ceil() as i32 + 2;
    // Contiguity is judged on the straight interior only: the outermost anti-aliased ring and
    // the corner arcs have sub-pixel coverage that is not monotonic mid-animation (the leak
    // check below covers them with a 2 px inset).
    for y in (y0 + band)..=(y1 - band) {
        let n = runs((x0..=x1).map(|x| soft_island(cap.at(x as usize, y as usize))));
        if n != 1 {
            return Err(format!(
                "row {} has {n} island runs (gap/seam)",
                y + origin.1
            ));
        }
    }
    for x in (x0 + band)..=(x1 - band) {
        let n = runs((y0..=y1).map(|y| soft_island(cap.at(x as usize, y as usize))));
        if n != 1 {
            return Err(format!(
                "column {} has {n} island runs (gap/seam at the panel edge?)",
                x + origin.0
            ));
        }
    }
    for y in (y0 + band)..=(y1 - band) {
        let row = (x0..=x1)
            .filter(|x| soft_island(cap.at(*x as usize, y as usize)))
            .count() as i32;
        if (row - w).abs() > 3 {
            return Err(format!(
                "row {} is {row} px wide, bbox is {w}: stepped/torn frame",
                y + origin.1
            ));
        }
    }
    // Every pixel strictly inside the ideal rounded rect (2 px inset) must be island colour.
    let mut leaks = 0usize;
    for y in y0..=y1 {
        for x in x0..=x1 {
            if inside_rounded(
                f64::from(x) + 0.5,
                f64::from(y) + 0.5,
                f64::from(x0) + 2.,
                f64::from(y0) + 2.,
                f64::from(w) - 4.,
                f64::from(h) - 4.,
                (r - 2.).max(0.),
            ) && !soft_island(cap.at(x as usize, y as usize))
            {
                leaks += 1;
            }
        }
    }
    if leaks > 0 {
        return Err(format!("{leaks} non-island pixels inside the rounded rect"));
    }
    Ok(Measured {
        bbox: (sx, sy, x1 + origin.0, y1 + origin.1),
        w,
        h,
        t,
        teal,
        island_px,
    })
}

pub fn percentile(sorted: &[f64], p: f64) -> f64 {
    assert!(!sorted.is_empty());
    let rank = (p * (sorted.len() - 1) as f64).round() as usize;
    sorted[rank]
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stats {
    pub n: usize,
    pub p50: f64,
    pub p95: f64,
    pub max: f64,
    pub over_20ms: usize,
}

pub fn stats(values: &[f64]) -> Stats {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    Stats {
        n: sorted.len(),
        p50: percentile(&sorted, 0.50),
        p95: percentile(&sorted, 0.95),
        max: *sorted.last().unwrap(),
        over_20ms: sorted.iter().filter(|v| **v > 20.).count(),
    }
}

/// Fraction of horizontally adjacent pixel pairs in the patch whose green differs by > 30:
/// a sharp 8 px checkerboard gives ~1/8, a blurred one ~0.
pub fn sharpness(cap: &Capture, x: usize, y: usize, w: usize, h: usize) -> f64 {
    let mut edges = 0usize;
    let mut pairs = 0usize;
    for yy in y..y + h {
        for xx in x..x + w - 1 {
            pairs += 1;
            if cap.at(xx, yy)[1].abs_diff(cap.at(xx + 1, yy)[1]) > 30 {
                edges += 1;
            }
        }
    }
    edges as f64 / pairs as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic 700x170 frame at origin (500, 0): blue bar (0..44), black below, optional pill.
    fn scene(t: f64, with_pill: bool) -> Capture {
        let mut c = Capture::solid(700, 170, [0, 0, 0]);
        for y in 0..44 {
            for x in 0..700 {
                c.set(x, y, PANEL_BLUE);
            }
        }
        if with_pill {
            // The stand-in pill is as round as the island's start rectangle.
            for y in 4..40 {
                for x in 225..375 {
                    if inside_rounded(x as f64 + 0.5, y as f64 + 0.5, 225., 4., 150., 36., 18.) {
                        c.set(x, y, PILL_TEAL);
                    }
                }
            }
        }
        let (x, y, w, h, r) = (
            lerp(PILL.0, HOVER.0, t) - 500.,
            PILL.1,
            lerp(PILL.2, HOVER.2, t),
            lerp(PILL.3, HOVER.3, t),
            lerp(PILL.4, HOVER.4, t),
        );
        for py in 0..170 {
            for px in 0..700 {
                if inside_rounded(px as f64 + 0.5, py as f64 + 0.5, x, y, w, h, r) {
                    c.set(px, py, ISLAND_YELLOW);
                }
            }
        }
        c
    }

    #[test]
    fn coherent_frames_measure_and_track_the_morph_progress() {
        for (t, pill) in [(0.0, true), (0.3, false), (0.77, false), (1.0, false)] {
            let m = measure(&scene(t, pill), (500, 0)).unwrap();
            assert!((m.t - t).abs() < 0.03, "t {t}: {m:?}");
        }
        let start = measure(&scene(0., true), (500, 0)).unwrap();
        assert_eq!((start.bbox.0, start.bbox.1), (725, 4));
        assert!(start.bbox.2 >= 873 && start.bbox.3 >= 38, "{start:?}");
        assert_eq!(start.teal, 0, "the pill is fully covered: {start:?}");
    }

    #[test]
    fn a_stripe_across_the_panel_edge_is_a_seam() {
        let mut c = scene(0.6, false);
        let m = measure(&c, (500, 0)).unwrap();
        let seam_y = 44usize;
        for x in 0..700 {
            if soft_island(c.at(x, seam_y)) {
                c.set(x, seam_y, PANEL_BLUE);
            }
        }
        let err = measure(&c, (500, 0)).unwrap_err();
        assert!(
            err.contains("runs") || err.contains("non-island"),
            "{err} {m:?}"
        );
    }

    #[test]
    fn a_torn_frame_and_a_wrong_start_rectangle_are_rejected() {
        // Top half from t=0.2, bottom half from t=0.9: widths step, heights disagree.
        let (a, b) = (scene(0.2, false), scene(0.9, false));
        let mut torn = a;
        for y in 80..170 {
            for x in 0..700 {
                torn.set(x, y, b.at(x, y));
            }
        }
        assert!(measure(&torn, (500, 0)).is_err());
        // The same shape drawn 6 px to the right of where this progress belongs.
        let c = scene(0.0, true);
        let mut shifted = Capture::solid(700, 170, [0, 0, 0]);
        for y in 0..170 {
            for x in 6..700 {
                shifted.set(x, y, c.at(x - 6, y));
            }
        }
        assert!(measure(&shifted, (500, 0)).is_err());
        assert!(measure(&Capture::solid(700, 170, [0, 0, 0]), (500, 0)).is_err());
    }

    #[test]
    fn a_visible_teal_gap_is_counted() {
        let mut c = scene(0., true);
        for y in 10..30 {
            for x in 226..236 {
                c.set(x, y, PILL_TEAL);
            }
        }
        assert!(measure(&c, (500, 0)).is_err() || measure(&c, (500, 0)).unwrap().teal > 100);
    }

    #[test]
    fn percentile_stats_and_sharpness() {
        let s = stats(&[16.0, 17.0, 16.5, 40.0, 16.7, 16.6, 21.0, 16.4, 16.8, 16.9]);
        assert_eq!(s.n, 10);
        assert_eq!(s.max, 40.0);
        assert_eq!(s.over_20ms, 2);
        assert!((s.p50 - 16.8).abs() < 0.11 && s.p95 >= 21.0, "{s:?}");
        let mut board = Capture::solid(64, 8, [0, 0, 0]);
        for y in 0..8 {
            for x in 0..64 {
                let v = if (x / 8) % 2 == 0 { 255 } else { 0 };
                board.set(x, y, [v, v, v]);
            }
        }
        assert!(sharpness(&board, 0, 0, 64, 8) > 0.10);
        assert_eq!(
            sharpness(&Capture::solid(64, 8, [90, 90, 90]), 0, 0, 64, 8),
            0.0
        );
    }
}
