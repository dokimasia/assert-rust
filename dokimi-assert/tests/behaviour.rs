//! How a subject behaves, rather than what it answers.
//!
//! Each assertion is driven with a subject that honours it and one that does not, and the
//! honouring subject records that it ran. Both halves are needed: an assertion that
//! arranges cancellation so early the subject never runs reports nothing whatever it is
//! handed, and reads "it did not finish" as "it stopped when told". That bug has shipped
//! in three other languages.

use dokimi_assert::matcher::behaviour::{Cancel, Stop};
use dokimi_assert::seat::Recorder;
use dokimi_assert::{check, soft};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// A subject that reads the handle and stops when told.
fn stops_when_told(ran: &AtomicBool) -> impl FnOnce(Option<&Cancel>) -> Result<(), Stop> + '_ {
    move |handle| {
        ran.store(true, Ordering::SeqCst);
        match handle.and_then(Cancel::stopped) {
            Some(why) => Err(why),
            None => Ok(()),
        }
    }
}

/// A subject that never looks at the handle and does the work anyway.
fn ignores_the_handle(ran: &AtomicBool) -> impl FnOnce(Option<&Cancel>) -> Result<(), Stop> + '_ {
    move |_handle| {
        ran.store(true, Ordering::SeqCst);
        Ok(())
    }
}

#[test]
fn honours_cancellation_passes_a_subject_that_stops_and_it_ran() {
    let ran = AtomicBool::new(false);
    let seat = Recorder::new();
    check::honours_cancellation(&seat, stops_when_told(&ran), "the worker stops when told");

    assert!(!seat.failed(), "{}", seat.message());
    assert!(
        ran.load(Ordering::SeqCst),
        "the subject has to run, or passing means nothing"
    );
}

#[test]
fn honours_cancellation_reports_a_subject_that_ignores_the_handle() {
    let ran = AtomicBool::new(false);
    let seat = Recorder::new();
    check::honours_cancellation(
        &seat,
        ignores_the_handle(&ran),
        "the worker stops when told",
    );

    assert!(ran.load(Ordering::SeqCst), "the subject has to run");
    assert!(
        seat.failed(),
        "a subject that never reads the handle must be reported"
    );
    assert!(named(&seat, "honours-cancellation"), "{}", seat.message());
}

#[test]
fn honours_cancellation_reports_a_subject_that_fails_for_its_own_reasons() {
    // Failing and happening to do so in time is not honouring the handle.
    let seat = Recorder::new();
    check::honours_cancellation(
        &seat,
        |_handle| Err(Stop::DeadlineExceeded),
        "the worker stops when told",
    );

    assert!(seat.failed(), "the wrong reason is not the right reason");
    assert!(named(&seat, "honours-cancellation"), "{}", seat.message());
}

#[test]
fn honours_deadline_asks_for_the_other_reason() {
    let ran = AtomicBool::new(false);
    let seat = Recorder::new();
    check::honours_deadline(&seat, stops_when_told(&ran), "the worker respects no time");
    assert!(!seat.failed(), "{}", seat.message());
    assert!(ran.load(Ordering::SeqCst), "the subject has to run");

    let wrong = Recorder::new();
    check::honours_deadline(
        &wrong,
        |_h| Err(Stop::Cancelled),
        "the worker respects no time",
    );
    assert!(
        wrong.failed(),
        "a subject reporting the other reason must be reported"
    );
}

#[test]
fn a_stopped_handle_says_why() {
    assert_eq!(Cancel::cancelled().stopped(), Some(Stop::Cancelled));
    assert_eq!(Cancel::expired().stopped(), Some(Stop::DeadlineExceeded));
    assert_eq!(
        Cancel::new().stopped(),
        None,
        "a fresh handle has not been stopped"
    );

    let held = Cancel::new();
    held.stop();
    assert!(
        held.is_stopped(),
        "stopping is what a caller giving up looks like"
    );

    // The first reason wins, so a later expire does not rewrite history.
    held.expire();
    assert_eq!(held.stopped(), Some(Stop::Cancelled));
}

#[test]
fn completes_within_passes_a_quick_body_and_reports_a_slow_one() {
    let passing = Recorder::new();
    check::completes_within(
        &passing,
        Duration::from_secs(10),
        |_h| Ok::<(), Stop>(()),
        "get stays quick",
    );
    assert!(!passing.failed(), "{}", passing.message());

    let failing = Recorder::new();
    check::completes_within(
        &failing,
        Duration::from_millis(1),
        |_h| {
            std::thread::sleep(Duration::from_millis(60));
            Ok::<(), Stop>(())
        },
        "get stays quick",
    );
    assert!(failing.failed(), "a body over the ceiling must be reported");
    assert!(named(&failing, "completes-within"), "{}", failing.message());
}

#[test]
fn completes_within_hands_the_body_a_handle_that_stops_at_the_deadline() {
    let noticed = AtomicBool::new(false);
    let seat = Recorder::new();

    check::completes_within(
        &seat,
        Duration::from_millis(10),
        |handle| {
            let until = std::time::Instant::now() + Duration::from_secs(2);
            while std::time::Instant::now() < until {
                if handle.is_some_and(Cancel::is_stopped) {
                    noticed.store(true, Ordering::SeqCst);
                    return Err(Stop::DeadlineExceeded);
                }
                std::thread::yield_now();
            }
            Ok::<(), Stop>(())
        },
        "get gives up when it runs out of time",
    );

    assert!(
        noticed.load(Ordering::SeqCst),
        "a subject reading the handle can give up"
    );
}

#[test]
fn none_handle_safe_passes_a_subject_that_declines_and_reports_one_that_panics() {
    let returning = Recorder::new();
    check::none_handle_safe(
        &returning,
        |_h| Ok::<(), Stop>(()),
        "get survives no handle",
    );
    assert!(!returning.failed(), "{}", returning.message());

    let declining = Recorder::new();
    check::none_handle_safe(
        &declining,
        |_h| Err(Stop::Cancelled),
        "get survives no handle",
    );
    assert!(
        !declining.failed(),
        "an error of its own is declining, not crashing"
    );

    let crashing = Recorder::new();
    check::none_handle_safe(
        &crashing,
        |handle| {
            let _ = handle.expect("a handle is required").is_stopped();
            Ok::<(), Stop>(())
        },
        "get survives no handle",
    );
    assert!(
        crashing.failed(),
        "unwrapping the missing handle must be reported"
    );
    assert!(
        named(&crashing, "nil-context-safe"),
        "{}",
        crashing.message()
    );
}

#[test]
fn is_pure_passes_an_unchanged_projection_and_reports_a_changed_one() {
    let store = Mutex::new(vec!["a".to_owned(), "b".to_owned()]);
    let read = || store.lock().expect("the lock is held briefly").clone();

    let passing = Recorder::new();
    check::is_pure(&passing, read, || {}, "count reads");
    assert!(!passing.failed(), "{}", passing.message());

    let failing = Recorder::new();
    check::is_pure(
        &failing,
        read,
        || {
            store
                .lock()
                .expect("the lock is held briefly")
                .push("c".to_owned());
        },
        "count reads",
    );
    assert!(failing.failed(), "a changed projection must be reported");
    assert!(named(&failing, "pure"), "{}", failing.message());
}

#[test]
fn is_pure_answers_an_owned_value_so_the_reading_is_a_snapshot() {
    // Rust makes the trap the other implementations warn about hard to
    // fall into: the projection returns an owned value, so the two
    // readings cannot alias the subject.
    let store = Mutex::new(vec![1]);
    let seat = Recorder::new();

    check::is_pure(
        &seat,
        || store.lock().expect("the lock is held briefly").clone(),
        || store.lock().expect("the lock is held briefly").push(2),
        "the store is only read",
    );

    assert!(
        seat.failed(),
        "an owned reading sees the change a borrow would have missed"
    );
}

#[test]
fn rejects_passes_a_failing_body_and_reports_a_passing_one() {
    let seat = Recorder::new();
    let reported = check::rejects(
        &seat,
        |trial| check::is_not_empty(trial, "", "the name"),
        "an empty name is refused",
    );
    assert!(!seat.failed(), "{}", seat.message());
    assert!(reported.contains("the name"), "{reported}");

    let passing = Recorder::new();
    let nothing = check::rejects(
        &passing,
        |trial| check::is_not_empty(trial, "ada", "the name"),
        "an empty name is refused",
    );
    assert!(
        passing.failed(),
        "a body that reported nothing is what rejects looks for"
    );
    assert!(nothing.is_empty(), "there is no failure text to hand back");
}

#[test]
fn rejects_sees_a_soft_assertion_inside_the_body() {
    let seat = Recorder::new();
    check::rejects(
        &seat,
        |trial| soft::is_not_empty(trial, "", "name"),
        "an empty name is refused",
    );
    assert!(
        !seat.failed(),
        "recorded or thrown, the body failed either way"
    );
}

/// Whether the seat's first record names that assertion.
fn named(seat: &Recorder, assertion: &str) -> bool {
    seat.failures()
        .first()
        .is_some_and(|held| held.assertion == assertion)
}
