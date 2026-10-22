// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! Assertions over futures that record a failure and let the test continue.
//!
//! The assertions are those of [`check`](crate::check), under the same names
//! and with the same signatures. A failure goes to the seat's `record`, so
//! one run of a test reports every failure.

use crate::matcher;
use dokimi_assert::__private::{Mode, Run};
use dokimi_assert::{Cancel, Seat, Stop};
use std::error::Error;
use std::future::Future;
use std::time::Duration;

const MODE: Mode = Mode::Soft;

/// Records a failure when a subject that receives a cancelled handle does
/// not return an error whose chain contains [`Stop::Cancelled`].
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

/// Records a failure when a subject that receives an expired handle does not
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

/// Records a failure when `body` has not completed within `within` on
/// tokio's clock. The assertion drops a body that has not completed by then.
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

/// Records a failure when `body` does not pass within `timeout`, retried
/// every `interval` on tokio's clock.
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

/// Records a failure when `predicate` does not become true within `timeout`
/// on tokio's clock.
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

/// Records a failure when a task that `body` spawned on the runtime still
/// runs half a second after the body completes.
#[track_caller]
pub fn no_task_leaks<'a>(
    seat: &'a dyn Seat,
    body: impl AsyncFnOnce() + 'a,
    msg: &'a str,
) -> impl Future<Output = ()> + 'a {
    seat.helper();
    matcher::no_task_leaks(Run::of(seat), MODE, body, msg)
}
