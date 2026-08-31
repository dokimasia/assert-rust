//! Performance ceilings, and what happens when one is crossed.
//!
//! Every ceiling is stated twice: once loose enough that any machine meets it, and once
//! tight enough that none does. Pinning a real number would fail on a busy machine and
//! teach people to rerun the suite.

use dokimi_assert::bench::{Contract, CountingAllocator};
use dokimi_assert::seat::Recorder;
use std::time::Duration;

// Installed here so the allocation ceilings are measurable. A library
// cannot install one on a caller's behalf, which is what the overlay
// records as a limit.
#[global_allocator]
static ALLOC: CountingAllocator = CountingAllocator::new();

/// Enough samples for the p99 to be a percentile rather than the slowest reading.
const SAMPLES: usize = 100;

#[test]
fn a_run_inside_every_ceiling_reports_nothing() {
    let seat = Recorder::new();
    Contract::new(&seat, "get stays quick")
        .max_latency(Duration::from_secs(10))
        .max_mean(Duration::from_secs(10))
        .run(SAMPLES, || {})
        .check();

    assert!(!seat.failed(), "{}", seat.message());
}

#[test]
fn a_crossed_latency_ceiling_names_the_p99() {
    let seat = Recorder::new();
    Contract::new(&seat, "get stays quick")
        .max_latency(Duration::from_nanos(1))
        .run(SAMPLES, || std::thread::sleep(Duration::from_millis(1)))
        .check();

    assert!(seat.failed(), "a p99 over the ceiling must be reported");
    assert!(named(&seat, "bench-max-latency"), "{}", seat.message());
    assert!(named(&seat, "bench-max-latency"), "{}", seat.message());
}

#[test]
fn a_crossed_mean_ceiling_is_reported_separately() {
    let seat = Recorder::new();
    Contract::new(&seat, "get stays quick")
        .max_mean(Duration::from_nanos(1))
        .run(SAMPLES, || std::thread::sleep(Duration::from_millis(1)))
        .check();

    assert!(seat.failed(), "a mean over the ceiling must be reported");
    assert!(named(&seat, "bench-max-mean"), "{}", seat.message());
}

#[test]
fn allocations_are_counted_exactly() {
    let seat = Recorder::new();
    Contract::new(&seat, "get allocates little")
        .max_allocs(4)
        .run(1_000, || {
            let held: Vec<u8> = Vec::with_capacity(64);
            std::hint::black_box(held);
        })
        .check();

    assert!(
        !seat.failed(),
        "one allocation per iteration is under four: {}",
        seat.message()
    );

    let crossed = Recorder::new();
    Contract::new(&crossed, "get allocates little")
        .max_allocs(0)
        .run(1_000, || {
            let held: Vec<u8> = Vec::with_capacity(64);
            std::hint::black_box(held);
        })
        .check();

    assert!(
        crossed.failed(),
        "an allocation over the ceiling must be reported"
    );
    assert!(named(&crossed, "bench-max-allocs"), "{}", crossed.message());
}

#[test]
fn bytes_are_counted_too() {
    let seat = Recorder::new();
    Contract::new(&seat, "get allocates little")
        .max_bytes(8)
        .run(1_000, || {
            let held: Vec<u8> = Vec::with_capacity(4_096);
            std::hint::black_box(held);
        })
        .check();

    assert!(
        seat.failed(),
        "four kilobytes an iteration is over eight bytes"
    );
    assert!(named(&seat, "bench-max-bytes"), "{}", seat.message());
}

#[test]
fn checking_without_running_says_nothing_was_measured() {
    let seat = Recorder::new();
    Contract::new(&seat, "get stays quick")
        .max_latency(Duration::from_secs(10))
        .check();

    assert!(seat.failed(), "a contract nobody ran proves nothing");
    assert!(
        seat.message().contains("nothing was measured"),
        "{}",
        seat.message()
    );
}

#[test]
fn a_ceiling_nobody_stated_is_not_checked() {
    let seat = Recorder::new();
    Contract::new(&seat, "get runs").run(10, || {}).check();

    assert!(
        !seat.failed(),
        "a contract with no ceiling has nothing to cross"
    );
}

/// Whether the seat's first record names that assertion.
fn named(seat: &Recorder, assertion: &str) -> bool {
    seat.failures()
        .first()
        .is_some_and(|held| held.assertion == assertion)
}
