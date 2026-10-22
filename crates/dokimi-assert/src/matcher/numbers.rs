// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! Numbers, where exact equality is the wrong question.

use crate::seat::Seat;
use crate::value::Value;
use crate::verdict::{Mode, Run};
use std::fmt::Debug;

/// A value that [`check::close_to`](crate::check::close_to) and
/// [`check::in_range`](crate::check::in_range) read as a number.
///
/// Every integer and float type is a number, so a test passes an `f32` or a
/// `u64` without a cast. A [`Value`] is a number when it is an `Int` or a
/// `Float`. Any other value is no number, and fails both assertions.
pub trait Number: Debug {
    /// Returns the value as a float, or `None` for a value that is no number.
    fn to_f64(&self) -> Option<f64>;

    /// Returns the value as a failure's record states it.
    fn to_value(&self) -> Value;
}

macro_rules! number {
    ($($t:ty)*) => {$(
        impl Number for $t {
            #[allow(clippy::cast_precision_loss, clippy::cast_lossless, reason = "a number converts to the nearest float")]
            fn to_f64(&self) -> Option<f64> {
                Some(*self as f64)
            }

            fn to_value(&self) -> Value {
                Value::from(*self)
            }
        }
    )*};
}

number!(i8 i16 i32 i64 i128 u8 u16 u32 u64 isize usize f32 f64);

impl Number for u128 {
    #[allow(
        clippy::cast_precision_loss,
        reason = "a number converts to the nearest float"
    )]
    fn to_f64(&self) -> Option<f64> {
        Some(*self as f64)
    }

    fn to_value(&self) -> Value {
        i128::try_from(*self).map_or_else(|_| Value::debug(self), Value::Int)
    }
}

impl Number for Value {
    #[allow(
        clippy::cast_precision_loss,
        reason = "an integer converts to the nearest float"
    )]
    fn to_f64(&self) -> Option<f64> {
        match self {
            Self::Int(held) => Some(*held as f64),
            Self::Float(held) => Some(*held),
            _ => None,
        }
    }

    fn to_value(&self) -> Value {
        self.clone()
    }
}

impl<N: Number + ?Sized> Number for &N {
    fn to_f64(&self) -> Option<f64> {
        (**self).to_f64()
    }

    fn to_value(&self) -> Value {
        (**self).to_value()
    }
}

/// Reports whether `got` is within `tolerance` of `want`, by absolute
/// difference. NaN is outside every tolerance, and so are two equal
/// infinities, whose difference is NaN.
#[track_caller]
pub(crate) fn close_to<N: Number>(
    seat: &dyn Seat,
    mode: Mode,
    got: N,
    want: f64,
    tolerance: f64,
    msg: &str,
) {
    seat.helper();
    let run = Run::of(seat);
    // A comparison with NaN is false, so the test states the passing side.
    let close = got
        .to_f64()
        .is_some_and(|got| (got - want).abs() <= tolerance);
    if !close {
        run.fail(
            mode,
            "close-to",
            msg,
            vec![
                ("got", got.to_value()),
                ("want", Value::Float(want)),
                ("tolerance", Value::Float(tolerance)),
            ],
        );
        return;
    }
    run.pass(mode, "close-to", msg);
}

/// Reports whether `got` falls in the closed interval from `low` to `high`.
/// NaN is in no range, and a range with a NaN bound contains no number.
#[track_caller]
pub(crate) fn in_range<N: Number>(
    seat: &dyn Seat,
    mode: Mode,
    got: N,
    low: f64,
    high: f64,
    msg: &str,
) {
    seat.helper();
    let run = Run::of(seat);
    let inside = got.to_f64().is_some_and(|got| low <= got && got <= high);
    if !inside {
        run.fail(
            mode,
            "in-range",
            msg,
            vec![
                ("got", got.to_value()),
                ("low", Value::Float(low)),
                ("high", Value::Float(high)),
            ],
        );
        return;
    }
    run.pass(mode, "in-range", msg);
}
