//! Assertions about numbers, where exact equality is the wrong question.

use super::report::{Mode, fail};
use crate::seat::Seat;

/// Report when got is further than tolerance from want.
///
/// The tolerance is an absolute difference and the bound is inclusive, so a difference
/// exactly equal to tolerance passes. NaN is outside every tolerance, whether it is the
/// value, the target or the tolerance.
#[track_caller]
pub fn close_to(seat: &dyn Seat, mode: Mode, got: f64, want: f64, tolerance: f64, msg: &str) {
    seat.helper();

    // Every comparison against NaN is false, so a bare `diff > tolerance`
    // would pass a NaN rather than reject it. Name the case instead.
    let diff = (got - want).abs();
    if diff.is_nan() || tolerance.is_nan() {
        fail(
            seat,
            mode,
            "close-to",
            msg,
            vec![
                ("got", format!("{got}").into()),
                ("want", want.into()),
                ("tolerance", tolerance.into()),
            ],
        );
        return;
    }
    if diff > tolerance {
        fail(
            seat,
            mode,
            "close-to",
            msg,
            vec![
                ("got", format!("{got}").into()),
                ("want", want.into()),
                ("tolerance", tolerance.into()),
            ],
        );
    }
}

/// Report when got falls outside low to high.
///
/// The interval is closed, so both bounds pass. A range with low above high can hold
/// nothing, and says so rather than reporting the value. NaN is in no range.
#[track_caller]
pub fn in_range(seat: &dyn Seat, mode: Mode, got: f64, low: f64, high: f64, msg: &str) {
    seat.helper();
    if low > high {
        fail(
            seat,
            mode,
            "in-range",
            msg,
            vec![
                ("got", format!("{got}").into()),
                ("low", low.into()),
                ("high", high.into()),
            ],
        );
        return;
    }
    if got.is_nan() || got < low || got > high {
        fail(
            seat,
            mode,
            "in-range",
            msg,
            vec![
                ("got", format!("{got}").into()),
                ("low", low.into()),
                ("high", high.into()),
            ],
        );
    }
}
