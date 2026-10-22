// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `prop::engine::edge`, the four cases of the edge phase.

use crate::helpers::records;
use dokimi_assert::{Recorder, check, prop};

mod edge {
    use super::*;

    #[test]
    fn finds_a_failure_at_the_largest_value_after_the_random_cases() {
        let seat = Recorder::new();
        prop::for_all(
            &seat,
            "n stays below its maximum",
            prop::hermetic().seed(1).cases(1),
            |c| {
                let n = c.draw(&prop::integer(0..=1_000_000_u32), "n");
                check::is_true(c, n < 1_000_000, "n stays below its maximum");
            },
        );
        let detail = &records(&seat)[0]["detail"];
        assert_eq!(detail["outcome"], "counterexample", "{detail}");
        assert_eq!(
            detail["counterexample"][0]["value"]["value"], 1_000_000,
            "{detail}"
        );
    }
}
