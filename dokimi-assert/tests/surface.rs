//! The two surfaces, the three seats, and how they differ.
//!
//! The conformance run drives both against the corpus and asks only whether each case
//! passed or failed. What it cannot see is which path the failure took, which is the one
//! thing the two surfaces disagree about.

use dokimi_assert::seat::{Collector, Recorder, Seat, Standard};
use dokimi_assert::{check, soft};

#[test]
fn check_reports_through_the_path_that_stops_the_test() {
    let seat = Recorder::new();
    check::equal(&seat, &1, &2, "the count is right");

    assert!(seat.failed(), "the failure has to reach the seat");
    assert!(
        seat.messages().is_empty(),
        "check does not record, it fails"
    );
}

#[test]
fn soft_reports_through_the_path_the_test_carries_on_past() {
    let seat = Recorder::new();
    soft::equal(&seat, &1, &2, "the count is right");

    assert!(seat.failed(), "the failure has to reach the seat");
    assert_eq!(seat.messages().len(), 1, "soft records rather than failing");
}

#[test]
fn soft_carries_on_so_one_run_reports_every_failure() {
    let seat = Recorder::new();
    soft::equal(&seat, &1, &2, "the count is right");
    soft::is_true(&seat, false, "the flag is set");
    soft::has_prefix(&seat, "GET", "POST", "the method is right");

    assert_eq!(
        seat.messages().len(),
        3,
        "three failing assertions, three reports"
    );
}

#[test]
fn a_passing_assertion_marks_its_frame_and_reports_nothing() {
    let seat = Recorder::new();
    check::equal(&seat, &1, &1, "the count is right");

    assert!(!seat.failed(), "{}", seat.message());
    assert!(
        seat.helper_calls() > 0,
        "the frame is still marked as the library's"
    );
}

#[test]
fn the_message_leads_the_failure() {
    let seat = Recorder::new();
    check::equal(&seat, &1, &2, "the count is right");

    assert!(
        seat.message().starts_with("the count is right"),
        "{}",
        seat.message()
    );
}

#[test]
fn the_standard_seat_panics_through_both_paths() {
    let aborting = std::panic::catch_unwind(|| {
        check::equal(&Standard::new(), &1, &2, "the count is right");
    });
    assert!(aborting.is_err(), "check stops the test");

    let recording = std::panic::catch_unwind(|| {
        soft::equal(&Standard::new(), &1, &2, "the count is right");
    });
    assert!(
        recording.is_err(),
        "a recorded failure needs somewhere to report, and a bare seat has no end"
    );
}

#[test]
fn the_collector_panics_on_a_check_and_holds_what_soft_recorded() {
    let seat = Collector::new();
    soft::equal(&seat, &1, &2, "the count is right");
    soft::is_true(&seat, false, "the flag is set");

    assert_eq!(
        seat.collected().len(),
        2,
        "soft collects rather than panicking"
    );

    let aborting = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        check::equal(&seat, &1, &2, "the count is right");
    }));
    assert!(aborting.is_err(), "check still stops the test");

    // The two soft failures are still held, and the drop would report
    // them and fail this test. Taking them is what a real test's end
    // does, and doing it here is what leaves the drop with nothing.
    let held = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| seat.flush()));
    assert!(
        held.is_err(),
        "what soft recorded outlives the check that stopped the test"
    );
}

#[test]
fn a_collector_reports_everything_it_holds_when_it_is_flushed() {
    let seat = Collector::new();
    soft::equal(&seat, &1, &2, "the count is right");
    soft::is_true(&seat, false, "the flag is set");

    let flushed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| seat.flush()));
    let raised = flushed.expect_err("a collector holding failures reports them");
    let text = raised
        .downcast_ref::<String>()
        .expect("the report is a string")
        .clone();

    assert!(text.contains("2 failures"), "{text}");
    assert!(text.contains("the count is right"), "{text}");
    assert!(text.contains("the flag is set"), "{text}");
    assert!(
        text.contains("surface.rs"),
        "each failure says where it was written: {text}"
    );
}

#[test]
fn a_collector_holding_nothing_flushes_quietly() {
    let seat = Collector::new();
    soft::equal(&seat, &1, &1, "the count is right");
    seat.flush();
}

#[test]
fn flushing_twice_reports_once() {
    let seat = Collector::new();
    soft::equal(&seat, &1, &2, "the count is right");

    let first = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| seat.flush()));
    assert!(first.is_err(), "the first flush reports what was held");

    // Dropping the collector must not report the same failure again, which
    // is what would abort the process if the test were already unwinding.
    seat.flush();
}

#[test]
fn a_recorder_keeps_the_first_fatal_and_every_record() {
    let seat = Recorder::new();
    Seat::fail(&seat, "first");
    Seat::fail(&seat, "second");
    Seat::record(&seat, "recorded");

    assert_eq!(
        seat.message(),
        "first",
        "the aborting path keeps the first failure"
    );
    assert_eq!(
        seat.messages(),
        vec!["recorded"],
        "and the recording path keeps its own"
    );
}
