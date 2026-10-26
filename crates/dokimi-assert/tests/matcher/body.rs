// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `matcher/body`: the seat that `eventually` and `rejects` hand
//! the body that they run.

use crate::helpers::panic_of;
use dokimi_assert::{Cancel, Clock, Failure, Recorder, Seat, Stop, check, fault, soft};
use std::cell::{Cell, RefCell};
use std::io;
use std::sync::Arc;
use std::time::Duration;

mod body {
    use super::*;

    #[test]
    fn hands_the_body_a_handle_that_derives_from_the_handle_of_the_seat() {
        let parent = Cancel::new();
        let seat = Recorder::new().with_cancellation(parent.clone());
        parent.expire();
        let reason = Cell::new(None);
        let _ = check::rejects(&seat, "c", |body| {
            reason.set(body.cancellation().reason());
            body.fail("stop");
        });
        assert_eq!(reason.get(), Some(Stop::DeadlineExceeded));
    }

    #[test]
    fn cancels_the_handle_of_the_body_when_the_body_ends() {
        let seat = Recorder::new();
        let handle = RefCell::new(None);
        let _ = check::rejects(&seat, "c", |body| {
            *handle.borrow_mut() = Some(body.cancellation().clone());
            body.fail("stop");
        });
        let handle = handle.into_inner().expect("the body hands over its handle");
        assert_eq!(handle.reason(), Some(Stop::Cancelled));
        assert_eq!(seat.cancellation().reason(), None);
    }

    #[test]
    fn ends_the_body_at_a_failure_that_ends_a_test() {
        let reached = Cell::new(false);
        let failures = check::rejects(&Recorder::new(), "c", |body| {
            check::is_true(body, false, "the first check");
            reached.set(true);
        });
        assert!(!reached.get());
        assert_eq!(failures.len(), 1);
    }

    #[test]
    fn continues_the_body_past_a_failure_that_lets_a_test_continue() {
        let reached = Cell::new(false);
        let failures = check::rejects(&Recorder::new(), "c", |body| {
            soft::is_true(body, false, "the first check");
            soft::equal(body, 1, 2, "the second check");
            reached.set(true);
        });
        assert!(reached.get());
        let assertions: Vec<_> = failures.iter().map(Failure::assertion).collect();
        assert_eq!(assertions, [Some("true"), Some("equal")]);
    }

    #[test]
    fn reads_the_clock_of_the_seat_of_its_assertion() {
        let clock = Arc::new(dokimi_assert::Controlled::new());
        clock.advance(Duration::from_secs(5));
        let seat = Recorder::new().with_clock(clock);
        let now = Cell::new(Duration::ZERO);
        let _ = check::rejects(&seat, "c", |body| {
            now.set(body.clock().now());
            body.fail("stop");
        });
        assert_eq!(now.get(), Duration::from_secs(5));
    }

    #[test]
    fn states_its_failures_in_its_debug_text() {
        let seat = Recorder::new();
        let body = dokimi_assert::__private::Body::new(&seat, None);
        body.record("the first check");
        let text = format!("{body:?}");
        assert!(text.starts_with("Body { cancellation: "), "{text}");
        assert!(
            text.ends_with(r#"state: State { failed: true, first: Some("the first check"), failures: [], fault: None, skip: None }, .. }"#),
            "{text}"
        );
    }

    #[test]
    fn ends_the_body_with_the_text_of_a_fault_that_ends_its_call() {
        let seat = Recorder::new();
        let body = dokimi_assert::__private::Body::new(&seat, None);
        let reached = Cell::new(false);
        body.run(|s| {
            fault(s, io::Error::other("the input is refused"));
            reached.set(true);
        });
        assert!(!reached.get());
        assert_eq!(body.first_message(), "the input is refused");
        assert_eq!(
            body.fault().map(|kept| kept.to_string()).as_deref(),
            Some("the input is refused")
        );
        assert!(!seat.failed());
    }

    #[test]
    fn passes_a_noted_fault_to_the_seat_of_its_assertion() {
        let seat = Recorder::new();
        let body = dokimi_assert::__private::Body::new(&seat, None);
        body.report_fault(Arc::new(io::Error::other("the store skips a file")), false);
        let texts: Vec<String> = seat.faults().iter().map(ToString::to_string).collect();
        assert_eq!(texts, ["the store skips a file"]);
        assert!(!body.failed());
        assert!(body.fault().is_none());
    }

    #[test]
    fn panics_again_with_another_panic_of_the_body() {
        let seat = Recorder::new();
        let raised = panic_of(|| drop(check::rejects(&seat, "c", |_| panic!("the body broke"))));
        assert_eq!(raised.as_deref(), Some("the body broke"));
    }

    #[test]
    fn runs_an_attempt_on_past_its_skip() {
        let seat = Recorder::new().with_clock(Arc::new(dokimi_assert::Controlled::new()));
        let reached = Cell::new(false);
        check::eventually(
            &seat,
            Duration::ZERO,
            Duration::ZERO,
            |body: &dyn Seat| {
                body.skip("the attempt needs a disk");
                reached.set(true);
            },
            "the attempt runs on",
        );
        assert!(reached.get());
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn states_the_first_failure_of_the_last_attempt() {
        let seat = Recorder::new().with_clock(Arc::new(dokimi_assert::Controlled::new()));
        check::eventually(
            &seat,
            Duration::ZERO,
            Duration::ZERO,
            |body: &dyn Seat| {
                soft::is_true(body, false, "the first check");
                soft::is_true(body, false, "the second check");
            },
            "c",
        );
        assert_eq!(
            seat.failures()[0].field("last"),
            Some(&dokimi_assert::Value::from("the first check"))
        );
    }
}
