//! The property forms: an assertion of the root namespace, run on each input
//! that a property generates.
//!
//! Each form takes a function of the input in place of each value that its
//! assertion takes, and its settings last. Its input is a value of `T`,
//! generated from the shape of `T` unless [`Options::using`] states another
//! generator. A failing input shrinks as a body's failing case does, and the
//! failure is one record of the form's id, such as `prop-equal`, whose
//! detail states the failure of the assertion.
//!
//! ```
//! use dokimi_assert::{Collector, prop};
//!
//! let t = Collector::new();
//! prop::equal(
//!     &t,
//!     |xs: &Vec<u8>| xs.iter().rev().rev().copied().collect::<Vec<_>>(),
//!     |xs: &Vec<u8>| xs.clone(),
//!     "reversing twice returns the list",
//!     prop::cases(50),
//! );
//! ```

use super::arbitrary::Arbitrary;
use super::form::{self, Form, INPUT, PAIR, TRIPLE};
use super::options::Options;
use crate::cancel::Cancel;
use crate::matcher::containment::{self, Contains};
use crate::matcher::numbers::{self, Number};
use crate::matcher::sizes::{self, Container};
use crate::matcher::{allocations, behaviour, errors, order, panics, relation, text, values};
use crate::seat::Seat;
use crate::verdict::Mode;
use std::error::Error;
use std::fmt::Debug;

/// Every form ends its case at the first failure, as `check` does.
const MODE: Mode = Mode::Fatal;

/// Returns the form of an operation and an id whose cases may run on more
/// than one worker.
const fn form(op: &'static str, id: &'static str) -> Form {
    Form {
        op,
        id,
        serial: false,
    }
}

/// Returns the error of a result, as the error forms read it.
fn error_of<O, E: Error + 'static>(result: &Result<O, E>) -> Option<&(dyn Error + 'static)> {
    result
        .as_ref()
        .err()
        .map(|error| error as &(dyn Error + 'static))
}

/// Runs [`check::equal`](crate::check::equal) on `got(&input)` and
/// `want(&input)` for each input that the run generates. It is the property
/// form `prop-equal`.
#[track_caller]
pub fn equal<T, U, W>(
    seat: &dyn Seat,
    got: impl Fn(&T) -> U + Sync,
    want: impl Fn(&T) -> W + Sync,
    msg: &str,
    options: Options<T>,
) where
    T: Arbitrary + Debug + Send,
    U: PartialEq<W> + Debug,
    W: Debug,
{
    seat.helper();
    form::run(
        seat,
        &form("prop::equal", "prop-equal"),
        INPUT,
        msg,
        options,
        |c, [input]| {
            values::equal(c, MODE, got(&input), want(&input), msg);
        },
    );
}

/// Runs [`check::not_equal`](crate::check::not_equal) on `got(&input)` and
/// `want(&input)` for each input that the run generates. It is the property
/// form `prop-not-equal`.
#[track_caller]
pub fn not_equal<T, U, W>(
    seat: &dyn Seat,
    got: impl Fn(&T) -> U + Sync,
    want: impl Fn(&T) -> W + Sync,
    msg: &str,
    options: Options<T>,
) where
    T: Arbitrary + Debug + Send,
    U: PartialEq<W> + Debug,
    W: Debug,
{
    seat.helper();
    form::run(
        seat,
        &form("prop::not_equal", "prop-not-equal"),
        INPUT,
        msg,
        options,
        |c, [input]| {
            values::not_equal(c, MODE, got(&input), want(&input), msg);
        },
    );
}

/// Runs [`check::is_true`](crate::check::is_true) on `condition(&input)` for
/// each input that the run generates. It is the property form `prop-true`.
#[track_caller]
pub fn is_true<T>(
    seat: &dyn Seat,
    condition: impl Fn(&T) -> bool + Sync,
    msg: &str,
    options: Options<T>,
) where
    T: Arbitrary + Debug + Send,
{
    seat.helper();
    form::run(
        seat,
        &form("prop::is_true", "prop-true"),
        INPUT,
        msg,
        options,
        |c, [input]| {
            values::is_true(c, MODE, condition(&input), msg);
        },
    );
}

/// Runs [`check::is_false`](crate::check::is_false) on `condition(&input)`
/// for each input that the run generates. It is the property form
/// `prop-false`.
#[track_caller]
pub fn is_false<T>(
    seat: &dyn Seat,
    condition: impl Fn(&T) -> bool + Sync,
    msg: &str,
    options: Options<T>,
) where
    T: Arbitrary + Debug + Send,
{
    seat.helper();
    form::run(
        seat,
        &form("prop::is_false", "prop-false"),
        INPUT,
        msg,
        options,
        |c, [input]| {
            values::is_false(c, MODE, condition(&input), msg);
        },
    );
}

/// Runs [`check::is_none`](crate::check::is_none) on `got(&input)` for each
/// input that the run generates. It is the property form `prop-nil`.
#[track_caller]
pub fn is_none<T, U>(
    seat: &dyn Seat,
    got: impl Fn(&T) -> Option<U> + Sync,
    msg: &str,
    options: Options<T>,
) where
    T: Arbitrary + Debug + Send,
    U: Debug,
{
    seat.helper();
    form::run(
        seat,
        &form("prop::is_none", "prop-nil"),
        INPUT,
        msg,
        options,
        |c, [input]| {
            values::is_none(c, MODE, got(&input), msg);
        },
    );
}

/// Runs [`check::is_some`](crate::check::is_some) on `got(&input)` for each
/// input that the run generates. It is the property form `prop-not-nil`.
#[track_caller]
pub fn is_some<T, U>(
    seat: &dyn Seat,
    got: impl Fn(&T) -> Option<U> + Sync,
    msg: &str,
    options: Options<T>,
) where
    T: Arbitrary + Debug + Send,
    U: Debug,
{
    seat.helper();
    form::run(
        seat,
        &form("prop::is_some", "prop-not-nil"),
        INPUT,
        msg,
        options,
        |c, [input]| {
            values::is_some(c, MODE, got(&input), msg);
        },
    );
}

/// Runs [`check::length`](crate::check::length) on `got(&input)` and `want`
/// for each input that the run generates. It is the property form
/// `prop-length`.
#[track_caller]
pub fn length<T, C>(
    seat: &dyn Seat,
    got: impl Fn(&T) -> C + Sync,
    want: usize,
    msg: &str,
    options: Options<T>,
) where
    T: Arbitrary + Debug + Send,
    C: Container,
{
    seat.helper();
    form::run(
        seat,
        &form("prop::length", "prop-length"),
        INPUT,
        msg,
        options,
        |c, [input]| {
            sizes::length(c, MODE, &got(&input), want, msg);
        },
    );
}

/// Runs [`check::is_empty`](crate::check::is_empty) on `got(&input)` for
/// each input that the run generates. It is the property form `prop-empty`.
#[track_caller]
pub fn is_empty<T, C>(seat: &dyn Seat, got: impl Fn(&T) -> C + Sync, msg: &str, options: Options<T>)
where
    T: Arbitrary + Debug + Send,
    C: Container,
{
    seat.helper();
    form::run(
        seat,
        &form("prop::is_empty", "prop-empty"),
        INPUT,
        msg,
        options,
        |c, [input]| {
            sizes::is_empty(c, MODE, &got(&input), msg);
        },
    );
}

/// Runs [`check::is_not_empty`](crate::check::is_not_empty) on `got(&input)`
/// for each input that the run generates. It is the property form
/// `prop-not-empty`.
#[track_caller]
pub fn is_not_empty<T, C>(
    seat: &dyn Seat,
    got: impl Fn(&T) -> C + Sync,
    msg: &str,
    options: Options<T>,
) where
    T: Arbitrary + Debug + Send,
    C: Container,
{
    seat.helper();
    form::run(
        seat,
        &form("prop::is_not_empty", "prop-not-empty"),
        INPUT,
        msg,
        options,
        |c, [input]| {
            sizes::is_not_empty(c, MODE, &got(&input), msg);
        },
    );
}

/// Runs [`check::contains`](crate::check::contains) on `got(&input)` and
/// `needle` for each input that the run generates. It is the property form
/// `prop-contains`.
#[track_caller]
pub fn contains<T, H, N>(
    seat: &dyn Seat,
    got: impl Fn(&T) -> H + Sync,
    needle: &N,
    msg: &str,
    options: Options<T>,
) where
    T: Arbitrary + Debug + Send,
    H: Contains<N> + Debug,
    N: Debug + Sync + ?Sized,
{
    seat.helper();
    form::run(
        seat,
        &form("prop::contains", "prop-contains"),
        INPUT,
        msg,
        options,
        |c, [input]| {
            containment::contains(c, MODE, &got(&input), needle, msg);
        },
    );
}

/// Runs [`check::not_contains`](crate::check::not_contains) on `got(&input)`
/// and `needle` for each input that the run generates. It is the property
/// form `prop-not-contains`.
#[track_caller]
pub fn not_contains<T, H, N>(
    seat: &dyn Seat,
    got: impl Fn(&T) -> H + Sync,
    needle: &N,
    msg: &str,
    options: Options<T>,
) where
    T: Arbitrary + Debug + Send,
    H: Contains<N> + Debug,
    N: Debug + Sync + ?Sized,
{
    seat.helper();
    form::run(
        seat,
        &form("prop::not_contains", "prop-not-contains"),
        INPUT,
        msg,
        options,
        |c, [input]| {
            containment::not_contains(c, MODE, &got(&input), needle, msg);
        },
    );
}

/// Runs [`check::contains_in_order`](crate::check::contains_in_order) on
/// `got(&input)` and `needles` for each input that the run generates. It is
/// the property form `prop-contains-in-order`.
#[track_caller]
pub fn contains_in_order<T, S>(
    seat: &dyn Seat,
    got: impl Fn(&T) -> S + Sync,
    needles: &[&str],
    msg: &str,
    options: Options<T>,
) where
    T: Arbitrary + Debug + Send,
    S: AsRef<str>,
{
    seat.helper();
    let form = form("prop::contains_in_order", "prop-contains-in-order");
    form::run(seat, &form, INPUT, msg, options, |c, [input]| {
        containment::contains_in_order(c, MODE, got(&input).as_ref(), needles, msg);
    });
}

/// Runs [`check::is_permutation`](crate::check::is_permutation) on
/// `got(&input)` and `want(&input)` for each input that the run generates. It
/// is the property form `prop-permutation`.
#[track_caller]
pub fn is_permutation<T, E>(
    seat: &dyn Seat,
    got: impl Fn(&T) -> Vec<E> + Sync,
    want: impl Fn(&T) -> Vec<E> + Sync,
    msg: &str,
    options: Options<T>,
) where
    T: Arbitrary + Debug + Send,
    E: PartialEq + Debug,
{
    seat.helper();
    form::run(
        seat,
        &form("prop::is_permutation", "prop-permutation"),
        INPUT,
        msg,
        options,
        |c, [input]| {
            containment::is_permutation(c, MODE, &got(&input), &want(&input), msg);
        },
    );
}

/// Runs [`check::has_prefix`](crate::check::has_prefix) on `got(&input)` and
/// `prefix` for each input that the run generates. It is the property form
/// `prop-has-prefix`.
#[track_caller]
pub fn has_prefix<T, S>(
    seat: &dyn Seat,
    got: impl Fn(&T) -> S + Sync,
    prefix: &str,
    msg: &str,
    options: Options<T>,
) where
    T: Arbitrary + Debug + Send,
    S: AsRef<str>,
{
    seat.helper();
    form::run(
        seat,
        &form("prop::has_prefix", "prop-has-prefix"),
        INPUT,
        msg,
        options,
        |c, [input]| {
            text::has_prefix(c, MODE, got(&input).as_ref(), prefix, msg);
        },
    );
}

/// Runs [`check::has_suffix`](crate::check::has_suffix) on `got(&input)` and
/// `suffix` for each input that the run generates. It is the property form
/// `prop-has-suffix`.
#[track_caller]
pub fn has_suffix<T, S>(
    seat: &dyn Seat,
    got: impl Fn(&T) -> S + Sync,
    suffix: &str,
    msg: &str,
    options: Options<T>,
) where
    T: Arbitrary + Debug + Send,
    S: AsRef<str>,
{
    seat.helper();
    form::run(
        seat,
        &form("prop::has_suffix", "prop-has-suffix"),
        INPUT,
        msg,
        options,
        |c, [input]| {
            text::has_suffix(c, MODE, got(&input).as_ref(), suffix, msg);
        },
    );
}

/// Runs [`check::matches`](crate::check::matches) on `got(&input)` and
/// `pattern` for each input that the run generates. It is the property form
/// `prop-matches`.
#[track_caller]
pub fn matches<T, S>(
    seat: &dyn Seat,
    got: impl Fn(&T) -> S + Sync,
    pattern: &str,
    msg: &str,
    options: Options<T>,
) where
    T: Arbitrary + Debug + Send,
    S: AsRef<str>,
{
    seat.helper();
    form::run(
        seat,
        &form("prop::matches", "prop-matches"),
        INPUT,
        msg,
        options,
        |c, [input]| {
            text::matches(c, MODE, got(&input).as_ref(), pattern, msg);
        },
    );
}

/// Runs [`check::close_to`](crate::check::close_to) on `got(&input)`, `want`
/// and `tolerance` for each input that the run generates. It is the property
/// form `prop-close-to`.
#[track_caller]
pub fn close_to<T, N>(
    seat: &dyn Seat,
    got: impl Fn(&T) -> N + Sync,
    want: f64,
    tolerance: f64,
    msg: &str,
    options: Options<T>,
) where
    T: Arbitrary + Debug + Send,
    N: Number,
{
    seat.helper();
    form::run(
        seat,
        &form("prop::close_to", "prop-close-to"),
        INPUT,
        msg,
        options,
        |c, [input]| {
            numbers::close_to(c, MODE, got(&input), want, tolerance, msg);
        },
    );
}

/// Runs [`check::in_range`](crate::check::in_range) on `got(&input)`, `low`
/// and `high` for each input that the run generates. It is the property form
/// `prop-in-range`.
#[track_caller]
pub fn in_range<T, N>(
    seat: &dyn Seat,
    got: impl Fn(&T) -> N + Sync,
    low: f64,
    high: f64,
    msg: &str,
    options: Options<T>,
) where
    T: Arbitrary + Debug + Send,
    N: Number,
{
    seat.helper();
    form::run(
        seat,
        &form("prop::in_range", "prop-in-range"),
        INPUT,
        msg,
        options,
        |c, [input]| {
            numbers::in_range(c, MODE, got(&input), low, high, msg);
        },
    );
}

/// Runs [`check::pairwise`](crate::check::pairwise) on `got(&input)` and
/// `predicate` for each input that the run generates. It is the property
/// form `prop-pairwise`.
#[track_caller]
pub fn pairwise<T, E>(
    seat: &dyn Seat,
    got: impl Fn(&T) -> Vec<E> + Sync,
    predicate: impl Fn(&E, &E) -> bool + Sync,
    msg: &str,
    options: Options<T>,
) where
    T: Arbitrary + Debug + Send,
    E: Debug,
{
    seat.helper();
    form::run(
        seat,
        &form("prop::pairwise", "prop-pairwise"),
        INPUT,
        msg,
        options,
        |c, [input]| {
            order::pairwise(c, MODE, &got(&input), &predicate, msg);
        },
    );
}

/// Runs [`check::no_error`](crate::check::no_error) on `call(&input)` for
/// each input that the run generates. It is the property form
/// `prop-err-absent`.
#[track_caller]
pub fn no_error<T, O, E>(
    seat: &dyn Seat,
    call: impl Fn(&T) -> Result<O, E> + Sync,
    msg: &str,
    options: Options<T>,
) where
    T: Arbitrary + Debug + Send,
    E: Debug,
{
    seat.helper();
    form::run(
        seat,
        &form("prop::no_error", "prop-err-absent"),
        INPUT,
        msg,
        options,
        |c, [input]| {
            errors::no_error(c, MODE, &call(&input), msg);
        },
    );
}

/// Runs [`check::has_error`](crate::check::has_error) on `call(&input)` for
/// each input that the run generates. It is the property form
/// `prop-err-present`.
#[track_caller]
pub fn has_error<T, O, E>(
    seat: &dyn Seat,
    call: impl Fn(&T) -> Result<O, E> + Sync,
    msg: &str,
    options: Options<T>,
) where
    T: Arbitrary + Debug + Send,
    O: Debug,
{
    seat.helper();
    form::run(
        seat,
        &form("prop::has_error", "prop-err-present"),
        INPUT,
        msg,
        options,
        |c, [input]| {
            errors::has_error(c, MODE, &call(&input), msg);
        },
    );
}

/// Runs [`check::error_is`](crate::check::error_is) on the error of
/// `call(&input)` and `target` for each input that the run generates. A call
/// that returns `Ok` fails. It is the property form `prop-err-is`.
#[track_caller]
pub fn error_is<T, O, E, X>(
    seat: &dyn Seat,
    call: impl Fn(&T) -> Result<O, E> + Sync,
    target: &X,
    msg: &str,
    options: Options<T>,
) where
    T: Arbitrary + Debug + Send,
    E: Error + 'static,
    X: Error + PartialEq + Sync + 'static,
{
    seat.helper();
    form::run(
        seat,
        &form("prop::error_is", "prop-err-is"),
        INPUT,
        msg,
        options,
        |c, [input]| {
            errors::error_is(c, MODE, error_of(&call(&input)), target, msg);
        },
    );
}

/// Runs [`check::error_is_not`](crate::check::error_is_not) on the error of
/// `call(&input)` and `target` for each input that the run generates. A call
/// that returns `Ok` passes. It is the property form `prop-err-is-not`.
#[track_caller]
pub fn error_is_not<T, O, E, X>(
    seat: &dyn Seat,
    call: impl Fn(&T) -> Result<O, E> + Sync,
    target: &X,
    msg: &str,
    options: Options<T>,
) where
    T: Arbitrary + Debug + Send,
    E: Error + 'static,
    X: Error + PartialEq + Sync + 'static,
{
    seat.helper();
    form::run(
        seat,
        &form("prop::error_is_not", "prop-err-is-not"),
        INPUT,
        msg,
        options,
        |c, [input]| {
            errors::error_is_not(c, MODE, error_of(&call(&input)), target, msg);
        },
    );
}

/// Runs [`check::error_as`](crate::check::error_as) of the error type `X` on
/// the error of `call(&input)` for each input that the run generates. A call
/// that returns `Ok` fails. It is the property form `prop-err-as`. A caller
/// states `X` first: `prop::error_as::<io::Error, _, _, _>(…)`.
#[track_caller]
pub fn error_as<X, T, O, E>(
    seat: &dyn Seat,
    call: impl Fn(&T) -> Result<O, E> + Sync,
    msg: &str,
    options: Options<T>,
) where
    X: Error + 'static,
    T: Arbitrary + Debug + Send,
    E: Error + 'static,
{
    seat.helper();
    form::run(
        seat,
        &form("prop::error_as", "prop-err-as"),
        INPUT,
        msg,
        options,
        |c, [input]| {
            errors::error_as::<X>(c, MODE, error_of(&call(&input)), msg);
        },
    );
}

/// Runs [`check::panics`](crate::check::panics) on a call of `body` with the
/// input for each input that the run generates. It is the property form
/// `prop-throws`.
#[track_caller]
pub fn panics<T>(seat: &dyn Seat, body: impl Fn(&T) + Sync, msg: &str, options: Options<T>)
where
    T: Arbitrary + Debug + Send,
{
    seat.helper();
    form::run(
        seat,
        &form("prop::panics", "prop-throws"),
        INPUT,
        msg,
        options,
        |c, [input]| {
            panics::panics(c, MODE, || body(&input), msg);
        },
    );
}

/// Runs [`check::does_not_panic`](crate::check::does_not_panic) on a call of
/// `body` with the input for each input that the run generates. It is the
/// property form `prop-not-throws`.
#[track_caller]
pub fn does_not_panic<T>(seat: &dyn Seat, body: impl Fn(&T) + Sync, msg: &str, options: Options<T>)
where
    T: Arbitrary + Debug + Send,
{
    seat.helper();
    form::run(
        seat,
        &form("prop::does_not_panic", "prop-not-throws"),
        INPUT,
        msg,
        options,
        |c, [input]| {
            panics::does_not_panic(c, MODE, || body(&input), msg);
        },
    );
}

/// Runs [`check::is_pure`](crate::check::is_pure) on `observe` and a call of
/// `body` with the input for each input that the run generates. It is the
/// property form `prop-pure`.
#[track_caller]
pub fn is_pure<T, S>(
    seat: &dyn Seat,
    observe: impl Fn() -> S + Sync,
    body: impl Fn(&T) + Sync,
    msg: &str,
    options: Options<T>,
) where
    T: Arbitrary + Debug + Send,
    S: PartialEq + Debug,
{
    seat.helper();
    form::run(
        seat,
        &form("prop::is_pure", "prop-pure"),
        INPUT,
        msg,
        options,
        |c, [input]| {
            behaviour::is_pure(c, MODE, &observe, || body(&input), msg);
        },
    );
}

/// Runs [`check::is_not_pure`](crate::check::is_not_pure) on `observe` and a
/// call of `body` with the input for each input that the run generates. It
/// is the property form `prop-not-pure`.
#[track_caller]
pub fn is_not_pure<T, S>(
    seat: &dyn Seat,
    observe: impl Fn() -> S + Sync,
    body: impl Fn(&T) + Sync,
    msg: &str,
    options: Options<T>,
) where
    T: Arbitrary + Debug + Send,
    S: PartialEq + Debug,
{
    seat.helper();
    form::run(
        seat,
        &form("prop::is_not_pure", "prop-not-pure"),
        INPUT,
        msg,
        options,
        |c, [input]| {
            relation::is_not_pure(c, MODE, &observe, || body(&input), msg);
        },
    );
}

/// Runs [`check::none_handle_safe`](crate::check::none_handle_safe) on `call`
/// with the input for each input that the run generates. It is the property
/// form `prop-nil-context-safe`.
#[track_caller]
pub fn none_handle_safe<T, E>(
    seat: &dyn Seat,
    call: impl Fn(Option<&Cancel>, &T) -> Result<(), E> + Sync,
    msg: &str,
    options: Options<T>,
) where
    T: Arbitrary + Debug + Send,
    E: Debug,
{
    seat.helper();
    let form = form("prop::none_handle_safe", "prop-nil-context-safe");
    form::run(seat, &form, INPUT, msg, options, |c, [input]| {
        behaviour::none_handle_safe(c, MODE, |cancel| call(cancel, &input), msg);
    });
}

/// Runs [`check::honours_cancellation`](crate::check::honours_cancellation)
/// on `call` with the input for each input that the run generates. It is the
/// property form `prop-honours-cancellation`.
#[track_caller]
pub fn honours_cancellation<T, E>(
    seat: &dyn Seat,
    call: impl Fn(Option<&Cancel>, &T) -> Result<(), E> + Sync,
    msg: &str,
    options: Options<T>,
) where
    T: Arbitrary + Debug + Send,
    E: Error + 'static,
{
    seat.helper();
    let form = form("prop::honours_cancellation", "prop-honours-cancellation");
    form::run(seat, &form, INPUT, msg, options, |c, [input]| {
        behaviour::honours_cancellation(c, MODE, |cancel| call(cancel, &input), msg);
    });
}

/// Runs [`check::honours_deadline`](crate::check::honours_deadline) on `call`
/// with the input for each input that the run generates. It is the property
/// form `prop-honours-deadline`.
#[track_caller]
pub fn honours_deadline<T, E>(
    seat: &dyn Seat,
    call: impl Fn(Option<&Cancel>, &T) -> Result<(), E> + Sync,
    msg: &str,
    options: Options<T>,
) where
    T: Arbitrary + Debug + Send,
    E: Error + 'static,
{
    seat.helper();
    let form = form("prop::honours_deadline", "prop-honours-deadline");
    form::run(seat, &form, INPUT, msg, options, |c, [input]| {
        behaviour::honours_deadline(c, MODE, |cancel| call(cancel, &input), msg);
    });
}

/// Runs [`check::max_allocs`](crate::check::max_allocs) on a call of `body`
/// with the input and `ceiling` for each input that the run generates, and
/// shrinks to the smallest input that allocates more. It is the property
/// form `prop-max-allocs`. The count is of the calling thread, so the cases
/// run one at a time whatever [`Options::workers`] states.
#[track_caller]
pub fn max_allocs<T>(
    seat: &dyn Seat,
    body: impl Fn(&T) + Sync,
    ceiling: u64,
    msg: &str,
    options: Options<T>,
) where
    T: Arbitrary + Debug + Send,
{
    seat.helper();
    let form = Form {
        op: "prop::max_allocs",
        id: "prop-max-allocs",
        serial: true,
    };
    form::run(seat, &form, INPUT, msg, options, |c, [input]| {
        allocations::max_allocs(c, MODE, || body(&input), ceiling, msg);
    });
}

/// Runs [`check::max_allocs_with_setup`](crate::check::max_allocs_with_setup)
/// on `body` and `ceiling` for each input that the run generates. Each call
/// of `body` takes what a call of `setup` builds from the input, outside the
/// count. It is the property form `prop-max-allocs-with-setup`, and its
/// cases run one at a time.
#[track_caller]
pub fn max_allocs_with_setup<T, I>(
    seat: &dyn Seat,
    setup: impl Fn(&T) -> I + Sync,
    body: impl Fn(I) + Sync,
    ceiling: u64,
    msg: &str,
    options: Options<T>,
) where
    T: Arbitrary + Debug + Send,
{
    seat.helper();
    let form = Form {
        op: "prop::max_allocs_with_setup",
        id: "prop-max-allocs-with-setup",
        serial: true,
    };
    form::run(seat, &form, INPUT, msg, options, |c, [input]| {
        allocations::max_allocs_with_setup(c, MODE, || setup(&input), &body, ceiling, msg);
    });
}

/// Runs [`check::is_idempotent`](crate::check::is_idempotent) on `call`, the
/// input and `observe` for each input that the run generates. It is the
/// property form `prop-idempotent`.
#[track_caller]
pub fn is_idempotent<I, S, E>(
    seat: &dyn Seat,
    call: impl Fn(&I) -> Result<(), E> + Sync,
    observe: impl Fn() -> S + Sync,
    msg: &str,
    options: Options<I>,
) where
    I: Arbitrary + Debug + Send,
    S: PartialEq + Debug,
    E: Debug,
{
    seat.helper();
    form::run(
        seat,
        &form("prop::is_idempotent", "prop-idempotent"),
        INPUT,
        msg,
        options,
        |c, [input]| {
            relation::is_idempotent(c, MODE, &call, input, &observe, msg);
        },
    );
}

/// Runs [`check::accumulates`](crate::check::accumulates) on `call`, the
/// input and `observe` for each input that the run generates. It is the
/// property form `prop-accumulates`.
#[track_caller]
pub fn accumulates<I, E>(
    seat: &dyn Seat,
    call: impl Fn(&I) -> Result<(), E> + Sync,
    observe: impl Fn() -> i64 + Sync,
    msg: &str,
    options: Options<I>,
) where
    I: Arbitrary + Debug + Send,
    E: Debug,
{
    seat.helper();
    form::run(
        seat,
        &form("prop::accumulates", "prop-accumulates"),
        INPUT,
        msg,
        options,
        |c, [input]| {
            relation::accumulates(c, MODE, &call, input, &observe, msg);
        },
    );
}

/// Runs [`check::is_deterministic`](crate::check::is_deterministic) on `call`
/// and the input for each input that the run generates. It is the property
/// form `prop-deterministic`.
#[track_caller]
pub fn is_deterministic<I, O, E>(
    seat: &dyn Seat,
    call: impl Fn(&I) -> Result<O, E> + Sync,
    msg: &str,
    options: Options<I>,
) where
    I: Arbitrary + Debug + Send,
    O: PartialEq + Debug,
    E: Debug,
{
    seat.helper();
    let form = form("prop::is_deterministic", "prop-deterministic");
    form::run(seat, &form, INPUT, msg, options, |c, [input]| {
        relation::is_deterministic(c, MODE, &call, input, msg);
    });
}

/// Runs [`check::is_commutative`](crate::check::is_commutative) on `combine`
/// and the two inputs `a` and `b` that each case generates. It is the
/// property form `prop-commutative`.
#[track_caller]
pub fn is_commutative<T, R>(
    seat: &dyn Seat,
    combine: impl Fn(&T, &T) -> R + Sync,
    msg: &str,
    options: Options<T>,
) where
    T: Arbitrary + Debug + Send,
    R: PartialEq + Debug,
{
    seat.helper();
    form::run(
        seat,
        &form("prop::is_commutative", "prop-commutative"),
        PAIR,
        msg,
        options,
        |c, [a, b]| {
            relation::is_commutative(c, MODE, &combine, a, b, msg);
        },
    );
}

/// Runs [`check::is_associative`](crate::check::is_associative) on `combine`
/// and the three inputs `a`, `b` and `c` that each case generates. It is the
/// property form `prop-associative`.
#[track_caller]
pub fn is_associative<T>(
    seat: &dyn Seat,
    combine: impl Fn(&T, &T) -> T + Sync,
    msg: &str,
    options: Options<T>,
) where
    T: Arbitrary + PartialEq + Debug + Send,
{
    seat.helper();
    let form = form("prop::is_associative", "prop-associative");
    form::run(seat, &form, TRIPLE, msg, options, |case, [a, b, c]| {
        relation::is_associative(case, MODE, &combine, a, b, c, msg);
    });
}

/// Runs [`check::round_trip`](crate::check::round_trip) on `forward`,
/// `inverse` and the input for each input that the run generates. It is the
/// property form `prop-round-trip`.
#[track_caller]
pub fn round_trip<I, O, E, F>(
    seat: &dyn Seat,
    forward: impl Fn(&I) -> Result<O, E> + Sync,
    inverse: impl Fn(&O) -> Result<I, F> + Sync,
    msg: &str,
    options: Options<I>,
) where
    I: Arbitrary + PartialEq + Debug + Send,
    E: Debug,
    F: Debug,
{
    seat.helper();
    form::run(
        seat,
        &form("prop::round_trip", "prop-round-trip"),
        INPUT,
        msg,
        options,
        |c, [input]| {
            relation::round_trip(c, MODE, &forward, &inverse, input, msg);
        },
    );
}
