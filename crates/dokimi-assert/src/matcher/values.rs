// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! Equality, truth and absence.

use crate::seat::Seat;
use crate::value::Value;
use crate::verdict::{Mode, Run};
use std::fmt::Debug;

/// Reports whether `got` and `want` are equal, as `==` compares them.
#[track_caller]
#[expect(
    clippy::needless_pass_by_value,
    reason = "the operands move into the assertion, as they move into assert_eq!"
)]
pub(crate) fn equal<G, W>(seat: &dyn Seat, mode: Mode, got: G, want: W, msg: &str)
where
    G: PartialEq<W> + Debug,
    W: Debug,
{
    seat.helper();
    let run = Run::of(seat);
    if got != want {
        run.fail(
            mode,
            "equal",
            msg,
            vec![("want", Value::debug(&want)), ("got", Value::debug(&got))],
        );
        return;
    }
    run.pass(mode, "equal", msg);
}

/// Reports whether `got` and `want` differ, as `!=` compares them.
#[track_caller]
#[expect(
    clippy::needless_pass_by_value,
    reason = "the operands move into the assertion, as they move into assert_ne!"
)]
pub(crate) fn not_equal<G, W>(seat: &dyn Seat, mode: Mode, got: G, want: W, msg: &str)
where
    G: PartialEq<W> + Debug,
    W: Debug,
{
    seat.helper();
    let run = Run::of(seat);
    if got == want {
        run.fail(mode, "not-equal", msg, vec![("got", Value::debug(&got))]);
        return;
    }
    run.pass(mode, "not-equal", msg);
}

/// Reports whether `condition` holds.
#[track_caller]
pub(crate) fn is_true(seat: &dyn Seat, mode: Mode, condition: bool, msg: &str) {
    seat.helper();
    let run = Run::of(seat);
    if !condition {
        run.fail(mode, "true", msg, vec![]);
        return;
    }
    run.pass(mode, "true", msg);
}

/// Reports whether `condition` does not hold.
#[track_caller]
pub(crate) fn is_false(seat: &dyn Seat, mode: Mode, condition: bool, msg: &str) {
    seat.helper();
    let run = Run::of(seat);
    if condition {
        run.fail(mode, "false", msg, vec![]);
        return;
    }
    run.pass(mode, "false", msg);
}

/// Reports whether `got` is `None`.
#[track_caller]
pub(crate) fn is_none<T: Debug>(seat: &dyn Seat, mode: Mode, got: Option<T>, msg: &str) {
    seat.helper();
    let run = Run::of(seat);
    if let Some(held) = got {
        run.fail(mode, "nil", msg, vec![("got", Value::debug(&held))]);
        return;
    }
    run.pass(mode, "nil", msg);
}

/// Reports whether `got` is `Some`.
#[track_caller]
#[expect(
    clippy::needless_pass_by_value,
    reason = "the option moves into the assertion, as it moves into is_none"
)]
pub(crate) fn is_some<T: Debug>(seat: &dyn Seat, mode: Mode, got: Option<T>, msg: &str) {
    seat.helper();
    let run = Run::of(seat);
    match got {
        Some(_) => run.pass(mode, "not-nil", msg),
        None => run.fail(mode, "not-nil", msg, vec![]),
    }
}
