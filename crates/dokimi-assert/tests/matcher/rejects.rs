// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `matcher/rejects`: the assertion that a check fails.

use crate::helpers::{Scratch, Skipping, records};
use dokimi_assert::{Failure, Recorder, Seat, check, fault, golden, soft};
use std::cell::Cell;
use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

mod rejects {
    use super::*;

    #[test]
    fn passes_a_check_that_fails() {
        let seat = Recorder::new();
        let failures = check::rejects(&seat, "c", |s| {
            check::is_not_empty(s, "", "the name is not empty");
        });
        assert!(!seat.failed());
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].assertion(), Some("not-empty"));
    }

    #[test]
    fn fails_a_check_that_passes() {
        let seat = Recorder::new();
        let failures = check::rejects(&seat, "the check refuses an empty name", |s| {
            check::is_not_empty(s, "x", "the name is not empty");
        });
        assert_eq!(failures, [] as [Failure; 0]);
        let failure = &seat.failures()[0];
        assert_eq!(failure.assertion(), Some("rejects"));
        assert_eq!(failure.detail(), []);
        assert_eq!(seat.message(), "the check refuses an empty name");
    }

    #[test]
    fn passes_a_check_that_fails_without_a_record() {
        let seat = Recorder::new();
        let failures = check::rejects(&seat, "c", |s| s.fail("stopped by hand"));
        assert!(!seat.failed());
        assert_eq!(failures, [] as [Failure; 0]);
    }

    #[test]
    fn records_the_calls_of_the_check_under_the_call() {
        let seat = Recorder::new();
        let _ = check::rejects(&seat, "the check refuses", |s| {
            check::is_true(s, false, "inner");
        });
        let records = records(&seat);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0]["assertion"], "rejects");
        assert_eq!(records[0]["verdict"], "pass");
        assert_eq!(records[1]["assertion"], "true");
        assert_eq!(records[1]["parent"], 1);
    }

    #[test]
    fn ends_with_the_fault_that_ends_a_call_of_the_check() {
        let seat = Recorder::new();
        let failures = check::rejects(&seat, "a check that faults", |s| {
            fault(
                s,
                io::Error::new(io::ErrorKind::Unsupported, "restart needs restartable"),
            );
        });
        assert_eq!(failures, [] as [Failure; 0]);
        let faults = seat.faults();
        let kept = faults[0]
            .downcast_ref::<io::Error>()
            .expect("the seat receives the error of the check");
        assert_eq!(kept.kind(), io::ErrorKind::Unsupported);
        let records = records(&seat);
        assert_eq!(records[0]["verdict"], "error");
        assert_eq!(records[0]["error"], "restart needs restartable");
        assert_eq!(seat.message(), "restart needs restartable");
    }

    #[test]
    fn ends_with_the_fault_of_an_assertion_of_the_check() {
        let dir = Scratch::new();
        let seat = Recorder::new();
        let _ = check::rejects(&seat, "a check of a directory as a golden file", |s| {
            golden::matches_at(s, dir.join(""), "output", false, &[]);
        });
        let records = records(&seat);
        assert_eq!(records[0]["verdict"], "error");
        assert_eq!(records[1]["verdict"], "error");
        assert_eq!(records[0]["error"], records[1]["error"]);
        assert_eq!(seat.faults().len(), 1);
    }

    #[test]
    fn ends_with_the_first_fault_of_a_check_that_runs_on_past_it() {
        let seat = Recorder::new();
        let _ = check::rejects(&seat, "a check with two faults", |s| {
            let _ = catch_unwind(AssertUnwindSafe(|| {
                fault(s, io::Error::other("the first fault"));
            }));
            fault(s, io::Error::other("the second fault"));
        });
        let texts: Vec<String> = seat.faults().iter().map(ToString::to_string).collect();
        assert_eq!(texts, ["the first fault"]);
    }

    #[test]
    fn passes_a_noted_fault_of_the_check_to_its_seat() {
        let seat = Recorder::new();
        let failures = check::rejects(&seat, "a check with a note", |s: &dyn Seat| {
            s.report_fault(Arc::new(io::Error::other("the store skips a file")), false);
            check::is_true(s, false, "it fails after the note");
        });
        assert_eq!(failures.len(), 1);
        let texts: Vec<String> = seat.faults().iter().map(ToString::to_string).collect();
        assert_eq!(texts, ["the store skips a file"]);
        assert!(!seat.failed());
    }

    #[test]
    fn ends_as_skipped_for_a_check_that_skips_before_it_fails() {
        let seat = Recorder::new();
        let reached = Cell::new(false);
        let failures = check::rejects(&seat, "a check that skips", |s| {
            s.skip("the check needs a disk");
            reached.set(true);
        });
        assert!(!reached.get());
        assert_eq!(failures, [] as [Failure; 0]);
        assert!(!seat.failed(), "{}", seat.message());
        let records = records(&seat);
        assert_eq!(records[0]["verdict"], "skip");
        assert_eq!(records[0]["reason"], "the check needs a disk");
        assert_eq!(records[0]["detail"], serde_json::Value::Null);
    }

    #[test]
    fn skips_its_seat_with_the_reason_of_the_check() {
        let seat = Skipping::default();
        let _ = check::rejects(&seat, "a check that skips", |s| {
            s.skip("the check needs a disk");
        });
        assert_eq!(seat.skips(), ["the check needs a disk"]);
    }

    #[test]
    fn keeps_the_reason_of_the_first_skip() {
        let seat = Recorder::new();
        let _ = check::rejects(&seat, "a check with two skips", |s| {
            let _ = catch_unwind(AssertUnwindSafe(|| s.skip("the first reason")));
            s.skip("the second reason");
        });
        assert_eq!(records(&seat)[0]["reason"], "the first reason");
    }

    #[test]
    fn passes_a_check_that_fails_and_then_skips() {
        let seat = Recorder::new();
        let failures = check::rejects(&seat, "a check that skips late", |s| {
            soft::is_true(s, false, "it fails first");
            s.skip("the check needs a disk");
        });
        assert_eq!(failures.len(), 1);
        assert_eq!(records(&seat)[0]["verdict"], "pass");
    }

    #[test]
    fn passes_a_noted_fault_of_a_check_that_then_skips_to_its_seat() {
        let seat = Recorder::new();
        let failures = check::rejects(&seat, "a check with a note", |s: &dyn Seat| {
            s.report_fault(Arc::new(io::Error::other("the store skips a file")), false);
            s.skip("the check needs a disk");
        });
        assert_eq!(failures, [] as [Failure; 0]);
        let texts: Vec<String> = seat.faults().iter().map(ToString::to_string).collect();
        assert_eq!(texts, ["the store skips a file"]);
        assert_eq!(records(&seat)[0]["verdict"], "skip");
    }
}
