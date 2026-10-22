//! The spec of `history::linearizable`, the check that a history is
//! linearizable.

use crate::helpers::{Register, read, records, write};
use dokimi_assert::history::{self, History, Operation, Spec};
use dokimi_assert::{Recorder, Value};
use serde_json::json;

/// The contract of every check of the spec.
const CONTRACT: &str = "the register is linearizable";

/// Returns the recorder of a check of `h` against `spec`.
fn checked<S: Spec>(h: &History, spec: &S) -> Recorder {
    let seat = Recorder::new();
    history::is_linearizable(&seat, h, spec, CONTRACT, history::Options::default());
    seat
}

/// Returns a history of a write of 1 and a read that returns 2.
fn violated() -> History {
    let h = History::new();
    write(&h, 0, 1);
    read(&h, 0, 2);
    h
}

/// A spec whose state has no typed literal.
struct Tallied;

/// The state of [`Tallied`].
#[derive(Debug, Clone, PartialEq)]
struct Tally(i64);

impl Spec for Tallied {
    type State = Tally;

    fn initial(&self) -> Tally {
        Tally(0)
    }

    fn next(&self, state: &Tally, op: &Operation) -> Vec<Tally> {
        if op.name() == "add" {
            vec![Tally(state.0 + 1)]
        } else {
            Vec::new()
        }
    }
}

/// A spec whose functions panic: `initial` when `at_start`, and `next`
/// otherwise.
struct Panicking {
    at_start: bool,
}

impl Spec for Panicking {
    type State = i64;

    fn initial(&self) -> i64 {
        assert!(!self.at_start, "the spec has no initial state");
        0
    }

    fn next(&self, _: &i64, _: &Operation) -> Vec<i64> {
        panic!("the spec has no next state")
    }
}

mod is_linearizable {
    use super::*;

    #[test]
    fn passes_a_history_whose_calls_have_an_order_that_the_spec_accepts() {
        let h = History::new();
        write(&h, 0, 1);
        read(&h, 1, 1);
        let seat = checked(&h, &Register);
        assert!(!seat.failed());
        assert_eq!(records(&seat)[0]["verdict"], "pass");
    }

    #[test]
    fn reports_the_ten_fields_of_a_violated_partition() {
        let seat = checked(&violated(), &Register);
        let detail = &records(&seat)[0]["detail"];
        assert_eq!(detail["outcome"], "violated");
        assert_eq!(detail["partitions"], 1);
        assert_eq!(detail["steps"], 2);
        assert_eq!(detail["partition"][0]["value"], "x");
        assert_eq!(detail["calls"], 2);
        assert_eq!(detail["concurrency"], 1);
        assert_eq!(detail["linearized"][0]["operation"], "write");
        assert_eq!(detail["states"], json!([{"type": "int", "value": 1}]));
        assert_eq!(detail["candidates"][0]["operation"], "read");
        assert_eq!(detail["limit"], serde_json::Value::Null);
    }

    #[test]
    fn reports_a_failure_of_the_assertion_linearizable() {
        let seat = checked(&violated(), &Register);
        let failure = &seat.failures()[0];
        assert_eq!(failure.assertion(), Some("linearizable"));
        assert_eq!(failure.contract(), CONTRACT);
        assert_eq!(failure.field("outcome"), Some(&Value::from("violated")));
        assert_eq!(failure.field("calls"), Some(&Value::from(2)));
    }

    #[test]
    fn states_a_state_without_a_typed_literal_by_its_debug_text() {
        let h = History::new();
        h.invoke(0, "add", [], []).ok(Value::Null);
        h.invoke(0, "remove", [], []).ok(Value::Null);
        let seat = checked(&h, &Tallied);
        let states = records(&seat)[0]["detail"]["states"].clone();
        assert_eq!(states, json!([{"type": "opaque", "text": "Tally(1)"}]));
    }

    #[test]
    fn ends_with_a_fault_at_a_spec_whose_next_panics() {
        let h = History::new();
        write(&h, 0, 1);
        let seat = checked(&h, &Panicking { at_start: false });
        assert_eq!(
            seat.message(),
            r#"history::is_linearizable: calls[0]: the spec's next panics on "write": the spec has no next state"#
        );
    }

    #[test]
    fn ends_with_a_fault_at_a_spec_whose_initial_state_panics() {
        let h = History::new();
        write(&h, 0, 1);
        let seat = checked(&h, &Panicking { at_start: true });
        assert_eq!(
            seat.message(),
            "history::is_linearizable: the spec's initial panics: the spec has no initial state"
        );
    }
}
