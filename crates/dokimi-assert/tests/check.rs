// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `check`, the surface whose failures end the test.

use crate::helpers::{Case, failing_calls, records};
use dokimi_assert::{Recorder, Stop, Value, check};
use std::path::Path;

/// Returns a failing call of each assertion of the surface.
fn tests() -> Vec<Case> {
    let mut tests = failing_calls!(check);
    tests.push(Case {
        name: "rejects",
        give: |t| drop(check::rejects(t, "c", |_| {})),
        want_assertion: "rejects",
    });
    tests
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
    fn ends_the_test_at_each_failure() {
        for tt in tests() {
            let seat = Recorder::new();
            (tt.give)(&seat);
            assert!(!seat.message().is_empty(), "{} reports no failure", tt.name);
            assert_eq!(
                seat.messages(),
                [] as [String; 0],
                "{} lets the test continue",
                tt.name
            );
        }
    }

    #[test]
    fn states_a_call_site_in_the_file_of_the_caller() {
        for tt in tests() {
            let seat = Recorder::new();
            (tt.give)(&seat);
            let at = seat.failures()[0]
                .location()
                .expect("a failure states its call site");
            let in_tests = Path::new(at.file())
                .components()
                .any(|part| part.as_os_str() == "tests");
            assert!(in_tests, "{}: {at}", tt.name);
        }
    }

    #[test]
    fn writes_an_aborting_call_record_for_each_failure() {
        for tt in tests() {
            let seat = Recorder::new();
            (tt.give)(&seat);
            let record = &records(&seat)[0];
            assert_eq!(record["assertion"], tt.want_assertion, "{}", tt.name);
            assert_eq!(record["verdict"], "fail", "{}", tt.name);
            assert_eq!(record["aborting"], true, "{}", tt.name);
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
}

mod equal {
    use super::*;

    #[test]
    fn compares_a_string_with_a_str() {
        let seat = Recorder::new();
        check::equal(&seat, String::from("widget"), "widget", "the name is right");
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn states_the_line_of_the_call() {
        let seat = Recorder::new();
        let line = line!() + 1;
        check::equal(&seat, 1 + 1, 3, "the sum is right");
        let at = seat.failures()[0]
            .location()
            .expect("a failure states its call site");
        assert_eq!(at.line(), line);
        assert!(at.file().ends_with("check.rs"), "{at}");
    }
}

mod panics {
    use super::*;

    #[test]
    fn returns_the_message_of_the_panic() {
        let seat = Recorder::new();
        let raised = check::panics(&seat, || panic!("the index is out of bounds"), "c");
        assert_eq!(raised.as_deref(), Some("the index is out of bounds"));
        assert!(!seat.failed());
    }
}

mod error_as {
    use super::*;

    #[test]
    fn returns_the_error_of_the_type() {
        let seat = Recorder::new();
        let found = check::error_as::<Stop>(&seat, &Stop::DeadlineExceeded, "c");
        assert_eq!(found, Some(&Stop::DeadlineExceeded));
        assert!(!seat.failed());
    }
}

mod rejects {
    use super::*;

    #[test]
    fn returns_the_failures_of_a_check_that_fails() {
        let seat = Recorder::new();
        let failures = check::rejects(&seat, "the check refuses an empty name", |s| {
            check::is_not_empty(s, "", "the name is not empty");
        });
        assert!(!seat.failed(), "{}", seat.message());
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].assertion(), Some("not-empty"));
        assert_eq!(failures[0].got(), Some(&Value::Opaque("\"\"".to_owned())));
    }
}
