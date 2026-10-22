// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `prop::forms`, the property forms of the assertions.

use crate::helpers::records;
use dokimi_assert::prop::{self, Options};
use dokimi_assert::{Cancel, Recorder, Stop};
use std::hint::black_box;
use std::sync::atomic::{AtomicU8, Ordering};

/// Returns the settings of a form's run: 5 cases of the seed 1.
fn settings<T>() -> Options<T> {
    prop::hermetic().seed(1).cases(5).store("")
}

/// One call of a form whose property fails, and the id of its record.
struct Case {
    give: fn(&Recorder),
    want_assertion: &'static str,
}

/// Returns a failing call of each form.
#[expect(
    clippy::too_many_lines,
    reason = "the table states one call of each of the 39 forms"
)]
fn failing() -> Vec<Case> {
    vec![
        Case {
            give: |t| prop::equal(t, |n: &u8| *n, |n: &u8| n.wrapping_add(1), "c", settings()),
            want_assertion: "prop-equal",
        },
        Case {
            give: |t| prop::not_equal(t, |n: &u8| *n, |n: &u8| *n, "c", settings()),
            want_assertion: "prop-not-equal",
        },
        Case {
            give: |t| prop::is_true(t, |_: &u8| false, "c", settings()),
            want_assertion: "prop-true",
        },
        Case {
            give: |t| prop::is_false(t, |_: &u8| true, "c", settings()),
            want_assertion: "prop-false",
        },
        Case {
            give: |t| prop::is_none(t, |n: &u8| Some(*n), "c", settings()),
            want_assertion: "prop-nil",
        },
        Case {
            give: |t| prop::is_some(t, |_: &u8| None::<u8>, "c", settings()),
            want_assertion: "prop-not-nil",
        },
        Case {
            give: |t| prop::length(t, |_: &u8| "ab", 3, "c", settings()),
            want_assertion: "prop-length",
        },
        Case {
            give: |t| prop::is_empty(t, |_: &u8| "x", "c", settings()),
            want_assertion: "prop-empty",
        },
        Case {
            give: |t| prop::is_not_empty(t, |_: &u8| "", "c", settings()),
            want_assertion: "prop-not-empty",
        },
        Case {
            give: |t| prop::contains(t, |_: &u8| "ab", "z", "c", settings()),
            want_assertion: "prop-contains",
        },
        Case {
            give: |t| prop::not_contains(t, |_: &u8| "ab", "a", "c", settings()),
            want_assertion: "prop-not-contains",
        },
        Case {
            give: |t| prop::contains_in_order(t, |_: &u8| "ba", &["a", "b"], "c", settings()),
            want_assertion: "prop-contains-in-order",
        },
        Case {
            give: |t| prop::is_permutation(t, |_: &u8| vec![1], |_: &u8| vec![2], "c", settings()),
            want_assertion: "prop-permutation",
        },
        Case {
            give: |t| prop::has_prefix(t, |_: &u8| "ab", "b", "c", settings()),
            want_assertion: "prop-has-prefix",
        },
        Case {
            give: |t| prop::has_suffix(t, |_: &u8| "ab", "a", "c", settings()),
            want_assertion: "prop-has-suffix",
        },
        Case {
            give: |t| prop::matches(t, |_: &u8| "ab", "^b", "c", settings()),
            want_assertion: "prop-matches",
        },
        Case {
            give: |t| prop::close_to(t, |_: &u8| 2.0, 1.0, 0.5, "c", settings()),
            want_assertion: "prop-close-to",
        },
        Case {
            give: |t| prop::in_range(t, |_: &u8| 5, 0.0, 1.0, "c", settings()),
            want_assertion: "prop-in-range",
        },
        Case {
            give: |t| prop::pairwise(t, |_: &u8| vec![2, 1], |a, b| a < b, "c", settings()),
            want_assertion: "prop-pairwise",
        },
        Case {
            give: |t| prop::no_error(t, |_: &u8| Err::<(), _>(Stop::Cancelled), "c", settings()),
            want_assertion: "prop-err-absent",
        },
        Case {
            give: |t| prop::has_error(t, |_: &u8| Ok::<_, Stop>(1), "c", settings()),
            want_assertion: "prop-err-present",
        },
        Case {
            give: |t| {
                prop::error_is(
                    t,
                    |_: &u8| Err::<(), _>(Stop::Cancelled),
                    &Stop::DeadlineExceeded,
                    "c",
                    settings(),
                );
            },
            want_assertion: "prop-err-is",
        },
        Case {
            give: |t| {
                prop::error_is_not(
                    t,
                    |_: &u8| Err::<(), _>(Stop::Cancelled),
                    &Stop::Cancelled,
                    "c",
                    settings(),
                );
            },
            want_assertion: "prop-err-is-not",
        },
        Case {
            give: |t| {
                prop::error_as::<std::fmt::Error, u8, (), Stop>(
                    t,
                    |_| Err(Stop::Cancelled),
                    "c",
                    settings(),
                );
            },
            want_assertion: "prop-err-as",
        },
        Case {
            give: |t| prop::panics(t, |_: &u8| {}, "c", settings()),
            want_assertion: "prop-throws",
        },
        Case {
            give: |t| prop::does_not_panic(t, |_: &u8| panic!("the body fails"), "c", settings()),
            want_assertion: "prop-not-throws",
        },
        Case {
            give: |t| {
                let state = AtomicU8::new(0);
                prop::is_pure(
                    t,
                    || state.load(Ordering::SeqCst),
                    |n: &u8| state.store(n.wrapping_add(1), Ordering::SeqCst),
                    "c",
                    settings(),
                );
            },
            want_assertion: "prop-pure",
        },
        Case {
            give: |t| prop::is_not_pure(t, || 1, |_: &u8| {}, "c", settings()),
            want_assertion: "prop-not-pure",
        },
        Case {
            give: |t| {
                let call = |cancel: Option<&Cancel>, _: &u8| {
                    let _ = cancel.expect("the subject expects a handle");
                    Ok::<(), ()>(())
                };
                prop::none_handle_safe(t, call, "c", settings());
            },
            want_assertion: "prop-nil-context-safe",
        },
        Case {
            give: |t| {
                prop::honours_cancellation(t, |_, _: &u8| Ok::<(), Stop>(()), "c", settings());
            },
            want_assertion: "prop-honours-cancellation",
        },
        Case {
            give: |t| prop::honours_deadline(t, |_, _: &u8| Ok::<(), Stop>(()), "c", settings()),
            want_assertion: "prop-honours-deadline",
        },
        Case {
            give: |t| prop::max_allocs(t, |n: &u8| drop(black_box(vec![*n])), 0, "c", settings()),
            want_assertion: "prop-max-allocs",
        },
        Case {
            give: |t| {
                prop::max_allocs_with_setup(
                    t,
                    |n: &u8| *n,
                    |n| drop(black_box(vec![n])),
                    0,
                    "c",
                    settings(),
                );
            },
            want_assertion: "prop-max-allocs-with-setup",
        },
        Case {
            give: |t| {
                let state = AtomicU8::new(0);
                let add = |n: &u8| {
                    state.fetch_add(n.max(&1).to_owned(), Ordering::SeqCst);
                    Ok::<(), ()>(())
                };
                prop::is_idempotent(t, add, || state.load(Ordering::SeqCst), "c", settings());
            },
            want_assertion: "prop-idempotent",
        },
        Case {
            give: |t| prop::accumulates(t, |_: &u8| Ok::<(), ()>(()), || 0, "c", settings()),
            want_assertion: "prop-accumulates",
        },
        Case {
            give: |t| {
                let calls = AtomicU8::new(0);
                prop::is_deterministic(
                    t,
                    |_: &u8| Ok::<u8, ()>(calls.fetch_add(1, Ordering::SeqCst)),
                    "c",
                    settings(),
                );
            },
            want_assertion: "prop-deterministic",
        },
        Case {
            give: |t| prop::is_commutative(t, |a: &u8, b: &u8| a.wrapping_sub(*b), "c", settings()),
            want_assertion: "prop-commutative",
        },
        Case {
            give: |t| prop::is_associative(t, |a: &u8, b: &u8| a.wrapping_sub(*b), "c", settings()),
            want_assertion: "prop-associative",
        },
        Case {
            give: |t| {
                prop::round_trip(
                    t,
                    |n: &u8| Ok::<u8, ()>(n.wrapping_add(1)),
                    |n: &u8| Ok::<u8, ()>(*n),
                    "c",
                    settings(),
                );
            },
            want_assertion: "prop-round-trip",
        },
    ]
}

mod forms {
    use super::*;

    #[test]
    fn reports_a_failure_of_the_id_of_each_form() {
        let cases = failing();
        assert_eq!(cases.len(), 39);
        for case in cases {
            let seat = Recorder::new();
            (case.give)(&seat);
            let record = records(&seat).remove(0);
            assert_eq!(record["assertion"], case.want_assertion);
            assert_eq!(
                record["verdict"],
                "fail",
                "{}: {}",
                case.want_assertion,
                seat.message()
            );
        }
    }

    #[test]
    fn passes_a_form_whose_assertion_holds_for_every_input() {
        let seat = Recorder::new();
        prop::is_true(
            &seat,
            |n: &u8| u16::from(*n) < 256,
            "a byte is below 256",
            settings(),
        );
        prop::max_allocs(
            &seat,
            |n: &u8| _ = black_box(*n),
            0,
            "a copy allocates nothing",
            settings(),
        );
        prop::max_allocs_with_setup(
            &seat,
            |n: &u8| vec![*n],
            |list| drop(black_box(list)),
            0,
            "a drop allocates nothing",
            settings(),
        );
        assert!(!seat.failed(), "{}", seat.message());
    }
}
