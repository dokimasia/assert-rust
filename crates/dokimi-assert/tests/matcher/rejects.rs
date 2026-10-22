//! The spec of `matcher/rejects`: the assertion that a check fails.

use crate::helpers::records;
use dokimi_assert::{Failure, Recorder, check};

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
}
