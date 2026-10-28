// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `history::sentence`, the sentences of the records of the
//! history checks.

use crate::helpers::{Op, Register, commit, read, txn, write};
use dokimi_assert::Recorder;
use dokimi_assert::history::{self, History};

/// Returns the sentence of the failure of a check of linearizability of `h`
/// under `options`.
fn linearizable(h: &History, options: history::Options) -> String {
    let seat = Recorder::new();
    history::is_linearizable(&seat, h, &Register, "the register is linearizable", options);
    seat.failures()[0].to_string()
}

/// Returns the sentence of the failure of a check of serializability of `h`.
fn serializable(h: &History) -> String {
    let seat = Recorder::new();
    history::is_serializable(&seat, h, "the transactions are serializable");
    seat.failures()[0].to_string()
}

mod linearizable {
    use super::*;

    #[test]
    fn states_the_outcome_the_counts_and_the_frontier() {
        let h = History::new();
        write(&h, 0, 1);
        read(&h, 0, 2);
        assert_eq!(
            linearizable(&h, history::Options::default()),
            "the register is linearizable: violated in the partition of \"x\"\
             \n    steps 2, partitions 1, calls 2, concurrency 1\
             \n    linearized: call 0 write(1) → None\
             \n    states: 1\
             \n    rejected: call 2 read() → 2"
        );
    }

    #[test]
    fn states_the_limit_of_an_undecided_check_of_every_key() {
        let h = History::new();
        h.invoke(0, "write", [1.into()], [])
            .ok(dokimi_assert::Value::Null);
        let _pending = h.invoke(1, "read", [], []);
        assert_eq!(
            linearizable(&h, history::memo_limit(1)),
            "the register is linearizable: undecided in the partition of every key, at the memo limit\
             \n    steps 1, partitions 1, calls 2, concurrency 1\
             \n    linearized: none\
             \n    states: 0\
             \n    rejected: none"
        );
    }

    #[test]
    fn states_a_call_that_is_not_known_as_one_of_unknown_outcome() {
        let h = History::new();
        h.invoke(0, "write", [1.into()], ["x".into()])
            .unknown("timeout");
        read(&h, 1, 2);
        assert!(
            linearizable(&h, history::Options::default())
                .contains("call 0 write(1), outcome unknown")
        );
    }
}

mod isolation {
    use super::*;

    #[test]
    fn states_the_anomaly_the_kinds_and_each_edge_of_a_cycle() {
        let h = History::new();
        commit(&h, 0, &[Op::Read("x", &[]), Op::Append("y", 1)]);
        commit(&h, 1, &[Op::Read("y", &[]), Op::Append("x", 1)]);
        assert_eq!(
            serializable(&h),
            "the transactions are serializable: G2\
             \n    kinds: G2\
             \n    call 0 -rw-> call 2: call 0 read [] from \"x\", and call 2 appended 1 to it\
             \n    call 2 -rw-> call 0: call 2 read [] from \"y\", and call 0 appended 1 to it"
        );
    }

    #[test]
    fn states_a_read_that_ended_with_another_value_than_its_transaction_knew() {
        let h = History::new();
        commit(&h, 0, &[Op::Append("x", 5)]);
        commit(&h, 1, &[Op::Append("x", 1), Op::Read("x", &[5])]);
        assert_eq!(
            serializable(&h),
            "the transactions are serializable: internal-inconsistency\
             \n    kinds: internal-inconsistency\
             \n    call 2 read [5] from \"x\", and it knew the list ended with [1]"
        );
    }

    #[test]
    fn states_a_read_of_a_value_that_no_transaction_appended() {
        let h = History::new();
        commit(&h, 0, &[Op::Read("x", &[9])]);
        assert!(
            serializable(&h).ends_with("call 0 read [9] from \"x\", and no transaction appended 9")
        );
    }

    #[test]
    fn states_each_edge_of_a_register_cycle() {
        let h = History::new();
        commit(
            &h,
            0,
            &[Op::Write("x", 1), Op::Get("y", Some(3)), Op::Write("y", 4)],
        );
        commit(
            &h,
            1,
            &[Op::Get("x", Some(1)), Op::Write("x", 2), Op::Write("y", 3)],
        );
        assert_eq!(
            serializable(&h),
            "the transactions are serializable: G0\
             \n    kinds: G0, G1c\
             \n    call 0 -ww-> call 2: call 2 wrote 2 to \"x\" after 1\
             \n    call 2 -ww-> call 0: call 0 wrote 4 to \"y\" after 3"
        );
    }

    #[test]
    fn states_each_read_of_a_register_write_skew() {
        let h = History::new();
        commit(&h, 0, &[Op::Get("x", None), Op::Write("y", 1)]);
        commit(&h, 1, &[Op::Get("y", None), Op::Write("x", 2)]);
        assert_eq!(
            serializable(&h),
            "the transactions are serializable: G2\
             \n    kinds: G2\
             \n    call 0 -rw-> call 2: call 0 read the initial value of \"x\", and call 2 wrote 2 to it\
             \n    call 2 -rw-> call 0: call 2 read the initial value of \"y\", and call 0 wrote 1 to it"
        );
    }

    #[test]
    fn states_the_read_and_the_overwrite_of_a_register_read_skew() {
        let h = History::new();
        commit(&h, 0, &[Op::Write("x", 1), Op::Write("y", 1)]);
        commit(
            &h,
            1,
            &[Op::Get("x", Some(1)), Op::Write("x", 2), Op::Write("y", 2)],
        );
        commit(&h, 2, &[Op::Get("x", Some(1)), Op::Get("y", Some(2))]);
        assert!(serializable(&h).contains(
            "\n    call 4 -rw-> call 2: call 4 read 1 from \"x\", and call 2 wrote 2 after it\
             \n    call 2 -wr-> call 4: call 4 read 2 from \"y\""
        ));
    }

    #[test]
    fn states_the_observation_of_a_register_read() {
        let tests: [(&[Op], &str); 4] = [
            (
                &[Op::Write("y", 1), Op::Get("x", Some(9))],
                "call 0 read 9 from \"x\", and no transaction wrote it",
            ),
            (
                &[Op::Write("x", 1), Op::Get("x", None)],
                "call 0 read the initial value from \"x\", and it knew the value was 1",
            ),
            (
                &[
                    Op::Get("x", Some(1)),
                    Op::Get("x", Some(1)),
                    Op::Write("x", 1),
                ],
                "call 0 read 1 from \"x\", and 1 is a value that call 0 writes later",
            ),
            (
                &[Op::Get("x", None), Op::Get("x", Some(1)), Op::Write("x", 1)],
                "call 0 read 1 from \"x\", and it knew the value was the initial value, and 1 is a value that call 0 writes later",
            ),
        ];
        for (give, want) in tests {
            let h = History::new();
            commit(&h, 0, give);
            let got = serializable(&h);
            assert!(got.ends_with(want), "{got}");
        }
    }

    #[test]
    fn states_a_read_of_a_register_that_its_writer_aborted_or_overwrote() {
        let h = History::new();
        txn(&h, 0, &[Op::Write("x", 1)]).fail("aborted");
        commit(&h, 1, &[Op::Write("y", 1), Op::Write("y", 2)]);
        commit(&h, 2, &[Op::Get("x", Some(1)), Op::Get("y", Some(1))]);
        let seat = Recorder::new();
        history::has_snapshot_isolation(&seat, &h, "the transactions have snapshot isolation");
        let got = seat.failures()[0].to_string();
        assert!(
            got.ends_with("call 4 read 1 from \"x\", which call 0 wrote and aborted"),
            "{got}"
        );
        let h = History::new();
        commit(&h, 1, &[Op::Write("y", 1), Op::Write("y", 2)]);
        commit(&h, 2, &[Op::Get("y", Some(1))]);
        let got = serializable(&h);
        assert!(
            got.ends_with("call 2 read 1 from \"y\", which call 0 followed with 2"),
            "{got}"
        );
    }
}
