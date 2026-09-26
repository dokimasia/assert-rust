//! Performance ceilings, stated as a contract a benchmark must meet.
//!
//! A benchmark that only prints numbers tells you what happened; a ceiling tells you
//! whether it was acceptable. State the ceiling once and the run fails when it is
//! crossed, the same way any other assertion does.
//!
//! ```no_run
//! use dokimi_assert::{bench::Contract, seat::Collector};
//! use std::time::Duration;
//!
//! # let seat = Collector::new();
//! Contract::new(&seat, "get stays quick")
//!     .max_latency(Duration::from_millis(2))
//!     .max_allocs(4)
//!     .run(10_000, || { std::hint::black_box(1 + 1); })
//!     .check();
//! ```
//!
//! # Counting allocations
//!
//! [`Contract::max_allocs`] and [`Contract::max_bytes`] need [`CountingAllocator`]
//! installed as the test binary's global allocator. Rust counts allocations through the
//! allocator, as Go counts them through its runtime, and the installation is the price: a
//! library cannot install an allocator on a caller's behalf.
//!
//! [`check::max_allocs`](crate::check::max_allocs) states the same ceiling on a call in a
//! test, which the ordinary test run checks. It needs the allocator too.

use crate::matcher::{Mode, fail, report};
use crate::seat::Seat;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// How many samples a p99 needs before it means anything.
const P99_MINIMUM: usize = 100;

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);
static INSTALLED: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    /// The allocations this thread has made. An allocation ceiling on one call reads
    /// this rather than the process-wide count, so a test running beside it on another
    /// thread does not change its count.
    ///
    /// Initialised by a constant and holding no destructor, so reading it from the
    /// allocator allocates nothing.
    static THREAD_ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
}

/// Count one allocation against the thread that made it.
///
/// `try_with` rather than `with`, because a thread can allocate while its storage is
/// being torn down, and counting must never be why an allocation fails.
fn count_on_this_thread() {
    let _ = THREAD_ALLOCATIONS.try_with(|held| held.set(held.get() + 1));
}

/// The allocations the calling thread has made, or `None` when [`CountingAllocator`] is
/// not installed and nothing counts.
pub(crate) fn thread_allocations() -> Option<u64> {
    if INSTALLED.load(Ordering::Relaxed) != 1 {
        return None;
    }
    THREAD_ALLOCATIONS.try_with(Cell::get).ok()
}

/// A global allocator that counts what the program asks for.
///
/// Install it in the test binary to make the allocation ceilings measurable:
///
/// ```
/// # use dokimi_assert::bench::CountingAllocator;
/// #[global_allocator]
/// static ALLOC: CountingAllocator = CountingAllocator::new();
/// ```
///
/// It counts every allocation on every thread and hands the work to the system
/// allocator, so installing it changes what is measured and not what happens.
#[derive(Debug, Default)]
pub struct CountingAllocator;

impl CountingAllocator {
    /// Return the allocator.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

// The one unsafe impl in the crate. GlobalAlloc is unsafe to implement
// because a caller relies on the pointer and layout rules; every method
// here forwards to the system allocator unchanged and only adds a
// counter, so it upholds exactly what System upholds.
#[expect(unsafe_code, reason = "GlobalAlloc cannot be implemented safely")]
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        INSTALLED.store(1, Ordering::Relaxed);
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        count_on_this_thread();
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        count_on_this_thread();
        BYTES.fetch_add(
            new_size.saturating_sub(layout.size()) as u64,
            Ordering::Relaxed,
        );
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

/// What one run of the body cost.
#[derive(Debug)]
struct Measured {
    latencies: Vec<Duration>,
    allocations: Option<u64>,
    bytes: Option<u64>,
}

/// Performance ceilings, chained onto one contract.
pub struct Contract<'seat> {
    seat: &'seat dyn Seat,
    msg: &'seat str,
    max_latency: Option<Duration>,
    max_mean: Option<Duration>,
    max_allocs: Option<u64>,
    max_bytes: Option<u64>,
    measured: Option<Measured>,
}

impl std::fmt::Debug for Contract<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Contract")
            .field("msg", &self.msg)
            .field("max_latency", &self.max_latency)
            .field("max_mean", &self.max_mean)
            .field("max_allocs", &self.max_allocs)
            .field("max_bytes", &self.max_bytes)
            .field("measured", &self.measured)
            .finish_non_exhaustive()
    }
}

impl<'seat> Contract<'seat> {
    /// Return a contract that states no ceilings yet.
    #[must_use]
    pub fn new(seat: &'seat dyn Seat, msg: &'seat str) -> Self {
        Self {
            seat,
            msg,
            max_latency: None,
            max_mean: None,
            max_allocs: None,
            max_bytes: None,
            measured: None,
        }
    }

    /// State the highest acceptable p99 latency per iteration.
    ///
    /// The p99 rather than the mean, because the tail is what a caller waits for. With
    /// fewer than a hundred iterations it is the slowest one.
    #[must_use]
    pub fn max_latency(mut self, ceiling: Duration) -> Self {
        self.max_latency = Some(ceiling);
        self
    }

    /// State the highest acceptable mean latency per iteration.
    ///
    /// Use it beside [`Self::max_latency`] rather than instead of it: a mean that holds
    /// while the tail grows is the regression a mean alone misses.
    #[must_use]
    pub fn max_mean(mut self, ceiling: Duration) -> Self {
        self.max_mean = Some(ceiling);
        self
    }

    /// State the most allocations the body may make per iteration.
    ///
    /// Needs [`CountingAllocator`] installed, and says so rather than passing quietly
    /// when it is not.
    #[must_use]
    pub fn max_allocs(mut self, ceiling: u64) -> Self {
        self.max_allocs = Some(ceiling);
        self
    }

    /// State the most bytes the body may allocate per iteration.
    ///
    /// Needs [`CountingAllocator`] installed, and says so rather than passing quietly
    /// when it is not.
    #[must_use]
    pub fn max_bytes(mut self, ceiling: u64) -> Self {
        self.max_bytes = Some(ceiling);
        self
    }

    /// Measure a body whose input is built fresh each iteration.
    ///
    /// A benchmark whose operation consumes its input builds a new one every
    /// time, and [`Contract::run`] would state what the build and the operation
    /// cost together. This builds every input first, outside the measurement,
    /// and measures only the bodies:
    ///
    /// ```ignore
    /// Contract::new(&seat, "settling stays cheap")
    ///     .max_allocs(4)
    ///     .measuring(10_000, fresh_store, |store| store.settle())
    ///     .check();
    /// ```
    ///
    /// Every input exists at once, so a large fixture and a long run cost that
    /// much memory. A body needing no input wants [`Contract::run`], which
    /// holds nothing.
    #[must_use]
    pub fn measuring<T, S: FnMut() -> T, F: FnMut(T)>(
        mut self,
        iterations: usize,
        mut setup: S,
        mut body: F,
    ) -> Self {
        // Built first and all at once, so no counter is read between one
        // iteration's setup and the next iteration's work.
        let mut inputs = Vec::with_capacity(iterations);
        for _ in 0..iterations {
            inputs.push(setup());
        }

        let mut latencies = Vec::with_capacity(inputs.len());
        let counting = INSTALLED.load(Ordering::Relaxed) == 1;
        let allocs_before = ALLOCATIONS.load(Ordering::Relaxed);
        let bytes_before = BYTES.load(Ordering::Relaxed);

        for input in inputs {
            let started = Instant::now();
            body(input);
            latencies.push(started.elapsed());
        }

        let per = |before: u64, counter: &AtomicU64| -> Option<u64> {
            if !counting || iterations == 0 {
                return None;
            }
            Some((counter.load(Ordering::Relaxed) - before) / iterations as u64)
        };
        let allocations = per(allocs_before, &ALLOCATIONS);
        let bytes = per(bytes_before, &BYTES);

        latencies.sort_unstable();
        self.measured = Some(Measured {
            latencies,
            allocations,
            bytes,
        });
        self
    }

    /// Run the body the given number of times, timing each and weighing the whole.
    #[must_use]
    pub fn run<F: FnMut()>(mut self, iterations: usize, mut body: F) -> Self {
        let mut latencies = Vec::with_capacity(iterations);
        let counting = INSTALLED.load(Ordering::Relaxed) == 1;
        let allocs_before = ALLOCATIONS.load(Ordering::Relaxed);
        let bytes_before = BYTES.load(Ordering::Relaxed);

        for _ in 0..iterations {
            let started = Instant::now();
            body();
            latencies.push(started.elapsed());
        }

        let per = |before: u64, counter: &AtomicU64| -> Option<u64> {
            if !counting || iterations == 0 {
                return None;
            }
            Some((counter.load(Ordering::Relaxed) - before) / iterations as u64)
        };
        // Read before sorting, since sorting allocates nothing but the
        // reads should not straddle any work of ours either way.
        let allocations = per(allocs_before, &ALLOCATIONS);
        let bytes = per(bytes_before, &BYTES);

        latencies.sort_unstable();
        self.measured = Some(Measured {
            latencies,
            allocations,
            bytes,
        });
        self
    }

    /// Report every ceiling the run crossed.
    #[track_caller]
    pub fn check(self) {
        self.seat.helper();
        let Some(run) = self.measured.as_ref() else {
            report(
                self.seat,
                Mode::Fatal,
                &format!("{}: nothing was measured", self.msg),
            );
            return;
        };

        if let Some(ceiling) = self.max_latency {
            let p99 = percentile(&run.latencies);
            if p99 > ceiling {
                self.crossed("bench-max-latency", p99, ceiling);
            }
        }
        if let Some(ceiling) = self.max_mean {
            let mean = mean(&run.latencies);
            if mean > ceiling {
                self.crossed("bench-max-mean", mean, ceiling);
            }
        }
        self.weighed("bench-max-allocs", run.allocations, self.max_allocs);
        self.weighed("bench-max-bytes", run.bytes, self.max_bytes);
    }

    fn crossed(&self, assertion: &'static str, measured: Duration, ceiling: Duration) {
        fail(
            self.seat,
            Mode::Fatal,
            assertion,
            self.msg,
            vec![
                ("want", format!("{}ms", ceiling.as_millis()).into()),
                (
                    "got",
                    format!("{:.3}ms", measured.as_secs_f64() * 1000.0).into(),
                ),
            ],
        );
    }

    /// Report a crossed allocation ceiling, or that nothing was counting.
    fn weighed(&self, assertion: &'static str, measured: Option<u64>, ceiling: Option<u64>) {
        let Some(ceiling) = ceiling else { return };
        let Some(measured) = measured else {
            // Nothing counted, which is the allocator being absent rather
            // than a ceiling being crossed. It still reports as the
            // assertion the caller wrote.
            fail(
                self.seat,
                Mode::Fatal,
                assertion,
                self.msg,
                vec![
                    ("want", format!("{ceiling}").into()),
                    (
                        "got",
                        "nothing counted; CountingAllocator is not installed".into(),
                    ),
                ],
            );
            return;
        };
        if measured > ceiling {
            fail(
                self.seat,
                Mode::Fatal,
                assertion,
                self.msg,
                vec![
                    ("want", format!("{ceiling}").into()),
                    ("got", format!("{measured}").into()),
                ],
            );
        }
    }
}

fn mean(sorted: &[Duration]) -> Duration {
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    sorted.iter().sum::<Duration>() / u32::try_from(sorted.len()).unwrap_or(u32::MAX)
}

/// The p99, or the slowest sample when there are too few to mean anything.
///
/// With ten samples the 99th percentile is the tenth, and calling that a p99 would dress
/// one reading up as a distribution.
fn percentile(sorted: &[Duration]) -> Duration {
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    if sorted.len() < P99_MINIMUM {
        return sorted[sorted.len() - 1];
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "a sample count is far below 2^53"
    )]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the index is bounded by len"
    )]
    #[expect(clippy::cast_sign_loss, reason = "ceil of a positive product")]
    let at = (0.99 * sorted.len() as f64).ceil() as usize - 1;
    sorted[at.min(sorted.len() - 1)]
}
