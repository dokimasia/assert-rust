// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `history::derivation`, which derives the anomalies of the
//! list-append transactions of a history.

use crate::helpers::{Op, commit, records, txn};
use dokimi_assert::Recorder;
use dokimi_assert::history::{self, History};
use serde_json::{Value as Json, json};

/// Returns the detail of the record of a check of serializability of `h`.
fn detail(h: &History) -> Json {
    let seat = Recorder::new();
    history::is_serializable(&seat, h, "the transactions are serializable");
    records(&seat)[0]["detail"].clone()
}

/// Returns the typed literal of an int.
fn int(value: i64) -> Json {
    json!({"type": "int", "value": value})
}

/// Returns the typed literal of a string.
fn text(value: &str) -> Json {
    json!({"type": "string", "value": value})
}

mod anomalies_without_a_graph {
    use super::*;

    #[test]
    fn finds_a_read_of_a_value_that_no_transaction_appended() {
        let h = History::new();
        commit(&h, 0, &[Op::Read("x", &[9])]);
        let stated = detail(&h);
        assert_eq!(stated["anomaly"], "garbage-read");
        assert_eq!(stated["explanation"][0]["value"], int(9));
    }

    #[test]
    fn finds_a_read_that_contains_one_value_twice() {
        let h = History::new();
        commit(&h, 0, &[Op::Append("x", 1)]);
        commit(&h, 1, &[Op::Read("x", &[1, 1])]);
        assert_eq!(detail(&h)["anomaly"], "duplicate-append");
    }

    #[test]
    fn finds_a_read_that_misses_an_append_of_its_own_transaction() {
        let h = History::new();
        commit(
            &h,
            0,
            &[Op::Read("x", &[]), Op::Append("x", 1), Op::Read("x", &[])],
        );
        let stated = detail(&h);
        assert_eq!(stated["anomaly"], "internal-inconsistency");
        let observed = &stated["explanation"][0];
        assert_eq!(
            observed["expected"],
            json!({"type": "list", "of": "int", "value": [1]})
        );
        assert_eq!(observed["whole"], true);
        assert_eq!(observed["future"], Json::Null);
    }

    #[test]
    fn finds_a_read_of_a_value_that_its_transaction_appends_later() {
        let h = History::new();
        commit(&h, 0, &[Op::Read("x", &[1]), Op::Append("x", 1)]);
        let stated = detail(&h);
        assert_eq!(stated["anomaly"], "internal-inconsistency");
        assert_eq!(stated["explanation"][0]["future"], int(1));
    }

    #[test]
    fn finds_two_reads_that_are_not_prefixes_of_one_list() {
        let h = History::new();
        commit(&h, 0, &[Op::Append("x", 1), Op::Append("y", 1)]);
        commit(&h, 1, &[Op::Append("x", 2), Op::Append("y", 2)]);
        commit(&h, 2, &[Op::Read("x", &[1]), Op::Read("y", &[1])]);
        commit(&h, 3, &[Op::Read("x", &[2]), Op::Read("y", &[2])]);
        let stated = detail(&h);
        assert_eq!(stated["anomaly"], "incompatible-order");
        assert_eq!(stated["explanation"][0]["key"], text("x"));
        assert_eq!(stated["explanation"][0]["calls"], json!([4, 6]));
    }

    #[test]
    fn finds_a_read_of_an_append_that_its_transaction_aborted() {
        let h = History::new();
        txn(&h, 0, &[Op::Append("x", 1)]).fail("aborted");
        commit(&h, 1, &[Op::Read("x", &[1])]);
        let stated = detail(&h);
        assert_eq!(stated["anomaly"], "aborted-read");
        assert_eq!(stated["explanation"][0]["appender"], 0);
    }

    #[test]
    fn finds_a_read_of_an_append_that_its_appender_followed() {
        let h = History::new();
        commit(&h, 0, &[Op::Append("x", 1), Op::Append("x", 2)]);
        commit(&h, 1, &[Op::Read("x", &[1])]);
        let stated = detail(&h);
        assert_eq!(stated["anomaly"], "intermediate-read");
        assert_eq!(stated["explanation"][0]["next"], int(2));
    }

    #[test]
    fn commits_a_transaction_of_unknown_outcome_whose_append_a_read_observed() {
        let h = History::new();
        txn(&h, 0, &[Op::Append("x", 1)]).unknown("timeout");
        commit(&h, 1, &[Op::Read("x", &[1])]);
        let seat = Recorder::new();
        history::is_serializable(&seat, &h, "the transactions are serializable");
        assert!(!seat.failed());
    }
}

mod cycles {
    use super::*;

    #[test]
    fn finds_a_cycle_of_write_write_dependencies() {
        let h = History::new();
        commit(&h, 0, &[Op::Append("x", 1), Op::Append("y", 2)]);
        commit(&h, 1, &[Op::Append("x", 2), Op::Append("y", 1)]);
        commit(&h, 2, &[Op::Read("x", &[1, 2]), Op::Read("y", &[1, 2])]);
        let stated = detail(&h);
        assert_eq!(stated["anomaly"], "G0");
        assert_eq!(
            stated["cycle"],
            json!([{"call": 0, "relations": ["ww"]}, {"call": 2, "relations": ["ww"]}])
        );
    }

    #[test]
    fn finds_a_cycle_of_write_read_dependencies() {
        let h = History::new();
        commit(&h, 0, &[Op::Append("x", 1), Op::Read("y", &[1])]);
        commit(&h, 1, &[Op::Append("y", 1), Op::Read("x", &[1])]);
        assert_eq!(detail(&h)["anomaly"], "G1c");
    }

    #[test]
    fn finds_a_cycle_of_one_read_write_dependency() {
        let h = History::new();
        commit(&h, 0, &[Op::Read("x", &[]), Op::Read("y", &[1])]);
        commit(&h, 1, &[Op::Append("x", 1), Op::Append("y", 1)]);
        let stated = detail(&h);
        assert_eq!(stated["anomaly"], "G-single");
        assert_eq!(stated["kinds"], json!(["G-single", "G2"]));
    }

    #[test]
    fn finds_a_cycle_of_two_read_write_dependencies_that_are_not_adjacent() {
        let h = History::new();
        commit(&h, 0, &[Op::Read("a", &[]), Op::Read("d", &[1])]);
        commit(&h, 1, &[Op::Append("a", 1), Op::Append("b", 1)]);
        commit(&h, 2, &[Op::Read("b", &[1]), Op::Read("c", &[])]);
        commit(&h, 3, &[Op::Append("c", 1), Op::Append("d", 1)]);
        let stated = detail(&h);
        assert_eq!(stated["anomaly"], "G-nonadjacent");
        assert_eq!(stated["kinds"], json!(["G-nonadjacent", "G2"]));
        let relations: Vec<&Json> = stated["cycle"]
            .as_array()
            .expect("a cycle")
            .iter()
            .map(|link| &link["relations"])
            .collect();
        assert_eq!(
            relations,
            [
                &json!(["rw"]),
                &json!(["wr"]),
                &json!(["rw"]),
                &json!(["wr"])
            ]
        );
    }

    #[test]
    fn finds_a_cycle_of_two_adjacent_read_write_dependencies() {
        let h = History::new();
        commit(&h, 0, &[Op::Read("x", &[]), Op::Append("y", 1)]);
        commit(&h, 1, &[Op::Read("y", &[]), Op::Append("x", 1)]);
        assert_eq!(detail(&h)["anomaly"], "G2");
    }
}
