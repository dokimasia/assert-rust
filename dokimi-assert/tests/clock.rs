//! The clock a seat carries, and what a test can do with it.

use dokimi_assert::check;
use dokimi_assert::clock::{Clock, Controlled};
use dokimi_assert::seat::{Recorder, Seat};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

#[test]
fn now_reads_the_start_until_something_advances_it() {
    let clock = Controlled::new();

    assert_eq!(clock.now(), Duration::ZERO);
    clock.advance(Duration::from_secs(30));
    assert_eq!(clock.now(), Duration::from_secs(30));
}

#[test]
fn advance_does_not_move_time_backwards() {
    let clock = Controlled::new();
    clock.advance(Duration::ZERO);

    assert_eq!(
        clock.now(),
        Duration::ZERO,
        "an empty advance moves nothing"
    );
}

#[test]
fn sleep_returns_once_another_thread_advances_past_it() {
    let clock = Arc::new(Controlled::new());
    let sleeping = Arc::clone(&clock);
    let woke = Arc::new(AtomicUsize::new(0));
    let counting = Arc::clone(&woke);

    let worker = std::thread::spawn(move || {
        sleeping.sleep(Duration::from_secs(60));
        counting.store(1, Ordering::SeqCst);
    });

    clock.advance(Duration::from_secs(30));
    std::thread::sleep(Duration::from_millis(20));
    assert_eq!(
        woke.load(Ordering::SeqCst),
        0,
        "it does not return before the clock reaches the duration"
    );

    // Advancing well past the duration releases the sleeper whichever
    // side of the first advance it started on, which keeps this from
    // turning on thread scheduling.
    clock.advance(Duration::from_secs(3600));
    worker.join().expect("the sleeping thread finishes");
    assert_eq!(woke.load(Ordering::SeqCst), 1);
}

#[test]
fn a_seat_reads_the_platform_clock_by_default() {
    let seat = Recorder::new();

    // The platform clock moves on its own, which no controlled clock does.
    let first = seat.clock().now();
    std::thread::sleep(Duration::from_millis(5));
    assert!(
        seat.clock().now() > first,
        "the platform clock moves on its own"
    );
}

#[test]
fn with_clock_supplies_what_an_assertion_reads() {
    let clock = Arc::new(Controlled::new());
    clock.advance(Duration::from_secs(100));
    let seat = Recorder::new().with_clock(clock);

    assert_eq!(seat.clock().now(), Duration::from_secs(100));
}

#[test]
fn eventually_gives_up_without_spending_real_time() {
    let seat = Recorder::new().with_clock(Arc::new(Controlled::new()));

    let started = Instant::now();
    check::eventually(
        &seat,
        Duration::from_secs(3600),
        Duration::from_secs(60),
        |trial| check::is_true(trial, false, "never settles"),
        "the body settles",
    );
    let elapsed = started.elapsed();

    assert!(seat.failed(), "a body that never settles is reported");
    assert!(
        elapsed < Duration::from_secs(5),
        "an hour of controlled time costs no real waiting, spent {elapsed:?}"
    );
}

#[test]
fn eventually_stops_once_the_body_settles() {
    let seat = Recorder::new().with_clock(Arc::new(Controlled::new()));
    let attempts = AtomicUsize::new(0);

    check::eventually(
        &seat,
        Duration::from_secs(3600),
        Duration::from_secs(60),
        |trial| {
            let at = attempts.fetch_add(1, Ordering::SeqCst) + 1;
            check::is_true(trial, at >= 3, "not yet");
        },
        "the body settles",
    );

    assert!(
        !seat.failed(),
        "a body that settles is not reported: {}",
        seat.message()
    );
    assert_eq!(
        attempts.load(Ordering::SeqCst),
        3,
        "it stops once the body comes right"
    );
}

/// The verdict of `completes_within` comes from the clock the seat
/// carries, not from the platform.
///
/// A test that drives a controlled clock is stating how long the
/// subject took, so a body that really slept has still taken nothing
/// until the test says otherwise. Reading the platform here would make
/// the seam decorative for the one assertion that measures.
#[test]
fn completes_within_measures_on_the_seats_clock() {
    let seat = Recorder::new().with_clock(Arc::new(Controlled::new()));

    let started = Instant::now();
    check::completes_within(
        &seat,
        Duration::from_millis(1),
        |_| -> Result<(), String> {
            std::thread::sleep(Duration::from_millis(40));
            Ok(())
        },
        "the subject finishes within a millisecond",
    );

    assert!(
        started.elapsed() >= Duration::from_millis(40),
        "the body really did sleep"
    );
    assert!(
        !seat.failed(),
        "a controlled clock that nobody advanced measures no time, so the ceiling holds"
    );
}
