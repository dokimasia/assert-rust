//! The spec of `prop::engine::shrink`: the replay of a failing case, the
//! passes that shrink it, and the explanation of its draws.

use crate::helpers::records;
use dokimi_assert::prop::{self, Case};
use dokimi_assert::{Recorder, Value, check};
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering};

/// The record of a property under `options` whose body is `body`.
fn record(
    options: prop::Options,
    body: impl Fn(&Case<'_>) + Send + Sync,
) -> (serde_json::Value, String) {
    let (records, message) = run(options, body);
    (records[0].clone(), message)
}

/// Returns every call record of a property under `options` whose body is
/// `body`, and the message of its seat.
fn run(
    options: prop::Options,
    body: impl Fn(&Case<'_>) + Send + Sync,
) -> (Vec<serde_json::Value>, String) {
    let seat = Recorder::new();
    prop::for_all(
        &seat,
        "the property holds",
        options.hermetic().seed(6).store(""),
        body,
    );
    (records(&seat), seat.message())
}

/// Returns the values of the counterexample of a record, in order.
fn values(record: &serde_json::Value) -> Vec<serde_json::Value> {
    let entries = record["detail"]["counterexample"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    entries
        .iter()
        .map(|entry| entry["value"]["value"].clone())
        .collect()
}

/// Returns the largest int of a tree of lists, and 0 for a tree without an
/// int.
fn largest(tree: &Value) -> i128 {
    match tree {
        Value::Int(value) => *value,
        Value::List(children) => children.iter().map(largest).max().unwrap_or(0),
        _ => 0,
    }
}

/// Returns an around option that panics in place of the run of the body
/// once `fault` returns `true` for the value of `last` and the number of
/// the call.
fn faulting(
    last: Arc<AtomicI64>,
    fault: impl Fn(i64, usize) -> bool + Send + Sync + 'static,
) -> prop::Options {
    let calls = AtomicUsize::new(0);
    prop::around(move |run| {
        let call = calls.fetch_add(1, Ordering::SeqCst);
        run();
        assert!(
            !fault(last.load(Ordering::SeqCst), call),
            "the wrapper refuses the case"
        );
    })
}

/// Draws three values and fails with one of three identities: the first
/// while all three are not 0, the second while the first is 0, and the third
/// while the second is 0 too.
fn three_ways(c: &Case<'_>) {
    let n = c.draw(&prop::integer(0..=9_u8), "n");
    let m = c.draw(&prop::integer(0..=9_u8), "m");
    let k = c.draw(&prop::integer(0..=9_u8), "k");
    check::is_true(c, n == 0 || m == 0 || k == 0, "one value is 0");
    check::is_true(c, m == 0 || k == 0, "m or k is 0");
    check::is_true(c, k == 0, "k is 0");
}

/// Draws `n` and fails while it is 500 or more.
fn above_500(c: &Case<'_>) {
    let n = c.draw(&prop::integer(0..=1000_i64), "n");
    check::is_true(c, n < 500, "n stays below 500");
}

mod confirm {
    use super::*;

    #[test]
    fn ends_the_run_with_the_fault_of_the_replay() {
        let last = Arc::new(AtomicI64::new(0));
        let seen = Arc::clone(&last);
        let (record, message) = record(faulting(last, |_, call| call == 1), move |c| {
            let n = c.draw(&prop::integer(0..=9_i64), "n");
            seen.store(n, Ordering::SeqCst);
            check::is_true(c, false, "no case passes");
        });
        assert_eq!(record["verdict"], "error", "{record}");
        assert!(
            message.contains("around: the wrapper panicked: the wrapper refuses the case"),
            "{message}"
        );
    }
}

mod shrinker {
    use super::*;

    #[test]
    fn ends_the_run_with_the_fault_of_a_candidate() {
        let last = Arc::new(AtomicI64::new(0));
        let seen = Arc::clone(&last);
        let failed = Arc::new(AtomicBool::new(false));
        let failing = Arc::clone(&failed);
        let options = faulting(last, move |n, _| n == 0 && failed.load(Ordering::SeqCst));
        let (record, message) = record(options, move |c| {
            let n = c.draw(&prop::integer(0..=100_i64), "n");
            seen.store(n, Ordering::SeqCst);
            if n >= 50 {
                failing.store(true, Ordering::SeqCst);
            }
            check::is_true(c, n < 50, "n stays below 50");
        });
        assert_eq!(record["verdict"], "error", "{record}");
        assert!(
            message.contains("the wrapper refuses the case"),
            "{message}"
        );
    }

    #[test]
    fn shrinks_each_failure_that_the_shrink_of_another_meets() {
        let entries = r#"[{"label": "n", "value": {"type": "int", "value": 5}},
            {"label": "m", "value": {"type": "int", "value": 5}},
            {"label": "k", "value": {"type": "int", "value": 5}}]"#;
        let (record, _) = record(prop::draws(entries), three_ways);
        assert_eq!(values(&record), [json!(1), json!(1), json!(1)], "{record}");
        let others = record["detail"]["others"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let mut shrunk: Vec<Vec<serde_json::Value>> = others
            .iter()
            .map(|other| values(&json!({"detail": other})))
            .collect();
        shrunk.sort_by_key(|values| values.iter().map(ToString::to_string).collect::<Vec<_>>());
        assert_eq!(
            shrunk,
            [
                [json!(0), json!(0), json!(1)],
                [json!(0), json!(1), json!(1)]
            ],
            "{record}"
        );
    }

    #[test]
    fn lifts_a_descendant_with_the_label_of_its_ancestor() {
        let trees = prop::recursive(prop::integer(0..=9_u8).valued(), |tree| {
            prop::list(tree, 1..=1).valued()
        });
        let entries = r#"[{"label": "tree", "value": {"type": "list", "items": [{"type": "list", "of": "int", "value": [7]}]}}]"#;
        let (record, _) = record(prop::draws(entries), move |c| {
            let tree = c.draw(&trees, "tree");
            check::is_true(c, largest(&tree) < 5, "every int stays below 5");
        });
        assert_eq!(
            record["detail"]["counterexample"][0]["value"],
            json!({"type": "int", "value": 5}),
            "{record}"
        );
    }

    #[test]
    fn lowers_two_integers_together_by_the_largest_amount_that_keeps_the_failure() {
        let entries = r#"[{"label": "a", "value": {"type": "int", "value": 30}},
            {"label": "b", "value": {"type": "int", "value": 23}}]"#;
        let (record, _) = record(prop::draws(entries), |c| {
            let a = c.draw(&prop::integer(0..=100_i64), "a");
            let b = c.draw(&prop::integer(0..=100_i64), "b");
            check::is_true(c, a - b != 7 || b < 13, "the difference is not 7 above 13");
        });
        assert_eq!(values(&record), [json!(20), json!(13)], "{record}");
    }

    #[test]
    fn keeps_a_sequence_at_its_minimum_size() {
        let (record, _) = record(prop::hermetic(), |c| {
            let bytes = c.draw(&prop::bytes(2..=10), "bytes");
            check::is_true(c, bytes.len() < 3, "the bytes are short");
        });
        assert_eq!(values(&record), ["000000"], "{record}");
    }

    #[test]
    fn deletes_an_element_of_an_earlier_sequence_to_lower_a_count() {
        let entries = r#"[{"label": "bytes", "value": {"type": "bytes", "value": "010502"}},
            {"label": "n", "value": {"type": "int", "value": 3}}]"#;
        let (record, _) = record(prop::draws(entries), |c| {
            let bytes = c.draw(&prop::bytes(1..=8), "bytes");
            let n = c.draw(&prop::integer(0..=8_usize), "n");
            let counted = n != 0 && n == bytes.len() && bytes.contains(&5);
            check::is_true(c, !counted, "n is not the length of bytes with a 5");
        });
        assert_eq!(values(&record), [json!("05"), json!(1)], "{record}");
    }

    #[test]
    fn moves_the_value_of_an_integer_to_the_next_with_its_bounds() {
        let (record, _) = record(prop::hermetic(), |c| {
            let a = c.draw(&prop::integer(0..=20_u8), "a");
            let b = c.draw(&prop::integer(0..=20_u8), "b");
            check::is_true(c, a + b != 10, "the sum is not 10");
        });
        assert_eq!(values(&record), [json!(0), json!(10)], "{record}");
    }

    #[test]
    fn moves_equal_floats_to_their_target_together() {
        let entries = r#"[{"label": "x", "value": {"type": "float", "value": 0.5}},
            {"label": "y", "value": {"type": "float", "value": 0.5}},
            {"label": "z", "value": {"type": "float", "value": 0.75}}]"#;
        let (record, _) = record(prop::draws(entries), |c| {
            let x = c.draw(&prop::float(0.0..=1.0_f64), "x");
            let y = c.draw(&prop::float(0.0..=1.0_f64), "y");
            let z = c.draw(&prop::float(0.0..=1.0_f64), "z");
            check::is_true(
                c,
                x.to_bits() != y.to_bits() || z.to_bits() != 0.75_f64.to_bits(),
                "the floats differ",
            );
        });
        assert_eq!(
            values(&record),
            [json!(0.0), json!(0.0), json!(0.75)],
            "{record}"
        );
    }

    #[test]
    fn rounds_a_float_to_fewer_fractional_bits() {
        let entries = r#"[{"label": "x", "value": {"type": "float", "value": 7.375}}]"#;
        let (record, _) = record(prop::draws(entries), |c| {
            let x = c.draw(&prop::float(0.0..=10.0_f64), "x");
            check::is_true(c, x < 2.3, "x stays below 2.3");
        });
        assert_eq!(values(&record), [json!(3.0)], "{record}");
    }

    #[test]
    fn lowers_an_integral_float_without_bounds() {
        let entries = r#"[{"label": "x", "value": {"type": "float", "value": 9.0}}]"#;
        let (record, _) = record(prop::draws(entries), |c| {
            let x = c.draw(&prop::float(f64::NEG_INFINITY..=f64::INFINITY), "x");
            check::is_true(c, !(7.0..=1e9).contains(&x), "x stays outside 7 to 1e9");
        });
        assert_eq!(values(&record), [json!(7.0)], "{record}");
    }

    #[test]
    fn lowers_an_integral_float_of_width_32_to_a_value_of_the_width() {
        let (record, _) = record(prop::hermetic(), |c| {
            let x = c.draw(&prop::float(16_777_216.0..=33_554_432.0_f32), "x");
            check::is_true(c, x < 20_000_000.0, "x stays below 2e7");
        });
        assert_eq!(values(&record), [json!(20_000_000.0)], "{record}");
    }
}

mod explain {
    use super::*;

    /// Returns the runs of a property of `above_500` that shrinking and
    /// explaining spend: the calls of its body in those phases, one in each
    /// run.
    fn shrinker_runs() -> usize {
        let (records, _) = run(prop::hermetic(), above_500);
        let entry = &records[0]["detail"]["counterexample"][0];
        assert_eq!(entry["nearest-passing"]["value"], 499, "{entry}");
        records
            .iter()
            .filter(|record| record["phase"] == "shrink" || record["phase"] == "explain")
            .count()
    }

    #[test]
    fn ends_the_run_with_the_fault_of_a_filling() {
        let last = Arc::new(AtomicI64::new(0));
        let seen = Arc::clone(&last);
        let (record, message) = record(faulting(last, |n, _| n != 0), move |c| {
            let n = c.draw(&prop::integer(0..=1000_i64), "n");
            seen.store(n, Ordering::SeqCst);
            check::is_true(c, false, "no case passes");
        });
        assert_eq!(record["verdict"], "error", "{record}");
        assert!(
            message.contains("the wrapper refuses the case"),
            "{message}"
        );
    }

    #[test]
    fn ends_the_run_with_the_fault_of_the_nearest_value() {
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&calls);
        let counting = prop::around(move |run| {
            counted.fetch_add(1, Ordering::SeqCst);
            run();
        });
        let (counted_record, _) = record(counting, above_500);
        assert_eq!(
            counted_record["detail"]["counterexample"][0]["nearest-passing"]["value"],
            499
        );
        let nearest = calls.load(Ordering::SeqCst) - 1;
        let (record, message) = record(
            faulting(Arc::new(AtomicI64::new(0)), move |_, call| call == nearest),
            above_500,
        );
        assert_eq!(record["verdict"], "error", "{record}");
        assert!(
            message.contains("the wrapper refuses the case"),
            "{message}"
        );
    }

    #[test]
    fn states_no_explanation_of_a_draw_whose_nearest_value_the_budget_leaves_unrun() {
        let budget = shrinker_runs() - 1;
        let (record, _) = record(prop::shrink(budget), above_500);
        let entry = &record["detail"]["counterexample"][0];
        assert_eq!(entry["value"]["value"], 500, "{record}");
        assert_eq!(
            entry["any-value-fails"],
            serde_json::Value::Null,
            "{record}"
        );
        assert_eq!(
            entry["nearest-passing"],
            serde_json::Value::Null,
            "{record}"
        );
    }

    #[test]
    fn states_no_nearest_value_that_fails_another_way() {
        let (record, _) = record(prop::hermetic(), |c| {
            let n = c.draw(&prop::integer(0..=10_i64), "n");
            check::is_true(c, n < 4, "n stays below 4");
            check::is_true(c, n != 3, "n is not 3");
        });
        let entry = &record["detail"]["counterexample"][0];
        assert_eq!(entry["value"]["value"], 4, "{record}");
        assert_eq!(entry["any-value-fails"], false, "{record}");
        assert_eq!(
            entry["nearest-passing"],
            serde_json::Value::Null,
            "{record}"
        );
    }
}
