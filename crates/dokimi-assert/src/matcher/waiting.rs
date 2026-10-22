//! Assertions that retry until a condition holds.
//!
//! They wait on the seat's clock. On the platform clock they spend real time,
//! so they suit a condition that something outside the test makes true. On a
//! controlled clock they advance it between attempts and spend no time.

use super::body::Body;
use crate::clock::Clock;
use crate::seat::Seat;
use crate::value::Value;
use crate::verdict::{Mode, Run};
use std::time::Duration;

/// The shortest wait between two attempts. Every wait moves a controlled
/// clock forward.
const MIN_WAIT: Duration = Duration::from_millis(1);

/// Waits `duration` on `clock`: a clock that a test controls is advanced, and
/// the platform clock, which an advance does not move, is slept on for the
/// time that the advance did not cover.
pub(crate) fn wait(clock: &dyn Clock, duration: Duration) {
    let until = clock.now() + duration;
    clock.advance(duration);
    let left = until.saturating_sub(clock.now());
    if !left.is_zero() {
        clock.sleep(left);
    }
}

/// Waits `duration` on `clock`, or the time left before `deadline` when that
/// is less, and returns `true` when another attempt may start: the clock read
/// before the deadline, and after the wait it reads no later than it.
fn wait_within(clock: &dyn Clock, deadline: Duration, duration: Duration) -> bool {
    let now = clock.now();
    if now >= deadline {
        return false;
    }
    wait(clock, duration.min(deadline.saturating_sub(now)));
    clock.now() <= deadline
}

/// Reports whether `body` passes within `timeout`, retried every `interval`.
///
/// Each attempt runs on a seat of its own, so a failure records the attempt
/// rather than ending the test. The body runs at least once, however short
/// the timeout. No attempt starts after the timeout, and an attempt passes
/// the call only when it ends by the deadline. The failure states the
/// attempts and the first failure of the last attempt.
#[track_caller]
pub(crate) fn eventually(
    seat: &dyn Seat,
    mode: Mode,
    timeout: Duration,
    interval: Duration,
    body: impl Fn(&dyn Seat),
    msg: &str,
) {
    seat.helper();
    let run = Run::begin(seat);
    let clock = seat.clock();
    let deadline = clock.now() + timeout;
    for attempts in 1_usize.. {
        let calls = run.body_calls();
        let attempt = Body::new(seat, calls.clone());
        attempt.run(&body);
        run.take(calls.as_ref());
        if !attempt.failed() && clock.now() <= deadline {
            run.pass(mode, "eventually", msg);
            return;
        }
        if !wait_within(clock, deadline, interval.max(MIN_WAIT)) {
            run.fail(
                mode,
                "eventually",
                msg,
                vec![
                    ("attempts", Value::from(attempts)),
                    ("last", Value::from(attempt.first_message())),
                ],
            );
            return;
        }
    }
}

/// Reports whether `predicate` becomes true within `timeout`, retried with a
/// backoff that starts at a millisecond and doubles, up to a quarter of the
/// timeout.
#[track_caller]
pub(crate) fn eventually_true(
    seat: &dyn Seat,
    mode: Mode,
    timeout: Duration,
    predicate: impl Fn() -> bool,
    msg: &str,
) {
    seat.helper();
    let run = Run::of(seat);
    let clock = seat.clock();
    let deadline = clock.now() + timeout;
    let cap = (timeout / 4).max(MIN_WAIT);
    let mut backoff = MIN_WAIT;
    for attempts in 1_usize.. {
        if predicate() && clock.now() <= deadline {
            run.pass(mode, "eventually-true", msg);
            return;
        }
        if !wait_within(clock, deadline, backoff) {
            run.fail(
                mode,
                "eventually-true",
                msg,
                vec![("attempts", Value::from(attempts))],
            );
            return;
        }
        backoff = (backoff * 2).min(cap);
    }
}
