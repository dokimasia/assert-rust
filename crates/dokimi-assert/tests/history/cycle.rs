// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `history::cycle`, the searches of the dependency graph of
//! committed transactions.

use crate::helpers::{Op, commit, records};
use dokimi_assert::Recorder;
use dokimi_assert::history::{self, History};
use serde_json::json;

/// Returns the detail of the record of a check of serializability of `h`.
fn detail(h: &History) -> serde_json::Value {
    let seat = Recorder::new();
    history::is_serializable(&seat, h, "the transactions are serializable");
    records(&seat)[0]["detail"].clone()
}

mod simple {
    use super::*;

    #[test]
    fn reduces_a_walk_that_visits_a_transaction_twice_to_a_cycle() {
        // The dependencies: a -rw-> b, b -ww,wr-> n, n -rw-> c, c -wr-> d,
        // d -rw-> n and n -wr-> a. The shortest alternating walk back to n
        // passes n twice, and the part between the two visits has two
        // adjacent read-write edges.
        let h = History::new();
        commit(&h, 0, &[Op::Read("k1", &[]), Op::Read("k6", &[1])]);
        commit(
            &h,
            1,
            &[
                Op::Append("k1", 1),
                Op::Append("k2", 1),
                Op::Append("k7", 1),
            ],
        );
        commit(
            &h,
            2,
            &[
                Op::Read("k2", &[1]),
                Op::Read("k3", &[]),
                Op::Append("k5", 1),
                Op::Append("k6", 1),
                Op::Append("k7", 2),
            ],
        );
        commit(&h, 3, &[Op::Append("k3", 1), Op::Append("k4", 1)]);
        commit(&h, 4, &[Op::Read("k4", &[1]), Op::Read("k5", &[])]);
        commit(&h, 5, &[Op::Read("k7", &[1, 2])]);
        let stated = detail(&h);
        assert_eq!(stated["anomaly"], "G-single");
        assert_eq!(stated["kinds"], json!(["G-single", "G2"]));
    }
}

mod components {
    use super::*;

    #[test]
    fn reports_the_cycle_of_the_component_with_the_lowest_transaction() {
        let h = History::new();
        commit(&h, 0, &[Op::Read("x", &[]), Op::Append("y", 1)]);
        commit(&h, 1, &[Op::Read("y", &[]), Op::Append("x", 1)]);
        commit(&h, 2, &[Op::Read("u", &[]), Op::Append("v", 1)]);
        commit(&h, 3, &[Op::Read("v", &[]), Op::Append("u", 1)]);
        let stated = detail(&h);
        assert_eq!(stated["cycle"][0]["call"], 0);
        assert_eq!(stated["cycle"][1]["call"], 2);
    }
}
