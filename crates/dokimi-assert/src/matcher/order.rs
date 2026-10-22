//! How neighbouring items relate.

use crate::seat::Seat;
use crate::value::Value;
use crate::verdict::{Mode, Run};
use std::fmt::Debug;

/// Reports whether every adjacent pair of `items` satisfies `predicate`.
/// Nought or one item passes. The predicate is not called again after the
/// first pair that fails.
#[track_caller]
pub(crate) fn pairwise<T: Debug>(
    seat: &dyn Seat,
    mode: Mode,
    items: &[T],
    predicate: impl Fn(&T, &T) -> bool,
    msg: &str,
) {
    seat.helper();
    let run = Run::of(seat);
    for (index, pair) in items.windows(2).enumerate() {
        if !predicate(&pair[0], &pair[1]) {
            run.fail(
                mode,
                "pairwise",
                msg,
                vec![
                    ("index", Value::from(index)),
                    ("first", Value::debug(&pair[0])),
                    ("second", Value::debug(&pair[1])),
                ],
            );
            return;
        }
    }
    run.pass(mode, "pairwise", msg);
}
