// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `matcher/text`: prefixes, suffixes and patterns of text.

use crate::helpers::{failure_of, passes};
use dokimi_assert::{Value, check, soft};

mod has_prefix {
    use super::*;

    #[test]
    fn passes_text_that_starts_with_the_prefix() {
        passes(|t| check::has_prefix(t, "GET /", "GET", "c"));
    }

    #[test]
    fn states_the_text_with_the_prefix() {
        let failure = failure_of(|t| check::has_prefix(t, "GET /", "POST", "c"));
        assert_eq!(failure.assertion(), Some("has-prefix"));
        assert_eq!(
            failure.detail(),
            [
                ("got", Value::from("GET /")),
                ("prefix", Value::from("POST"))
            ]
        );
    }
}

mod has_suffix {
    use super::*;

    #[test]
    fn passes_text_that_ends_with_the_suffix() {
        passes(|t| check::has_suffix(t, "report.json", ".json", "c"));
    }

    #[test]
    fn states_the_text_with_the_suffix() {
        let failure = failure_of(|t| soft::has_suffix(t, "report.json", ".csv", "c"));
        assert_eq!(failure.assertion(), Some("has-suffix"));
        assert_eq!(
            failure.detail(),
            [
                ("got", Value::from("report.json")),
                ("suffix", Value::from(".csv"))
            ]
        );
    }
}

mod matches {
    use super::*;

    #[test]
    fn passes_a_pattern_that_matches_inside_the_text() {
        passes(|t| check::matches(t, "id=42;", "[0-9]+", "c"));
    }

    #[test]
    fn states_a_null_reason_for_a_pattern_that_matches_nowhere() {
        let failure = failure_of(|t| check::matches(t, "abc", "^b", "c"));
        assert_eq!(failure.assertion(), Some("matches"));
        assert_eq!(
            failure.detail(),
            [
                ("got", Value::from("abc")),
                ("pattern", Value::from("^b")),
                ("reason", Value::Null)
            ]
        );
    }

    #[test]
    fn states_the_refusal_of_a_pattern_outside_the_subset() {
        let failure = failure_of(|t| soft::matches(t, "abc", "a(?=b)", "c"));
        let Some(Value::String(reason)) = failure.field("reason") else {
            panic!("the reason is text: {failure:?}");
        };
        assert!(reason.contains("\"a(?=b)\" at 2: "), "{reason}");
    }
}
