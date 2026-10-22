// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `history::search`, the search of one partition for a
//! linearization.

use crate::helpers::{Register, read, records, write};
use dokimi_assert::history::{self, History, Operation, Options, Spec};
use dokimi_assert::{Recorder, Value};

/// Returns the call record of a check of `h` against `spec` under `options`.
fn checked<S: Spec>(h: &History, spec: &S, options: Options) -> (Recorder, serde_json::Value) {
    let seat = Recorder::new();
    history::is_linearizable(&seat, h, spec, "the register is linearizable", options);
    let record = records(&seat).remove(0);
    (seat, record)
}

/// A register spec whose `hash` or `equal` panics.
struct Unhashable {
    in_hash: bool,
}

impl Spec for Unhashable {
    type State = i64;

    fn initial(&self) -> i64 {
        0
    }

    fn next(&self, state: &i64, _: &Operation) -> Vec<i64> {
        vec![*state, state + 1]
    }

    fn equal(&self, a: &i64, b: &i64) -> bool {
        assert!(self.in_hash, "the spec compares no states");
        a == b
    }

    fn hash(&self, state: &i64) -> u64 {
        assert!(!self.in_hash, "the spec hashes no state");
        u64::try_from(*state).unwrap_or_default()
    }
}

mod search {
    use super::*;

    #[test]
    fn finds_the_order_of_two_concurrent_writes_that_a_read_needs() {
        let h = History::new();
        let first = h.invoke(0, "write", [1.into()], ["x".into()]);
        let second = h.invoke(1, "write", [2.into()], ["x".into()]);
        first.ok(Value::Null);
        second.ok(Value::Null);
        read(&h, 0, 1);
        let (seat, _) = checked(&h, &Register, Options::default());
        assert!(!seat.failed());
    }

    #[test]
    fn reports_the_frontier_with_the_most_linearized_calls() {
        let h = History::new();
        write(&h, 0, 1);
        write(&h, 0, 2);
        read(&h, 0, 1);
        let (_, record) = checked(&h, &Register, Options::default());
        let detail = &record["detail"];
        assert_eq!(detail["outcome"], "violated");
        let order: Vec<&serde_json::Value> = detail["linearized"]
            .as_array()
            .expect("a list")
            .iter()
            .map(|span| &span["call"])
            .collect();
        assert_eq!(order, [0, 2]);
        assert_eq!(detail["candidates"][0]["call"], 4);
    }

    #[test]
    fn visits_a_configuration_that_two_orders_reach_once() {
        let h = History::new();
        let first = h.invoke(0, "read", [], ["x".into()]);
        let second = h.invoke(1, "read", [], ["x".into()]);
        first.ok(0);
        second.ok(0);
        read(&h, 0, 5);
        let (_, record) = checked(&h, &Register, Options::default());
        assert_eq!(
            record["detail"]["linearized"].as_array().map(Vec::len),
            Some(2)
        );
        assert_eq!(record["detail"]["steps"], 5);
    }

    #[test]
    fn names_the_limit_that_ended_an_undecided_search() {
        let h = History::new();
        write(&h, 0, 1);
        write(&h, 0, 2);
        read(&h, 0, 2);
        let (_, record) = checked(&h, &Register, history::budget(2));
        assert_eq!(record["detail"]["outcome"], "undecided");
        assert_eq!(record["detail"]["limit"], "steps");
        assert_eq!(
            record["detail"]["linearized"].as_array().map(Vec::len),
            Some(2)
        );
    }

    #[test]
    fn ends_with_a_fault_at_a_spec_whose_hash_panics() {
        let h = History::new();
        write(&h, 0, 1);
        let (seat, _) = checked(&h, &Unhashable { in_hash: true }, Options::default());
        assert_eq!(
            seat.message(),
            r#"history::is_linearizable: calls[0]: the spec's hash panics on "write": the spec hashes no state"#
        );
    }

    #[test]
    fn ends_with_a_fault_at_a_spec_whose_equal_panics() {
        let h = History::new();
        write(&h, 0, 1);
        let (seat, _) = checked(&h, &Unhashable { in_hash: false }, Options::default());
        assert_eq!(
            seat.message(),
            r#"history::is_linearizable: calls[0]: the spec's equal panics on "write": the spec compares no states"#
        );
    }
}
