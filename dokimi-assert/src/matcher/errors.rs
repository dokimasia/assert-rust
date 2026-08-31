//! Assertions about a failure handed back rather than raised.
//!
//! Rust states failure in the type: a call that can fail answers a [`Result`], and the
//! caller holds the error as a value. There is nothing to catch, so these read a value
//! the way the other assertions do.
//!
//! Matching follows the chain of sources, which is what [`Error::source`] is for and what
//! lets a wrapped failure still be recognised.

use super::report::{Mode, report};
use crate::seat::Seat;
use std::error::Error;
use std::fmt::Debug;

/// How far down a source chain to look before calling it cyclic.
const MAX_SOURCES: usize = 100;

/// Every error in the chain, starting with the error itself.
#[track_caller]
fn chain<'a>(error: &'a (dyn Error + 'static)) -> Vec<&'a (dyn Error + 'static)> {
    let mut found: Vec<&(dyn Error + 'static)> = Vec::new();
    let mut current = Some(error);

    while let Some(link) = current {
        if found.len() >= MAX_SOURCES {
            break;
        }
        // Identity, not equality: a chain that loops back on itself has
        // to stop, and two distinct errors can compare equal.
        if found.iter().any(|seen| std::ptr::eq(*seen, link)) {
            break;
        }
        found.push(link);
        current = link.source();
    }
    found
}

/// Report when the result is an error.
#[track_caller]
pub fn no_error<T, E>(seat: &dyn Seat, mode: Mode, got: &Result<T, E>, msg: &str)
where
    E: Debug,
{
    seat.helper();
    if let Err(error) = got {
        report(seat, mode, &format!("{msg}: unexpected error {error:?}"));
    }
}

/// Report when the result is not an error.
#[track_caller]
pub fn has_error<T, E>(seat: &dyn Seat, mode: Mode, got: &Result<T, E>, msg: &str)
where
    T: Debug,
{
    seat.helper();
    if let Ok(value) = got {
        report(
            seat,
            mode,
            &format!("{msg}: expected an error, got {value:?}"),
        );
    }
}

/// Whether a value equal to target sits anywhere in the chain.
#[track_caller]
fn matched<T>(error: &(dyn Error + 'static), target: &T) -> bool
where
    T: PartialEq + Error + 'static,
{
    chain(error)
        .into_iter()
        .any(|link| link.downcast_ref::<T>().is_some_and(|held| held == target))
}

/// Report when the error does not match target, through the chain of sources.
///
/// Rust has no `errors.Is`, so matching is by downcast and comparison: a link of the
/// chain that is a `T` and equals target is a match. That is what a sentinel comparison
/// means here.
#[track_caller]
pub fn error_is<T>(
    seat: &dyn Seat,
    mode: Mode,
    error: &(dyn Error + 'static),
    target: &T,
    msg: &str,
) where
    T: PartialEq + Error + Debug + 'static,
{
    seat.helper();
    if !matched(error, target) {
        report(
            seat,
            mode,
            &format!("{msg}: {error} does not match {target:?}"),
        );
    }
}

/// Report when the error matches target.
#[track_caller]
pub fn error_is_not<T>(
    seat: &dyn Seat,
    mode: Mode,
    error: &(dyn Error + 'static),
    target: &T,
    msg: &str,
) where
    T: PartialEq + Error + Debug + 'static,
{
    seat.helper();
    if matched(error, target) {
        report(seat, mode, &format!("{msg}: {error} matches {target:?}"));
    }
}

/// Report when no error of the given type is in the chain, and yield it when one is.
///
/// Use it to read fields off a specific type rather than parsing a message. The borrow
/// answers for as long as the error does, so its fields can be asserted on directly.
#[track_caller]
pub fn error_as<'a, T>(
    seat: &dyn Seat,
    mode: Mode,
    error: &'a (dyn Error + 'static),
    msg: &str,
) -> Option<&'a T>
where
    T: Error + 'static,
{
    seat.helper();
    let found = chain(error)
        .into_iter()
        .find_map(<dyn Error>::downcast_ref::<T>);
    if found.is_none() {
        report(
            seat,
            mode,
            &format!("{msg}: no {} in {error}", std::any::type_name::<T>()),
        );
    }
    found
}
