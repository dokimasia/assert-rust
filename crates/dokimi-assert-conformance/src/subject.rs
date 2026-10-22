// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The subjects that a corpus case names in place of a value, built
//! natively, as the `subjects` section of the definition states them.

use crate::corpus::{Library, Surface};
use dokimi_assert::{Cancel, Seat, Stop, check, soft};
use std::error::Error;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering};
use std::time::Duration;

/// Calls the assertion `$name` on the surface `$surface` with the arguments
/// that follow.
macro_rules! call {
    ($surface:expr, $name:ident ( $($arg:expr),* $(,)? )) => {
        match $surface {
            Surface::Check => check::$name($($arg),*),
            Surface::Soft => soft::$name($($arg),*),
        }
    };
}

/// Calls the assertion `$name` of `dokimi-assert-tokio` on the surface
/// `$surface` with the arguments that follow, and runs its future to its end.
macro_rules! call_future {
    ($surface:expr, $name:ident ( $($arg:expr),* $(,)? )) => {
        match $surface {
            Surface::Check => block_on(dokimi_assert_tokio::check::$name($($arg),*)),
            Surface::Soft => block_on(dokimi_assert_tokio::soft::$name($($arg),*)),
        }
    };
}

/// Runs `future` to its end on a tokio runtime whose clock starts paused, so
/// a wait of the future does not spend real time.
fn block_on(future: impl Future<Output = ()>) {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .start_paused(true)
        .build()
        .expect("a runtime on the current thread builds")
        .block_on(future);
}

/// A failure of a subject's own, unrelated to any handle it was given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Own;

impl fmt::Display for Own {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a failure of the subject's own")
    }
}

impl Error for Own {}

/// The closed sentinel of a resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Closed;

impl fmt::Display for Closed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the resource is closed")
    }
}

impl Error for Closed {}

/// The handle-reading subject: the reason that the handle gives, or success
/// when the handle is absent or still running.
pub(crate) fn reads_handle(cancel: Option<&Cancel>) -> Result<(), Stop> {
    match cancel.and_then(Cancel::reason) {
        Some(reason) => Err(reason),
        None => Ok(()),
    }
}

/// One case's subject: the surface, the assertion, the subject's kind, the
/// seat and the contract.
struct Subject<'a> {
    surface: Surface,
    assertion: &'a str,
    kind: &'a str,
    seat: &'a dyn Seat,
    msg: &'a str,
}

/// Runs the case of `assertion` over the subject `kind` on `surface` of
/// `library`, on `seat` and with `msg` as the contract. Returns `false` when
/// the driver builds no such subject for the assertion.
#[must_use]
pub fn run(
    library: Library,
    surface: Surface,
    assertion: &str,
    kind: &str,
    seat: &dyn Seat,
    msg: &str,
) -> bool {
    let subject = Subject {
        surface,
        assertion,
        kind,
        seat,
        msg,
    };
    match library {
        Library::Core => {
            handles(&subject)
                || waits(&subject)
                || state(&subject)
                || combinations(&subject)
                || sequences(&subject)
        }
        Library::Tokio => futures(&subject),
    }
}

/// The subjects of the assertions of `dokimi-assert-tokio`, whose subject is
/// a future.
fn futures(subject: &Subject<'_>) -> bool {
    let Subject {
        surface, seat, msg, ..
    } = *subject;
    let calls = AtomicUsize::new(0);
    let settles = async |attempt: &dyn Seat| {
        let settled = calls.fetch_add(1, Ordering::SeqCst) >= 2;
        check::is_true(attempt, settled, "the subject settles on its third attempt");
    };
    let never =
        async |attempt: &dyn Seat| check::is_true(attempt, false, "the subject never settles");
    let (timeout, interval) = (Duration::from_secs(1), Duration::from_millis(10));
    match (subject.assertion, subject.kind) {
        ("honours-cancellation", "reads-handle") => {
            call_future!(
                surface,
                honours_cancellation(seat, async |cancel| reads_handle(cancel), msg)
            );
        }
        ("honours-cancellation", "ignores-handle") => {
            call_future!(
                surface,
                honours_cancellation(seat, async |_| Ok::<(), Own>(()), msg)
            );
        }
        ("honours-cancellation", "fails-otherwise") => {
            call_future!(
                surface,
                honours_cancellation(seat, async |_| Err::<(), Own>(Own), msg)
            );
        }
        ("honours-deadline", "reads-handle") => {
            call_future!(
                surface,
                honours_deadline(seat, async |cancel| reads_handle(cancel), msg)
            );
        }
        ("honours-deadline", "ignores-handle") => {
            call_future!(
                surface,
                honours_deadline(seat, async |_| Ok::<(), Own>(()), msg)
            );
        }
        ("eventually", "settles-after") => {
            call_future!(surface, eventually(seat, timeout, interval, settles, msg));
        }
        ("eventually", "never-settles") => {
            call_future!(surface, eventually(seat, timeout, interval, never, msg));
        }
        ("eventually-true", "settles-after") => {
            let settles = async || calls.fetch_add(1, Ordering::SeqCst) >= 2;
            call_future!(surface, eventually_true(seat, timeout, settles, msg));
        }
        ("eventually-true", "never-settles") => {
            call_future!(surface, eventually_true(seat, timeout, async || false, msg));
        }
        _ => return false,
    }
    true
}

/// The subjects of the assertions on handles and panics.
fn handles(subject: &Subject<'_>) -> bool {
    let Subject {
        surface, seat, msg, ..
    } = *subject;
    match (subject.assertion, subject.kind) {
        ("honours-cancellation", "reads-handle") => {
            call!(surface, honours_cancellation(seat, reads_handle, msg));
        }
        ("honours-cancellation", "ignores-handle") => {
            call!(
                surface,
                honours_cancellation(seat, |_| Ok::<(), Own>(()), msg)
            );
        }
        ("honours-cancellation", "fails-otherwise") => {
            call!(
                surface,
                honours_cancellation(seat, |_| Err::<(), Own>(Own), msg)
            );
        }
        ("honours-deadline", "reads-handle") => {
            call!(surface, honours_deadline(seat, reads_handle, msg));
        }
        ("honours-deadline", "ignores-handle") => {
            call!(surface, honours_deadline(seat, |_| Ok::<(), Own>(()), msg));
        }
        ("nil-context-safe", "returns-ok") => {
            call!(surface, none_handle_safe(seat, |_| Ok::<(), Own>(()), msg));
        }
        ("nil-context-safe", "dereferences-handle") => call!(
            surface,
            none_handle_safe(
                seat,
                |cancel: Option<&Cancel>| {
                    let _ = cancel
                        .expect("the subject dereferences the handle")
                        .reason();
                    Ok::<(), Own>(())
                },
                msg,
            )
        ),
        ("throws", "raises") => {
            let _ = call!(surface, panics(seat, || panic!("the subject raises"), msg));
        }
        ("throws", "returns-ok") => {
            let _ = call!(surface, panics(seat, || {}, msg));
        }
        ("not-throws", "returns-ok") => call!(surface, does_not_panic(seat, || {}, msg)),
        ("not-throws", "raises") => call!(
            surface,
            does_not_panic(seat, || panic!("the subject raises"), msg)
        ),
        _ => return false,
    }
    true
}

/// The subjects of the assertions that retry.
fn waits(subject: &Subject<'_>) -> bool {
    let Subject {
        surface, seat, msg, ..
    } = *subject;
    let calls = AtomicUsize::new(0);
    match (subject.assertion, subject.kind) {
        ("eventually", "settles-after") => call!(
            surface,
            eventually(
                seat,
                Duration::from_secs(1),
                Duration::from_millis(10),
                |attempt: &dyn Seat| {
                    let settled = calls.fetch_add(1, Ordering::SeqCst) >= 2;
                    check::is_true(attempt, settled, "the subject settles on its third attempt");
                },
                msg,
            )
        ),
        ("eventually", "never-settles") => call!(
            surface,
            eventually(
                seat,
                Duration::from_secs(1),
                Duration::from_millis(10),
                |attempt: &dyn Seat| check::is_true(attempt, false, "the subject never settles"),
                msg,
            )
        ),
        ("eventually-true", "settles-after") => call!(
            surface,
            eventually_true(
                seat,
                Duration::from_secs(1),
                || calls.fetch_add(1, Ordering::SeqCst) >= 2,
                msg
            )
        ),
        ("eventually-true", "never-settles") => {
            call!(
                surface,
                eventually_true(seat, Duration::from_secs(1), || false, msg)
            );
        }
        _ => return false,
    }
    true
}

/// The subjects of the assertions on the state that a call leaves.
fn state(subject: &Subject<'_>) -> bool {
    let Subject {
        surface, seat, msg, ..
    } = *subject;
    let counter = AtomicI64::new(0);
    let observe = || counter.load(Ordering::SeqCst);
    let leave = || {
        let _ = counter.load(Ordering::SeqCst);
    };
    let add = || {
        counter.fetch_add(1, Ordering::SeqCst);
    };
    let set = |input: &i64| {
        counter.store(*input, Ordering::SeqCst);
        Ok::<(), Own>(())
    };
    let accumulate = |_: &i64| {
        counter.fetch_add(1, Ordering::SeqCst);
        Ok::<(), Own>(())
    };
    match (subject.assertion, subject.kind) {
        ("pure", "leaves-state-alone") => call!(surface, is_pure(seat, observe, leave, msg)),
        ("pure", "accumulates") => call!(surface, is_pure(seat, observe, add, msg)),
        ("not-pure", "accumulates") => call!(surface, is_not_pure(seat, observe, add, msg)),
        ("not-pure", "leaves-state-alone") => {
            call!(surface, is_not_pure(seat, observe, leave, msg));
        }
        ("idempotent", "sets-value") => call!(surface, is_idempotent(seat, set, 7, observe, msg)),
        ("idempotent", "accumulates") => {
            call!(surface, is_idempotent(seat, accumulate, 7, observe, msg));
        }
        ("accumulates", "accumulates") => {
            call!(surface, accumulates(seat, accumulate, 7, observe, msg));
        }
        ("accumulates", "sets-value") => call!(surface, accumulates(seat, set, 7, observe, msg)),
        ("deterministic", "returns-ok") => {
            call!(
                surface,
                is_deterministic(seat, |input: &i64| Ok::<i64, Own>(*input), 1, msg)
            );
        }
        ("deterministic", "counts-calls") => call!(
            surface,
            is_deterministic(
                seat,
                |_: &i64| Ok::<i64, Own>(counter.fetch_add(1, Ordering::SeqCst) + 1),
                1,
                msg
            )
        ),
        _ => return false,
    }
    true
}

/// The subjects of the assertions on how calls combine.
fn combinations(subject: &Subject<'_>) -> bool {
    let Subject {
        surface, seat, msg, ..
    } = *subject;
    let parse = |text: &String| text.parse::<i64>();
    match (subject.assertion, subject.kind) {
        ("commutative", "adds") => call!(
            surface,
            is_commutative(seat, |a: &i64, b: &i64| a + b, 2, 3, msg)
        ),
        ("commutative", "subtracts") => call!(
            surface,
            is_commutative(seat, |a: &i64, b: &i64| a - b, 2, 3, msg)
        ),
        ("associative", "adds") => call!(
            surface,
            is_associative(seat, |a: &i64, b: &i64| a + b, 2, 3, 5, msg)
        ),
        ("associative", "subtracts") => {
            call!(
                surface,
                is_associative(seat, |a: &i64, b: &i64| a - b, 2, 3, 5, msg)
            );
        }
        ("round-trip", "renders-decimal") => call!(
            surface,
            round_trip(
                seat,
                |input: &i64| Ok::<String, Own>(input.to_string()),
                parse,
                -42,
                msg
            )
        ),
        ("round-trip", "drops-the-sign") => call!(
            surface,
            round_trip(
                seat,
                |input: &i64| Ok::<String, Own>(input.abs().to_string()),
                parse,
                -42,
                msg
            )
        ),
        _ => return false,
    }
    true
}

/// The subjects of the assertions on sequences of calls and of results.
fn sequences(subject: &Subject<'_>) -> bool {
    let Subject {
        surface, seat, msg, ..
    } = *subject;
    let counter = AtomicI64::new(0);
    let calls = AtomicUsize::new(0);
    let in_order = || Ok::<Vec<i64>, Own>(vec![1, 2, 3, 4, 5]);
    match (subject.assertion, subject.kind) {
        ("stable-order", "yields-in-order") => {
            call!(surface, has_stable_order(seat, in_order, msg));
        }
        ("stable-order", "rotates") => call!(
            surface,
            has_stable_order(
                seat,
                || {
                    let mut items = vec![1_i64, 2, 3, 4, 5];
                    items.rotate_left(calls.fetch_add(1, Ordering::SeqCst) % 5);
                    Ok::<Vec<i64>, Own>(items)
                },
                msg,
            )
        ),
        ("no-duplicates", "yields-in-order") => call!(surface, no_duplicates(seat, in_order, msg)),
        ("no-duplicates", "repeats-an-element") => {
            call!(
                surface,
                no_duplicates(seat, || Ok::<Vec<i64>, Own>(vec![1, 2, 2, 3]), msg)
            );
        }
        ("no-duplicates", "yields-two-equal-objects") => {
            call!(
                surface,
                no_duplicates(
                    seat,
                    || Ok::<Vec<Box<i64>>, Own>(vec![Box::new(1), Box::new(1)]),
                    msg
                )
            );
        }
        ("monotonic", "accumulates") => call!(
            surface,
            is_monotonic(
                seat,
                || counter.load(Ordering::SeqCst),
                || {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Ok::<(), Own>(())
                },
                5,
                msg,
            )
        ),
        ("monotonic", "wraps-around") => call!(
            surface,
            is_monotonic(
                seat,
                || counter.load(Ordering::SeqCst),
                || {
                    let next = (counter.load(Ordering::SeqCst) + 1) % 4;
                    counter.store(next, Ordering::SeqCst);
                    Ok::<(), Own>(())
                },
                5,
                msg,
            )
        ),
        ("total", "returns-ok") => call!(
            surface,
            is_total(seat, |_: &i64| Ok::<(), Own>(()), &[1, 2, 3], msg)
        ),
        ("total", "fails-otherwise") => call!(
            surface,
            is_total(seat, |_: &i64| Err::<(), Own>(Own), &[1, 2, 3], msg)
        ),
        _ => return resources(subject),
    }
    true
}

/// The subjects of the assertions on a resource that closes or breaks.
fn resources(subject: &Subject<'_>) -> bool {
    let Subject {
        surface, seat, msg, ..
    } = *subject;
    let calls = AtomicUsize::new(0);
    match (subject.assertion, subject.kind) {
        ("after-close", kind @ ("refuses-after-close" | "serves-after-close")) => {
            let closed = AtomicBool::new(false);
            let refuses = kind == "refuses-after-close";
            call!(
                surface,
                fails_after_close(
                    seat,
                    || {
                        closed.store(true, Ordering::SeqCst);
                        Ok::<(), Own>(())
                    },
                    || {
                        if refuses && closed.load(Ordering::SeqCst) {
                            return Err(Closed);
                        }
                        Ok(())
                    },
                    &Closed,
                    msg,
                )
            );
        }
        ("poisoned", "never-settles") => call!(
            surface,
            is_poisoned(seat, || {}, || Err::<(), Own>(Own), msg)
        ),
        ("poisoned", "settles-after") => call!(
            surface,
            is_poisoned(
                seat,
                || {},
                || {
                    if calls.fetch_add(1, Ordering::SeqCst) >= 2 {
                        return Ok(());
                    }
                    Err(Own)
                },
                msg,
            )
        ),
        _ => return false,
    }
    true
}
