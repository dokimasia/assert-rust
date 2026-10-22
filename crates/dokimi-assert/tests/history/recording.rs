//! The spec of `history::recording`, which records the events of a history.

use crate::helpers::panic_of;
use dokimi_assert::Value;
use dokimi_assert::history::{History, Kind};

mod history {
    use super::*;

    #[test]
    fn records_an_invocation_with_its_arguments_and_keys() {
        let h = History::new();
        let _pending = h.invoke(3, "write", [Value::from(1)], [Value::from("x")]);
        let events = h.events();
        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert_eq!(
            (event.index, event.call, event.client, event.process),
            (0, 0, 3, 0)
        );
        let want = Kind::Invoke {
            operation: "write".to_owned(),
            args: vec![Value::from(1)],
            keys: vec![Value::from("x")],
        };
        assert_eq!(event.kind, want);
    }

    #[test]
    fn numbers_the_processes_in_the_order_of_first_invocation() {
        let h = History::new();
        h.invoke(5, "read", [], []).ok(0);
        h.invoke(2, "read", [], []).ok(0);
        h.invoke(5, "read", [], []).ok(0);
        let processes: Vec<(usize, usize)> = h
            .events()
            .iter()
            .map(|event| (event.client, event.process))
            .collect();
        assert_eq!(processes, [(5, 0), (5, 0), (2, 1), (2, 1), (5, 0), (5, 0)]);
    }

    #[test]
    fn starts_a_new_process_for_a_client_after_an_unknown_completion() {
        let h = History::new();
        h.invoke(0, "write", [Value::from(1)], [])
            .unknown("timeout");
        h.invoke(0, "read", [], []).ok(1);
        let processes: Vec<usize> = h.events().iter().map(|event| event.process).collect();
        assert_eq!(processes, [0, 0, 1, 1]);
    }

    #[test]
    fn panics_for_a_client_with_an_open_call() {
        let h = History::new();
        let _open = h.invoke(0, "write", [Value::from(1)], []);
        let raised = panic_of(|| drop(h.invoke(0, "read", [], [])));
        assert_eq!(
            raised.as_deref(),
            Some(r#"history: invoke(0, "read") while call 0 of client 0 is open"#)
        );
    }

    #[test]
    fn panics_for_a_key_that_contains_an_opaque_value() {
        let opaque = || Value::Opaque("handle".to_owned());
        let tests = [
            Value::List(vec![Value::from(1), opaque()]),
            Value::Map(vec![(opaque(), Value::from(1))]),
            Value::Map(vec![(Value::from(1), opaque())]),
            Value::Record(vec![("at".to_owned(), opaque())]),
            Value::Variant {
                name: "Some".to_owned(),
                payload: Some(Box::new(opaque())),
            },
        ];
        for give_key in tests {
            let h = History::new();
            let raised = panic_of(|| drop(h.invoke(0, "read", [], [give_key.clone()])))
                .expect("a key without a typed literal panics");
            assert!(
                raised.ends_with("which no typed literal states"),
                "{raised}"
            );
        }
    }

    #[test]
    fn takes_a_key_of_typed_literals_at_every_depth() {
        let tests = [
            Value::List(vec![Value::from(1), Value::from("a")]),
            Value::Map(vec![(Value::from("k"), Value::from(1))]),
            Value::Record(vec![("at".to_owned(), Value::from(1))]),
            Value::Variant {
                name: "None".to_owned(),
                payload: None,
            },
            Value::Variant {
                name: "Some".to_owned(),
                payload: Some(Box::new(Value::from(1))),
            },
        ];
        for give_key in tests {
            let h = History::new();
            h.invoke(0, "read", [], [give_key.clone()]).ok(Value::Null);
            assert_eq!(h.events().len(), 2);
        }
    }

    #[test]
    fn returns_the_events_of_every_completion_in_recording_order() {
        let h = History::new();
        let first = h.invoke(0, "write", [Value::from(1)], []);
        let second = h.invoke(1, "write", [Value::from(2)], []);
        second.ok(Value::Null);
        first.fail("refused");
        let calls: Vec<(usize, usize)> = h
            .events()
            .iter()
            .map(|event| (event.index, event.call))
            .collect();
        assert_eq!(calls, [(0, 0), (1, 1), (2, 1), (3, 0)]);
    }
}

mod call {
    use super::*;

    #[test]
    fn records_an_ok_completion_with_its_output() {
        let h = History::new();
        h.invoke(0, "read", [], []).ok(7);
        assert_eq!(
            h.events()[1].kind,
            Kind::Ok {
                output: Value::from(7)
            }
        );
    }

    #[test]
    fn records_a_failed_completion_with_its_error() {
        let h = History::new();
        h.invoke(0, "write", [Value::from(1)], []).fail("refused");
        let want = Kind::Fail {
            error: "refused".to_owned(),
        };
        assert_eq!(h.events()[1].kind, want);
    }

    #[test]
    fn records_an_unknown_completion_with_its_error() {
        let h = History::new();
        h.invoke(0, "write", [Value::from(1)], [])
            .unknown("timeout");
        let want = Kind::Unknown {
            error: "timeout".to_owned(),
        };
        assert_eq!(h.events()[1].kind, want);
    }
}
