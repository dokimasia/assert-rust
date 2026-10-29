// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! Calls that panic.
//!
//! A panic is how Rust reports a broken invariant. A failure that a caller
//! handles is a `Result`, and the errors family reads it.

use crate::failure::Where;
use crate::seat::Seat;
use crate::value::Value;
use crate::verdict::{Mode, Run};
use std::any::Any;
use std::cell::Cell;
use std::collections::HashSet;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Mutex, Once, PoisonError};

thread_local! {
    /// Whether this thread runs a callable whose panic an assertion expects.
    static EXPECTING: Cell<bool> = const { Cell::new(false) };
    /// The location of the last panic that the thread's expected callable
    /// raised.
    static LOCATION: Cell<Option<Where>> = const { Cell::new(None) };
}

/// Returns text for the life of the process, such as a file's path that the
/// panic hook receives for the life of the panic alone. The library keeps one
/// copy of each text.
pub(crate) fn intern(file: &str) -> &'static str {
    static FILES: Mutex<Option<HashSet<&'static str>>> = Mutex::new(None);
    let mut files = FILES.lock().unwrap_or_else(PoisonError::into_inner);
    let files = files.get_or_insert_with(HashSet::new);
    if let Some(kept) = files.get(file) {
        return kept;
    }
    let kept: &'static str = Box::leak(file.to_owned().into_boxed_str());
    files.insert(kept);
    kept
}

/// Installs, once per process, a panic hook that prints nothing for a panic
/// that an assertion expects and defers to the previous hook otherwise.
///
/// The hook belongs to the process and libtest runs tests on parallel
/// threads, so a hook that an assertion swapped in for its own call would
/// also silence another test's panic. The installed hook asks the thread.
///
/// The hook cannot change while the thread unwinds, so the call returns at
/// once there. A drop that runs a cleanup during the unwinding keeps the hook
/// of the process.
fn install() {
    static INSTALLED: Once = Once::new();
    if std::thread::panicking() {
        return;
    }
    INSTALLED.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if EXPECTING.with(Cell::get) {
                let at = info
                    .location()
                    .map(|at| Where::at(intern(at.file()), at.line()));
                LOCATION.with(|held| held.set(at));
                return;
            }
            previous(info);
        }));
    });
}

/// Returns the location of the last panic that an expected callable of this
/// thread raised, and forgets it.
pub(crate) fn last_location() -> Option<Where> {
    LOCATION.with(Cell::take)
}

/// Runs `body`, and returns the message of its panic, or its value when it
/// returns.
pub(crate) fn caught<R>(body: impl FnOnce() -> R) -> Result<R, String> {
    quietly(body).map_err(|raised| message(&*raised))
}

/// Runs `body`, and returns its value. The panic hook prints nothing for a
/// panic of the body.
///
/// # Errors
///
/// Returns the payload of the panic when `body` panics.
pub fn quietly<R>(body: impl FnOnce() -> R) -> std::thread::Result<R> {
    install();
    let expecting = EXPECTING.with(|held| held.replace(true));
    let outcome = catch_unwind(AssertUnwindSafe(body));
    EXPECTING.with(|held| held.set(expecting));
    outcome
}

/// Returns the text that `panic!` received, in the two forms it arrives in.
#[must_use]
pub fn message(raised: &(dyn Any + Send)) -> String {
    if let Some(text) = raised.downcast_ref::<&'static str>() {
        return (*text).to_owned();
    }
    if let Some(text) = raised.downcast_ref::<String>() {
        return text.clone();
    }
    "a panic with a payload that is no text".to_owned()
}

/// Reports whether `body` panics, and returns its message when it does.
#[track_caller]
pub(crate) fn panics(
    seat: &dyn Seat,
    mode: Mode,
    body: impl FnOnce(),
    msg: &str,
) -> Option<String> {
    seat.helper();
    let run = Run::of(seat);
    match caught(body) {
        Ok(()) => {
            run.fail(mode, "throws", msg, vec![]);
            None
        }
        Err(raised) => {
            run.pass(mode, "throws", msg);
            Some(raised)
        }
    }
}

/// Reports whether `body` returns without a panic.
#[track_caller]
pub(crate) fn does_not_panic(seat: &dyn Seat, mode: Mode, body: impl FnOnce(), msg: &str) {
    seat.helper();
    let run = Run::of(seat);
    if let Err(raised) = caught(body) {
        run.fail(mode, "not-throws", msg, vec![("got", Value::from(raised))]);
        return;
    }
    run.pass(mode, "not-throws", msg);
}
