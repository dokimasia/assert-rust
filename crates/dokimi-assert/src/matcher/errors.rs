// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! Failures that a call returns, as the error of a `Result`.
//!
//! Matching follows the chain of [`Error::source`], so an error that wraps
//! the sentinel still matches it.

use crate::seat::Seat;
use crate::value::Value;
use crate::verdict::{Mode, Run};
use std::error::Error;
use std::fmt::Debug;

/// The most links of a chain of sources that a match reads, so a chain that
/// loops back on itself ends.
const MAX_SOURCES: usize = 100;

/// Returns every error of the chain of `error`, starting with `error` itself.
#[must_use]
pub fn chain<'a>(error: &'a (dyn Error + 'static)) -> Vec<&'a (dyn Error + 'static)> {
    let mut found: Vec<&(dyn Error + 'static)> = Vec::new();
    let mut current = Some(error);
    while let Some(link) = current {
        // A wrapper and the source that is its first field have one address,
        // so a repeated link is one with the same address and the same type.
        if found.len() >= MAX_SOURCES || found.iter().any(|seen| std::ptr::eq(*seen, link)) {
            break;
        }
        found.push(link);
        current = link.source();
    }
    found
}

/// Returns `true` when a link of the chain of `error` is a `T` equal to
/// `target`.
pub(crate) fn matches_target<T>(error: &(dyn Error + 'static), target: &T) -> bool
where
    T: Error + PartialEq + 'static,
{
    chain(error)
        .into_iter()
        .any(|link| link.downcast_ref::<T>().is_some_and(|held| held == target))
}

/// Reports whether `got` is `Ok`.
#[track_caller]
pub(crate) fn no_error<T, E: Debug>(seat: &dyn Seat, mode: Mode, got: &Result<T, E>, msg: &str) {
    seat.helper();
    let run = Run::of(seat);
    if let Err(error) = got {
        run.fail(mode, "err-absent", msg, vec![("got", Value::debug(error))]);
        return;
    }
    run.pass(mode, "err-absent", msg);
}

/// Reports whether `got` is `Err`.
#[track_caller]
pub(crate) fn has_error<T: Debug, E>(seat: &dyn Seat, mode: Mode, got: &Result<T, E>, msg: &str) {
    seat.helper();
    let run = Run::of(seat);
    if got.is_ok() {
        run.fail(mode, "err-present", msg, vec![]);
        return;
    }
    run.pass(mode, "err-present", msg);
}

/// Returns the value of an error that a failure states: the error's `Debug`,
/// and null for no error.
fn stated(error: Option<&(dyn Error + 'static)>) -> Value {
    error.map_or(Value::Null, Value::debug)
}

/// Reports whether a link of the chain of `error` is a `T` equal to
/// `target`. No error has no link, so it fails, as the call of a property
/// form that returns `Ok` does.
#[track_caller]
pub(crate) fn error_is<T>(
    seat: &dyn Seat,
    mode: Mode,
    error: Option<&(dyn Error + 'static)>,
    target: &T,
    msg: &str,
) where
    T: Error + PartialEq + 'static,
{
    seat.helper();
    let run = Run::of(seat);
    if !error.is_some_and(|error| matches_target(error, target)) {
        run.fail(
            mode,
            "err-is",
            msg,
            vec![("want", Value::debug(target)), ("got", stated(error))],
        );
        return;
    }
    run.pass(mode, "err-is", msg);
}

/// Reports whether no link of the chain of `error` is a `T` equal to
/// `target`. No error passes.
#[track_caller]
pub(crate) fn error_is_not<T>(
    seat: &dyn Seat,
    mode: Mode,
    error: Option<&(dyn Error + 'static)>,
    target: &T,
    msg: &str,
) where
    T: Error + PartialEq + 'static,
{
    seat.helper();
    let run = Run::of(seat);
    if error.is_some_and(|error| matches_target(error, target)) {
        run.fail(mode, "err-is-not", msg, vec![("got", stated(error))]);
        return;
    }
    run.pass(mode, "err-is-not", msg);
}

/// Returns the first link of the chain of `error` that is a `T`, and reports
/// a failure when no link is. No error fails.
#[track_caller]
pub(crate) fn error_as<'e, T: Error + 'static>(
    seat: &dyn Seat,
    mode: Mode,
    error: Option<&'e (dyn Error + 'static)>,
    msg: &str,
) -> Option<&'e T> {
    seat.helper();
    let run = Run::of(seat);
    let found = error.and_then(|error| {
        chain(error)
            .into_iter()
            .find_map(<dyn Error>::downcast_ref::<T>)
    });
    if found.is_none() {
        run.fail(
            mode,
            "err-as",
            msg,
            vec![
                ("want", Value::from(std::any::type_name::<T>())),
                ("got", stated(error)),
            ],
        );
        return None;
    }
    run.pass(mode, "err-as", msg);
    found
}
