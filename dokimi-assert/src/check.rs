//! Assertions that stop the test at the first failure.
//!
//! Every assertion takes a seat first and a message last. The message states the contract
//! under test and is the first line of the failure, so a failure says what was supposed
//! to be true rather than only what was observed.
//!
//! ```
//! use dokimi_assert::{check, seat::Collector};
//!
//! let seat = Collector::new();
//! check::equal(&seat, "widget", "widget", "get answers the stored item");
//! ```
//!
//! [`soft`](crate::soft) carries the same assertions under the same names, with the same
//! signatures and the same comparisons. Only what happens on failure differs.
//!
//! The members are grouped by family, in the order the other implementations of this
//! standard use: values, sizes, containment, text, numbers, errors, panicking, ordering,
//! behaviour and waiting.

use crate::matcher::Mode;
use crate::matcher::behaviour::Cancel;
use crate::matcher::sizes::Container;
use crate::matcher::{behaviour, containment, errors, numbers, order, panics, sizes, text, values};
use crate::matcher::{containment::Holds, waiting};
use crate::seat::{Recorder, Seat};
use std::error::Error;
use std::fmt::Debug;
use std::time::Duration;

/// Every failure on this surface stops the test.
const MODE: Mode = Mode::Fatal;

/// Fail when got and want differ.
///
/// Comparison is the language's own `==`, which is already what the standard asks for:
/// NaN is unequal to itself, `0.0` equals `-0.0`, and containers compare by their
/// elements. Values of different types never compare, because they do not compile.
#[track_caller]
pub fn equal<T>(seat: &dyn Seat, got: &T, want: &T, msg: &str)
where
    T: PartialEq + Debug + ?Sized,
{
    seat.helper();
    values::equal(seat, MODE, got, want, msg);
}

/// Fail when got and want are equal.
///
/// The failure shows the value the two shared, since printing one says everything.
#[track_caller]
pub fn not_equal<T>(seat: &dyn Seat, got: &T, want: &T, msg: &str)
where
    T: PartialEq + Debug + ?Sized,
{
    seat.helper();
    values::not_equal(seat, MODE, got, want, msg);
}

/// Fail when the condition does not hold.
///
/// The failure carries the message alone: a bare false says nothing the message does not.
/// Where a more specific assertion exists, it will say more on failure.
#[track_caller]
pub fn is_true(seat: &dyn Seat, condition: bool, msg: &str) {
    seat.helper();
    values::is_true(seat, MODE, condition, msg);
}

/// Fail when the condition holds.
#[track_caller]
pub fn is_false(seat: &dyn Seat, condition: bool, msg: &str) {
    seat.helper();
    values::is_false(seat, MODE, condition, msg);
}

/// Fail when got is present.
///
/// Rust states absence in the type rather than in the value, so this takes an `Option`
/// and there is no typed nil to catch.
#[track_caller]
pub fn is_none<T: Debug>(seat: &dyn Seat, got: Option<&T>, msg: &str) {
    seat.helper();
    values::is_none(seat, MODE, got, msg);
}

/// Fail when got is absent.
///
/// Use it before reading a value that may be absent: the test stops here with your
/// message rather than further down on an unwrap nobody wrote.
#[track_caller]
pub fn is_some<T: Debug>(seat: &dyn Seat, got: Option<&T>, msg: &str) {
    seat.helper();
    values::is_some(seat, MODE, got, msg);
}

/// Fail when got does not hold want items.
///
/// Answers for text, a slice, a `Vec`, a map or a set. A value with no length does not
/// compile, so it cannot be a failure at run time.
#[track_caller]
pub fn length<C: Container + ?Sized>(seat: &dyn Seat, got: &C, want: usize, msg: &str) {
    seat.helper();
    sizes::length(seat, MODE, got, want, msg);
}

/// Fail when got holds anything.
///
/// Empty is not absent. An absent container is an `Option` and does not reach here.
#[track_caller]
pub fn is_empty<C: Container + ?Sized>(seat: &dyn Seat, got: &C, msg: &str) {
    seat.helper();
    sizes::is_empty(seat, MODE, got, msg);
}

/// Fail when got holds nothing.
#[track_caller]
pub fn is_not_empty<C: Container + ?Sized>(seat: &dyn Seat, got: &C, msg: &str) {
    seat.helper();
    sizes::is_not_empty(seat, MODE, got, msg);
}

/// Fail when haystack does not hold needle.
///
/// What holding means follows the haystack: text holds a substring, a sequence holds an
/// element, and a map holds a key. Which one applies is decided by the types.
#[track_caller]
pub fn contains<H, N>(seat: &dyn Seat, haystack: &H, needle: &N, msg: &str)
where
    H: Holds<N> + Debug + ?Sized,
    N: Debug + ?Sized,
{
    seat.helper();
    containment::contains(seat, MODE, haystack, needle, msg);
}

/// Fail when haystack holds needle.
#[track_caller]
pub fn not_contains<H, N>(seat: &dyn Seat, haystack: &H, needle: &N, msg: &str)
where
    H: Holds<N> + Debug + ?Sized,
    N: Debug + ?Sized,
{
    seat.helper();
    containment::not_contains(seat, MODE, haystack, needle, msg);
}

/// Fail when got does not hold every needle, in order.
///
/// Each needle is looked for after the previous one's match ends, so the same text cannot
/// satisfy two needles. Anything may sit between them.
#[track_caller]
pub fn contains_in_order(seat: &dyn Seat, got: &str, needles: &[&str], msg: &str) {
    seat.helper();
    containment::contains_in_order(seat, MODE, got, needles, msg);
}

/// Fail when got does not start with prefix.
#[track_caller]
pub fn has_prefix(seat: &dyn Seat, got: &str, prefix: &str, msg: &str) {
    seat.helper();
    text::has_prefix(seat, MODE, got, prefix, msg);
}

/// Fail when got does not end with suffix.
#[track_caller]
pub fn has_suffix(seat: &dyn Seat, got: &str, suffix: &str, msg: &str) {
    seat.helper();
    text::has_suffix(seat, MODE, got, suffix, msg);
}

/// Fail when got does not match the pattern.
///
/// The pattern is searched rather than anchored: use `^` and `$` where you mean the whole
/// value. A pattern that does not compile is reported as the failure.
#[track_caller]
pub fn matches(seat: &dyn Seat, got: &str, pattern: &str, msg: &str) {
    seat.helper();
    text::matches(seat, MODE, got, pattern, msg);
}

/// Fail when got is further than tolerance from want.
///
/// The tolerance is an absolute difference and the bound is inclusive. This is the
/// assertion for a floating value, where exact equality is the wrong question. NaN is
/// outside every tolerance.
#[track_caller]
pub fn close_to(seat: &dyn Seat, got: f64, want: f64, tolerance: f64, msg: &str) {
    seat.helper();
    numbers::close_to(seat, MODE, got, want, tolerance, msg);
}

/// Fail when got falls outside low to high.
///
/// The interval is closed, so both bounds pass. A range with low above high can hold
/// nothing, and says so rather than reporting the value. NaN is in no range.
#[track_caller]
pub fn in_range(seat: &dyn Seat, got: f64, low: f64, high: f64, msg: &str) {
    seat.helper();
    numbers::in_range(seat, MODE, got, low, high, msg);
}

/// Fail when the result is an error.
#[track_caller]
pub fn no_error<T, E: Debug>(seat: &dyn Seat, got: &Result<T, E>, msg: &str) {
    seat.helper();
    errors::no_error(seat, MODE, got, msg);
}

/// Fail when the result is not an error.
#[track_caller]
pub fn has_error<T: Debug, E>(seat: &dyn Seat, got: &Result<T, E>, msg: &str) {
    seat.helper();
    errors::has_error(seat, MODE, got, msg);
}

/// Fail when the error does not match target, through the chain of sources.
///
/// Rust has no `errors.Is`, so matching is by downcast and comparison: a link of the
/// chain that is a `T` and equals target is a match.
#[track_caller]
pub fn error_is<T>(seat: &dyn Seat, error: &(dyn Error + 'static), target: &T, msg: &str)
where
    T: PartialEq + Error + Debug + 'static,
{
    seat.helper();
    errors::error_is(seat, MODE, error, target, msg);
}

/// Fail when the error matches target.
#[track_caller]
pub fn error_is_not<T>(seat: &dyn Seat, error: &(dyn Error + 'static), target: &T, msg: &str)
where
    T: PartialEq + Error + Debug + 'static,
{
    seat.helper();
    errors::error_is_not(seat, MODE, error, target, msg);
}

/// Fail when no error of the given type is in the chain, and yield it when one is.
///
/// Use it to read fields off a specific type rather than parsing a message.
#[track_caller]
pub fn error_as<'a, T>(
    seat: &dyn Seat,
    error: &'a (dyn Error + 'static),
    msg: &str,
) -> Option<&'a T>
where
    T: Error + 'static,
{
    seat.helper();
    errors::error_as(seat, MODE, error, msg)
}

/// Fail when the body does not panic, and yield what it panicked with when it does.
///
/// A panic, not an error: a failure a caller is meant to handle is a `Result`, and
/// [`no_error`] covers that. The message is yielded so a caller can assert on it.
#[track_caller]
pub fn panics<F: FnOnce()>(seat: &dyn Seat, body: F, msg: &str) -> Option<String> {
    seat.helper();
    panics::panics(seat, MODE, body, msg)
}

/// Fail when the body panics.
#[track_caller]
pub fn does_not_panic<F: FnOnce()>(seat: &dyn Seat, body: F, msg: &str) {
    seat.helper();
    panics::does_not_panic(seat, MODE, body, msg);
}

/// Fail when an adjacent pair does not satisfy the predicate.
///
/// Nought or one item passes, since neither has a pair. One assertion rather than sorted,
/// unique and strictly increasing, because each of those is a relation between neighbours.
#[track_caller]
pub fn pairwise<T, P>(seat: &dyn Seat, items: &[T], predicate: P, msg: &str)
where
    T: Debug,
    P: Fn(&T, &T) -> bool,
{
    seat.helper();
    order::pairwise(seat, MODE, items, predicate, msg);
}

/// Fail when a subject told to stop does not say so.
///
/// The subject is handed a [`Cancel`] that has already been cancelled, so this asks
/// whether it reads the handle at all. One that ignores it does the work and answers
/// `Ok`, which fails here.
#[track_caller]
pub fn honours_cancellation<E, F>(seat: &dyn Seat, body: F, msg: &str)
where
    E: Error + 'static,
    F: FnOnce(Option<&Cancel>) -> Result<(), E>,
{
    seat.helper();
    behaviour::honours_cancellation(seat, MODE, body, msg);
}

/// Fail when a subject given no time does not say its deadline passed.
///
/// This differs from [`honours_cancellation`] in which reason it asks for: a subject may
/// distinguish a caller who gave up from one who ran out of time.
#[track_caller]
pub fn honours_deadline<E, F>(seat: &dyn Seat, body: F, msg: &str)
where
    E: Error + 'static,
    F: FnOnce(Option<&Cancel>) -> Result<(), E>,
{
    seat.helper();
    behaviour::honours_deadline(seat, MODE, body, msg);
}

/// Fail when the body takes longer than the given duration.
///
/// The body is given a handle that is stopped once the time is up, so a subject that
/// reads it can give up. It is measured either way. This spends real time.
#[track_caller]
pub fn completes_within<E, F>(seat: &dyn Seat, within: Duration, body: F, msg: &str)
where
    E: Debug,
    F: FnOnce(Option<&Cancel>) -> Result<(), E>,
{
    seat.helper();
    behaviour::completes_within(seat, MODE, within, body, msg);
}

/// Fail when the body changes what observe reads.
///
/// What observe answers defines what nothing means: whatever it leaves out, the body is
/// free to change. Answer an owned value, so the reading is a snapshot rather than a
/// borrow that reads the same memory twice.
#[track_caller]
pub fn is_pure<S, O, F>(seat: &dyn Seat, observe: O, body: F, msg: &str)
where
    S: PartialEq + Debug,
    O: Fn() -> S,
    F: FnOnce(),
{
    seat.helper();
    behaviour::is_pure(seat, MODE, observe, body, msg);
}

/// Fail when a subject given no handle at all panics.
///
/// Answering an error of its own is fine and is usually right. What fails here is
/// panicking on the missing handle, which is what a caller hits by accident.
#[track_caller]
pub fn none_handle_safe<E, F>(seat: &dyn Seat, body: F, msg: &str)
where
    E: Debug,
    F: FnOnce(Option<&Cancel>) -> Result<(), E> + std::panic::UnwindSafe,
{
    seat.helper();
    behaviour::none_handle_safe(seat, MODE, body, msg);
}

/// Fail when a body of assertions never passes within the timeout.
///
/// The body is handed a seat of its own, so assertions inside it record an attempt rather
/// than ending the test. It runs at least once however short the timeout, and the failure
/// carries the last attempt's own reason. This spends real time.
#[track_caller]
pub fn eventually<F>(seat: &dyn Seat, timeout: Duration, interval: Duration, body: F, msg: &str)
where
    F: Fn(&Recorder),
{
    seat.helper();
    waiting::eventually(seat, MODE, timeout, interval, body, msg);
}

/// Fail when a predicate never becomes true within the timeout.
///
/// Retried with a backoff that doubles. A predicate carries no reason, so the failure
/// says only that the wait ran out; where the reason matters, use [`eventually`].
#[track_caller]
pub fn eventually_true<P>(seat: &dyn Seat, timeout: Duration, predicate: P, msg: &str)
where
    P: Fn() -> bool,
{
    seat.helper();
    waiting::eventually_true(seat, MODE, timeout, predicate, msg);
}

/// Fail when the body does not report a failure.
///
/// The body is handed a [`Recorder`], so assertions inside it record instead of ending the
/// test. A body that passes is the failure. Yields the failure the body produced, so its
/// text can be asserted on.
///
/// On the aborting surface only: the recording surface cannot drive a check to failure,
/// because it does not stop.
#[track_caller]
pub fn rejects<F>(seat: &dyn Seat, body: F, msg: &str) -> String
where
    F: FnOnce(&Recorder),
{
    seat.helper();
    behaviour::rejects(seat, MODE, body, msg)
}
