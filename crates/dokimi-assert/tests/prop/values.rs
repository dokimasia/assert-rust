//! The spec of `prop/values`: the values of a generator for a seed.

use crate::helpers::panic_of;
use dokimi_assert::history::{Operation, Spec};
use dokimi_assert::prop::Case;
use dokimi_assert::stateful::{self, Action, Machine};
use dokimi_assert::{Recorder, check, prop};
use std::sync::Mutex;

/// A spec whose initial state panics, so the steps of its machine end their
/// case with a fault.
struct Unstarted;

impl Spec for Unstarted {
    type State = i64;

    fn initial(&self) -> i64 {
        panic!("the spec has no initial state")
    }

    fn next(&self, state: &i64, _: &Operation) -> Vec<i64> {
        vec![*state]
    }
}

mod values {
    use super::*;

    #[test]
    fn returns_the_values_of_the_random_cases_of_the_seed() {
        // The vector integer/draws-from-a-signed-range of the definition.
        assert_eq!(
            prop::values(&prop::integer(-100..=100_i64), 42, 6),
            [11, 0, 76, 0, 96, 11]
        );
    }

    #[test]
    fn leaves_out_the_cases_that_the_generator_rejects() {
        // The vector filter/reuses-no-value-of-a-rejected-attempt of the
        // definition, whose cases 0, 2 and 3 the filter rejects.
        let lists = prop::list(prop::integer(0..=9_i64), ..=4)
            .filter(|xs: &Vec<i64>| xs.iter().sum::<i64>() > 10);
        assert_eq!(
            prop::values(&lists, 7, 3),
            [vec![0, 6, 8, 8], vec![2, 2, 9], vec![7, 4, 0, 5]]
        );
    }

    #[test]
    fn returns_the_same_first_values_for_a_larger_count() {
        let numbers = prop::integer(-100..=100_i64);
        assert_eq!(
            prop::values(&numbers, 42, 6)[..3],
            prop::values(&numbers, 42, 3)
        );
    }

    #[test]
    fn returns_no_value_for_a_count_of_0() {
        assert_eq!(
            prop::values(&prop::integer(-100..=100_i64), 42, 0),
            [] as [i64; 0]
        );
    }

    #[test]
    fn panics_for_a_generator_that_rejects_more_than_ten_cases_for_each_value() {
        let never = prop::integer(0..=9_u8).filter(|_| false);
        assert_eq!(
            panic_of(|| drop(prop::values(&never, 42, 2))).as_deref(),
            Some(
                "prop: values(generator, 42, 2) stops after the generator rejects 21 cases for 2 values"
            )
        );
    }

    #[test]
    fn passes_on_a_panic_of_the_generator() {
        let refusing = prop::integer(0..=9_u8).map(|_| -> u8 {
            panic!("the conversion refuses the value");
        });
        assert_eq!(
            panic_of(|| drop(prop::values(&refusing, 42, 1))).as_deref(),
            Some("the conversion refuses the value")
        );
    }

    #[test]
    fn panics_with_the_failure_of_a_generator_that_fails_its_case() {
        let failing = prop::composite(|c| {
            check::is_true(c, false, "the generator fails");
            0_u8
        });
        assert_eq!(
            panic_of(|| drop(prop::values(&failing, 1, 1))).as_deref(),
            Some(
                "prop: values(generator, 1, 1): the generator fails its case: the generator fails"
            )
        );
    }

    #[test]
    fn panics_with_the_reason_of_a_generator_that_ends_its_case_with_a_fault() {
        let faulting = prop::composite(|c| {
            let tick = Action::new("tick", async |_: &Case<'_>, _: usize| {});
            stateful::steps(c, &Machine::new([tick]).spec(Unstarted), stateful::max(1));
            0_u8
        });
        let message = panic_of(|| drop(prop::values(&faulting, 1, 1))).unwrap_or_default();
        assert!(
            message
                .starts_with("prop: values(generator, 1, 1): the generator ends its case: Fault("),
            "{message}"
        );
    }

    #[test]
    fn decodes_a_draw_of_of_with_the_generator_of_the_test_process() {
        let seat = Recorder::new();
        let inside = Mutex::new(Vec::new());
        let options = prop::using(&prop::just(7_u8))
            .hermetic()
            .seed(1)
            .store("")
            .cases(1);
        prop::for_all(&seat, "c", options, |c| {
            let _ = c.draw(&prop::boolean(), "flag");
            *inside.lock().expect("the lock is free") = prop::values(&prop::of::<u8>(), 42, 3);
        });
        assert!(!seat.failed(), "{}", seat.message());
        assert_eq!(
            *inside.lock().expect("the lock is free"),
            prop::values(&prop::of::<u8>(), 42, 3)
        );
    }
}
