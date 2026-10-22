//! Assertions over futures that end the test at the first failure.
//!
//! Each assertion takes the seat first and the contract last, as the
//! assertions of `dokimi_assert::check` do, and returns a future that the
//! test awaits. The future reports to the seat when it completes.

use crate::matcher;
use dokimi_assert::__private::{Mode, Run};
use dokimi_assert::{Cancel, Seat, Stop};
use std::error::Error;
use std::future::Future;
use std::time::Duration;

const MODE: Mode = Mode::Fatal;

/// Ends the test when a subject that receives a cancelled handle does not
/// return an error whose chain contains [`Stop::Cancelled`].
///
/// The handle is cancelled before the subject starts, so the assertion
/// checks that the subject reads the handle at all. A subject that returns
/// an error for a reason of its own fails too. The failure states the error
/// that the subject returned, the message of its panic, or null when it
/// returned `Ok`.
#[track_caller]
pub fn honours_cancellation<'a, E: Error + 'static>(
    seat: &'a dyn Seat,
    body: impl AsyncFnOnce(Option<&Cancel>) -> Result<(), E> + 'a,
    msg: &'a str,
) -> impl Future<Output = ()> + 'a {
    seat.helper();
    matcher::honours(
        Run::of(seat),
        MODE,
        "honours-cancellation",
        Stop::Cancelled,
        body,
        msg,
    )
}

/// Ends the test when a subject that receives an expired handle does not
/// return an error whose chain contains [`Stop::DeadlineExceeded`].
#[track_caller]
pub fn honours_deadline<'a, E: Error + 'static>(
    seat: &'a dyn Seat,
    body: impl AsyncFnOnce(Option<&Cancel>) -> Result<(), E> + 'a,
    msg: &'a str,
) -> impl Future<Output = ()> + 'a {
    seat.helper();
    matcher::honours(
        Run::of(seat),
        MODE,
        "honours-deadline",
        Stop::DeadlineExceeded,
        body,
        msg,
    )
}

/// Ends the test when `body` has not completed within `within` on tokio's
/// clock.
///
/// The assertion drops a body that has not completed by then, as
/// `tokio::time::timeout` drops it, and does not wait for it. The failure
/// states the duration and the time waited, in nanoseconds. A panic of the
/// body panics past the assertion.
#[track_caller]
pub fn completes_within<'a>(
    seat: &'a dyn Seat,
    within: Duration,
    body: impl Future + 'a,
    msg: &'a str,
) -> impl Future<Output = ()> + 'a {
    seat.helper();
    matcher::completes_within(Run::of(seat), MODE, within, body, msg)
}

/// Ends the test when `body` does not pass within `timeout`, retried every
/// `interval` on tokio's clock.
///
/// Each attempt runs on a seat of its own, so a failure ends the attempt and
/// not the test. The body runs at least once, and the wait between two
/// attempts is at least a millisecond. The failure states the number of
/// attempts and the first failure of the last attempt.
#[track_caller]
pub fn eventually<'a>(
    seat: &'a dyn Seat,
    timeout: Duration,
    interval: Duration,
    body: impl AsyncFn(&dyn Seat) + 'a,
    msg: &'a str,
) -> impl Future<Output = ()> + 'a {
    seat.helper();
    matcher::eventually(Run::begin(seat), seat, MODE, timeout, interval, body, msg)
}

/// Ends the test when `predicate` does not become true within `timeout` on
/// tokio's clock.
///
/// The predicate is retried with a backoff that starts at a millisecond and
/// doubles, up to a quarter of the timeout. The failure states the number of
/// attempts.
#[track_caller]
pub fn eventually_true<'a>(
    seat: &'a dyn Seat,
    timeout: Duration,
    predicate: impl AsyncFn() -> bool + 'a,
    msg: &'a str,
) -> impl Future<Output = ()> + 'a {
    seat.helper();
    matcher::eventually_true(Run::of(seat), MODE, timeout, predicate, msg)
}

/// Ends the test when a task that `body` spawned on the runtime still runs
/// half a second after the body completes.
///
/// The assertion counts the live tasks of the runtime that runs the test, on
/// either side of the body, and the failure states the difference. Tokio
/// names no task, so the count is all that the failure states. A task that
/// another test spawns on the same runtime counts too, and a task that ends
/// while the body runs hides a leaked task. Outside a tokio runtime the call
/// ends with a fault.
#[track_caller]
pub fn no_task_leaks<'a>(
    seat: &'a dyn Seat,
    body: impl AsyncFnOnce() + 'a,
    msg: &'a str,
) -> impl Future<Output = ()> + 'a {
    seat.helper();
    matcher::no_task_leaks(Run::of(seat), MODE, body, msg)
}
