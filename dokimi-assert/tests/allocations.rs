//! An allocation ceiling on a call, and what it counts.
//!
//! The counter is the calling thread's own, so these run in parallel with each other and
//! with a thread that allocates without changing a count.

use dokimi_assert::bench::CountingAllocator;
use dokimi_assert::failure::Detail;
use dokimi_assert::seat::Recorder;
use dokimi_assert::{check, soft};
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

// Installed here so the ceiling is measurable. A library cannot install one on a
// caller's behalf, which is what the overlay records as a limit.
#[global_allocator]
static ALLOC: CountingAllocator = CountingAllocator::new();

/// The number of calls a ceiling makes: one to warm the body and 100 to count.
const WARM_AND_COUNTED: usize = 101;

/// Allocate once per call, where the optimiser cannot remove it.
fn allocate_once() {
    let held: Vec<u8> = Vec::with_capacity(64);
    std::hint::black_box(held);
}

#[test]
fn a_body_that_allocates_nothing_passes() {
    let seat = Recorder::new();
    check::max_allocs(&seat, || {}, 0, "an empty body allocates nothing");

    assert!(!seat.failed(), "{}", seat.message());
}

#[test]
fn a_body_at_its_ceiling_passes() {
    let seat = Recorder::new();
    check::max_allocs(
        &seat,
        allocate_once,
        1,
        "one allocation meets a ceiling of one",
    );

    assert!(!seat.failed(), "{}", seat.message());
}

#[test]
fn a_body_over_its_ceiling_reports_the_ceiling_and_the_count() {
    let seat = Recorder::new();
    check::max_allocs(&seat, allocate_once, 0, "the body allocates nothing");

    let failures = seat.failures();
    let first = failures
        .first()
        .expect("a body over its ceiling must be reported");
    assert_eq!(first.assertion, "max-allocs");
    assert_eq!(first.detail("want"), Some(&Detail::Said("0".to_owned())));
    assert_eq!(first.detail("got"), Some(&Detail::Said("1".to_owned())));
    assert!(
        seat.messages().is_empty(),
        "the aborting surface reports through fail, not record"
    );
}

#[test]
fn the_call_that_warms_the_body_is_not_counted() {
    let cache: Cell<Option<Vec<u8>>> = Cell::new(None);
    let seat = Recorder::new();
    check::max_allocs(
        &seat,
        || {
            let held = cache.take().unwrap_or_else(|| Vec::with_capacity(64));
            cache.set(Some(held));
        },
        0,
        "a body that fills its cache on the first call allocates nothing after it",
    );

    assert!(!seat.failed(), "{}", seat.message());
}

#[test]
fn another_threads_allocations_are_not_counted() {
    let running = AtomicBool::new(false);
    let stop = AtomicBool::new(false);
    let seat = Recorder::new();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            while !stop.load(Ordering::Relaxed) {
                allocate_once();
                running.store(true, Ordering::Relaxed);
            }
        });
        while !running.load(Ordering::Relaxed) {
            std::hint::spin_loop();
        }
        // Each call waits long enough for the other thread to allocate many times, so a
        // count that included it would be far above the ceiling after dividing by 100.
        check::max_allocs(
            &seat,
            || std::thread::sleep(Duration::from_micros(100)),
            0,
            "a sleeping body allocates nothing beside a busy thread",
        );
        stop.store(true, Ordering::Relaxed);
    });

    assert!(!seat.failed(), "{}", seat.message());
}

#[test]
fn the_body_runs_once_to_warm_and_a_hundred_times_to_count() {
    let calls = Cell::new(0_usize);
    let seat = Recorder::new();
    check::max_allocs(&seat, || calls.set(calls.get() + 1), 0, "the body runs");

    assert_eq!(calls.get(), WARM_AND_COUNTED);
}

#[test]
fn the_recording_surface_records_every_crossing() {
    let seat = Recorder::new();
    soft::max_allocs(&seat, allocate_once, 0, "the first body allocates nothing");
    soft::max_allocs(&seat, allocate_once, 0, "and neither does the second");

    assert_eq!(
        seat.messages().len(),
        2,
        "both crossings are recorded: {:?}",
        seat.messages()
    );
}
