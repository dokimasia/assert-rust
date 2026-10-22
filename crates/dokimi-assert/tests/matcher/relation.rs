//! The spec of `matcher/relation`: properties of a subject that hold across
//! calls, rather than of one value.

use crate::helpers::{failure_of, opaque, passes};
use dokimi_assert::{Value, check, soft};
use std::cell::Cell;
use std::error::Error;
use std::fmt;

/// A failure of a subject's own.
#[derive(Debug, PartialEq)]
struct Own;

impl fmt::Display for Own {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a failure of the subject's own")
    }
}

impl Error for Own {}

/// The sentinel of a closed resource.
#[derive(Debug, PartialEq)]
struct Closed;

impl fmt::Display for Closed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the resource is closed")
    }
}

impl Error for Closed {}

/// Returns `Err(Own)` from the call whose number is `failing`, counted from
/// 0, and `Ok` from every other call.
fn fails_on(calls: &Cell<usize>, failing: usize) -> Result<(), Own> {
    let call = calls.get();
    calls.set(call + 1);
    if call == failing { Err(Own) } else { Ok(()) }
}

mod is_idempotent {
    use super::*;

    #[test]
    fn passes_a_call_that_sets_a_value() {
        let state = Cell::new(0);
        let set = |input: &i32| {
            state.set(*input);
            Ok::<(), Own>(())
        };
        passes(|t| check::is_idempotent(t, set, 7, || state.get(), "c"));
    }

    #[test]
    fn states_the_readings_after_one_call_and_after_two() {
        let state = Cell::new(0);
        let add = |input: &i32| {
            state.set(state.get() + input);
            Ok::<(), Own>(())
        };
        let failure = failure_of(|t| check::is_idempotent(t, add, 7, || state.get(), "c"));
        assert_eq!(failure.assertion(), Some("idempotent"));
        assert_eq!(
            failure.detail(),
            [("first", opaque("7")), ("second", opaque("14"))]
        );
    }

    #[test]
    fn states_the_error_of_the_first_call_as_first() {
        let calls = Cell::new(0);
        let failure =
            failure_of(|t| check::is_idempotent(t, |_: &i32| fails_on(&calls, 0), 1, || 0, "c"));
        assert_eq!(
            failure.detail(),
            [("first", opaque("Own")), ("second", Value::Null)]
        );
    }

    #[test]
    fn states_the_error_of_the_second_call_as_second() {
        let calls = Cell::new(0);
        let failure =
            failure_of(|t| soft::is_idempotent(t, |_: &i32| fails_on(&calls, 1), 1, || 0, "c"));
        assert_eq!(
            failure.detail(),
            [("first", Value::Null), ("second", opaque("Own"))]
        );
    }

    #[test]
    fn states_the_message_of_a_panic() {
        let broken = |_: &i32| -> Result<(), Own> { panic!("broke") };
        let failure = failure_of(|t| check::is_idempotent(t, broken, 1, || 0, "c"));
        assert_eq!(
            failure.detail(),
            [("first", Value::from("broke")), ("second", Value::Null)]
        );
    }
}

mod accumulates {
    use super::*;

    #[test]
    fn passes_calls_that_each_add_the_same_amount() {
        let count = Cell::new(0_i64);
        let add = |n: &i64| {
            count.set(count.get() + n);
            Ok::<(), Own>(())
        };
        passes(|t| check::accumulates(t, add, 3, || count.get(), "c"));
    }

    #[test]
    fn fails_a_call_that_changes_nothing() {
        let failure =
            failure_of(|t| check::accumulates(t, |_: &i64| Ok::<(), Own>(()), 3, || 5, "c"));
        assert_eq!(failure.assertion(), Some("accumulates"));
        assert_eq!(
            failure.detail(),
            [("first", Value::Int(0)), ("second", Value::Int(0))]
        );
    }

    #[test]
    fn states_two_changes_that_differ() {
        let count = Cell::new(0_i64);
        let set = |n: &i64| {
            count.set(*n);
            Ok::<(), Own>(())
        };
        let failure = failure_of(|t| check::accumulates(t, set, 3, || count.get(), "c"));
        assert_eq!(
            failure.detail(),
            [("first", Value::Int(3)), ("second", Value::Int(0))]
        );
    }

    #[test]
    fn states_the_error_of_each_call_in_its_field() {
        let calls = Cell::new(0);
        let first =
            failure_of(|t| check::accumulates(t, |_: &i64| fails_on(&calls, 0), 1, || 0, "c"));
        assert_eq!(
            first.detail(),
            [("first", opaque("Own")), ("second", Value::Null)]
        );
        let calls = Cell::new(0);
        let second =
            failure_of(|t| check::accumulates(t, |_: &i64| fails_on(&calls, 1), 1, || 0, "c"));
        assert_eq!(
            second.detail(),
            [("first", Value::Null), ("second", opaque("Own"))]
        );
    }
}

mod is_deterministic {
    use super::*;

    #[test]
    fn passes_a_call_that_returns_the_same_result() {
        passes(|t| check::is_deterministic(t, |n: &i32| Ok::<i32, Own>(n * 2), 4, "c"));
    }

    #[test]
    fn states_the_first_result_with_the_first_that_differs() {
        let calls = Cell::new(0);
        let third = |_: &i32| {
            calls.set(calls.get() + 1);
            Ok::<usize, Own>(calls.get() / 3)
        };
        let failure = failure_of(|t| check::is_deterministic(t, third, 0, "c"));
        assert_eq!(failure.assertion(), Some("deterministic"));
        assert_eq!(
            failure.detail(),
            [("first", opaque("0")), ("second", opaque("1"))]
        );
    }

    #[test]
    fn calls_the_subject_32_times() {
        let calls = Cell::new(0);
        let count = |_: &i32| {
            calls.set(calls.get() + 1);
            Ok::<(), Own>(())
        };
        passes(|t| check::is_deterministic(t, count, 0, "c"));
        assert_eq!(calls.get(), 32);
    }

    #[test]
    fn states_an_error_after_the_first_call_as_second() {
        let calls = Cell::new(0);
        let failure =
            failure_of(|t| soft::is_deterministic(t, |_: &i32| fails_on(&calls, 5), 0, "c"));
        assert_eq!(
            failure.detail(),
            [("first", Value::Null), ("second", opaque("Own"))]
        );
    }
}

mod is_commutative {
    use super::*;

    #[test]
    fn passes_addition() {
        passes(|t| check::is_commutative(t, |a: &i32, b: &i32| a + b, 2, 3, "c"));
    }

    #[test]
    fn states_both_orders_of_subtraction() {
        let failure = failure_of(|t| check::is_commutative(t, |a: &i32, b: &i32| a - b, 2, 3, "c"));
        assert_eq!(failure.assertion(), Some("commutative"));
        assert_eq!(
            failure.detail(),
            [("first", opaque("-1")), ("second", opaque("1"))]
        );
    }

    #[test]
    fn states_the_message_of_a_panic_in_the_order_that_panicked() {
        let ordered = |a: &i32, b: &i32| {
            assert!(a < b, "the operands are reversed");
            a + b
        };
        let failure = failure_of(|t| check::is_commutative(t, ordered, 2, 3, "c"));
        assert_eq!(
            failure.detail(),
            [
                ("first", Value::Null),
                ("second", Value::from("the operands are reversed"))
            ]
        );
    }
}

mod is_associative {
    use super::*;

    #[test]
    fn passes_addition() {
        passes(|t| check::is_associative(t, |a: &i32, b: &i32| a + b, 2, 3, 5, "c"));
    }

    #[test]
    fn states_both_groupings_of_subtraction() {
        let failure =
            failure_of(|t| soft::is_associative(t, |a: &i32, b: &i32| a - b, 2, 3, 5, "c"));
        assert_eq!(failure.assertion(), Some("associative"));
        assert_eq!(
            failure.detail(),
            [("first", opaque("-6")), ("second", opaque("4"))]
        );
    }

    #[test]
    fn states_the_message_of_a_panic_in_the_left_grouping() {
        let overflowing = |_: &i32, _: &i32| -> i32 { panic!("overflow") };
        let failure = failure_of(|t| check::is_associative(t, overflowing, 1, 2, 3, "c"));
        assert_eq!(
            failure.detail(),
            [("first", Value::from("overflow")), ("second", Value::Null)]
        );
    }
}

mod round_trip {
    use super::*;

    #[test]
    fn passes_a_conversion_that_returns_the_input() {
        let render = |n: &i64| Ok::<_, Own>(n.to_string());
        passes(|t| check::round_trip(t, render, |s: &String| s.parse::<i64>(), -42, "c"));
    }

    #[test]
    fn states_the_input_with_what_came_back() {
        let render = |n: &i64| Ok::<_, Own>(n.abs().to_string());
        let failure =
            failure_of(|t| check::round_trip(t, render, |s: &String| s.parse::<i64>(), -42, "c"));
        assert_eq!(failure.assertion(), Some("round-trip"));
        assert_eq!(
            failure.detail(),
            [("want", opaque("-42")), ("got", opaque("42"))]
        );
    }

    #[test]
    fn states_an_error_of_the_forward_conversion() {
        let refuse = |_: &i64| Err::<String, _>(Own);
        let failure =
            failure_of(|t| check::round_trip(t, refuse, |s: &String| s.parse::<i64>(), 1, "c"));
        assert_eq!(
            failure.detail(),
            [("want", Value::Null), ("got", opaque("Own"))]
        );
    }

    #[test]
    fn states_an_error_of_the_inverse_conversion() {
        let render = |_: &i64| Ok::<_, Own>(String::from("x"));
        let failure =
            failure_of(|t| soft::round_trip(t, render, |s: &String| s.parse::<i64>(), 1, "c"));
        assert_eq!(
            failure.detail(),
            [
                ("want", Value::Null),
                ("got", opaque("ParseIntError { kind: InvalidDigit }"))
            ]
        );
    }

    #[test]
    fn states_the_message_of_a_panic() {
        let inverse = |_: &u8| -> Result<i64, Own> { panic!("no inverse") };
        let failure =
            failure_of(|t| check::round_trip(t, |_: &i64| Ok::<_, Own>(0_u8), inverse, 1, "c"));
        assert_eq!(
            failure.detail(),
            [("want", Value::Null), ("got", Value::from("no inverse"))]
        );
    }
}

mod has_stable_order {
    use super::*;

    #[test]
    fn passes_iterations_that_yield_one_order() {
        passes(|t| check::has_stable_order(t, || Ok::<_, Own>(vec![1, 2, 3]), "c"));
    }

    #[test]
    fn states_the_first_sequence_with_the_first_that_differs() {
        let calls = Cell::new(0);
        let rotating = || {
            let mut items = vec![1, 2, 3];
            items.rotate_left(calls.get() % 3);
            calls.set(calls.get() + 1);
            Ok::<_, Own>(items)
        };
        let failure = failure_of(|t| check::has_stable_order(t, rotating, "c"));
        assert_eq!(failure.assertion(), Some("stable-order"));
        assert_eq!(
            failure.detail(),
            [
                ("first", opaque("[1, 2, 3]")),
                ("second", opaque("[2, 3, 1]"))
            ]
        );
    }

    #[test]
    fn states_the_error_of_the_first_iteration_as_first() {
        let failure = failure_of(|t| check::has_stable_order(t, || Err::<Vec<i32>, _>(Own), "c"));
        assert_eq!(
            failure.detail(),
            [("first", opaque("Own")), ("second", Value::Null)]
        );
    }
}

mod no_duplicates {
    use super::*;

    #[test]
    fn passes_distinct_elements() {
        passes(|t| check::no_duplicates(t, || Ok::<_, Own>(vec!["a", "b"]), "c"));
    }

    #[test]
    fn states_the_first_element_that_repeats_with_its_position() {
        let failure =
            failure_of(|t| check::no_duplicates(t, || Ok::<_, Own>(vec![1, 2, 2, 1]), "c"));
        assert_eq!(failure.assertion(), Some("no-duplicates"));
        assert_eq!(
            failure.detail(),
            [("got", opaque("2")), ("index", Value::Int(2))]
        );
    }

    #[test]
    fn states_an_error_with_a_null_position() {
        let failure = failure_of(|t| soft::no_duplicates(t, || Err::<Vec<i32>, _>(Own), "c"));
        assert_eq!(
            failure.detail(),
            [("got", opaque("Own")), ("index", Value::Null)]
        );
    }
}

mod is_monotonic {
    use super::*;

    #[test]
    fn passes_a_reading_that_never_falls() {
        let count = Cell::new(0);
        let step = || {
            count.set(count.get() + 1);
            Ok::<(), Own>(())
        };
        passes(|t| check::is_monotonic(t, || count.get(), step, 5, "c"));
        assert_eq!(count.get(), 5);
    }

    #[test]
    fn passes_a_reading_that_stays_level() {
        passes(|t| check::is_monotonic(t, || 1, || Ok::<(), Own>(()), 3, "c"));
    }

    #[test]
    fn states_the_reading_that_falls_with_the_one_before() {
        let count = Cell::new(0);
        let wrap = || {
            count.set((count.get() + 1) % 3);
            Ok::<(), Own>(())
        };
        let failure = failure_of(|t| check::is_monotonic(t, || count.get(), wrap, 5, "c"));
        assert_eq!(failure.assertion(), Some("monotonic"));
        assert_eq!(
            failure.detail(),
            [
                ("index", Value::Int(3)),
                ("first", opaque("2")),
                ("second", opaque("0"))
            ]
        );
    }

    #[test]
    fn fails_a_first_reading_of_nan_at_position_0() {
        let failure =
            failure_of(|t| check::is_monotonic(t, || f64::NAN, || Ok::<(), Own>(()), 1, "c"));
        assert_eq!(
            failure.detail(),
            [
                ("index", Value::Int(0)),
                ("first", Value::Null),
                ("second", opaque("NaN"))
            ]
        );
    }

    #[test]
    fn fails_a_later_reading_of_nan() {
        let reading = Cell::new(1.0);
        let spoil = || {
            reading.set(f64::NAN);
            Ok::<(), Own>(())
        };
        let failure = failure_of(|t| soft::is_monotonic(t, || reading.get(), spoil, 2, "c"));
        assert_eq!(
            failure.detail(),
            [
                ("index", Value::Int(1)),
                ("first", opaque("1.0")),
                ("second", opaque("NaN"))
            ]
        );
    }

    #[test]
    fn states_an_error_of_a_step_with_null_positions() {
        let failure = failure_of(|t| check::is_monotonic(t, || 0, || Err::<(), _>(Own), 2, "c"));
        assert_eq!(
            failure.detail(),
            [
                ("index", Value::Null),
                ("first", Value::Null),
                ("second", opaque("Own"))
            ]
        );
    }

    #[test]
    fn states_the_message_of_a_panic_of_the_first_reading() {
        let unread = || -> i32 { panic!("no reading") };
        let failure = failure_of(|t| check::is_monotonic(t, unread, || Ok::<(), Own>(()), 2, "c"));
        assert_eq!(
            failure.detail(),
            [
                ("index", Value::Null),
                ("first", Value::Null),
                ("second", Value::from("no reading"))
            ]
        );
    }
}

mod is_total {
    use super::*;

    #[test]
    fn passes_a_call_that_succeeds_for_every_element() {
        passes(|t| check::is_total(t, |_: &u8| Ok::<(), Own>(()), &[1, 2, 3], "c"));
    }

    #[test]
    fn states_the_first_element_that_fails_with_what_ended_it() {
        let partial = |n: &u8| if *n > 1 { Err(Own) } else { Ok(()) };
        let failure = failure_of(|t| check::is_total(t, partial, &[1, 2, 3], "c"));
        assert_eq!(failure.assertion(), Some("total"));
        assert_eq!(
            failure.detail(),
            [("index", Value::Int(1)), ("got", opaque("Own"))]
        );
    }

    #[test]
    fn states_the_message_of_a_panic() {
        let unreachable = |_: &u8| -> Result<(), Own> { panic!("unreachable input") };
        let failure = failure_of(|t| soft::is_total(t, unreachable, &[9], "c"));
        assert_eq!(
            failure.detail(),
            [
                ("index", Value::Int(0)),
                ("got", Value::from("unreachable input"))
            ]
        );
    }
}

mod is_not_pure {
    use super::*;

    #[test]
    fn passes_a_body_that_changes_the_reading() {
        let count = Cell::new(0);
        passes(|t| check::is_not_pure(t, || count.get(), || count.set(1), "c"));
    }

    #[test]
    fn states_the_reading_that_did_not_change() {
        let failure = failure_of(|t| check::is_not_pure(t, || 4, || {}, "c"));
        assert_eq!(failure.assertion(), Some("not-pure"));
        assert_eq!(failure.detail(), [("got", opaque("4"))]);
    }

    #[test]
    fn states_the_message_of_a_panic() {
        let failure = failure_of(|t| soft::is_not_pure(t, || 4, || panic!("the body broke"), "c"));
        assert_eq!(failure.detail(), [("got", Value::from("the body broke"))]);
    }
}

mod fails_after_close {
    use super::*;

    #[test]
    fn passes_a_call_that_returns_the_sentinel() {
        let closed = Cell::new(false);
        let close = || {
            closed.set(true);
            Ok::<(), Own>(())
        };
        let call = || if closed.get() { Err(Closed) } else { Ok(()) };
        passes(|t| check::fails_after_close(t, close, call, &Closed, "c"));
    }

    #[test]
    fn states_the_sentinel_with_null_for_a_call_that_returns_ok() {
        let failure = failure_of(|t| {
            check::fails_after_close(
                t,
                || Ok::<(), Own>(()),
                || Ok::<(), Closed>(()),
                &Closed,
                "c",
            );
        });
        assert_eq!(failure.assertion(), Some("after-close"));
        assert_eq!(
            failure.detail(),
            [("want", opaque("Closed")), ("got", Value::Null)]
        );
    }

    #[test]
    fn states_another_error_of_the_call() {
        let failure = failure_of(|t| {
            check::fails_after_close(t, || Ok::<(), Own>(()), || Err::<(), _>(Own), &Closed, "c");
        });
        assert_eq!(
            failure.detail(),
            [("want", opaque("Closed")), ("got", opaque("Own"))]
        );
    }

    #[test]
    fn states_the_message_of_a_panic_of_the_call() {
        let freed = || -> Result<(), Closed> { panic!("used after free") };
        let failure =
            failure_of(|t| soft::fails_after_close(t, || Ok::<(), Own>(()), freed, &Closed, "c"));
        assert_eq!(
            failure.detail(),
            [
                ("want", opaque("Closed")),
                ("got", Value::from("used after free"))
            ]
        );
    }

    #[test]
    fn states_an_error_of_close_with_a_null_sentinel() {
        let failure = failure_of(|t| {
            check::fails_after_close(
                t,
                || Err::<(), _>(Own),
                || Err::<(), _>(Closed),
                &Closed,
                "c",
            );
        });
        assert_eq!(
            failure.detail(),
            [("want", Value::Null), ("got", opaque("Own"))]
        );
    }
}

mod is_poisoned {
    use super::*;

    #[test]
    fn passes_readings_that_each_return_an_error() {
        passes(|t| check::is_poisoned(t, || {}, || Err::<(), _>(Own), "c"));
    }

    #[test]
    fn passes_readings_that_each_panic() {
        let poisoned = || -> Result<(), Own> { panic!("poisoned") };
        passes(|t| check::is_poisoned(t, || {}, poisoned, "c"));
    }

    #[test]
    fn states_the_position_of_the_first_reading_that_succeeds() {
        let calls = Cell::new(0);
        let recovering = || {
            calls.set(calls.get() + 1);
            if calls.get() > 2 { Ok(()) } else { Err(Own) }
        };
        let failure = failure_of(|t| check::is_poisoned(t, || {}, recovering, "c"));
        assert_eq!(failure.assertion(), Some("poisoned"));
        assert_eq!(
            failure.detail(),
            [("index", Value::Int(2)), ("got", Value::Null)]
        );
    }

    #[test]
    fn states_the_message_of_a_panic_of_induce() {
        let failure = failure_of(|t| {
            soft::is_poisoned(t, || panic!("induce broke"), || Err::<(), _>(Own), "c");
        });
        assert_eq!(
            failure.detail(),
            [("index", Value::Null), ("got", Value::from("induce broke"))]
        );
    }
}
