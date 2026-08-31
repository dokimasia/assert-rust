//! Assertions that retry.

use dokimi_assert::check;
use dokimi_assert::seat::{Recorder, Seat};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

const BRIEF: Duration = Duration::from_millis(300);
const TICK: Duration = Duration::from_millis(1);

#[test]
fn eventually_passes_once_the_body_stops_failing() {
    let attempts = AtomicUsize::new(0);
    let seat = Recorder::new();

    check::eventually(
        &seat,
        BRIEF,
        TICK,
        |trial| {
            if attempts.fetch_add(1, Ordering::SeqCst) < 2 {
                trial.fail("not ready yet");
            }
        },
        "the queue drains",
    );

    assert!(!seat.failed(), "{}", seat.message());
    assert!(
        attempts.load(Ordering::SeqCst) >= 3,
        "the body has to be retried, not called once"
    );
}

#[test]
fn eventually_carries_the_last_attempts_own_reason() {
    let seat = Recorder::new();
    check::eventually(
        &seat,
        Duration::from_millis(20),
        TICK,
        |trial| trial.fail("queue still holds 4"),
        "the queue drains",
    );

    assert!(seat.failed(), "a body that never passes must be reported");
    assert!(
        seat.failures()[0]
            .detail("last")
            .map(ToString::to_string)
            .as_deref()
            == Some("queue still holds 4"),
        "{}",
        seat.message()
    );
    assert!(named(&seat, "eventually"), "{}", seat.message());
}

#[test]
fn eventually_runs_the_body_once_however_short_the_timeout() {
    let attempts = AtomicUsize::new(0);
    let seat = Recorder::new();

    check::eventually(
        &seat,
        Duration::ZERO,
        TICK,
        |_trial| {
            attempts.fetch_add(1, Ordering::SeqCst);
        },
        "the queue drains",
    );

    assert!(!seat.failed(), "{}", seat.message());
    assert_eq!(
        attempts.load(Ordering::SeqCst),
        1,
        "a zero timeout still gets one attempt"
    );
}

#[test]
fn a_body_that_records_counts_as_a_failed_attempt() {
    let seat = Recorder::new();
    check::eventually(
        &seat,
        Duration::ZERO,
        TICK,
        |trial| trial.record("queue still holds 4"),
        "the queue drains",
    );

    assert!(
        seat.failed(),
        "a soft assertion inside the body is still the attempt failing"
    );
}

#[test]
fn eventually_true_passes_a_predicate_that_flips_and_reports_one_that_never_does() {
    let calls = AtomicUsize::new(0);

    let passing = Recorder::new();
    check::eventually_true(
        &passing,
        BRIEF,
        || calls.fetch_add(1, Ordering::SeqCst) >= 2,
        "the file appears",
    );
    assert!(!passing.failed(), "{}", passing.message());
    assert!(
        calls.load(Ordering::SeqCst) >= 3,
        "the predicate has to be retried"
    );

    let failing = Recorder::new();
    check::eventually_true(
        &failing,
        Duration::from_millis(20),
        || false,
        "the file appears",
    );
    assert!(
        failing.failed(),
        "a predicate that never holds must be reported"
    );
    assert!(named(&failing, "eventually-true"), "{}", failing.message());
}

/// Whether the seat's first record names that assertion.
fn named(seat: &Recorder, assertion: &str) -> bool {
    seat.failures()
        .first()
        .is_some_and(|held| held.assertion == assertion)
}
