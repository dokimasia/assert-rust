// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `history::specs`, the named specs of the definition.

use dokimi_assert::history::{History, Spec};
use dokimi_assert::{Recorder, Value, history};
use dokimi_assert_conformance::history::specs::Named;

mod named {
    use super::*;

    #[test]
    fn names_each_spec_of_the_definition() {
        for name in [
            "register",
            "cas-register",
            "key-value",
            "queue",
            "set",
            "lossy-register",
        ] {
            assert!(Named::of(name).is_some(), "{name}");
        }
        assert_eq!(Named::of("made-up"), None);
    }

    #[test]
    fn compares_set_states_in_any_order() {
        let set = Named::Set;
        let a = Value::List(vec![Value::Int(1), Value::Int(2)]);
        let b = Value::List(vec![Value::Int(2), Value::Int(1)]);
        assert!(set.equal(&a, &b));
        assert_eq!(set.hash(&a), set.hash(&b));
        assert!(!Named::Queue.equal(&a, &b));
    }

    #[test]
    fn panics_on_an_operation_that_it_does_not_define() {
        let h = History::new();
        h.invoke(0, "pop", [], ["q".into()]).ok(Value::Null);
        let seat = Recorder::new();
        history::is_linearizable(
            &seat,
            &h,
            &Named::Queue,
            "the queue is linearizable",
            history::Options::default(),
        );
        let record: serde_json::Value =
            serde_json::from_str(&seat.records()[0]).expect("a call record is JSON");
        assert_eq!(record["verdict"], "error");
        assert!(
            record["error"].as_str().is_some_and(|error| error
                .contains(r#"the queue spec has no operation \"pop\""#)
                || error.contains("the queue spec has no operation \"pop\"")),
            "{record}"
        );
    }
}
