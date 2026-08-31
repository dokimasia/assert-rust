//! Sending one failure to a seat, under a mode.

use crate::seat::Seat;

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
