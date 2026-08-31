//! Sending one failure to a seat, under a mode.

use crate::failure::{Detail, Failure};
use crate::seat::Seat;
use std::panic::Location;

/// Whether a failure stops the test or is recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Stop the test at this failure.
    Fatal,
    /// Record the failure and carry on.
    Soft,
}

/// Send one failure to the seat.
///
/// This decides nothing about whether anything failed. A matcher calls it only once its
/// own comparison has failed, so every call produces exactly one reported failure. Under
/// [`Mode::Fatal`] it may not return.
#[track_caller]
pub fn report(seat: &dyn Seat, mode: Mode, message: &str) {
    seat.helper();
    match mode {
        Mode::Soft => seat.record(message),
        Mode::Fatal => seat.fail(message),
    }
}

/// Send one record to the seat.
///
/// A seat that takes records receives it; any other receives the sentence rendered from
/// it. The call site comes from `#[track_caller]`, so it names the line the caller wrote.
///
/// This decides nothing about whether anything failed. A matcher calls it only once its
/// own comparison has failed. Under [`Mode::Fatal`] it may not return.
#[track_caller]
pub fn fail(
    seat: &dyn Seat,
    mode: Mode,
    assertion: &'static str,
    contract: &str,
    detail: Vec<(&'static str, Detail)>,
) {
    seat.helper();
    let held = Failure {
        assertion,
        contract: contract.to_owned(),
        detail,
        where_at: Location::caller().into(),
    };

    if seat.takes_records() {
        seat.report(&held, mode == Mode::Fatal);
        return;
    }
    report(seat, mode, &held.render());
}
