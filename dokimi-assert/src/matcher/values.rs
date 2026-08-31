//! Equality, truth and absence.

use super::report::{Mode, fail};
use crate::seat::Seat;
use std::fmt::Debug;

/// Report when got and want differ.
///
/// Comparison is the language's own `==`. The standard asks that NaN be unequal to
/// itself, that the two zeroes be equal and that containers compare by their elements,
/// and Rust already answers all three that way. Values of different types never compare
/// because they do not compile.
#[track_caller]
pub fn equal<T>(seat: &dyn Seat, mode: Mode, got: &T, want: &T, msg: &str)
where
    T: PartialEq + Debug + ?Sized,
{
    seat.helper();
    if got != want {
        fail(
            seat,
            mode,
            "equal",
            msg,
            vec![
                ("want", format!("{want:?}").into()),
                ("got", format!("{got:?}").into()),
            ],
        );
    }
}

/// Report when got and want are equal.
///
/// The failure shows the value the two shared, since printing one says everything.
#[track_caller]
pub fn not_equal<T>(seat: &dyn Seat, mode: Mode, got: &T, want: &T, msg: &str)
where
    T: PartialEq + Debug + ?Sized,
{
    seat.helper();
    if got == want {
        fail(
            seat,
            mode,
            "not-equal",
            msg,
            vec![("got", format!("{got:?}").into())],
        );
    }
}

/// Report when the condition does not hold.
///
/// The failure carries the message alone: a bare false says nothing the message does not.
/// Where a more specific assertion exists, it will say more on failure.
#[track_caller]
pub fn is_true(seat: &dyn Seat, mode: Mode, condition: bool, msg: &str) {
    seat.helper();
    if !condition {
        fail(seat, mode, "true", msg, vec![]);
    }
}

/// Report when the condition holds.
#[track_caller]
pub fn is_false(seat: &dyn Seat, mode: Mode, condition: bool, msg: &str) {
    seat.helper();
    if condition {
        fail(seat, mode, "false", msg, vec![]);
    }
}

/// Report when got is present.
///
/// Rust states absence in the type rather than in the value, so this takes an `Option`
/// and there is no such thing as a typed nil to catch.
#[track_caller]
pub fn is_none<T: Debug>(seat: &dyn Seat, mode: Mode, got: Option<&T>, msg: &str) {
    seat.helper();
    if let Some(held) = got {
        fail(
            seat,
            mode,
            "nil",
            msg,
            vec![("got", format!("{held:?}").into())],
        );
    }
}

/// Report when got is absent.
///
/// Use it before reading a value that may be absent: the test stops here with your
/// message rather than further down on an unwrap nobody wrote.
#[track_caller]
pub fn is_some<T: Debug>(seat: &dyn Seat, mode: Mode, got: Option<&T>, msg: &str) {
    seat.helper();
    if got.is_none() {
        fail(seat, mode, "not-nil", msg, vec![]);
    }
}
