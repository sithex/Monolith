//! `InputRegionController` — the only code allowed to change pointer pass-through (SPEC §2.4).
//!
//! On Linux nothing may call Tauri's `set_ignore_cursor_events`: tao implements it with
//! `input_shape_combine_region`, overwriting the very input shape this controller owns.
//!
//! Rules implemented here (all platform-neutral and unit-tested with a fake backend):
//! * stale reports (older `state_rev`, or an unsettled frame after that rev settled) are rejected;
//! * growing/shrinking: while a report is not `settled`, the applied region is the *union* of the
//!   current and target region, so the pointer never falls outside mid-animation; the final region
//!   is applied only on `settled`;
//! * native-pill `still` → empty region (100 % click-through);
//! * apply only on change (identical rows are never re-applied; rows already encode mode+scale);
//! * every apply is verified by a readback and fails closed on mismatch.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::PlatformBackend;

#[cfg(not(target_os = "linux"))]
mod unsupported;
#[cfg(not(target_os = "linux"))]
pub use unsupported::PlatformBackend;

use crate::contract::island::{GeometryAck, GeometryReport, IslandMode, RoundedRect};
use crate::error::IslandError;
use crate::window::shape::{normalize_rows, region_rows, subtract_rect, union_rows, ShapeRow};

pub trait InputRegionBackend: Send {
    /// Apply `region` (physical px, window-relative). Empty slice = fully click-through.
    fn apply(&mut self, region: &[ShapeRow]) -> Result<(), IslandError>;
    /// Read the region back from the windowing system (tests + status).
    fn readback(&self) -> Result<Vec<ShapeRow>, IslandError>;
    /// Painted blur is independent of input (the native pill owns overlapped input).
    fn paint(&mut self, _region: &[ShapeRow]) -> Result<(), IslandError> {
        Ok(())
    }
}

pub struct InputRegionController<B: InputRegionBackend> {
    backend: B,
    native_pill: bool,
    newest_rev: Option<u32>,
    /// `(rev, mode)` of the last settled report. An unsettled frame of the same rev is a new
    /// animation of a layout-only change (view switch, offset/scale) when its mode agrees, and
    /// a stale/contradicting frame when it does not.
    settled: Option<(u32, IslandMode)>,
    /// `None` until the first verified apply: the window's initial input shape is unknown
    /// (the X default is the whole window), so nothing may be deduplicated against it.
    applied: Option<Vec<ShapeRow>>,
    exact_paint: bool,
    pill_exclusion: Option<RoundedRect>,
    painted: Option<Vec<ShapeRow>>,
}

/// Wrapping revision comparison: is `a` older than `b`?
fn rev_before(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) < 0
}

fn ack_rows(rows: &[ShapeRow]) -> u16 {
    u16::try_from(rows.len()).unwrap_or(u16::MAX)
}

impl<B: InputRegionBackend> InputRegionController<B> {
    pub fn new(backend: B, native_pill: bool) -> Self {
        Self {
            backend,
            native_pill,
            newest_rev: None,
            settled: None,
            applied: None,
            exact_paint: false,
            pill_exclusion: None,
            painted: None,
        }
    }

    /// Phase 7b island page only: current paint/input, never swept animation trails.
    /// Legacy/harness retain their earlier transition policy.
    pub fn with_island_geometry(mut self, pill_exclusion: Option<RoundedRect>) -> Self {
        self.exact_paint = true;
        self.pill_exclusion = pill_exclusion;
        self
    }

    /// Apply empty input before show, without deduplicating against the unknown X default.
    pub fn install(&mut self) -> Result<(), IslandError> {
        if self.exact_paint {
            self.backend.paint(&[])?;
            self.painted = Some(Vec::new());
        }
        self.apply_verified(&[])
    }

    pub fn applied(&self) -> Option<&[ShapeRow]> {
        self.applied.as_deref()
    }

    /// Number of rows currently applied (what a rejected report acknowledges).
    pub fn applied_row_count(&self) -> u16 {
        self.applied.as_deref().map(ack_rows).unwrap_or(0)
    }

    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    /// A monitor transfer invalidates cached surface/scale geometry, not actor or PTY state.
    pub fn relocated(&mut self, pill: RoundedRect) -> Result<(), IslandError> {
        self.pill_exclusion = Some(pill);
        self.applied = None;
        self.painted = None;
        self.settled = None;
        self.install()
    }

    pub fn backend(&self) -> &B {
        &self.backend
    }

    pub fn report(&mut self, report: &GeometryReport) -> Result<GeometryAck, IslandError> {
        let current_rows = self.applied_row_count();
        if let Some(newest) = self.newest_rev {
            if rev_before(report.state_rev, newest) {
                return Ok(GeometryAck {
                    applied: false,
                    rows: current_rows,
                });
            }
        }
        // SPEC §8 Phase 3: several reports per rev are legal (view switches inside `large`
        // and offset/scale changes re-report at the same rev). Only a frame that CONTRADICTS
        // the settled mode of its own rev is stale.
        if self
            .settled
            .is_some_and(|(rev, mode)| rev == report.state_rev && mode != report.mode)
            && !report.settled
        {
            return Ok(GeometryAck {
                applied: false,
                rows: current_rows,
            });
        }
        let measured = region_rows(&report.rects, report.mode, report.scale, false)?;
        let painted = if self.exact_paint
            && self.native_pill
            && report.mode == IslandMode::Still
            && report.settled
        {
            Vec::new()
        } else {
            measured
        };
        let target = if self.exact_paint {
            if self.native_pill && report.mode == IslandMode::Still {
                Vec::new()
            } else if let Some(pill) = self.pill_exclusion {
                subtract_rect(&painted, pill, report.scale)?
            } else {
                painted.clone()
            }
        } else {
            region_rows(&report.rects, report.mode, report.scale, self.native_pill)?
        };
        let desired = match (&self.applied, report.settled) {
            (Some(current), false) if !self.exact_paint => union_rows(current, &target),
            _ => target,
        };
        if self.exact_paint && self.painted.as_deref() != Some(painted.as_slice()) {
            self.backend.paint(&painted)?;
            self.painted = Some(painted);
        }
        let changed = self.applied.as_deref() != Some(desired.as_slice());
        if changed {
            self.apply_verified(&desired)?;
        }
        self.newest_rev = Some(report.state_rev);
        if report.settled {
            self.settled = Some((report.state_rev, report.mode));
        }
        Ok(GeometryAck {
            applied: changed,
            rows: ack_rows(&desired),
        })
    }

    fn apply_verified(&mut self, desired: &[ShapeRow]) -> Result<(), IslandError> {
        let desired = normalize_rows(desired);
        self.backend.apply(&desired)?;
        let actual = normalize_rows(&self.backend.readback()?);
        if actual != desired {
            // The windowing system holds something other than what we applied: do not record it
            // as applied, and tell the caller. Fail closed.
            self.applied = None;
            return Err(IslandError::internal(format!(
                "input region readback mismatch: applied {} rows, window reports {}",
                desired.len(),
                actual.len()
            )));
        }
        self.applied = Some(desired);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::island::RoundedRect;
    use crate::error::ErrorCode;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct Log {
        applies: Vec<Vec<ShapeRow>>,
        lie: bool,
        fail: bool,
    }
    #[derive(Clone, Default)]
    struct Fake(Arc<Mutex<Log>>);
    impl InputRegionBackend for Fake {
        fn apply(&mut self, region: &[ShapeRow]) -> Result<(), IslandError> {
            let mut log = self.0.lock().unwrap();
            if log.fail {
                return Err(IslandError::unavailable("fake apply failure"));
            }
            log.applies.push(region.to_vec());
            Ok(())
        }
        fn readback(&self) -> Result<Vec<ShapeRow>, IslandError> {
            let log = self.0.lock().unwrap();
            if log.lie {
                return Ok(vec![ShapeRow {
                    x: 0,
                    y: 0,
                    width: 1180,
                    height: 820,
                }]);
            }
            Ok(log.applies.last().cloned().unwrap_or_default())
        }
    }

    fn rect(x: f64, w: f64) -> RoundedRect {
        RoundedRect {
            x,
            y: 0.,
            width: w,
            height: 10.,
            radius: 0.,
        }
    }
    fn report(
        rev: u32,
        mode: IslandMode,
        rects: Vec<RoundedRect>,
        settled: bool,
    ) -> GeometryReport {
        GeometryReport {
            state_rev: rev,
            mode,
            rects,
            scale: 1.,
            settled,
        }
    }
    fn ctl(native: bool) -> (InputRegionController<Fake>, Fake) {
        let fake = Fake::default();
        (InputRegionController::new(fake.clone(), native), fake)
    }
    fn count(f: &Fake) -> usize {
        f.0.lock().unwrap().applies.len()
    }

    #[test]
    fn install_applies_an_empty_region_exactly_once_and_does_not_dedupe_against_unknown_default() {
        let (mut c, f) = ctl(true);
        assert!(c.applied().is_none());
        c.install().unwrap();
        assert_eq!(count(&f), 1);
        assert_eq!(c.applied(), Some(&[][..]));
        // a native-pill still report is empty == applied: no second apply
        let ack = c
            .report(&report(1, IslandMode::Still, vec![rect(0., 50.)], true))
            .unwrap();
        assert_eq!(
            ack,
            GeometryAck {
                applied: false,
                rows: 0
            }
        );
        assert_eq!(count(&f), 1);
    }

    #[test]
    fn without_install_the_first_report_is_always_applied() {
        let (mut c, f) = ctl(true);
        let ack = c
            .report(&report(1, IslandMode::Still, vec![rect(0., 50.)], true))
            .unwrap();
        assert!(ack.applied && ack.rows == 0);
        assert_eq!(count(&f), 1);
    }

    #[test]
    fn non_native_still_keeps_the_pill_rect() {
        let (mut c, _f) = ctl(false);
        c.install().unwrap();
        let ack = c
            .report(&report(1, IslandMode::Still, vec![rect(100., 50.)], true))
            .unwrap();
        assert!(ack.applied);
        assert_eq!(ack.rows, 10);
    }

    #[test]
    fn growing_applies_target_union_current_immediately_then_final_on_settle() {
        let (mut c, f) = ctl(false);
        c.install().unwrap();
        c.report(&report(1, IslandMode::Hover, vec![rect(0., 10.)], true))
            .unwrap();
        // grow to a disjoint rect, not settled: union of both
        let ack = c
            .report(&report(2, IslandMode::Large, vec![rect(20., 10.)], false))
            .unwrap();
        assert!(ack.applied);
        let union = f.0.lock().unwrap().applies.last().cloned().unwrap();
        assert!(union.iter().any(|r| r.x == 0) && union.iter().any(|r| r.x == 20));
        // settle: only the target remains
        let ack = c
            .report(&report(2, IslandMode::Large, vec![rect(20., 10.)], true))
            .unwrap();
        assert!(ack.applied);
        let fin = f.0.lock().unwrap().applies.last().cloned().unwrap();
        assert!(fin.iter().all(|r| r.x == 20));
    }

    #[test]
    fn shrinking_keeps_the_larger_region_until_settled() {
        let (mut c, f) = ctl(true);
        c.install().unwrap();
        c.report(&report(1, IslandMode::Large, vec![rect(0., 100.)], true))
            .unwrap();
        let before = count(&f);
        // collapse to native still, mid-animation: must not shrink yet
        let ack = c
            .report(&report(2, IslandMode::Still, vec![rect(0., 50.)], false))
            .unwrap();
        assert!(!ack.applied);
        assert_eq!(count(&f), before);
        assert_eq!(ack.rows, 10);
        // settled: empty region
        let ack = c
            .report(&report(2, IslandMode::Still, vec![rect(0., 50.)], true))
            .unwrap();
        assert!(ack.applied && ack.rows == 0);
        assert!(f.0.lock().unwrap().applies.last().unwrap().is_empty());
    }

    #[test]
    fn stale_revisions_and_late_unsettled_frames_cannot_reopen_a_collapsed_region() {
        let (mut c, f) = ctl(true);
        c.install().unwrap();
        c.report(&report(5, IslandMode::Large, vec![rect(0., 100.)], true))
            .unwrap();
        c.report(&report(6, IslandMode::Still, vec![rect(0., 50.)], true))
            .unwrap();
        let n = count(&f);
        // an old animation frame of rev 5 arrives late
        let ack = c
            .report(&report(5, IslandMode::Large, vec![rect(0., 100.)], false))
            .unwrap();
        assert!(!ack.applied);
        // and an unsettled frame of the already-settled rev 6
        let ack = c
            .report(&report(6, IslandMode::Hover, vec![rect(0., 100.)], false))
            .unwrap();
        assert!(!ack.applied);
        assert_eq!(count(&f), n);
        assert!(f.0.lock().unwrap().applies.last().unwrap().is_empty());
    }

    /// SPEC §8 Phase 3: several reports per rev. A layout-only change (view switch inside
    /// `large`, offset/scale) re-reports at the SAME rev after that rev settled; it re-animates
    /// with the grow/shrink rules, while a frame contradicting the settled mode stays stale.
    #[test]
    fn several_reports_per_rev_reanimate_layout_only_changes() {
        let widths = |f: &Fake| -> Vec<u16> {
            f.0.lock()
                .unwrap()
                .applies
                .last()
                .unwrap()
                .iter()
                .map(|r| r.width)
                .collect()
        };
        let (mut c, f) = ctl(false);
        c.install().unwrap();
        c.report(&report(3, IslandMode::Large, vec![rect(0., 100.)], true))
            .unwrap();
        // shrinking view switch: the larger region is kept until settled
        let ack = c
            .report(&report(3, IslandMode::Large, vec![rect(0., 40.)], false))
            .unwrap();
        assert!(!ack.applied);
        assert!(widths(&f).iter().all(|w| *w == 100));
        let ack = c
            .report(&report(3, IslandMode::Large, vec![rect(0., 40.)], true))
            .unwrap();
        assert!(ack.applied);
        assert!(widths(&f).iter().all(|w| *w == 40));
        // growing view switch: the target is open IMMEDIATELY (unsettled), no wait for settle
        let ack = c
            .report(&report(3, IslandMode::Large, vec![rect(0., 100.)], false))
            .unwrap();
        assert!(ack.applied, "unsettled same-rev growth must be accepted");
        assert!(widths(&f).iter().all(|w| *w == 100));
        // a frame that contradicts the settled mode of this rev is stale
        let n = count(&f);
        let ack = c
            .report(&report(3, IslandMode::Hover, vec![rect(0., 400.)], false))
            .unwrap();
        assert!(!ack.applied);
        assert_eq!(count(&f), n);
    }

    #[test]
    fn revision_comparison_wraps() {
        let (mut c, f) = ctl(false);
        c.install().unwrap();
        c.report(&report(
            u32::MAX,
            IslandMode::Hover,
            vec![rect(0., 10.)],
            true,
        ))
        .unwrap();
        // rev 0 is *newer* than u32::MAX after wrapping
        let ack = c
            .report(&report(0, IslandMode::Large, vec![rect(20., 10.)], true))
            .unwrap();
        assert!(ack.applied);
        assert!(f
            .0
            .lock()
            .unwrap()
            .applies
            .last()
            .unwrap()
            .iter()
            .all(|r| r.x == 20));
    }

    #[test]
    fn invalid_layout_is_rejected_without_touching_state() {
        let (mut c, f) = ctl(false);
        c.install().unwrap();
        let bad = report(1, IslandMode::Hover, vec![rect(-5., 10.)], true);
        let err = c.report(&bad).unwrap_err();
        assert_eq!(err.code, ErrorCode::Invalid);
        assert_eq!(count(&f), 1);
        // a later valid report with the same rev is still accepted (rev was not consumed)
        assert!(
            c.report(&report(1, IslandMode::Hover, vec![rect(0., 10.)], true))
                .unwrap()
                .applied
        );
    }

    #[test]
    fn readback_mismatch_fails_closed_and_forgets_the_applied_region() {
        let (mut c, f) = ctl(false);
        c.install().unwrap();
        f.0.lock().unwrap().lie = true;
        let err = c
            .report(&report(1, IslandMode::Hover, vec![rect(0., 10.)], true))
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::Internal);
        assert!(
            c.applied().is_none(),
            "unverified region must not be remembered"
        );
    }

    #[test]
    fn backend_failure_propagates() {
        let (mut c, f) = ctl(false);
        f.0.lock().unwrap().fail = true;
        assert_eq!(c.install().unwrap_err().code, ErrorCode::Unavailable);
    }
}
