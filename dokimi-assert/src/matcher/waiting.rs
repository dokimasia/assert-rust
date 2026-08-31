//! Assertions that retry, and the one that checks nothing was left running.
//!
//! These spend real time, deliberately. They are for a condition something outside the
//! test makes true, which is what a controlled clock cannot reach: a fake clock only
//! moves when someone advances it, and nobody will while this is waiting.

use super::report::{Mode, report};
use crate::seat::{Recorder, Seat};
use std::time::{Duration, Instant};

/// Report when a body of assertions never passes within the timeout.
///
/// The body is handed a seat of its own, so assertions inside it record an attempt rather
/// than ending the test. It runs at least once however short the timeout, and the failure
/// carries the last attempt's own reason rather than a bare timeout.
#[track_caller]
pub fn eventually<F>(
    seat: &dyn Seat,
    mode: Mode,
    timeout: Duration,
    interval: Duration,
    body: F,
    msg: &str,
) where
    F: Fn(&Recorder),
{
    seat.helper();
    let deadline = Instant::now() + timeout;

    for attempt in 1.. {
        let trial = Recorder::new();
        body(&trial);

        if !trial.failed() {
            return;
        }
        if Instant::now() > deadline {
            report(
                seat,
                mode,
                &format!(
                    "{msg}: still failing after {}ms and {attempt} attempts: {}",
                    timeout.as_millis(),
                    trial.message()
                ),
            );
            return;
        }
        std::thread::sleep(interval.max(Duration::from_millis(1)));
    }
}

/// Report when a predicate never becomes true within the timeout.
///
/// Retried with a backoff that starts at a millisecond and doubles, capped at a quarter
/// of the timeout so the last attempts are not one long sleep. A predicate carries no
/// reason, so the failure says only that the wait ran out; where the reason matters, use
/// [`eventually`].
#[track_caller]
pub fn eventually_true<P>(seat: &dyn Seat, mode: Mode, timeout: Duration, predicate: P, msg: &str)
where
    P: Fn() -> bool,
{
    seat.helper();
    let deadline = Instant::now() + timeout;
    let cap = timeout / 4;
    let mut backoff = Duration::from_millis(1);

    for attempt in 1.. {
        if predicate() {
            return;
        }
        if Instant::now() > deadline {
            report(
                seat,
                mode,
                &format!(
                    "{msg}: still false after {}ms and {attempt} attempts",
                    timeout.as_millis()
                ),
            );
            return;
        }
        std::thread::sleep(backoff.max(Duration::from_millis(1)));
        backoff = (backoff * 2).min(cap.max(Duration::from_millis(1)));
    }
}
