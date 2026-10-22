// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The relations: properties of a subject that hold across calls, rather than
//! of one value.
//!
//! A callable fails the relation when it returns an error or panics, unless
//! the relation requires a failure, as `fails_after_close` and `is_poisoned`
//! do. The failure states the error or the panic's message in the field of
//! the step that it ended, and null in the field of a step that did not run.

use super::errors::matches_target;
use super::panics::caught;
use crate::seat::Seat;
use crate::value::Value;
use crate::verdict::{Mode, Run};
use std::error::Error;
use std::fmt::Debug;

/// How many times `is_deterministic` calls its subject, `has_stable_order`
/// iterates it and `is_poisoned` reads it. A subject whose results vary can
/// agree with itself by chance in fewer runs.
const REPETITIONS: usize = 32;

/// Runs a step that returns a `Result`, and returns its value, or what ended
/// it: the `Debug` text of its error, or the message of its panic.
fn attempt<T, E: Debug>(step: impl FnOnce() -> Result<T, E>) -> Result<T, Value> {
    match caught(step) {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(Value::debug(&error)),
        Err(raised) => Err(Value::from(raised)),
    }
}

/// Runs a step that cannot return an error, and returns its value or the
/// message of its panic.
fn returning<T>(step: impl FnOnce() -> T) -> Result<T, Value> {
    caught(step).map_err(Value::from)
}

/// Returns the detail of a step that ended at position `i` of a relation that
/// compares `first` and `second`: the raised value is `first` for the first
/// step, and `second` for every later one.
fn ended_at(i: usize, raised: Value) -> Vec<(&'static str, Value)> {
    if i == 0 {
        vec![("first", raised), ("second", Value::Null)]
    } else {
        vec![("first", Value::Null), ("second", raised)]
    }
}

/// Reports the first of two steps that ends, or a failure of both values
/// when they differ.
#[track_caller]
fn settle<T: PartialEq + Debug>(
    run: &Run<'_>,
    mode: Mode,
    assertion: &'static str,
    msg: &str,
    first: Result<T, Value>,
    second: impl FnOnce() -> Result<T, Value>,
) {
    let first = match first {
        Ok(value) => value,
        Err(raised) => return run.fail(mode, assertion, msg, ended_at(0, raised)),
    };
    let second = match second() {
        Ok(value) => value,
        Err(raised) => return run.fail(mode, assertion, msg, ended_at(1, raised)),
    };
    if second != first {
        run.fail(
            mode,
            assertion,
            msg,
            vec![
                ("first", Value::debug(&first)),
                ("second", Value::debug(&second)),
            ],
        );
        return;
    }
    run.pass(mode, assertion, msg);
}

/// Runs `produce` [`REPETITIONS`] times, and reports the first result that
/// differs from the first result, or the first run that ends.
#[track_caller]
fn agree<T: PartialEq + Debug>(
    run: &Run<'_>,
    mode: Mode,
    assertion: &'static str,
    msg: &str,
    mut produce: impl FnMut() -> Result<T, Value>,
) {
    let mut first: Option<T> = None;
    for i in 0..REPETITIONS {
        let next = match produce() {
            Ok(value) => value,
            Err(raised) => return run.fail(mode, assertion, msg, ended_at(i, raised)),
        };
        match &first {
            None => first = Some(next),
            Some(held) if next != *held => {
                run.fail(
                    mode,
                    assertion,
                    msg,
                    vec![
                        ("first", Value::debug(held)),
                        ("second", Value::debug(&next)),
                    ],
                );
                return;
            }
            Some(_) => {}
        }
    }
    run.pass(mode, assertion, msg);
}

/// Reports whether a second call of `call` with `input` leaves `observe`
/// reading what the first call left. `first` is the reading after one call,
/// and `second` the reading after two.
#[track_caller]
#[expect(
    clippy::needless_pass_by_value,
    reason = "the input moves into the relation, which lends it to each call"
)]
pub(crate) fn is_idempotent<I, S, E>(
    seat: &dyn Seat,
    mode: Mode,
    mut call: impl FnMut(&I) -> Result<(), E>,
    input: I,
    mut observe: impl FnMut() -> S,
    msg: &str,
) where
    S: PartialEq + Debug,
    E: Debug,
{
    seat.helper();
    let run = Run::of(seat);
    let first = attempt(|| call(&input).map(|()| observe()));
    settle(&run, mode, "idempotent", msg, first, || {
        attempt(|| call(&input).map(|()| observe()))
    });
}

/// Reports whether each call of `call` with `input` changes the count that
/// `observe` reads by the same amount, and the first call changes it.
/// `first` and `second` are the changes of the two calls.
#[track_caller]
#[expect(
    clippy::needless_pass_by_value,
    reason = "the input moves into the relation, which lends it to each call"
)]
pub(crate) fn accumulates<I, E: Debug>(
    seat: &dyn Seat,
    mode: Mode,
    mut call: impl FnMut(&I) -> Result<(), E>,
    input: I,
    mut observe: impl FnMut() -> i64,
    msg: &str,
) {
    seat.helper();
    let run = Run::of(seat);
    let first = attempt(|| {
        let before = observe();
        call(&input).map(|()| (before, observe()))
    });
    let (before, after) = match first {
        Ok(readings) => readings,
        Err(raised) => return run.fail(mode, "accumulates", msg, ended_at(0, raised)),
    };
    let second = attempt(|| call(&input).map(|()| observe()));
    let last = match second {
        Ok(reading) => reading,
        Err(raised) => return run.fail(mode, "accumulates", msg, ended_at(1, raised)),
    };
    let (once, twice) = (
        i128::from(after) - i128::from(before),
        i128::from(last) - i128::from(after),
    );
    if once == 0 || once != twice {
        run.fail(
            mode,
            "accumulates",
            msg,
            vec![("first", Value::Int(once)), ("second", Value::Int(twice))],
        );
        return;
    }
    run.pass(mode, "accumulates", msg);
}

/// Reports whether 32 calls of `call` with `input` return equal results.
/// `first` is the first result, and `second` the first result that differs.
#[track_caller]
#[expect(
    clippy::needless_pass_by_value,
    reason = "the input moves into the relation, which lends it to each call"
)]
pub(crate) fn is_deterministic<I, O, E>(
    seat: &dyn Seat,
    mode: Mode,
    mut call: impl FnMut(&I) -> Result<O, E>,
    input: I,
    msg: &str,
) where
    O: PartialEq + Debug,
    E: Debug,
{
    seat.helper();
    let run = Run::of(seat);
    agree(&run, mode, "deterministic", msg, || {
        attempt(|| call(&input))
    });
}

/// Reports whether `combine(a, b)` equals `combine(b, a)`. `first` is
/// `combine(a, b)`, and `second` is `combine(b, a)`.
#[track_caller]
#[expect(
    clippy::needless_pass_by_value,
    reason = "the operands move into the relation, which lends them to each call"
)]
pub(crate) fn is_commutative<T, R>(
    seat: &dyn Seat,
    mode: Mode,
    mut combine: impl FnMut(&T, &T) -> R,
    a: T,
    b: T,
    msg: &str,
) where
    R: PartialEq + Debug,
{
    seat.helper();
    let run = Run::of(seat);
    let first = returning(|| combine(&a, &b));
    settle(&run, mode, "commutative", msg, first, || {
        returning(|| combine(&b, &a))
    });
}

/// Reports whether `combine(combine(a, b), c)` equals
/// `combine(a, combine(b, c))`. `first` is the left grouping, and `second` the
/// right one.
#[track_caller]
#[expect(
    clippy::needless_pass_by_value,
    reason = "the operands move into the relation, which lends them to each call"
)]
pub(crate) fn is_associative<T>(
    seat: &dyn Seat,
    mode: Mode,
    mut combine: impl FnMut(&T, &T) -> T,
    a: T,
    b: T,
    c: T,
    msg: &str,
) where
    T: PartialEq + Debug,
{
    seat.helper();
    let run = Run::of(seat);
    let first = returning(|| {
        let ab = combine(&a, &b);
        combine(&ab, &c)
    });
    settle(&run, mode, "associative", msg, first, || {
        returning(|| {
            let bc = combine(&b, &c);
            combine(&a, &bc)
        })
    });
}

/// Reports whether `inverse` of `forward` of `input` returns `input`. `want`
/// is the input, and `got` what came back. An error or a panic of either
/// conversion fails with `want` null and `got` what ended it.
#[track_caller]
#[expect(
    clippy::needless_pass_by_value,
    reason = "the input moves into the relation, which lends it to the conversions"
)]
pub(crate) fn round_trip<I, O, E, F>(
    seat: &dyn Seat,
    mode: Mode,
    forward: impl FnOnce(&I) -> Result<O, E>,
    inverse: impl FnOnce(&O) -> Result<I, F>,
    input: I,
    msg: &str,
) where
    I: PartialEq + Debug,
    E: Debug,
    F: Debug,
{
    seat.helper();
    let run = Run::of(seat);
    let back = caught(|| match forward(&input) {
        Ok(encoded) => inverse(&encoded).map_err(|error| Value::debug(&error)),
        Err(error) => Err(Value::debug(&error)),
    });
    let back = match back {
        Ok(Ok(value)) => value,
        Ok(Err(raised)) => {
            return run.fail(
                mode,
                "round-trip",
                msg,
                vec![("want", Value::Null), ("got", raised)],
            );
        }
        Err(raised) => {
            return run.fail(
                mode,
                "round-trip",
                msg,
                vec![("want", Value::Null), ("got", Value::from(raised))],
            );
        }
    };
    if back != input {
        run.fail(
            mode,
            "round-trip",
            msg,
            vec![("want", Value::debug(&input)), ("got", Value::debug(&back))],
        );
        return;
    }
    run.pass(mode, "round-trip", msg);
}

/// Reports whether 32 iterations of `iterate` yield equal sequences. `first`
/// is the first sequence, and `second` the first sequence that differs.
#[track_caller]
pub(crate) fn has_stable_order<T, E>(
    seat: &dyn Seat,
    mode: Mode,
    mut iterate: impl FnMut() -> Result<Vec<T>, E>,
    msg: &str,
) where
    T: PartialEq + Debug,
    E: Debug,
{
    seat.helper();
    let run = Run::of(seat);
    agree(&run, mode, "stable-order", msg, || attempt(&mut iterate));
}

/// Reports whether one iteration of `iterate` yields each element at most
/// once. `got` is the first element equal to an earlier one, and `index` its
/// position. An error or a panic fails with `got` what ended it and `index`
/// null.
#[track_caller]
pub(crate) fn no_duplicates<T, E>(
    seat: &dyn Seat,
    mode: Mode,
    iterate: impl FnOnce() -> Result<Vec<T>, E>,
    msg: &str,
) where
    T: PartialEq + Debug,
    E: Debug,
{
    seat.helper();
    let run = Run::of(seat);
    let items = match attempt(iterate) {
        Ok(items) => items,
        Err(raised) => {
            return run.fail(
                mode,
                "no-duplicates",
                msg,
                vec![("got", raised), ("index", Value::Null)],
            );
        }
    };
    for (index, item) in items.iter().enumerate() {
        if items[..index].iter().any(|earlier| earlier == item) {
            run.fail(
                mode,
                "no-duplicates",
                msg,
                vec![("got", Value::debug(item)), ("index", Value::from(index))],
            );
            return;
        }
    }
    run.pass(mode, "no-duplicates", msg);
}

/// Returns `true` for a reading that has no order with itself, such as NaN.
fn is_nan<N: PartialOrd>(reading: &N) -> bool {
    reading.partial_cmp(reading).is_none()
}

/// Reports whether the reading of `observe` never falls and is never NaN
/// while `advance` runs `steps` times. `index` is the position of the first
/// reading that falls or is NaN, the reading before the first step at
/// position 0. `first` is the reading before it, and `second` that reading.
#[track_caller]
pub(crate) fn is_monotonic<N, E>(
    seat: &dyn Seat,
    mode: Mode,
    mut observe: impl FnMut() -> N,
    mut advance: impl FnMut() -> Result<(), E>,
    steps: usize,
    msg: &str,
) where
    N: PartialOrd + Debug,
    E: Debug,
{
    seat.helper();
    let run = Run::of(seat);
    let ended = |raised: Value| {
        vec![
            ("index", Value::Null),
            ("first", Value::Null),
            ("second", raised),
        ]
    };
    let mut previous = match returning(&mut observe) {
        Ok(reading) => reading,
        Err(raised) => return run.fail(mode, "monotonic", msg, ended(raised)),
    };
    if is_nan(&previous) {
        run.fail(
            mode,
            "monotonic",
            msg,
            vec![
                ("index", Value::Int(0)),
                ("first", Value::Null),
                ("second", Value::debug(&previous)),
            ],
        );
        return;
    }
    for index in 1..=steps {
        let next = match attempt(|| advance().map(|()| observe())) {
            Ok(reading) => reading,
            Err(raised) => return run.fail(mode, "monotonic", msg, ended(raised)),
        };
        if is_nan(&next) || next < previous {
            run.fail(
                mode,
                "monotonic",
                msg,
                vec![
                    ("index", Value::from(index)),
                    ("first", Value::debug(&previous)),
                    ("second", Value::debug(&next)),
                ],
            );
            return;
        }
        previous = next;
    }
    run.pass(mode, "monotonic", msg);
}

/// Reports whether `call` succeeds for every element of `domain`, in order.
/// `index` is the position of the first element for which it fails, and
/// `got` what ended it.
#[track_caller]
pub(crate) fn is_total<I, E: Debug>(
    seat: &dyn Seat,
    mode: Mode,
    mut call: impl FnMut(&I) -> Result<(), E>,
    domain: &[I],
    msg: &str,
) {
    seat.helper();
    let run = Run::of(seat);
    for (index, element) in domain.iter().enumerate() {
        if let Err(raised) = attempt(|| call(element)) {
            run.fail(
                mode,
                "total",
                msg,
                vec![("index", Value::from(index)), ("got", raised)],
            );
            return;
        }
    }
    run.pass(mode, "total", msg);
}

/// Reports whether `body` changes what `observe` reads. `got` is the reading
/// that did not change, or what ended a call.
#[track_caller]
pub(crate) fn is_not_pure<S, O, F>(seat: &dyn Seat, mode: Mode, observe: O, body: F, msg: &str)
where
    S: PartialEq + Debug,
    O: Fn() -> S,
    F: FnOnce(),
{
    seat.helper();
    let run = Run::of(seat);
    let readings = returning(|| {
        let before = observe();
        body();
        (before, observe())
    });
    match readings {
        Err(raised) => run.fail(mode, "not-pure", msg, vec![("got", raised)]),
        Ok((before, after)) if after == before => {
            run.fail(mode, "not-pure", msg, vec![("got", Value::debug(&after))]);
        }
        Ok(_) => run.pass(mode, "not-pure", msg),
    }
}

/// Reports whether `call`, after `close`, returns an error whose chain
/// contains `sentinel`. `want` is the sentinel, and `got` what the call
/// returned, null for `Ok`. An error or a panic of `close` fails with `want`
/// null and `got` what ended it.
#[track_caller]
pub(crate) fn fails_after_close<T, E, C>(
    seat: &dyn Seat,
    mode: Mode,
    close: impl FnOnce() -> Result<(), C>,
    call: impl FnOnce() -> Result<(), E>,
    sentinel: &T,
    msg: &str,
) where
    T: Error + PartialEq + 'static,
    E: Error + 'static,
    C: Debug,
{
    seat.helper();
    let run = Run::of(seat);
    if let Err(raised) = attempt(close) {
        run.fail(
            mode,
            "after-close",
            msg,
            vec![("want", Value::Null), ("got", raised)],
        );
        return;
    }
    let got = match caught(call) {
        Ok(Ok(())) => Value::Null,
        Ok(Err(error)) if matches_target(&error, sentinel) => {
            run.pass(mode, "after-close", msg);
            return;
        }
        Ok(Err(error)) => Value::debug(&error),
        Err(raised) => Value::from(raised),
    };
    run.fail(
        mode,
        "after-close",
        msg,
        vec![("want", Value::debug(sentinel)), ("got", got)],
    );
}

/// Reports whether, after `induce`, 32 readings of `observe` each return an
/// error. `index` is the position of the first reading that does not, and
/// `got` what it returned. An error or a panic of `induce` fails with
/// `index` null and `got` what ended it.
#[track_caller]
pub(crate) fn is_poisoned<E: Debug>(
    seat: &dyn Seat,
    mode: Mode,
    induce: impl FnOnce(),
    mut observe: impl FnMut() -> Result<(), E>,
    msg: &str,
) {
    seat.helper();
    let run = Run::of(seat);
    if let Err(raised) = returning(induce) {
        run.fail(
            mode,
            "poisoned",
            msg,
            vec![("index", Value::Null), ("got", raised)],
        );
        return;
    }
    for index in 0..REPETITIONS {
        if let Ok(Ok(())) = caught(&mut observe) {
            run.fail(
                mode,
                "poisoned",
                msg,
                vec![("index", Value::from(index)), ("got", Value::Null)],
            );
            return;
        }
    }
    run.pass(mode, "poisoned", msg);
}
