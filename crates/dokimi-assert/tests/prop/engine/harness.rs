// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `prop::engine::harness`, the entry points that the
//! conformance crate runs the engine through.

use crate::helpers::panic_of;
use dokimi_assert::Value;
use dokimi_assert::prop::engine::choice::Choice;
use dokimi_assert::prop::engine::collection::Sizes;
use dokimi_assert::prop::engine::execution::{Execution, Phase, Status};
use dokimi_assert::prop::engine::harness;
use dokimi_assert::prop::engine::runner::{Kind, Settings};
use dokimi_assert::prop::{self, Case};
use std::panic::{AssertUnwindSafe, catch_unwind};

/// Runs `body` under the settings of the seed 1 with `cases` cases, and
/// returns the outcome's kind and the status of each call that the observer
/// saw.
fn statuses(cases: usize, body: &(dyn Fn(&Case<'static>) + Sync)) -> (Kind, Vec<Status>) {
    let mut settings = Settings::new(1);
    settings.cases = cases;
    let mut seen = Vec::new();
    let outcome = harness::run(body, settings, &mut |_: Phase, execution: &Execution| {
        seen.push(execution.status());
    })
    .expect("the run ends without a fault");
    (outcome.kind, seen)
}

mod case {
    use super::*;

    #[test]
    fn fails_a_case_with_an_identity_and_ends_every_later_call() {
        let (kind, _) = statuses(5, &|c: &Case<'static>| {
            let _ = c.draw(&prop::boolean(), "flag");
            let ended = catch_unwind(AssertUnwindSafe(|| c.fail_as("first", true)));
            assert!(ended.is_err());
            c.fail_as("second", false);
        });
        assert_eq!(kind, Kind::Counterexample);
    }

    #[test]
    fn fails_a_case_with_the_record_of_an_assertion() {
        let (kind, seen) = statuses(5, &|c: &Case<'static>| {
            let n = c.draw(&prop::integer(0..=9_u8), "n");
            if n > 4 {
                c.fail_with(
                    "lost-write",
                    "the store keeps every write",
                    vec![("lost", Value::Int(1))],
                );
            }
        });
        assert_eq!(kind, Kind::Counterexample);
        assert!(seen.contains(&Status::Failed), "{seen:?}");
    }
}

mod valued {
    use super::*;

    #[test]
    fn states_the_values_of_a_generator_as_their_typed_literals() {
        let valued = prop::integer(0..=9_u8).valued();
        assert_eq!(
            harness::decode(&valued, vec![Choice::Integer(3)]).1,
            Some(Value::Int(3))
        );
        assert_eq!(format!("{valued:?}"), r#"Generator("integer")"#);
        let message = panic_of(|| drop(valued.max_leaves(1)));
        assert_eq!(
            message.as_deref(),
            Some("prop: max_leaves applies to no generator of the kind integer")
        );
    }

    #[test]
    fn states_a_value_without_a_typed_literal_as_opaque() {
        let valued = prop::integer(0..=9_u8).map(std::num::Wrapping).valued();
        assert_eq!(
            harness::decode(&valued, vec![Choice::Integer(3)]).1,
            Some(Value::Opaque("3".to_owned()))
        );
    }
}

mod duration_nanos {
    use super::*;

    #[test]
    fn refuses_bounds_outside_the_64_bit_ranges() {
        let refused = harness::duration_nanos(-1, i128::from(u64::MAX))
            .expect_err("the bounds span both ranges");
        assert!(
            refused.to_string().starts_with("integer bounds [-1, "),
            "{refused}"
        );
        let durations = harness::duration_nanos(-5, 5).expect("the bounds are signed 64-bit");
        assert_eq!(
            harness::decode(&durations, vec![Choice::Integer(-2)]).1,
            Some(Value::Int(-2))
        );
    }
}

mod pairs {
    use super::*;

    #[test]
    fn generates_maps_with_distinct_keys() {
        let sizes = Sizes::new(0, Some(3)).expect("the sizes admit a length");
        let maps = harness::pairs(
            prop::integer(0..=2_u8).valued(),
            prop::boolean().valued(),
            sizes,
        );
        assert_eq!(format!("{maps:?}"), r#"Generator("dict")"#);
        for index in 0..20 {
            let Some(Value::Map(entries)) = harness::generate(&maps, 4, index).1 else {
                panic!("a map decodes to the entries of a map");
            };
            let mut keys: Vec<String> = entries.iter().map(|(key, _)| key.to_string()).collect();
            keys.sort();
            keys.dedup();
            assert_eq!(keys.len(), entries.len());
        }
    }
}
