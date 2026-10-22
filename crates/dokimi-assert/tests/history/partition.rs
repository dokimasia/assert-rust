// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `history::partition`: the calls of a history as a check of
//! linearizability reads them, and their partitions.

use crate::helpers::{Register, records, write};
use dokimi_assert::history::{self, History};
use dokimi_assert::{Recorder, Value};

/// Returns the detail of the call record of a check of `h`.
fn detail(h: &History) -> serde_json::Value {
    let seat = Recorder::new();
    history::is_linearizable(
        &seat,
        h,
        &Register,
        "the register is linearizable",
        history::Options::default(),
    );
    records(&seat)[0]["detail"].clone()
}

mod partitions {
    use super::*;

    #[test]
    fn checks_an_empty_history_as_no_partition() {
        let seat = Recorder::new();
        history::is_linearizable(
            &seat,
            &History::new(),
            &Register,
            "c",
            history::Options::default(),
        );
        assert!(!seat.failed());
    }

    #[test]
    fn removes_the_calls_that_failed() {
        let h = History::new();
        h.invoke(0, "write", [5.into()], ["x".into()])
            .fail("refused");
        h.invoke(0, "read", [], ["x".into()]).ok(0);
        let seat = Recorder::new();
        history::is_linearizable(&seat, &h, &Register, "c", history::Options::default());
        assert!(!seat.failed());
    }

    #[test]
    fn puts_the_calls_that_share_a_key_in_one_partition() {
        let h = History::new();
        h.invoke(0, "write", [1.into()], ["a".into()])
            .ok(Value::Null);
        h.invoke(0, "write", [2.into()], ["b".into()])
            .ok(Value::Null);
        h.invoke(0, "read", [], ["b".into(), "c".into()]).ok(7);
        let stated = detail(&h);
        assert_eq!(stated["partitions"], 2);
        let keys: Vec<&serde_json::Value> = stated["partition"]
            .as_array()
            .expect("a list of keys")
            .iter()
            .map(|key| &key["value"])
            .collect();
        assert_eq!(keys, ["b", "c"]);
    }

    #[test]
    fn puts_every_call_in_one_partition_beside_a_call_without_keys() {
        let h = History::new();
        h.invoke(0, "write", [1.into()], ["a".into()])
            .ok(Value::Null);
        h.invoke(0, "read", [], []).ok(7);
        let stated = detail(&h);
        assert_eq!(stated["partitions"], 1);
        assert_eq!(stated["partition"], serde_json::json!([]));
    }
}

mod concurrency {
    use super::*;

    #[test]
    fn counts_the_most_calls_open_at_one_event() {
        let h = History::new();
        let first = h.invoke(0, "write", [1.into()], ["x".into()]);
        let second = h.invoke(1, "write", [2.into()], ["x".into()]);
        let third = h.invoke(2, "read", [], ["x".into()]);
        first.ok(Value::Null);
        second.ok(Value::Null);
        third.ok(9);
        assert_eq!(detail(&h)["concurrency"], 3);
    }

    #[test]
    fn keeps_a_call_that_is_not_known_open_to_the_end() {
        let h = History::new();
        h.invoke(0, "write", [1.into()], ["x".into()])
            .unknown("timeout");
        h.invoke(1, "read", [], ["x".into()]).ok(9);
        assert_eq!(detail(&h)["concurrency"], 2);
    }
}

mod span_json {
    use super::*;

    #[test]
    fn states_a_call_that_is_not_known_without_a_completion_and_an_output() {
        let h = History::new();
        write(&h, 0, 1);
        let _pending = h.invoke(1, "write", [2.into()], ["x".into()]);
        h.invoke(2, "read", [], ["x".into()]).ok(9);
        let stated = detail(&h);
        let pending = &stated["linearized"][1];
        assert_eq!(pending["call"], 2);
        assert!(pending.get("completion").is_none());
        assert!(pending.get("output").is_none());
        let known = &stated["linearized"][0];
        assert_eq!(known["completion"], 1);
        assert_eq!(known["process"], 0);
        assert_eq!(
            known["args"],
            serde_json::json!([{"type": "int", "value": 1}])
        );
        assert!(known.get("output").is_some());
    }
}
