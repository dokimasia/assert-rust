// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `seat`, the seats that assertions report to.

use crate::helpers::{Scratch, Trace, failure_of, in_child, panic_of, records};
use dokimi_assert::{
    Call, Cancel, Clock, Collector, Controlled, Failure, Recorder, Seat, Standard, check, fault,
    prop, soft,
};
use std::error::Error;
use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Returns a fault of `text`, as the error that a seat receives.
fn refusal(text: &str) -> Arc<dyn Error + Send + Sync> {
    Arc::new(io::Error::other(text.to_owned()))
}

/// A seat that implements the two required members alone.
#[derive(Default)]
struct Plain(Mutex<Vec<String>>);

impl Plain {
    fn received(&self) -> Vec<String> {
        self.0.lock().expect("the lock is free").clone()
    }
}

impl Seat for Plain {
    fn fail(&self, message: &str) {
        self.0
            .lock()
            .expect("the lock is free")
            .push(format!("fail {message}"));
    }

    fn record(&self, message: &str) {
        self.0
            .lock()
            .expect("the lock is free")
            .push(format!("record {message}"));
    }
}

/// A seat that observes each call, and keeps what it receives in order.
#[derive(Default)]
struct Watching(Mutex<Vec<String>>);

impl Watching {
    fn received(&self) -> Vec<String> {
        self.0.lock().expect("the lock is free").clone()
    }

    fn keep(&self, line: String) {
        self.0.lock().expect("the lock is free").push(line);
    }
}

impl Seat for Watching {
    fn fail(&self, message: &str) {
        self.keep(format!("fail {message}"));
    }

    fn record(&self, message: &str) {
        self.keep(format!("record {message}"));
    }

    fn skip(&self, reason: &str) {
        self.keep(format!("skip {reason}"));
    }

    fn observe_call(&self, call: &Call<'_>) {
        self.keep(format!("observe {} {}", call.assertion, call.contract));
    }
}

/// Returns the failure record of a failing call of `length`.
fn failure() -> Failure {
    failure_of(|t| check::length(t, "ab", 3, "the code has three characters"))
}

mod seat {
    use super::*;

    #[test]
    fn renders_a_failure_of_the_aborting_surface_through_fail() {
        let plain = Plain::default();
        check::length(&plain, "ab", 3, "the code has three characters");
        assert_eq!(
            plain.received(),
            ["fail the code has three characters: want 3, got 2"]
        );
    }

    #[test]
    fn renders_a_failure_of_the_recording_surface_through_record() {
        let plain = Plain::default();
        soft::length(&plain, "ab", 3, "the code has three characters");
        assert_eq!(
            plain.received(),
            ["record the code has three characters: want 3, got 2"]
        );
    }

    #[test]
    fn supplies_a_handle_that_never_stops_by_default() {
        assert_eq!(Plain::default().cancellation().reason(), None);
    }

    #[test]
    fn supplies_the_platform_clock_by_default() {
        let plain = Plain::default();
        let first = plain.clock().now();
        assert!(plain.clock().now() >= first);
    }

    #[test]
    fn does_nothing_on_helper_by_default() {
        let plain = Plain::default();
        plain.helper();
        assert_eq!(plain.received(), [] as [String; 0]);
    }

    #[test]
    fn sends_the_text_of_an_ending_fault_through_fail_by_default() {
        let plain = Plain::default();
        plain.report_fault(refusal("the pattern does not compile"), true);
        assert_eq!(plain.received(), ["fail the pattern does not compile"]);
    }

    #[test]
    fn does_not_fail_for_a_noted_fault_by_default() {
        let plain = Plain::default();
        plain.report_fault(refusal("the store skips a file"), false);
        assert_eq!(plain.received(), [] as [String; 0]);
    }

    #[test]
    fn returns_from_a_skip_without_a_failure_by_default() {
        let plain = Plain::default();
        plain.skip("the disk is full");
        assert_eq!(plain.received(), [] as [String; 0]);
    }

    #[test]
    fn ends_the_call_with_a_fault_at_a_cleanup_by_default() {
        let plain = Plain::default();
        let trace = Trace::default();
        plain.cleanup(trace.cleanup("cleaned"));
        assert_eq!(
            plain.received(),
            ["fail Seat::cleanup: the seat does not run cleanups"]
        );
        assert_eq!(trace.ran(), [] as [&str; 0]);
    }

    #[test]
    fn observes_nothing_by_default() {
        let plain = Plain::default();
        plain.observe_call(&Call {
            assertion: "true",
            contract: "the flag is set",
        });
        assert_eq!(plain.received(), [] as [String; 0]);
    }

    #[test]
    fn observes_a_passing_call() {
        let seat = Watching::default();
        check::is_true(&seat, true, "the flag is set");
        assert_eq!(seat.received(), ["observe true the flag is set"]);
    }

    #[test]
    fn observes_a_failing_call_before_its_failure() {
        let seat = Watching::default();
        soft::is_true(&seat, false, "the flag is set");
        assert_eq!(
            seat.received(),
            ["observe true the flag is set", "record the flag is set"]
        );
    }

    #[test]
    fn observes_a_call_that_ends_with_a_fault_before_the_fault() {
        let seat = Watching::default();
        check::matches(&seat, "abc", "(", "the code is a word");
        let received = seat.received();
        assert_eq!(received[0], "observe matches the code is a word");
        assert!(received[1].starts_with("fail "), "{received:?}");
        assert_eq!(received.len(), 2);
    }

    #[test]
    fn does_not_observe_the_call_of_fault() {
        let seat = Watching::default();
        fault(&seat, io::Error::other("no such service"));
        assert_eq!(seat.received(), ["fail no such service"]);
    }

    #[test]
    fn observes_the_calls_of_a_body_before_the_call_that_ran_it() {
        let seat = Watching::default();
        check::rejects(&seat, "the check refuses false", |t| {
            check::is_true(t, false, "the flag is set");
        });
        assert_eq!(
            seat.received(),
            [
                "observe true the flag is set",
                "observe rejects the check refuses false"
            ]
        );
    }

    #[test]
    fn observes_the_calls_of_the_cases_of_a_property_before_the_property() {
        let seat = Watching::default();
        prop::for_all(&seat, "every x is small", prop::cases(5).workers(4), |c| {
            let x = c.draw(&prop::integer(0..=9_u8), "x");
            check::is_true(c, x < 10, "x is small");
        });
        let received = seat.received();
        let (last, cases) = received.split_last().expect("the seat observed calls");
        assert_eq!(last, "observe prop-for-all every x is small");
        assert_ne!(cases, [] as [String; 0]);
        assert!(
            cases.iter().all(|line| line == "observe true x is small"),
            "{received:?}"
        );
    }

    #[test]
    fn observes_a_skip_before_the_skip() {
        let seat = Watching::default();
        prop::for_all(&seat, "every disk has room", prop::cases(1), |c| {
            c.skip("the disk is full");
        });
        assert_eq!(
            seat.received(),
            [
                "observe prop-for-all every disk has room",
                "skip the disk is full"
            ]
        );
    }
}

mod fault {
    use super::*;

    #[test]
    fn hands_the_error_to_a_seat_that_keeps_it() {
        let seat = Recorder::new();
        fault(
            &seat,
            io::Error::new(io::ErrorKind::NotFound, "no such service"),
        );
        let faults = seat.faults();
        let kept = faults[0]
            .downcast_ref::<io::Error>()
            .expect("the recorder keeps the error itself");
        assert_eq!(kept.kind(), io::ErrorKind::NotFound);
        assert_eq!(seat.message(), "no such service");
        assert_eq!(seat.records(), [] as [String; 0]);
    }

    #[test]
    fn panics_with_the_text_on_a_standard_seat() {
        assert_eq!(
            panic_of(|| fault(&Standard, io::Error::other("no such service"))).as_deref(),
            Some("no such service")
        );
    }

    #[test]
    fn marks_the_frame_of_its_caller_as_a_helper() {
        let seat = Recorder::new();
        fault(&seat, io::Error::other("no such service"));
        assert_eq!(seat.helper_calls(), 1);
    }
}

mod standard {
    use super::*;

    #[test]
    fn panics_on_fail() {
        assert_eq!(
            panic_of(|| Standard::new().fail("stop")).as_deref(),
            Some("stop")
        );
    }

    #[test]
    fn panics_on_record() {
        assert_eq!(
            panic_of(|| Standard.record("carry on")).as_deref(),
            Some("carry on")
        );
    }

    #[test]
    fn panics_at_a_failure_of_either_surface() {
        assert!(panic_of(|| check::is_true(&Standard, false, "the flag is set")).is_some());
        assert!(panic_of(|| soft::is_true(&Standard, false, "the flag is set")).is_some());
    }

    #[test]
    fn names_the_test_that_its_thread_runs() {
        assert_eq!(
            Standard.test().as_deref(),
            Some("seat::standard::names_the_test_that_its_thread_runs")
        );
    }
}

mod recorder {
    use super::*;

    #[test]
    fn keeps_the_first_failure_that_ends_the_test() {
        let seat = Recorder::new();
        seat.fail("first");
        seat.fail("second");
        assert_eq!(seat.message(), "first");
        assert!(seat.failed());
    }

    #[test]
    fn keeps_every_failure_after_which_the_test_continues() {
        let seat = Recorder::default();
        seat.record("one");
        seat.record("two");
        assert_eq!(seat.messages(), ["one", "two"]);
        assert_eq!(seat.message(), "one");
    }

    #[test]
    fn returns_the_empty_message_when_nothing_failed() {
        let seat = Recorder::new();
        assert!(!seat.failed());
        assert_eq!(seat.message(), "");
    }

    #[test]
    fn keeps_each_record_that_it_receives() {
        let seat = Recorder::new();
        seat.report(&failure(), false);
        assert_eq!(seat.failures(), [failure()]);
        assert_eq!(
            seat.messages(),
            ["the code has three characters: want 3, got 2"]
        );
    }

    #[test]
    fn keeps_no_record_for_a_message() {
        let seat = Recorder::new();
        seat.fail("a message without a record");
        assert_eq!(seat.failures(), [] as [Failure; 0]);
    }

    #[test]
    fn keeps_each_fault_in_the_order_of_the_calls() {
        let seat = Recorder::new();
        seat.report_fault(refusal("the store skips a file"), false);
        seat.report_fault(refusal("the seed is no number"), true);
        let texts: Vec<String> = seat.faults().iter().map(ToString::to_string).collect();
        assert_eq!(texts, ["the store skips a file", "the seed is no number"]);
    }

    #[test]
    fn fails_with_the_text_of_a_fault_that_ends_its_call() {
        let seat = Recorder::new();
        seat.report_fault(refusal("the seed is no number"), true);
        assert_eq!(seat.message(), "the seed is no number");
        assert_eq!(seat.messages(), [] as [String; 0]);
    }

    #[test]
    fn does_not_fail_for_a_noted_fault() {
        let seat = Recorder::new();
        seat.report_fault(refusal("the store skips a file"), false);
        assert!(!seat.failed());
    }

    #[test]
    fn counts_the_calls_of_helper() {
        let seat = Recorder::new();
        seat.helper();
        seat.helper();
        assert_eq!(seat.helper_calls(), 2);
    }

    #[test]
    fn reads_the_clock_that_it_was_given() {
        let clock = Arc::new(Controlled::new());
        clock.advance(Duration::from_secs(5));
        let seat = Recorder::new().with_clock(clock);
        assert_eq!(seat.clock().now(), Duration::from_secs(5));
    }

    #[test]
    fn returns_the_handle_that_it_was_given() {
        let seat = Recorder::new().with_cancellation(Cancel::cancelled());
        assert!(seat.cancellation().is_stopped());
        assert!(!Recorder::new().cancellation().is_stopped());
    }

    #[test]
    fn keeps_the_call_record_of_each_call() {
        let seat = Recorder::new();
        assert_eq!(seat.records(), [] as [String; 0]);
        check::is_true(&seat, true, "c");
        assert_eq!(records(&seat).len(), 1);
    }

    #[test]
    fn keeps_a_cleanup_until_run_cleanups() {
        let seat = Recorder::new();
        let trace = Trace::default();
        seat.cleanup(trace.cleanup("cleaned"));
        assert_eq!(trace.ran(), [] as [&str; 0]);
        seat.run_cleanups();
        assert_eq!(trace.ran(), ["cleaned"]);
    }

    #[test]
    fn runs_the_last_registered_cleanup_first() {
        let seat = Recorder::new();
        let trace = Trace::default();
        seat.cleanup(trace.cleanup("first"));
        seat.cleanup(trace.cleanup("second"));
        seat.run_cleanups();
        assert_eq!(trace.ran(), ["second", "first"]);
    }

    #[test]
    fn runs_a_cleanup_that_a_cleanup_registers_in_the_same_pass() {
        let seat = Arc::new(Recorder::new());
        let trace = Trace::default();
        let registering = Arc::clone(&seat);
        let registered = trace.cleanup("registered");
        seat.cleanup(Box::new(move || registering.cleanup(registered)));
        seat.run_cleanups();
        assert_eq!(trace.ran(), ["registered"]);
    }

    #[test]
    fn leaves_a_cleanup_that_arrives_after_run_cleanups_to_the_next_call() {
        let seat = Recorder::new();
        let trace = Trace::default();
        seat.cleanup(trace.cleanup("first run"));
        seat.run_cleanups();
        seat.cleanup(trace.cleanup("second run"));
        assert_eq!(trace.ran(), ["first run"]);
        seat.run_cleanups();
        assert_eq!(trace.ran(), ["first run", "second run"]);
    }

    #[test]
    fn keeps_the_panic_of_a_cleanup_as_a_failure() {
        let seat = Recorder::new();
        let trace = Trace::default();
        seat.cleanup(trace.cleanup("later"));
        seat.cleanup(Box::new(|| panic!("the port is busy")));
        seat.run_cleanups();
        assert_eq!(trace.ran(), ["later"]);
        assert_eq!(seat.messages(), ["a cleanup failed: the port is busy"]);
    }

    #[test]
    fn runs_the_remaining_cleanups_when_it_drops() {
        let seat = Recorder::new();
        let trace = Trace::default();
        seat.cleanup(trace.cleanup("cleaned"));
        drop(seat);
        assert_eq!(trace.ran(), ["cleaned"]);
    }

    #[test]
    fn states_its_cleanups_in_its_debug_text() {
        let seat = Recorder::new();
        seat.cleanup(Box::new(|| {}));
        let text = format!("{seat:?}");
        assert!(text.contains("cleanups: Cleanups(1)"), "{text}");
    }
}

mod collector {
    use super::*;

    #[test]
    fn panics_on_fail() {
        let seat = Collector::new();
        assert_eq!(panic_of(|| seat.fail("stop")).as_deref(), Some("stop"));
    }

    #[test]
    fn collects_each_failure_after_which_the_test_continues() {
        let seat = Collector::default();
        seat.record("one");
        seat.record("two");
        assert_eq!(seat.collected(), ["one", "two"]);
        let _ = panic_of(|| seat.flush());
    }

    #[test]
    fn flushes_one_failure_with_its_location() {
        let seat = Collector::new();
        seat.record("the flag is set");
        let report = panic_of(|| seat.flush()).expect("the flush panics");
        assert!(report.starts_with("the flag is set\n  at "), "{report}");
        assert!(report.contains("seat.rs:"), "{report}");
    }

    #[test]
    fn flushes_several_failures_as_a_numbered_list() {
        let seat = Collector::new();
        seat.record("first");
        seat.record("second");
        let report = panic_of(|| seat.flush()).expect("the flush panics");
        assert!(
            report.starts_with("2 failures:\n  1. first\n     at "),
            "{report}"
        );
        assert!(report.contains("\n  2. second\n     at "), "{report}");
    }

    #[test]
    fn flushes_nothing_twice() {
        let seat = Collector::new();
        seat.record("once");
        assert!(panic_of(|| seat.flush()).is_some());
        assert_eq!(panic_of(|| seat.flush()), None);
    }

    #[test]
    fn panics_with_the_collected_failures_when_it_drops() {
        let report = panic_of(|| {
            let seat = Collector::new();
            seat.record("collected");
        });
        assert!(report.is_some_and(|report| report.starts_with("collected")));
    }

    #[test]
    fn drops_quietly_while_its_thread_unwinds() {
        let report = panic_of(|| {
            let seat = Collector::new();
            seat.record("collected");
            panic!("the test failed first");
        });
        assert_eq!(report.as_deref(), Some("the test failed first"));
    }

    #[test]
    fn runs_its_cleanups_when_it_drops() {
        let trace = Trace::default();
        let seat = Collector::new();
        seat.cleanup(trace.cleanup("first"));
        seat.cleanup(trace.cleanup("second"));
        drop(seat);
        assert_eq!(trace.ran(), ["second", "first"]);
    }

    #[test]
    fn runs_its_cleanups_before_it_panics_with_the_collected_failures() {
        let trace = Trace::default();
        let report = panic_of(|| {
            let seat = Collector::new();
            seat.cleanup(trace.cleanup("cleaned"));
            seat.record("collected");
        });
        assert_eq!(trace.ran(), ["cleaned"]);
        assert!(report.is_some_and(|report| report.starts_with("collected")));
    }

    #[test]
    fn panics_with_the_panic_of_a_cleanup_at_its_line_when_it_drops() {
        let trace = Trace::default();
        let line = line!() + 4;
        let report = panic_of(|| {
            let seat = Collector::new();
            seat.cleanup(trace.cleanup("later"));
            seat.cleanup(Box::new(|| panic!("the port is busy")));
        });
        assert_eq!(trace.ran(), ["later"]);
        let report = report.expect("the drop panics");
        assert!(
            report.starts_with("a cleanup failed: the port is busy\n  at "),
            "{report}"
        );
        assert!(report.ends_with(&format!("seat.rs:{line}")), "{report}");
    }

    #[test]
    fn runs_its_cleanups_while_its_thread_unwinds() {
        let trace = Trace::default();
        let report = panic_of(|| {
            let seat = Collector::new();
            seat.cleanup(trace.cleanup("later"));
            seat.cleanup(Box::new(|| panic!("the port is busy")));
            panic!("the test failed first");
        });
        assert_eq!(report.as_deref(), Some("the test failed first"));
        assert_eq!(trace.ran(), ["later"]);
    }

    #[test]
    fn writes_the_panic_of_a_cleanup_to_the_standard_error_while_its_thread_unwinds() {
        let dir = Scratch::new();
        let output = in_child(
            "seat::child::unwinds_past_a_cleanup_that_panics",
            &dir.join(""),
            &[],
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("a cleanup failed: the port is busy\n"),
            "{stderr}"
        );
        assert!(!output.status.success(), "{stderr}");
    }
}

mod child {
    use super::*;

    #[test]
    #[ignore = "a spec of the collector runs it in a child process"]
    fn unwinds_past_a_cleanup_that_panics() {
        let seat = Collector::new();
        seat.cleanup(Box::new(|| panic!("the port is busy")));
        panic!("the test failed first");
    }
}
