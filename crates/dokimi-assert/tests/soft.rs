// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `soft`, the surface whose failures let the test continue.

use crate::helpers::{Case, failing_calls, panic_of, records};
use dokimi_assert::{Collector, Recorder, soft};

/// Returns a failing call of each assertion of the surface.
fn tests() -> Vec<Case> {
    failing_calls!(soft)
}

mod surface {
    use super::*;

    #[test]
    fn names_the_assertion_of_each_failure() {
        for tt in tests() {
            let seat = Recorder::new();
            (tt.give)(&seat);
            let failures = seat.failures();
            assert_eq!(failures.len(), 1, "{}: {failures:?}", tt.name);
            assert_eq!(
                failures[0].assertion(),
                Some(tt.want_assertion),
                "{}",
                tt.name
            );
        }
    }

    #[test]
    fn lets_the_test_continue_after_each_failure() {
        for tt in tests() {
            let seat = Recorder::new();
            (tt.give)(&seat);
            assert_eq!(
                seat.messages().len(),
                1,
                "{} does not record its failure",
                tt.name
            );
        }
    }

    #[test]
    fn writes_a_call_record_that_does_not_abort_for_each_failure() {
        for tt in tests() {
            let seat = Recorder::new();
            (tt.give)(&seat);
            let record = &records(&seat)[0];
            assert_eq!(record["assertion"], tt.want_assertion, "{}", tt.name);
            assert_eq!(record["verdict"], "fail", "{}", tt.name);
            assert_eq!(record["aborting"], false, "{}", tt.name);
        }
    }

    #[test]
    fn marks_the_frame_of_each_assertion_as_a_helper() {
        for tt in tests() {
            let seat = Recorder::new();
            (tt.give)(&seat);
            assert!(seat.helper_calls() > 0, "{}", tt.name);
        }
    }

    #[test]
    fn collects_each_failure_until_the_collector_flushes() {
        let seat = Collector::new();
        assert_eq!(
            panic_of(|| soft::is_true(&seat, false, "the flag is set")),
            None
        );
        soft::equal(&seat, 1, 2, "the count is right");
        assert_eq!(
            seat.collected(),
            ["the flag is set", "the count is right: want 2, got 1"]
        );
        let report = panic_of(|| seat.flush()).expect("the flush panics");
        assert!(
            report.starts_with("2 failures:\n  1. the flag is set\n     at "),
            "{report}"
        );
        assert!(report.contains("soft.rs:"), "{report}");
    }
}

mod equal {
    use super::*;

    #[test]
    fn states_the_line_of_the_call() {
        let seat = Recorder::new();
        let line = line!() + 1;
        soft::equal(&seat, 1 + 1, 3, "the sum is right");
        let at = seat.failures()[0]
            .location()
            .expect("a failure states its call site");
        assert_eq!(at.line(), line);
        assert!(at.file().ends_with("soft.rs"), "{at}");
    }
}
