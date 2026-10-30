// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `prop/values`: the values of a generator for a seed.

use crate::helpers::panic_of;
use dokimi_assert::history::{Operation, Spec};
use dokimi_assert::prop::{Case, Options};
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
            prop::values(&prop::integer(-100..=100_i64), 42, 6, Options::default()),
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
            prop::values(&lists, 7, 3, Options::default()),
            [vec![0, 6, 8, 8], vec![2, 2, 9], vec![7, 4, 0, 5]]
        );
    }

    #[test]
    fn returns_the_same_first_values_for_a_larger_count() {
        let numbers = prop::integer(-100..=100_i64);
        assert_eq!(
            prop::values(&numbers, 42, 6, Options::default())[..3],
            prop::values(&numbers, 42, 3, Options::default())
        );
    }

    #[test]
    fn returns_no_value_for_a_count_of_0() {
        assert_eq!(
            prop::values(&prop::integer(-100..=100_i64), 42, 0, Options::default()),
            [] as [i64; 0]
        );
    }

    #[test]
    fn decodes_a_draw_of_of_with_the_generator_that_using_states() {
        assert_eq!(
            prop::values(&prop::of::<u8>(), 42, 3, prop::using(&prop::just(7_u8))),
            [7, 7, 7]
        );
    }

    #[test]
    fn generates_each_value_of_a_stated_type_inside_a_derived_value() {
        let digits = prop::integer(1..=3_u16);
        let stated = prop::values(&prop::of::<Vec<u16>>(), 42, 20, prop::using(&digits));
        assert!(
            stated.iter().flatten().all(|n| (1..=3).contains(n)),
            "{stated:?}"
        );
        let unstated = prop::values(&prop::of::<Vec<u16>>(), 42, 20, Options::default());
        assert!(
            unstated.iter().flatten().any(|n| !(1..=3).contains(n)),
            "{unstated:?}"
        );
    }

    #[test]
    fn decodes_a_draw_of_its_type_inside_the_stated_generator_without_it() {
        let above = prop::of::<u32>().map(|n| n.wrapping_add(100));
        let want: Vec<u32> = prop::values(&prop::of::<u32>(), 42, 3, Options::default())
            .into_iter()
            .map(|n| n.wrapping_add(100))
            .collect();
        assert_eq!(
            prop::values(&prop::of::<u32>(), 42, 3, prop::using(&above)),
            want
        );
    }

    #[test]
    fn takes_the_later_using_of_one_type() {
        let options = prop::using(&prop::just(1_u8)).using(&prop::just(2_u8));
        assert_eq!(prop::values(&prop::of::<u8>(), 42, 2, options), [2, 2]);
    }

    #[test]
    fn panics_for_a_setting_other_than_using() {
        let digits = prop::integer(0..=9_u8);
        let tests: [Options; 2] = [prop::seed(1), prop::on_result(|_| {})];
        for tt in tests {
            assert_eq!(
                panic_of(|| drop(prop::values(&digits, 42, 3, tt))).as_deref(),
                Some("prop: values(generator, 42, 3) states a setting other than using")
            );
        }
    }

    #[test]
    fn panics_for_a_generator_that_rejects_more_than_ten_cases_for_each_value() {
        let never = prop::integer(0..=9_u8).filter(|_| false);
        assert_eq!(
            panic_of(|| drop(prop::values(&never, 42, 2, Options::default()))).as_deref(),
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
            panic_of(|| drop(prop::values(&refusing, 42, 1, Options::default()))).as_deref(),
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
            panic_of(|| drop(prop::values(&failing, 1, 1, Options::default()))).as_deref(),
            Some(
                "prop: values(generator, 1, 1): the generator fails its case: the generator fails"
            )
        );
    }

    #[test]
    fn panics_with_the_reason_of_a_generator_that_skips_its_case() {
        let skipping = prop::composite(|c| -> u8 { c.skip("the generator skips") });
        assert_eq!(
            panic_of(|| drop(prop::values(&skipping, 1, 1, Options::default()))).as_deref(),
            Some(
                "prop: values(generator, 1, 1) stops at a case that the generator skips: \
                 the generator skips"
            )
        );
    }

    #[test]
    fn panics_with_the_fault_that_ends_a_call_of_a_generator() {
        let faulting = prop::composite(|c| -> u8 {
            dokimi_assert::fault(c, std::io::Error::other("the store is gone"));
            0
        });
        assert_eq!(
            panic_of(|| drop(prop::values(&faulting, 1, 1, Options::default()))).as_deref(),
            Some(
                "prop: values(generator, 1, 1) stops at a fault of the generator: the store is gone"
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
        let message = panic_of(|| drop(prop::values(&faulting, 1, 1, Options::default())))
            .unwrap_or_default();
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
            *inside.lock().expect("the lock is free") =
                prop::values(&prop::of::<u8>(), 42, 3, Options::default());
        });
        assert!(!seat.failed(), "{}", seat.message());
        assert_eq!(
            *inside.lock().expect("the lock is free"),
            prop::values(&prop::of::<u8>(), 42, 3, Options::default())
        );
    }
}
