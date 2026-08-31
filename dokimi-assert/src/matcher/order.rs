//! How neighbouring items relate.

use super::report::{Mode, report};
use crate::seat::Seat;
use std::fmt::Debug;

/// Report when an adjacent pair does not satisfy the predicate.
///
/// One assertion rather than sorted, unique and strictly increasing, because each of
/// those is a relation that has to hold between every adjacent pair and nothing more.
///
/// Nought or one item passes, since neither has a pair. The failure names the index where
/// it broke, and the predicate is not called again after that.
#[track_caller]
pub fn pairwise<T, P>(seat: &dyn Seat, mode: Mode, items: &[T], predicate: P, msg: &str)
where
    T: Debug,
    P: Fn(&T, &T) -> bool,
{
    seat.helper();
    for (at, pair) in items.windows(2).enumerate() {
        let (earlier, later) = (&pair[0], &pair[1]);
        if !predicate(earlier, later) {
            report(
                seat,
                mode,
                &format!("{msg}: the pair at index {at} fails: {earlier:?} then {later:?}"),
            );
            return;
        }
    }
}
