// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `prop::engine::coverage`, the test whether a label covers a
//! required share of the valid cases.

use dokimi_assert::prop::engine::coverage::{Verdict, verdict};

mod verdict {
    use super::*;

    #[test]
    fn leaves_a_requirement_without_a_valid_case_undecided_until_the_last_check() {
        assert_eq!(verdict(0, 0, 0.5, false, false), Verdict::Undecided);
        assert_eq!(verdict(0, 0, 0.5, true, false), Verdict::Unmet);
        assert_eq!(verdict(0, 0, 0.5, false, true), Verdict::Unmet);
    }

    #[test]
    fn decides_a_share_from_the_wilson_bounds() {
        assert_eq!(verdict(100, 100, 0.5, false, false), Verdict::Met);
        assert_eq!(verdict(0, 100, 0.5, false, false), Verdict::Refuted);
        assert_eq!(verdict(5, 10, 0.5, false, false), Verdict::Undecided);
        assert_eq!(verdict(5, 10, 0.5, true, false), Verdict::Met);
        assert_eq!(verdict(4, 10, 0.5, false, true), Verdict::Unmet);
    }

    #[test]
    fn names_each_verdict_as_a_record_states_it() {
        let names: Vec<&str> = [
            Verdict::Met,
            Verdict::Refuted,
            Verdict::Undecided,
            Verdict::Unmet,
        ]
        .into_iter()
        .map(Verdict::name)
        .collect();
        assert_eq!(names, ["met", "refuted", "undecided", "unmet"]);
    }
}
