//! The assertion that a check fails.
//!
//! A check that is meant to refuse a subject proves itself only by failing
//! against a subject that it must refuse.

use super::body::Body;
use crate::failure::Failure;
use crate::seat::Seat;
use crate::verdict::{Mode, Run};

/// Runs `body` as a check on a seat of its own, reports a failure when the
/// check does not fail, and returns the records of the check's failures in
/// call order.
///
/// A failure on `check` ends the check, as it ends a test. A message passed
/// to the seat's `fail` or `record` fails the check without a record.
#[track_caller]
pub(crate) fn rejects(seat: &dyn Seat, msg: &str, body: impl FnOnce(&dyn Seat)) -> Vec<Failure> {
    seat.helper();
    let run = Run::begin(seat);
    let calls = run.body_calls();
    let check = Body::new(seat, calls.clone());
    check.run(body);
    run.take(calls.as_ref());
    if check.failed() {
        run.pass(Mode::Fatal, "rejects", msg);
    } else {
        run.fail(Mode::Fatal, "rejects", msg, vec![]);
    }
    check.failures()
}
