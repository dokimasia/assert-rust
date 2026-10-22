//! The spec of `seat`, the seats that assertions report to.

use crate::helpers::{failure_of, panic_of, records};
use dokimi_assert::{
    Cancel, Clock, Collector, Controlled, Failure, Recorder, Seat, Standard, check, fault, soft,
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
}
