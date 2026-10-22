// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! How a subject behaves, rather than what it returns.
//!
//! A subject that can be cancelled takes a [`Cancel`], the handle that it
//! reads to learn that it should stop. Dropping a future cancels it whatever
//! the future does, so a subject that stopped because it was dropped never
//! read a handle. These assertions hand the subject a handle and read what
//! it returns.

use super::errors::chain;
use super::panics::caught;
use crate::cancel::{Cancel, Stop};
use crate::seat::Seat;
use crate::value::Value;
use crate::verdict::{Mode, Run};
use std::error::Error;
use std::fmt::Debug;
use std::panic::resume_unwind;
use std::sync::mpsc;
use std::time::Duration;

/// Returns the value of a duration in a record: its nanoseconds.
#[must_use]
pub fn nanoseconds(duration: Duration) -> Value {
    Value::Int(i128::try_from(duration.as_nanos()).unwrap_or(i128::MAX))
}

/// Hands `body` a handle that stopped for `reason`, and reports whether the
/// body returned an error whose chain contains that reason. The failure
/// states the error it returned instead, the message of its panic, or null
/// when it returned `Ok`.
#[track_caller]
fn honours<E, F>(
    seat: &dyn Seat,
    mode: Mode,
    assertion: &'static str,
    reason: Stop,
    body: F,
    msg: &str,
) where
    E: Error + 'static,
    F: FnOnce(Option<&Cancel>) -> Result<(), E>,
{
    let run = Run::of(seat);
    let handle = Cancel::new();
    match reason {
        Stop::Cancelled => handle.cancel(),
        Stop::DeadlineExceeded => handle.expire(),
    }
    let got = match caught(|| body(Some(&handle))) {
        Ok(Ok(())) => Value::Null,
        Ok(Err(error)) => {
            let read = chain(&error)
                .into_iter()
                .any(|link| link.downcast_ref::<Stop>() == Some(&reason));
            if read {
                run.pass(mode, assertion, msg);
                return;
            }
            Value::debug(&error)
        }
        Err(raised) => Value::from(raised),
    };
    run.fail(mode, assertion, msg, vec![("got", got)]);
}

/// Reports whether a subject that receives a cancelled handle returns an
/// error whose chain contains [`Stop::Cancelled`].
#[track_caller]
pub(crate) fn honours_cancellation<E, F>(seat: &dyn Seat, mode: Mode, body: F, msg: &str)
where
    E: Error + 'static,
    F: FnOnce(Option<&Cancel>) -> Result<(), E>,
{
    seat.helper();
    honours(
        seat,
        mode,
        "honours-cancellation",
        Stop::Cancelled,
        body,
        msg,
    );
}

/// Reports whether a subject that receives an expired handle returns an
/// error whose chain contains [`Stop::DeadlineExceeded`].
#[track_caller]
pub(crate) fn honours_deadline<E, F>(seat: &dyn Seat, mode: Mode, body: F, msg: &str)
where
    E: Error + 'static,
    F: FnOnce(Option<&Cancel>) -> Result<(), E>,
{
    seat.helper();
    honours(
        seat,
        mode,
        "honours-deadline",
        Stop::DeadlineExceeded,
        body,
        msg,
    );
}

/// Reports whether a subject that receives no handle returns without a
/// panic. An error of the subject's own passes.
#[track_caller]
pub(crate) fn none_handle_safe<E, F>(seat: &dyn Seat, mode: Mode, body: F, msg: &str)
where
    E: Debug,
    F: FnOnce(Option<&Cancel>) -> Result<(), E>,
{
    seat.helper();
    let run = Run::of(seat);
    if let Err(raised) = caught(|| body(None)) {
        run.fail(
            mode,
            "nil-context-safe",
            msg,
            vec![("got", Value::from(raised))],
        );
        return;
    }
    run.pass(mode, "nil-context-safe", msg);
}

/// Reports whether `body` returns within `within`.
///
/// The body runs on a thread of its own, with a handle that expires when the
/// duration passes on the platform clock. A body that has not returned by
/// then fails, with `got` the time waited, and the assertion does not wait
/// for it. A body that returns is measured on the seat's clock, which a test
/// can control. A panic of the body before the duration passes panics again
/// on the calling thread.
#[track_caller]
pub(crate) fn completes_within<E, F>(
    seat: &dyn Seat,
    mode: Mode,
    within: Duration,
    body: F,
    msg: &str,
) where
    E: Debug + Send + 'static,
    F: FnOnce(Option<&Cancel>) -> Result<(), E> + Send + 'static,
{
    seat.helper();
    let run = Run::of(seat);
    let clock = seat.clock();
    let started = clock.now();
    let started_waiting = std::time::Instant::now();
    let handle = Cancel::new();
    let subject = handle.clone();
    let (report, outcome) = mpsc::channel();
    std::thread::spawn(move || {
        let returned =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| body(Some(&subject))));
        // The receiver is gone when the assertion stopped waiting.
        let _ = report.send(returned.map(drop));
    });
    match outcome.recv_timeout(within) {
        Ok(Ok(())) => {
            let elapsed = clock.now().saturating_sub(started);
            if elapsed > within {
                run.fail(
                    mode,
                    "completes-within",
                    msg,
                    vec![("want", nanoseconds(within)), ("got", waited(elapsed))],
                );
                return;
            }
            run.pass(mode, "completes-within", msg);
        }
        Ok(Err(payload)) => resume_unwind(payload),
        Err(_) => {
            handle.expire();
            run.fail(
                mode,
                "completes-within",
                msg,
                vec![
                    ("want", nanoseconds(within)),
                    ("got", waited(started_waiting.elapsed())),
                ],
            );
        }
    }
}

/// Returns the value of a time that an assertion waited, in a record: the
/// nanoseconds of the duration rounded to the nearest millisecond, with a
/// half rounded up.
#[must_use]
pub fn waited(duration: Duration) -> Value {
    nanoseconds(Duration::from_millis(
        u64::try_from((duration.as_nanos() + 500_000) / 1_000_000).unwrap_or(u64::MAX),
    ))
}

/// Reports whether `body` leaves what `observe` reads unchanged.
#[track_caller]
pub(crate) fn is_pure<S, O, F>(seat: &dyn Seat, mode: Mode, observe: O, body: F, msg: &str)
where
    S: PartialEq + Debug,
    O: Fn() -> S,
    F: FnOnce(),
{
    seat.helper();
    let run = Run::of(seat);
    let before = observe();
    body();
    let after = observe();
    if after != before {
        run.fail(
            mode,
            "pure",
            msg,
            vec![
                ("want", Value::debug(&before)),
                ("got", Value::debug(&after)),
            ],
        );
        return;
    }
    run.pass(mode, "pure", msg);
}
