//! Assertions that retry, and the one that checks nothing was left running.
//!
//! These spend real time, deliberately. They are for a condition something outside the
//! test makes true, which is what a controlled clock cannot reach: a fake clock only
//! moves when someone advances it, and nobody will while this is waiting.

use super::report::{Mode, fail};
use crate::clock::Clock;
use crate::seat::{Recorder, Seat};
use std::time::Duration;

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
    let clock = seat.clock();
    let deadline = clock.now() + timeout;

    for attempt in 1.. {
        let trial = Recorder::new();
        body(&trial);

        if !trial.failed() {
            return;
        }
        if clock.now() > deadline {
            fail(
                seat,
                mode,
                "eventually",
                msg,
                vec![
                    ("attempts", attempt.into()),
                    ("last", trial.message().into()),
                ],
            );
            return;
        }
        wait(clock, interval.max(Duration::from_millis(1)));
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
    let clock = seat.clock();
    let deadline = clock.now() + timeout;
    let cap = timeout / 4;
    let mut backoff = Duration::from_millis(1);

    for attempt in 1.. {
        if predicate() {
            return;
        }
        if clock.now() > deadline {
            fail(
                seat,
                mode,
                "eventually-true",
                msg,
                vec![("attempts", attempt.into())],
            );
            return;
        }
        wait(clock, backoff.max(Duration::from_millis(1)));
        backoff = (backoff * 2).min(cap.max(Duration::from_millis(1)));
    }
}

/// Move time forward by the duration.
///
/// A clock a test controls is advanced, because nothing else will move it while this
/// call is running. The platform clock ignores an advance, so it is slept against.
fn wait(clock: &dyn Clock, duration: Duration) {
    let before = clock.now();
    clock.advance(duration);
    if clock.now() == before {
        clock.sleep(duration);
    }
}
