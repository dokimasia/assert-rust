//! The spec of `history::spec`, the sequential specification of an object
//! and a call as it sees one.

use crate::helpers::records;
use dokimi_assert::history::{self, History, Operation, Spec};
use dokimi_assert::{Recorder, Value};
use std::sync::Mutex;

/// A register spec that keeps each operation that the search stepped.
#[derive(Default)]
struct Watching {
    seen: Mutex<Vec<Operation>>,
}

impl Spec for Watching {
    type State = Value;

    fn initial(&self) -> Value {
        Value::Null
    }

    fn next(&self, state: &Value, op: &Operation) -> Vec<Value> {
        self.seen.lock().expect("no step panicked").push(op.clone());
        match op.name() {
            "write" => op.args().to_vec(),
            _ if op.returned(state.clone()) => vec![state.clone()],
            _ => Vec::new(),
        }
    }
}

/// A spec that rejects the operation `reject`, and whose every other
/// operation leaves two equal states.
struct Doubled;

impl Spec for Doubled {
    type State = i64;

    fn initial(&self) -> i64 {
        0
    }

    fn next(&self, state: &i64, op: &Operation) -> Vec<i64> {
        if op.name() == "reject" {
            return Vec::new();
        }
        vec![state + 1, state + 1]
    }
}

/// Returns the call record of a check of `h` against `spec`.
fn checked<S: Spec>(h: &History, spec: &S) -> serde_json::Value {
    let seat = Recorder::new();
    history::is_linearizable(
        &seat,
        h,
        spec,
        "the object is linearizable",
        history::Options::default(),
    );
    records(&seat).remove(0)
}

/// Returns a subject that counts: increment adds one and returns the count,
/// and any other operation returns the count.
fn counter() -> impl FnMut(&str, &[Value]) -> Value {
    let mut count = 0_i64;
    move |operation: &str, _: &[Value]| {
        if operation == "increment" {
            count += 1;
        }
        Value::from(count)
    }
}

mod operation {
    use super::*;

    #[test]
    fn states_the_name_the_arguments_and_the_output_of_a_known_call() {
        let h = History::new();
        h.invoke(0, "write", [Value::from(1)], []).ok(Value::Null);
        let spec = Watching::default();
        checked(&h, &spec);
        let seen = spec.seen.lock().expect("the check ended").clone();
        let op = &seen[0];
        assert_eq!(
            (op.name(), op.args()),
            ("write", [Value::from(1)].as_slice())
        );
        assert!(op.known());
        assert_eq!(op.output(), Some(&Value::Null));
        assert!(op.returned(Value::Null));
        assert!(!op.returned(1));
    }

    #[test]
    fn may_have_returned_any_value_for_a_call_that_is_not_known() {
        let h = History::new();
        h.invoke(0, "write", [Value::from(1)], [])
            .unknown("timeout");
        h.invoke(1, "read", [], []).ok(1);
        let spec = Watching::default();
        assert_eq!(checked(&h, &spec)["verdict"], "pass");
        let seen = spec.seen.lock().expect("the check ended").clone();
        let unknown = seen
            .iter()
            .find(|op| op.name() == "write")
            .expect("the search stepped the write");
        assert!(!unknown.known());
        assert_eq!(unknown.output(), None);
        assert!(unknown.returned(42));
    }
}

mod spec {
    use super::*;

    #[test]
    fn keeps_one_of_two_equal_states_by_default() {
        let h = History::new();
        h.invoke(0, "step", [], []).ok(Value::Null);
        h.invoke(0, "reject", [], []).ok(Value::Null);
        let record = checked(&h, &Doubled);
        assert_eq!(
            record["detail"]["states"],
            serde_json::json!([{"type": "int", "value": 1}])
        );
    }
}

mod spec_from {
    use super::*;

    #[test]
    fn passes_the_calls_of_an_atomic_subject() {
        let h = History::new();
        h.invoke(0, "increment", [], []).ok(1);
        h.invoke(1, "increment", [], []).ok(2);
        h.invoke(0, "read", [], []).ok(2);
        let record = checked(&h, &history::spec_from(counter));
        assert_eq!(record["verdict"], "pass");
    }

    #[test]
    fn fails_a_call_that_returned_what_no_order_returns() {
        let h = History::new();
        h.invoke(0, "increment", [], []).ok(1);
        h.invoke(1, "increment", [], []).ok(1);
        let record = checked(&h, &history::spec_from(counter));
        assert_eq!(record["detail"]["outcome"], "violated");
        assert_eq!(record["detail"]["steps"], 3);
    }

    #[test]
    fn hashes_the_arguments_of_the_calls_of_a_state() {
        let h = History::new();
        let first = h.invoke(0, "add", [Value::Int(2)], []);
        let second = h.invoke(1, "add", [Value::Int(3)], []);
        first.ok(2);
        second.ok(5);
        h.invoke(0, "read", [], []).ok(5);
        let adder = || {
            let mut total = 0;
            move |name: &str, args: &[Value]| {
                if let (Some(Value::Int(n)), "add") = (args.first(), name) {
                    total += n;
                }
                Value::Int(total)
            }
        };
        let record = checked(&h, &history::spec_from(adder));
        assert_eq!(record["verdict"], "pass", "{record}");
    }

    #[test]
    fn compares_the_arguments_of_the_calls_of_two_equal_states() {
        let h = History::new();
        let first = h.invoke(0, "add", [Value::Int(1)], []);
        let second = h.invoke(1, "add", [Value::Int(1)], []);
        first.ok(Value::Null);
        second.ok(Value::Null);
        h.invoke(0, "read", [], []).ok(5);
        let adder = || {
            let mut total = 0;
            move |name: &str, args: &[Value]| match (name, args.first()) {
                ("add", Some(Value::Int(n))) => {
                    total += n;
                    Value::Null
                }
                _ => Value::Int(total),
            }
        };
        let record = checked(&h, &history::spec_from(adder));
        assert_eq!(record["detail"]["outcome"], "violated", "{record}");
    }

    #[test]
    fn merges_two_orders_of_equal_calls() {
        let h = History::new();
        let first = h.invoke(0, "read", [], []);
        let second = h.invoke(1, "read", [], []);
        first.ok(0);
        second.ok(0);
        h.invoke(0, "read", [], []).ok(5);
        let record = checked(&h, &history::spec_from(counter));
        assert_eq!(record["detail"]["outcome"], "violated");
        assert_eq!(
            record["detail"]["linearized"].as_array().map(Vec::len),
            Some(2)
        );
    }
}
