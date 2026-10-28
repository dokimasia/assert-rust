// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `history::transaction`, which reads the calls of a history as
//! list-append or register transactions.

use crate::helpers::{Op, commit, records, txn};
use dokimi_assert::history::{self, History};
use dokimi_assert::{Recorder, Value};

/// Returns the fault of a check of serializability of `h`.
fn fault(h: &History) -> String {
    let seat = Recorder::new();
    history::is_serializable(&seat, h, "the transactions are serializable");
    assert_eq!(records(&seat)[0]["verdict"], "error");
    seat.message()
}

/// Returns a micro-operation of three parts.
fn mop(function: &str, key: Value, value: Value) -> Value {
    Value::List(vec![function.into(), key, value])
}

mod transactions {
    use super::*;

    #[test]
    fn refuses_a_call_of_another_operation() {
        let h = History::new();
        h.invoke(0, "write", [], []).ok(Value::Null);
        assert_eq!(
            fault(&h),
            r#"history::is_serializable: call 0 is "write", not "txn""#
        );
    }

    #[test]
    fn refuses_an_argument_that_is_no_micro_operation() {
        let tests = [
            Value::from(1),
            Value::List(vec!["delete".into(), "x".into(), 1.into()]),
            Value::List(vec!["append".into(), "x".into()]),
        ];
        for give in tests {
            let h = History::new();
            h.invoke(0, "txn", [give.clone()], []).ok(Value::Null);
            let refused = fault(&h);
            assert!(
                refused.ends_with(
                    "not [append, key, value], [write, key, value] or [read, key, null]"
                ),
                "{refused}"
            );
        }
    }

    #[test]
    fn refuses_a_read_that_states_a_value_before_it_ran() {
        let h = History::new();
        h.invoke(
            0,
            "txn",
            [mop("read", "x".into(), Value::List(Vec::new()))],
            [],
        )
        .ok(Value::Null);
        assert_eq!(
            fault(&h),
            "history::is_serializable: the read at argument 0 of call 0 states a value before it ran"
        );
    }

    #[test]
    fn refuses_a_write_of_null() {
        let h = History::new();
        let _pending = h.invoke(0, "txn", [mop("write", "x".into(), Value::Null)], []);
        assert_eq!(
            fault(&h),
            "history::is_serializable: the write at argument 0 of call 0 writes null, the initial value of every key"
        );
    }

    #[test]
    fn refuses_a_value_written_to_a_key_twice() {
        let tests: [(&[&[Op]], &str); 2] = [
            (
                &[&[Op::Write("x", 1), Op::Write("x", 1)]],
                "call 0 writes 1 to the key \"x\" twice",
            ),
            (
                &[&[Op::Write("x", 1)], &[Op::Write("x", 1)]],
                "calls 0 and 2 both write 1 to the key \"x\"",
            ),
        ];
        for (give, want) in tests {
            let h = History::new();
            for (client, ops) in give.iter().enumerate() {
                commit(&h, client, ops);
            }
            assert_eq!(fault(&h), format!("history::is_serializable: {want}"));
        }
    }

    #[test]
    fn refuses_a_history_that_appends_and_writes() {
        let h = History::new();
        commit(&h, 0, &[Op::Read("x", &[])]);
        commit(&h, 1, &[Op::Write("y", 1)]);
        commit(&h, 2, &[Op::Append("x", 1)]);
        assert_eq!(
            fault(&h),
            "history::is_serializable: call 4 appends and call 2 writes, and a history is either list-append or registers"
        );
    }

    #[test]
    fn refuses_a_register_read_of_a_value_without_a_typed_literal() {
        let h = History::new();
        commit(&h, 0, &[Op::Write("x", 1)]);
        h.invoke(1, "txn", [mop("read", "x".into(), Value::Null)], [])
            .ok(Value::List(vec![mop(
                "read",
                "x".into(),
                Value::Opaque("handle".to_owned()),
            )]));
        assert!(fault(&h).ends_with("which no typed literal states"));
    }

    #[test]
    fn reads_a_register_read_as_its_value() {
        let h = History::new();
        commit(&h, 0, &[Op::Write("x", 1)]);
        commit(&h, 1, &[Op::Get("x", Some(1)), Op::Get("y", None)]);
        let seat = Recorder::new();
        history::is_serializable(&seat, &h, "the transactions are serializable");
        assert!(!seat.failed());
    }

    #[test]
    fn refuses_a_key_or_a_value_without_a_typed_literal() {
        let opaque = Value::Opaque("handle".to_owned());
        let tests = [
            mop("append", opaque.clone(), 1.into()),
            mop("append", "x".into(), opaque.clone()),
            mop("read", opaque.clone(), Value::Null),
        ];
        for give in tests {
            let h = History::new();
            let _pending = h.invoke(0, "txn", [give.clone()], []);
            let refused = fault(&h);
            assert!(
                refused.ends_with("which no typed literal states"),
                "{refused}"
            );
        }
    }

    #[test]
    fn refuses_an_output_that_does_not_repeat_the_invocation() {
        let invoked = [
            mop("append", "x".into(), 1.into()),
            mop("read", "x".into(), Value::Null),
        ];
        let tests = [
            Value::Null,
            Value::List(vec![invoked[0].clone()]),
            Value::List(vec![Value::from(1), invoked[1].clone()]),
            Value::List(vec![Value::List(vec!["append".into()]), invoked[1].clone()]),
            Value::List(vec![mop("read", "x".into(), 1.into()), invoked[1].clone()]),
            Value::List(vec![
                mop("append", "y".into(), 1.into()),
                invoked[1].clone(),
            ]),
            Value::List(vec![
                mop("append", "x".into(), 2.into()),
                invoked[1].clone(),
            ]),
            Value::List(vec![
                invoked[0].clone(),
                mop("append", "x".into(), Value::Null),
            ]),
            Value::List(vec![
                invoked[0].clone(),
                mop("read", "y".into(), Value::Null),
            ]),
        ];
        for give_output in tests {
            let h = History::new();
            h.invoke(0, "txn", invoked.clone(), [])
                .ok(give_output.clone());
            let refused = fault(&h);
            assert!(
                refused.contains("does not repeat"),
                "the output {give_output} is refused with {refused}"
            );
        }
    }

    #[test]
    fn refuses_a_read_that_returned_no_list() {
        let h = History::new();
        let invoked = [mop("read", "x".into(), Value::Null)];
        h.invoke(0, "txn", invoked, [])
            .ok(Value::List(vec![mop("read", "x".into(), 1.into())]));
        assert_eq!(
            fault(&h),
            "history::is_serializable: the read at micro-operation 0 of the output of call 0 returned 1, not a list"
        );
    }

    #[test]
    fn refuses_a_read_of_a_value_without_a_typed_literal() {
        let h = History::new();
        let returned = Value::List(vec![Value::Opaque("handle".to_owned())]);
        h.invoke(0, "txn", [mop("read", "x".into(), Value::Null)], [])
            .ok(Value::List(vec![mop("read", "x".into(), returned)]));
        assert!(fault(&h).ends_with("which no typed literal states"));
    }

    #[test]
    fn refuses_a_call_that_appends_one_value_to_a_key_twice() {
        let h = History::new();
        commit(&h, 0, &[Op::Append("x", 1), Op::Append("x", 1)]);
        assert_eq!(
            fault(&h),
            "history::is_serializable: call 0 appends 1 to the key \"x\" twice"
        );
    }

    #[test]
    fn refuses_two_calls_that_append_one_value_to_a_key() {
        let h = History::new();
        commit(&h, 0, &[Op::Append("x", 1)]);
        commit(&h, 1, &[Op::Append("x", 1)]);
        assert_eq!(
            fault(&h),
            "history::is_serializable: calls 0 and 2 both append 1 to the key \"x\""
        );
    }

    #[test]
    fn reads_a_null_list_as_the_empty_list() {
        let h = History::new();
        h.invoke(0, "txn", [mop("read", "x".into(), Value::Null)], [])
            .ok(Value::List(vec![mop("read", "x".into(), Value::Null)]));
        let seat = Recorder::new();
        history::is_serializable(&seat, &h, "the transactions are serializable");
        assert!(!seat.failed());
    }

    #[test]
    fn reads_an_aborted_and_a_pending_transaction() {
        let h = History::new();
        txn(&h, 0, &[Op::Append("x", 1)]).fail("aborted");
        let _pending = txn(&h, 1, &[Op::Append("x", 2)]);
        commit(&h, 2, &[Op::Read("x", &[])]);
        let seat = Recorder::new();
        history::is_serializable(&seat, &h, "the transactions are serializable");
        assert!(!seat.failed());
    }
}

mod transaction_json {
    use super::*;

    #[test]
    fn states_the_completion_the_kind_and_the_output_of_each_transaction() {
        let h = History::new();
        txn(&h, 0, &[Op::Append("x", 1)]).fail("aborted");
        commit(&h, 1, &[Op::Read("x", &[1])]);
        let seat = Recorder::new();
        history::is_serializable(&seat, &h, "the transactions are serializable");
        let transactions = records(&seat)[0]["detail"]["transactions"].clone();
        assert_eq!(transactions[0]["kind"], "ok");
        assert_eq!(transactions[0]["completion"], 3);
        assert!(transactions[0].get("output").is_some());
        assert_eq!(transactions[1]["kind"], "fail");
        assert!(transactions[1].get("output").is_none());
    }
}
