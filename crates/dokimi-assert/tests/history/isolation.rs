//! The spec of `history::isolation`, the checks that list-append
//! transactions exhibit no anomaly that an isolation level forbids.

use crate::helpers::{Op, commit, records};
use dokimi_assert::history::{self, History};
use dokimi_assert::{Recorder, Value};
use serde_json::json;

/// The contract of every check of the spec.
const CONTRACT: &str = "the transactions are isolated";

/// Returns a write skew: two transactions that each read the key that the
/// other appends to, and read it empty.
fn write_skew() -> History {
    let h = History::new();
    commit(&h, 0, &[Op::Read("x", &[]), Op::Append("y", 1)]);
    commit(&h, 1, &[Op::Read("y", &[]), Op::Append("x", 1)]);
    h
}

mod is_serializable {
    use super::*;

    #[test]
    fn passes_transactions_in_one_order() {
        let h = History::new();
        commit(&h, 0, &[Op::Append("x", 1)]);
        commit(&h, 1, &[Op::Read("x", &[1]), Op::Append("x", 2)]);
        let seat = Recorder::new();
        history::is_serializable(&seat, &h, CONTRACT);
        assert!(!seat.failed());
        assert_eq!(records(&seat)[0]["verdict"], "pass");
    }

    #[test]
    fn reports_the_five_fields_of_a_write_skew() {
        let seat = Recorder::new();
        history::is_serializable(&seat, &write_skew(), CONTRACT);
        let detail = records(&seat)[0]["detail"].clone();
        assert_eq!(detail["anomaly"], "G2");
        assert_eq!(detail["kinds"], json!(["G2"]));
        assert_eq!(detail["transactions"].as_array().map(Vec::len), Some(2));
        assert_eq!(
            detail["cycle"],
            json!([{"call": 0, "relations": ["rw"]}, {"call": 2, "relations": ["rw"]}])
        );
        assert_eq!(detail["explanation"][0]["relation"], "rw");
        let failure = &seat.failures()[0];
        assert_eq!(failure.assertion(), Some("serializable"));
        assert_eq!(failure.field("anomaly"), Some(&Value::from("G2")));
    }

    #[test]
    fn ends_with_a_fault_at_a_call_that_is_no_transaction() {
        let h = History::new();
        h.invoke(0, "read", [], []).ok(Value::Null);
        let seat = Recorder::new();
        history::is_serializable(&seat, &h, CONTRACT);
        assert_eq!(
            seat.message(),
            r#"history::is_serializable: call 0 is "read", not "txn""#
        );
    }
}

mod has_snapshot_isolation {
    use super::*;

    #[test]
    fn permits_a_write_skew() {
        let seat = Recorder::new();
        history::has_snapshot_isolation(&seat, &write_skew(), CONTRACT);
        assert!(!seat.failed());
    }

    #[test]
    fn forbids_a_read_of_an_aborted_append() {
        let h = History::new();
        crate::helpers::txn(&h, 0, &[Op::Append("x", 1)]).fail("aborted");
        commit(&h, 1, &[Op::Read("x", &[1])]);
        let seat = Recorder::new();
        history::has_snapshot_isolation(&seat, &h, CONTRACT);
        let failure = &seat.failures()[0];
        assert_eq!(failure.assertion(), Some("snapshot-isolation"));
        assert_eq!(failure.field("anomaly"), Some(&Value::from("aborted-read")));
    }

    #[test]
    fn ends_with_a_fault_that_names_the_check() {
        let h = History::new();
        h.invoke(0, "read", [], []).ok(Value::Null);
        let seat = Recorder::new();
        history::has_snapshot_isolation(&seat, &h, CONTRACT);
        assert_eq!(
            seat.message(),
            r#"history::has_snapshot_isolation: call 0 is "read", not "txn""#
        );
    }
}
