//! Assertions about a call that panics.
//!
//! A panic is not how Rust reports a failure a caller is meant to handle; that is a
//! [`Result`], and [`errors`](super::errors) covers it. A panic means a broken invariant,
//! and these are for asserting on that.
//!
//! Both take a body rather than a value, because the panic has to happen inside the
//! assertion for it to be caught.

use super::report::{Mode, report};
use crate::seat::Seat;
use std::any::Any;
use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::OnceLock;

thread_local! {
    /// Whether this thread is inside an assertion that expects a panic.
    static EXPECTING: Cell<bool> = const { Cell::new(false) };
}

/// Set once, for the life of the process.
static INSTALLED: OnceLock<()> = OnceLock::new();

/// Install a hook that stays quiet for an expected panic and nothing else.
///
/// `catch_unwind` alone still runs the panic hook, which prints a backtrace. A panic this
/// library asked for is not news. Swapping the hook around each call would not do:
/// the hook is process-wide and the test harness runs tests on parallel threads, so one
/// assertion would swallow another thread's real panic. This installs one hook, once, and
/// decides per thread.
fn install() {
    INSTALLED.get_or_init(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if EXPECTING.with(Cell::get) {
                return;
            }
            previous(info);
        }));
    });
}

/// Run body, and answer what it panicked with.
fn caught<F: FnOnce()>(body: F) -> Option<String> {
    install();
    EXPECTING.with(|held| held.set(true));
    let outcome = catch_unwind(AssertUnwindSafe(body));
    EXPECTING.with(|held| held.set(false));

    outcome.err().map(|raised| describe(&*raised))
}

/// Read what was passed to `panic!`, in the two shapes it arrives in.
fn describe(raised: &(dyn Any + Send)) -> String {
    if let Some(text) = raised.downcast_ref::<&'static str>() {
        return (*text).to_owned();
    }
    if let Some(text) = raised.downcast_ref::<String>() {
        return text.clone();
    }
    "a panic carrying a value that is not a string".to_owned()
}

/// Report when the body does not panic, and yield what it panicked with when it does.
///
/// The message is yielded so a caller can assert on it, which is how the shape of a panic
/// is checked: a panic carries a value, not a type.
#[track_caller]
pub fn panics<F: FnOnce()>(seat: &dyn Seat, mode: Mode, body: F, msg: &str) -> Option<String> {
    seat.helper();
    let raised = caught(body);
    if raised.is_none() {
        report(seat, mode, &format!("{msg}: returned without panicking"));
    }
    raised
}

/// Report when the body panics.
#[track_caller]
pub fn does_not_panic<F: FnOnce()>(seat: &dyn Seat, mode: Mode, body: F, msg: &str) {
    seat.helper();
    if let Some(raised) = caught(body) {
        report(seat, mode, &format!("{msg}: panicked with {raised:?}"));
    }
}

/// Run body with panics from this thread kept quiet, and say whether it panicked.
///
/// For an assertion that has to survive a panicking subject without asserting on the
/// panic itself.
pub(crate) fn quietly<F: FnOnce()>(body: F) -> bool {
    caught(body).is_some()
}
