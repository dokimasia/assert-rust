// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! Assertions that record a failure and let the test continue.
//!
//! The assertions, their signatures and their comparisons are those of
//! [`check`](crate::check), and the documentation of each `check` assertion
//! states its comparison. A failure here is recorded rather than ending the
//! test, so one run reports every failure:
//!
//! ```should_panic
//! use dokimi_assert::{Collector, soft};
//!
//! let t = Collector::new();
//! soft::has_prefix(&t, "[1,2]", "{", "the body is a JSON object");
//! soft::length(&t, &[1, 2], 3, "every item comes back");
//! // The drop of the collector panics with both failures and their lines.
//! ```
//!
//! [`check::rejects`](crate::check::rejects) has no counterpart here,
//! because a check that continues past its failures cannot be driven to one.

use crate::cancel::Cancel;
use crate::matcher::containment::Contains;
use crate::matcher::numbers::Number;
use crate::matcher::sizes::Container;
use crate::matcher::{
    allocations, behaviour, containment, errors, numbers, order, panics, relation, sizes, text,
    values, waiting,
};
use crate::seat::Seat;
use crate::verdict::Mode;
use std::error::Error;
use std::fmt::Debug;
use std::time::Duration;

const MODE: Mode = Mode::Soft;

/// Records a failure when `got` and `want` differ, as
/// [`check::equal`](crate::check::equal) compares them.
#[track_caller]
pub fn equal<G, W>(seat: &dyn Seat, got: G, want: W, msg: &str)
where
    G: PartialEq<W> + Debug,
    W: Debug,
{
    seat.helper();
    values::equal(seat, MODE, got, want, msg);
}

/// Records a failure when `got` and `want` are equal.
#[track_caller]
pub fn not_equal<G, W>(seat: &dyn Seat, got: G, want: W, msg: &str)
where
    G: PartialEq<W> + Debug,
    W: Debug,
{
    seat.helper();
    values::not_equal(seat, MODE, got, want, msg);
}

/// Records a failure when `condition` is false.
#[track_caller]
pub fn is_true(seat: &dyn Seat, condition: bool, msg: &str) {
    seat.helper();
    values::is_true(seat, MODE, condition, msg);
}

/// Records a failure when `condition` is true.
#[track_caller]
pub fn is_false(seat: &dyn Seat, condition: bool, msg: &str) {
    seat.helper();
    values::is_false(seat, MODE, condition, msg);
}

/// Records a failure when `got` is `Some`.
#[track_caller]
pub fn is_none<T: Debug>(seat: &dyn Seat, got: Option<T>, msg: &str) {
    seat.helper();
    values::is_none(seat, MODE, got, msg);
}

/// Records a failure when `got` is `None`.
#[track_caller]
pub fn is_some<T: Debug>(seat: &dyn Seat, got: Option<T>, msg: &str) {
    seat.helper();
    values::is_some(seat, MODE, got, msg);
}

/// Records a failure when `got` does not have `want` items.
#[track_caller]
pub fn length<C: Container + ?Sized>(seat: &dyn Seat, got: &C, want: usize, msg: &str) {
    seat.helper();
    sizes::length(seat, MODE, got, want, msg);
}

/// Records a failure when `got` has an item, or has no length.
#[track_caller]
pub fn is_empty<C: Container + ?Sized>(seat: &dyn Seat, got: &C, msg: &str) {
    seat.helper();
    sizes::is_empty(seat, MODE, got, msg);
}

/// Records a failure when `got` has no item, or has no length.
#[track_caller]
pub fn is_not_empty<C: Container + ?Sized>(seat: &dyn Seat, got: &C, msg: &str) {
    seat.helper();
    sizes::is_not_empty(seat, MODE, got, msg);
}

/// Records a failure when `haystack` does not contain `needle`.
#[track_caller]
pub fn contains<H, N>(seat: &dyn Seat, haystack: &H, needle: &N, msg: &str)
where
    H: Contains<N> + Debug + ?Sized,
    N: Debug + ?Sized,
{
    seat.helper();
    containment::contains(seat, MODE, haystack, needle, msg);
}

/// Records a failure when `haystack` contains `needle`.
#[track_caller]
pub fn not_contains<H, N>(seat: &dyn Seat, haystack: &H, needle: &N, msg: &str)
where
    H: Contains<N> + Debug + ?Sized,
    N: Debug + ?Sized,
{
    seat.helper();
    containment::not_contains(seat, MODE, haystack, needle, msg);
}

/// Records a failure when `got` does not contain every needle in order.
#[track_caller]
pub fn contains_in_order(seat: &dyn Seat, got: &str, needles: &[&str], msg: &str) {
    seat.helper();
    containment::contains_in_order(seat, MODE, got, needles, msg);
}

/// Records a failure when `got` does not contain the elements of `want`,
/// each as often, in some order.
#[track_caller]
pub fn is_permutation<T: PartialEq + Debug>(seat: &dyn Seat, got: &[T], want: &[T], msg: &str) {
    seat.helper();
    containment::is_permutation(seat, MODE, got, want, msg);
}

/// Records a failure when `got` does not start with `prefix`.
#[track_caller]
pub fn has_prefix(seat: &dyn Seat, got: &str, prefix: &str, msg: &str) {
    seat.helper();
    text::has_prefix(seat, MODE, got, prefix, msg);
}

/// Records a failure when `got` does not end with `suffix`.
#[track_caller]
pub fn has_suffix(seat: &dyn Seat, got: &str, suffix: &str, msg: &str) {
    seat.helper();
    text::has_suffix(seat, MODE, got, suffix, msg);
}

/// Records a failure when `pattern`, a pattern of the portable subset,
/// matches nowhere in `got`.
#[track_caller]
pub fn matches(seat: &dyn Seat, got: &str, pattern: &str, msg: &str) {
    seat.helper();
    text::matches(seat, MODE, got, pattern, msg);
}

/// Records a failure when `got` is further than `tolerance` from `want`.
#[track_caller]
pub fn close_to<N: Number>(seat: &dyn Seat, got: N, want: f64, tolerance: f64, msg: &str) {
    seat.helper();
    numbers::close_to(seat, MODE, got, want, tolerance, msg);
}

/// Records a failure when `got` falls outside the closed interval from `low`
/// to `high`.
#[track_caller]
pub fn in_range<N: Number>(seat: &dyn Seat, got: N, low: f64, high: f64, msg: &str) {
    seat.helper();
    numbers::in_range(seat, MODE, got, low, high, msg);
}

/// Records a failure when an adjacent pair of `items` does not satisfy
/// `predicate`.
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

/// Records a failure when `got` is `Err`.
#[track_caller]
pub fn no_error<T, E: Debug>(seat: &dyn Seat, got: &Result<T, E>, msg: &str) {
    seat.helper();
    errors::no_error(seat, MODE, got, msg);
}

/// Records a failure when `got` is `Ok`.
#[track_caller]
pub fn has_error<T: Debug, E>(seat: &dyn Seat, got: &Result<T, E>, msg: &str) {
    seat.helper();
    errors::has_error(seat, MODE, got, msg);
}

/// Records a failure when no error of the chain of `error` is a `T` equal to
/// `target`.
#[track_caller]
pub fn error_is<T>(seat: &dyn Seat, error: &(dyn Error + 'static), target: &T, msg: &str)
where
    T: Error + PartialEq + 'static,
{
    seat.helper();
    errors::error_is(seat, MODE, Some(error), target, msg);
}

/// Records a failure when an error of the chain of `error` is a `T` equal to
/// `target`.
#[track_caller]
pub fn error_is_not<T>(seat: &dyn Seat, error: &(dyn Error + 'static), target: &T, msg: &str)
where
    T: Error + PartialEq + 'static,
{
    seat.helper();
    errors::error_is_not(seat, MODE, Some(error), target, msg);
}

/// Returns the first error of the chain of `error` that is a `T`, and records
/// a failure when no error is.
#[track_caller]
pub fn error_as<'e, T: Error + 'static>(
    seat: &dyn Seat,
    error: &'e (dyn Error + 'static),
    msg: &str,
) -> Option<&'e T> {
    seat.helper();
    errors::error_as(seat, MODE, Some(error), msg)
}

/// Returns the message of the panic of `body`, and records a failure when
/// `body` returns.
#[track_caller]
pub fn panics(seat: &dyn Seat, body: impl FnOnce(), msg: &str) -> Option<String> {
    seat.helper();
    panics::panics(seat, MODE, body, msg)
}

/// Records a failure when `body` panics.
#[track_caller]
pub fn does_not_panic(seat: &dyn Seat, body: impl FnOnce(), msg: &str) {
    seat.helper();
    panics::does_not_panic(seat, MODE, body, msg);
}

/// Records a failure when a subject that receives a cancelled handle does not
/// return an error whose chain contains [`Stop::Cancelled`](crate::Stop::Cancelled).
#[track_caller]
pub fn honours_cancellation<E: Error + 'static>(
    seat: &dyn Seat,
    body: impl FnOnce(Option<&Cancel>) -> Result<(), E>,
    msg: &str,
) {
    seat.helper();
    behaviour::honours_cancellation(seat, MODE, body, msg);
}

/// Records a failure when a subject that receives an expired handle does not
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

/// Records a failure when `body` has not returned within `within`.
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

/// Records a failure when `body` changes what `observe` returns.
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

/// Records a failure when `body` leaves what `observe` returns unchanged.
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

/// Records a failure when a subject that receives no handle panics.
#[track_caller]
pub fn none_handle_safe<E: Debug>(
    seat: &dyn Seat,
    body: impl FnOnce(Option<&Cancel>) -> Result<(), E>,
    msg: &str,
) {
    seat.helper();
    behaviour::none_handle_safe(seat, MODE, body, msg);
}

/// Records a failure when a second call of `call` with `input` leaves
/// `observe` returning other than the first call left it.
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

/// Records a failure when the calls of `call` with `input` do not change the
/// count that `observe` returns by one amount each.
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

/// Records a failure when one of 32 calls of `call` with `input` returns a
/// result other than the first.
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

/// Records a failure when `combine(a, b)` differs from `combine(b, a)`.
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

/// Records a failure when the two groupings of `combine` over `a`, `b` and
/// `c` differ.
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

/// Records a failure when `inverse` of `forward` of `input` does not return
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

/// Records a failure when one of 32 iterations of `iterate` yields a
/// sequence other than the first.
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

/// Records a failure when one iteration of `iterate` yields an element twice.
#[track_caller]
pub fn no_duplicates<T, E>(seat: &dyn Seat, iterate: impl FnOnce() -> Result<Vec<T>, E>, msg: &str)
where
    T: PartialEq + Debug,
    E: Debug,
{
    seat.helper();
    relation::no_duplicates(seat, MODE, iterate, msg);
}

/// Records a failure when the reading of `observe` falls, or is NaN, while
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

/// Records a failure at the first element of `domain` for which `call`
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

/// Records a failure when `call`, after `close`, does not return an error
/// whose chain contains `sentinel`.
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

/// Records a failure when, after `induce`, one of 32 readings of `observe`
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

/// Records a failure when `body` does not pass within `timeout`, retried
/// every `interval`.
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

/// Records a failure when `predicate` does not become true within `timeout`.
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

/// Records a failure when `body` makes more than `ceiling` heap allocations
/// per call, counted as [`check::max_allocs`](crate::check::max_allocs)
/// counts them.
#[track_caller]
pub fn max_allocs(seat: &dyn Seat, body: impl FnMut(), ceiling: u64, msg: &str) {
    seat.helper();
    allocations::max_allocs(seat, MODE, body, ceiling, msg);
}

/// Records a failure when `body` makes more than `ceiling` heap allocations
/// per call on an input that `setup` builds before each call.
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
