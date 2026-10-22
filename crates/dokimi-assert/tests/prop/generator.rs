//! The spec of `prop::generator`, the generator and its combinators.

use crate::helpers::records;
use dokimi_assert::prop::engine::choice::Choice;
use dokimi_assert::prop::engine::harness;
use dokimi_assert::prop::{self, Case};
use dokimi_assert::{Recorder, check};
use serde_json::json;

/// Returns the call record of a run of `body` with the seed 1 on a
/// recorder.
fn run(body: impl Fn(&Case<'_>) + Send + Sync) -> serde_json::Value {
    let seat = Recorder::new();
    prop::for_all(
        &seat,
        "the generator holds",
        prop::hermetic().seed(1).store(""),
        body,
    );
    records(&seat).remove(0)
}

mod generator {
    use super::*;

    #[test]
    fn names_its_kind_in_its_debug_text() {
        assert_eq!(
            format!("{:?}", prop::integer(0..=3_u8)),
            r#"Generator("integer")"#
        );
        assert_eq!(
            format!("{:?}", prop::integer(0..=3_u8).map(u16::from)),
            r#"Generator("map")"#
        );
    }
}

mod map {
    use super::*;

    #[test]
    fn applies_its_function_to_each_value() {
        let doubled = prop::integer(0..=9_u8).map(|n| u16::from(n) * 2);
        assert_eq!(
            harness::decode(&doubled, vec![Choice::Integer(4)]).1,
            Some(8)
        );
    }

    #[test]
    fn states_the_nearest_passing_value_through_its_function() {
        let record = run(|c| {
            let n = c.draw(&prop::integer(0..=100_u8).map(|n| u16::from(n) * 2), "n");
            check::is_true(c, n < 150, "the double stays below 150");
        });
        let draw = &record["detail"]["counterexample"][0];
        assert_eq!(draw["value"], json!({"type": "int", "value": 150}));
        assert_eq!(
            draw["nearest-passing"],
            json!({"type": "int", "value": 148})
        );
    }
}

mod filter {
    use super::*;

    #[test]
    fn keeps_the_values_that_its_predicate_keeps() {
        let even = prop::integer(0..=9_u8).filter(|n| n % 2 == 0);
        for index in 0..20 {
            if let Some(n) = harness::generate(&even, 1, index).1 {
                assert_eq!(n % 2, 0);
            }
        }
    }

    #[test]
    fn rejects_the_case_after_three_attempts() {
        let none = prop::integer(0..=9_u8).filter(|_| false);
        assert_eq!(harness::decode(&none, Vec::new()).1, None);
        let record = run(|c| _ = c.draw(&none, "n"));
        assert_eq!(record["detail"]["outcome"], "rejected");
    }

    #[test]
    fn inverts_a_value_through_its_source() {
        let even = prop::integer(0..=9_u8).filter(|n| n % 2 == 0);
        let choices =
            harness::invert(&even, &dokimi_assert::Value::Int(4)).expect("the filter keeps 4");
        assert_eq!(harness::decode(&even, choices).1, Some(4));
    }
}

mod bind {
    use super::*;

    #[test]
    fn decodes_a_value_of_the_generator_that_its_function_returns() {
        let sized = prop::integer(1..=3_usize).bind(|n| prop::list(prop::boolean(), n..=n));
        for index in 0..20 {
            let list = harness::generate(&sized, 1, index)
                .1
                .expect("every case decodes");
            assert!((1..=3).contains(&list.len()));
        }
    }

    #[test]
    fn states_the_literal_of_its_value() {
        let record = run(|c| {
            let n = c.draw(&prop::integer(0..=9_u8).bind(prop::just), "n");
            check::is_true(c, n < 5, "n stays below 5");
        });
        assert_eq!(
            record["detail"]["counterexample"][0]["value"],
            json!({"type": "int", "value": 5})
        );
    }
}
