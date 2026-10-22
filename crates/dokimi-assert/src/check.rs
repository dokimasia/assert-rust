//! Assertions that end the test at the first failure.
//!
//! Every assertion takes the seat first and the contract last, except
//! [`rejects`], which takes its check last. The contract is the caller's
//! message: it states what must be true, and it is the first line of the
//! failure.
//!
//! ```
//! use dokimi_assert::{Recorder, check};
//!
//! let t = Recorder::new();
//! let name = String::from("widget");
//! check::equal(&t, &name, "widget", "the store returns the item's name");
//! check::length(&t, &[1, 2, 3], 3, "the list has three items");
//! assert!(!t.failed());
//! ```
//!
//! On a [`Collector`](crate::Collector) or a [`Standard`](crate::Standard)
//! seat a failure panics at the caller's line, which ends the test.
//! [`soft`](crate::soft) states the same assertions with the same
//! signatures, and lets the test continue past a failure.

use crate::cancel::Cancel;
use crate::failure::Failure;
use crate::matcher::containment::Contains;
use crate::matcher::numbers::Number;
use crate::matcher::sizes::Container;
use crate::matcher::{
    allocations, behaviour, containment, errors, numbers, order, panics, rejects, relation, sizes,
    text, values, waiting,
};
use crate::seat::Seat;
use crate::verdict::Mode;
use std::error::Error;
use std::fmt::Debug;
use std::time::Duration;

const MODE: Mode = Mode::Fatal;

/// Ends the test when `got` and `want` differ.
///
/// The comparison is `==`, as `assert_eq!` makes it, so any pair of types
/// that `PartialEq` relates compares: a `String` with a `&str`, a `Vec` with
/// an array. NaN is unequal to itself, and `0.0` equals `-0.0`. A pair of
/// types that does not compare fails to compile.
#[track_caller]
pub fn equal<G, W>(seat: &dyn Seat, got: G, want: W, msg: &str)
where
    G: PartialEq<W> + Debug,
    W: Debug,
{
    seat.helper();
    values::equal(seat, MODE, got, want, msg);
}

/// Ends the test when `got` and `want` are equal, as `==` compares them.
#[track_caller]
pub fn not_equal<G, W>(seat: &dyn Seat, got: G, want: W, msg: &str)
where
    G: PartialEq<W> + Debug,
    W: Debug,
{
    seat.helper();
    values::not_equal(seat, MODE, got, want, msg);
}

/// Ends the test when `condition` is false.
///
/// The failure states the contract alone, because a bare `false` contains no
/// more information. A more specific assertion states the values.
#[track_caller]
pub fn is_true(seat: &dyn Seat, condition: bool, msg: &str) {
    seat.helper();
    values::is_true(seat, MODE, condition, msg);
}

/// Ends the test when `condition` is true.
#[track_caller]
pub fn is_false(seat: &dyn Seat, condition: bool, msg: &str) {
    seat.helper();
    values::is_false(seat, MODE, condition, msg);
}

/// Ends the test when `got` is `Some`.
///
/// The option moves into the assertion. A test that keeps it passes
/// `option.as_ref()`, and a lookup such as `map.get(key)` passes as it is.
#[track_caller]
pub fn is_none<T: Debug>(seat: &dyn Seat, got: Option<T>, msg: &str) {
    seat.helper();
    values::is_none(seat, MODE, got, msg);
}

/// Ends the test when `got` is `None`.
///
/// A test calls it before it unwraps a value that may be absent, so the test
/// ends at the contract rather than at a panic of `unwrap`.
#[track_caller]
pub fn is_some<T: Debug>(seat: &dyn Seat, got: Option<T>, msg: &str) {
    seat.helper();
    values::is_some(seat, MODE, got, msg);
}

/// Ends the test when `got` does not have `want` items.
///
/// Text counts its Unicode scalar values, so `"é"` has one item. A
/// [`Container`] without a length, such as `None`, fails.
#[track_caller]
pub fn length<C: Container + ?Sized>(seat: &dyn Seat, got: &C, want: usize, msg: &str) {
    seat.helper();
    sizes::length(seat, MODE, got, want, msg);
}

/// Ends the test when `got` has an item, or has no length.
#[track_caller]
pub fn is_empty<C: Container + ?Sized>(seat: &dyn Seat, got: &C, msg: &str) {
    seat.helper();
    sizes::is_empty(seat, MODE, got, msg);
}

/// Ends the test when `got` has no item, or has no length.
#[track_caller]
pub fn is_not_empty<C: Container + ?Sized>(seat: &dyn Seat, got: &C, msg: &str) {
    seat.helper();
    sizes::is_not_empty(seat, MODE, got, msg);
}

/// Ends the test when `haystack` does not contain `needle`.
///
/// Text contains a substring, a sequence an element, and a map or a set a
/// key, as [`Contains`] states.
#[track_caller]
pub fn contains<H, N>(seat: &dyn Seat, haystack: &H, needle: &N, msg: &str)
where
    H: Contains<N> + Debug + ?Sized,
    N: Debug + ?Sized,
{
    seat.helper();
    containment::contains(seat, MODE, haystack, needle, msg);
}

/// Ends the test when `haystack` contains `needle`.
#[track_caller]
pub fn not_contains<H, N>(seat: &dyn Seat, haystack: &H, needle: &N, msg: &str)
where
    H: Contains<N> + Debug + ?Sized,
    N: Debug + ?Sized,
{
    seat.helper();
    containment::not_contains(seat, MODE, haystack, needle, msg);
}

/// Ends the test when `got` does not contain every needle in order.
///
/// Each needle is searched for after the end of the previous needle's match,
/// so one occurrence cannot satisfy two needles.
#[track_caller]
pub fn contains_in_order(seat: &dyn Seat, got: &str, needles: &[&str], msg: &str) {
    seat.helper();
    containment::contains_in_order(seat, MODE, got, needles, msg);
}

/// Ends the test when `got` does not contain the elements of `want`, each as
/// often, in some order.
///
/// Elements compare as `==` compares them, so a type needs `PartialEq` alone,
/// and neither an order nor a hash.
#[track_caller]
pub fn is_permutation<T: PartialEq + Debug>(seat: &dyn Seat, got: &[T], want: &[T], msg: &str) {
    seat.helper();
    containment::is_permutation(seat, MODE, got, want, msg);
}

/// Ends the test when `got` does not start with `prefix`.
#[track_caller]
pub fn has_prefix(seat: &dyn Seat, got: &str, prefix: &str, msg: &str) {
    seat.helper();
    text::has_prefix(seat, MODE, got, prefix, msg);
}

/// Ends the test when `got` does not end with `suffix`.
#[track_caller]
pub fn has_suffix(seat: &dyn Seat, got: &str, suffix: &str, msg: &str) {
    seat.helper();
    text::has_suffix(seat, MODE, got, suffix, msg);
}

/// Ends the test when `pattern` matches nowhere in `got`.
///
/// The pattern is of the portable subset that every implementation of the
/// definition reads the same way: `\d`, `\w` and `\s` are ASCII classes, `.`
/// matches no line terminator, and `$` matches at the end of the text alone.
/// A pattern outside the subset fails, and the failure states why the subset
/// refuses it.
#[track_caller]
pub fn matches(seat: &dyn Seat, got: &str, pattern: &str, msg: &str) {
    seat.helper();
    text::matches(seat, MODE, got, pattern, msg);
}

/// Ends the test when `got` is further than `tolerance` from `want`.
///
/// The tolerance is an absolute difference, and a difference equal to it
/// passes. NaN is outside every tolerance, and so are two equal infinities.
#[track_caller]
pub fn close_to<N: Number>(seat: &dyn Seat, got: N, want: f64, tolerance: f64, msg: &str) {
    seat.helper();
    numbers::close_to(seat, MODE, got, want, tolerance, msg);
}

/// Ends the test when `got` falls outside the closed interval from `low` to
/// `high`.
///
/// NaN is in no interval, and an interval with a NaN bound contains nothing.
#[track_caller]
pub fn in_range<N: Number>(seat: &dyn Seat, got: N, low: f64, high: f64, msg: &str) {
    seat.helper();
    numbers::in_range(seat, MODE, got, low, high, msg);
}

/// Ends the test when an adjacent pair of `items` does not satisfy
/// `predicate`.
///
/// One assertion states sorted, unique and strictly increasing, because each
/// of those is a relation between neighbours. Nought or one item passes.
#[track_caller]
pub fn pairwise<T: Debug>(
    seat: &dyn Seat,
    items: &[T],
    predicate: impl Fn(&T, &T) -> bool,
    msg: &str,
) {
    seat.helper();
    order::pairwise(seat, MODE, items, predicate, msg);
}

/// Ends the test when `got` is `Err`.
#[track_caller]
pub fn no_error<T, E: Debug>(seat: &dyn Seat, got: &Result<T, E>, msg: &str) {
    seat.helper();
    errors::no_error(seat, MODE, got, msg);
}

/// Ends the test when `got` is `Ok`.
#[track_caller]
pub fn has_error<T: Debug, E>(seat: &dyn Seat, got: &Result<T, E>, msg: &str) {
    seat.helper();
    errors::has_error(seat, MODE, got, msg);
}

/// Ends the test when no error of the chain of `error` is a `T` equal to
/// `target`.
///
/// The chain is `error` and its sources, as [`Error::source`] returns them.
#[track_caller]
pub fn error_is<T>(seat: &dyn Seat, error: &(dyn Error + 'static), target: &T, msg: &str)
where
    T: Error + PartialEq + 'static,
{
    seat.helper();
    errors::error_is(seat, MODE, Some(error), target, msg);
}

/// Ends the test when an error of the chain of `error` is a `T` equal to
/// `target`.
#[track_caller]
pub fn error_is_not<T>(seat: &dyn Seat, error: &(dyn Error + 'static), target: &T, msg: &str)
where
    T: Error + PartialEq + 'static,
{
    seat.helper();
    errors::error_is_not(seat, MODE, Some(error), target, msg);
}

/// Returns the first error of the chain of `error` that is a `T`, and ends
/// the test when no error is.
///
/// A test reads the fields of the returned error rather than its message.
#[track_caller]
pub fn error_as<'e, T: Error + 'static>(
    seat: &dyn Seat,
    error: &'e (dyn Error + 'static),
    msg: &str,
) -> Option<&'e T> {
    seat.helper();
    errors::error_as(seat, MODE, Some(error), msg)
}

/// Returns the message of the panic of `body`, and ends the test when `body`
/// returns.
///
/// A panic that the assertion expects prints nothing.
#[track_caller]
pub fn panics(seat: &dyn Seat, body: impl FnOnce(), msg: &str) -> Option<String> {
    seat.helper();
    panics::panics(seat, MODE, body, msg)
}

/// Ends the test when `body` panics.
#[track_caller]
pub fn does_not_panic(seat: &dyn Seat, body: impl FnOnce(), msg: &str) {
    seat.helper();
    panics::does_not_panic(seat, MODE, body, msg);
}

/// Ends the test when a subject that receives a cancelled handle does not
/// return an error whose chain contains [`Stop::Cancelled`](crate::Stop::Cancelled).
///
/// The handle is cancelled before the subject starts, so the assertion asks
/// whether the subject reads the handle at all. A subject that returns an
/// error for a reason of its own fails too.
#[track_caller]
pub fn honours_cancellation<E: Error + 'static>(
    seat: &dyn Seat,
    body: impl FnOnce(Option<&Cancel>) -> Result<(), E>,
    msg: &str,
) {
    seat.helper();
    behaviour::honours_cancellation(seat, MODE, body, msg);
}

/// Ends the test when a subject that receives an expired handle does not
/// return an error whose chain contains [`Stop::DeadlineExceeded`](crate::Stop::DeadlineExceeded).
#[track_caller]
pub fn honours_deadline<E: Error + 'static>(
    seat: &dyn Seat,
    body: impl FnOnce(Option<&Cancel>) -> Result<(), E>,
    msg: &str,
) {
    seat.helper();
    behaviour::honours_deadline(seat, MODE, body, msg);
}

/// Ends the test when `body` has not returned within `within`.
///
/// The body runs on a thread of its own, with a handle that expires when the
/// duration passes. A body that has not returned by then fails, and the
/// assertion does not wait for it, so the body is `Send + 'static`. A body
/// that returns is measured on the seat's clock.
#[track_caller]
pub fn completes_within<E: Debug + Send + 'static>(
    seat: &dyn Seat,
    within: Duration,
    body: impl FnOnce(Option<&Cancel>) -> Result<(), E> + Send + 'static,
    msg: &str,
) {
    seat.helper();
    behaviour::completes_within(seat, MODE, within, body, msg);
}

/// Ends the test when `body` changes what `observe` returns.
///
/// What `observe` returns defines what is left alone, so the body may change
/// anything else. `observe` returns an owned value, so the two readings do
/// not share memory.
#[track_caller]
pub fn is_pure<S: PartialEq + Debug>(
    seat: &dyn Seat,
    observe: impl Fn() -> S,
    body: impl FnOnce(),
    msg: &str,
) {
    seat.helper();
    behaviour::is_pure(seat, MODE, observe, body, msg);
}

/// Ends the test when `body` leaves what `observe` returns unchanged.
#[track_caller]
pub fn is_not_pure<S: PartialEq + Debug>(
    seat: &dyn Seat,
    observe: impl Fn() -> S,
    body: impl FnOnce(),
    msg: &str,
) {
    seat.helper();
    relation::is_not_pure(seat, MODE, observe, body, msg);
}

/// Ends the test when a subject that receives no handle panics. An error of
/// the subject's own passes.
#[track_caller]
pub fn none_handle_safe<E: Debug>(
    seat: &dyn Seat,
    body: impl FnOnce(Option<&Cancel>) -> Result<(), E>,
    msg: &str,
) {
    seat.helper();
    behaviour::none_handle_safe(seat, MODE, body, msg);
}

/// Ends the test when a second call of `call` with `input` leaves `observe`
/// returning other than the first call left it.
#[track_caller]
pub fn is_idempotent<I, S, E>(
    seat: &dyn Seat,
    call: impl FnMut(&I) -> Result<(), E>,
    input: I,
    observe: impl FnMut() -> S,
    msg: &str,
) where
    S: PartialEq + Debug,
    E: Debug,
{
    seat.helper();
    relation::is_idempotent(seat, MODE, call, input, observe, msg);
}

/// Ends the test when the first call of `call` with `input` leaves the count
/// that `observe` returns unchanged, or a second call changes it by another
/// amount.
#[track_caller]
pub fn accumulates<I, E: Debug>(
    seat: &dyn Seat,
    call: impl FnMut(&I) -> Result<(), E>,
    input: I,
    observe: impl FnMut() -> i64,
    msg: &str,
) {
    seat.helper();
    relation::accumulates(seat, MODE, call, input, observe, msg);
}

/// Ends the test when one of 32 calls of `call` with `input` returns a result
/// other than the first.
#[track_caller]
pub fn is_deterministic<I, O, E>(
    seat: &dyn Seat,
    call: impl FnMut(&I) -> Result<O, E>,
    input: I,
    msg: &str,
) where
    O: PartialEq + Debug,
    E: Debug,
{
    seat.helper();
    relation::is_deterministic(seat, MODE, call, input, msg);
}

/// Ends the test when `combine(a, b)` differs from `combine(b, a)`.
#[track_caller]
pub fn is_commutative<T, R>(
    seat: &dyn Seat,
    combine: impl FnMut(&T, &T) -> R,
    a: T,
    b: T,
    msg: &str,
) where
    R: PartialEq + Debug,
{
    seat.helper();
    relation::is_commutative(seat, MODE, combine, a, b, msg);
}

/// Ends the test when `combine(combine(a, b), c)` differs from
/// `combine(a, combine(b, c))`.
#[track_caller]
pub fn is_associative<T>(
    seat: &dyn Seat,
    combine: impl FnMut(&T, &T) -> T,
    a: T,
    b: T,
    c: T,
    msg: &str,
) where
    T: PartialEq + Debug,
{
    seat.helper();
    relation::is_associative(seat, MODE, combine, a, b, c, msg);
}

/// Ends the test when `inverse` of `forward` of `input` does not return
/// `input`.
#[track_caller]
pub fn round_trip<I, O, E, F>(
    seat: &dyn Seat,
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
    relation::round_trip(seat, MODE, forward, inverse, input, msg);
}

/// Ends the test when one of 32 iterations of `iterate` yields a sequence
/// other than the first.
#[track_caller]
pub fn has_stable_order<T, E>(
    seat: &dyn Seat,
    iterate: impl FnMut() -> Result<Vec<T>, E>,
    msg: &str,
) where
    T: PartialEq + Debug,
    E: Debug,
{
    seat.helper();
    relation::has_stable_order(seat, MODE, iterate, msg);
}

/// Ends the test when one iteration of `iterate` yields an element twice.
#[track_caller]
pub fn no_duplicates<T, E>(seat: &dyn Seat, iterate: impl FnOnce() -> Result<Vec<T>, E>, msg: &str)
where
    T: PartialEq + Debug,
    E: Debug,
{
    seat.helper();
    relation::no_duplicates(seat, MODE, iterate, msg);
}

/// Ends the test when the reading of `observe` falls, or is NaN, while
/// `advance` runs `steps` times.
#[track_caller]
pub fn is_monotonic<N, E>(
    seat: &dyn Seat,
    observe: impl FnMut() -> N,
    advance: impl FnMut() -> Result<(), E>,
    steps: usize,
    msg: &str,
) where
    N: PartialOrd + Debug,
    E: Debug,
{
    seat.helper();
    relation::is_monotonic(seat, MODE, observe, advance, steps, msg);
}

/// Ends the test at the first element of `domain`, in order, for which `call`
/// returns an error or panics.
#[track_caller]
pub fn is_total<I, E: Debug>(
    seat: &dyn Seat,
    call: impl FnMut(&I) -> Result<(), E>,
    domain: &[I],
    msg: &str,
) {
    seat.helper();
    relation::is_total(seat, MODE, call, domain, msg);
}

/// Ends the test when `call`, after `close`, does not return an error whose
/// chain contains `sentinel`.
#[track_caller]
pub fn fails_after_close<T, E, C>(
    seat: &dyn Seat,
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
    relation::fails_after_close(seat, MODE, close, call, sentinel, msg);
}

/// Ends the test when, after `induce`, one of 32 readings of `observe`
/// returns `Ok`.
#[track_caller]
pub fn is_poisoned<E: Debug>(
    seat: &dyn Seat,
    induce: impl FnOnce(),
    observe: impl FnMut() -> Result<(), E>,
    msg: &str,
) {
    seat.helper();
    relation::is_poisoned(seat, MODE, induce, observe, msg);
}

/// Ends the test when `body` does not pass within `timeout`, retried every
/// `interval`.
///
/// Each attempt runs on a seat of its own, so a failure ends the attempt and
/// not the test. The body runs at least once. The failure states the number
/// of attempts and the first failure of the last attempt. On the seat's
/// [`Controlled`](crate::Controlled) clock the assertion advances the clock
/// between attempts and does not spend real time.
#[track_caller]
pub fn eventually(
    seat: &dyn Seat,
    timeout: Duration,
    interval: Duration,
    body: impl Fn(&dyn Seat),
    msg: &str,
) {
    seat.helper();
    waiting::eventually(seat, MODE, timeout, interval, body, msg);
}

/// Ends the test when `predicate` does not become true within `timeout`.
///
/// The predicate is retried with a backoff that starts at a millisecond and
/// doubles, up to a quarter of the timeout.
#[track_caller]
pub fn eventually_true(
    seat: &dyn Seat,
    timeout: Duration,
    predicate: impl Fn() -> bool,
    msg: &str,
) {
    seat.helper();
    waiting::eventually_true(seat, MODE, timeout, predicate, msg);
}

/// Ends the test when `body` makes more than `ceiling` heap allocations per
/// call.
///
/// A count calls the body once to warm it, and counts the allocations of the
/// next 100 calls on the calling thread. A ceiling of 0 admits one allocation
/// in all, and any other ceiling admits an average per call that rounds to at
/// most the ceiling. A count that exceeds the ceiling is taken again, ten
/// times at most. The test binary installs
/// [`CountingAllocator`](crate::bench::CountingAllocator), and without it the
/// call ends with a fault.
#[track_caller]
pub fn max_allocs(seat: &dyn Seat, body: impl FnMut(), ceiling: u64, msg: &str) {
    seat.helper();
    allocations::max_allocs(seat, MODE, body, ceiling, msg);
}

/// Ends the test when `body` makes more than `ceiling` heap allocations per
/// call on an input that `setup` builds before each call. The allocations of
/// `setup` are not counted, and the counts are those of [`max_allocs`].
#[track_caller]
pub fn max_allocs_with_setup<T>(
    seat: &dyn Seat,
    setup: impl FnMut() -> T,
    body: impl FnMut(T),
    ceiling: u64,
    msg: &str,
) {
    seat.helper();
    allocations::max_allocs_with_setup(seat, MODE, setup, body, ceiling, msg);
}

/// Returns the failures of `body`, a check that runs on a seat of its own,
/// and ends the test when the check does not fail.
///
/// A check proves that it can fail by failing against a subject that it must
/// refuse. A failure on `check` ends the check, as it ends a test. A fault
/// that ends a call of the check ends the call of `rejects` with the same
/// fault, because a check that ends with a fault rejects nothing: the seat
/// receives the fault through [`Seat::report_fault`], and `rejects` returns
/// no records. A fault that the check notes passes to the seat. The
/// assertion exists on this surface alone, because a recording surface does
/// not stop a check.
///
/// ```
/// use dokimi_assert::{Recorder, check};
///
/// let t = Recorder::new();
/// let failures = check::rejects(&t, "the check refuses an empty name", |s| {
///     check::is_not_empty(s, "", "the name is not empty");
/// });
/// assert_eq!(failures[0].assertion(), Some("not-empty"));
/// assert!(!t.failed());
/// ```
#[track_caller]
pub fn rejects(seat: &dyn Seat, msg: &str, body: impl FnOnce(&dyn Seat)) -> Vec<Failure> {
    seat.helper();
    rejects::rejects(seat, msg, body)
}
