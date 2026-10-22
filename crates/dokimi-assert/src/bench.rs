//! Performance ceilings, stated as a contract that a benchmark meets.
//!
//! A benchmark that prints numbers states what happened. A ceiling states
//! whether that was acceptable. A contract states its ceilings once, runs the
//! body, and fails each ceiling that the run crossed, as any other assertion
//! fails:
//!
//! ```
//! use dokimi_assert::Recorder;
//! use dokimi_assert::bench::Contract;
//! use std::time::Duration;
//!
//! let t = Recorder::new();
//! Contract::new(&t, "a lookup stays quick")
//!     .warmup(100)
//!     .max_latency(Duration::from_millis(5))
//!     .run(1_000, || {
//!         std::hint::black_box((1..100).sum::<u32>());
//!     })
//!     .check();
//! assert!(!t.failed(), "{}", t.message());
//! ```
//!
//! [`Contract::max_allocs`] and [`Contract::max_bytes`] count through
//! [`CountingAllocator`], which the test binary installs as its global
//! allocator. A contract reads time from the clock of its seat.

pub use crate::alloc::CountingAllocator;

use crate::alloc;
use crate::matcher::behaviour::nanoseconds;
use crate::seat::Seat;
use crate::value::Value;
use crate::verdict::{Mode, Run};
use std::sync::Barrier;
use std::time::Duration;

/// The samples that a p99 needs. Below it a p99 is the slowest sample, because
/// a p99 of ten samples is one reading.
const P99_MINIMUM: usize = 100;

/// The latencies of one worker, and its allocations and requested bytes.
type Worker = (Vec<Duration>, Option<(u64, u64)>);

/// What one run of the body cost.
#[derive(Debug)]
struct Measured {
    iterations: u64,
    /// Every sample, sorted.
    latencies: Vec<Duration>,
    /// The allocations and the requested bytes of the measured iterations, or
    /// `None` without the counting allocator.
    counts: Option<(u64, u64)>,
}

/// A set of ceilings, and the run that has to meet them.
///
/// Every ceiling method returns the contract, so the calls chain. Nothing is
/// asserted until [`Contract::check`] runs.
#[must_use = "a contract asserts nothing until check runs"]
pub struct Contract<'s> {
    seat: &'s dyn Seat,
    msg: String,
    warmup: usize,
    ran: bool,
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
            .field("warmup", &self.warmup)
            .field("max_latency", &self.max_latency)
            .field("max_mean", &self.max_mean)
            .field("max_allocs", &self.max_allocs)
            .field("max_bytes", &self.max_bytes)
            .field("measured", &self.measured)
            .finish_non_exhaustive()
    }
}

impl<'s> Contract<'s> {
    /// Returns a contract that states no ceiling yet. `msg` is the contract of
    /// every ceiling's assertion.
    pub fn new(seat: &'s dyn Seat, msg: &str) -> Self {
        Self {
            seat,
            msg: msg.to_owned(),
            warmup: 0,
            ran: false,
            max_latency: None,
            max_mean: None,
            max_allocs: None,
            max_bytes: None,
            measured: None,
        }
    }

    /// States iterations that the run makes before the measured ones. No
    /// ceiling covers them, and the default is 0.
    ///
    /// A warm-up iteration runs the whole body, and in
    /// [`Contract::measuring`] the setup as well. A parallel run makes its
    /// warm-up on the calling thread before the workers start.
    ///
    /// # Panics
    ///
    /// Panics when the contract has already run its body.
    pub fn warmup(mut self, iterations: usize) -> Self {
        assert!(
            !self.ran,
            "bench: warmup after the contract has run its body"
        );
        self.warmup = iterations;
        self
    }

    /// States the highest p99 latency of one iteration.
    ///
    /// The p99 rather than the mean, because the tail is what a caller waits
    /// for. A run of fewer than a hundred iterations states its slowest one.
    pub fn max_latency(mut self, ceiling: Duration) -> Self {
        self.max_latency = Some(ceiling);
        self
    }

    /// States the highest mean latency of one iteration.
    ///
    /// A mean within its ceiling while the tail grows is the regression that
    /// a mean alone misses, so a contract states it beside
    /// [`Contract::max_latency`].
    pub fn max_mean(mut self, ceiling: Duration) -> Self {
        self.max_mean = Some(ceiling);
        self
    }

    /// States the most heap allocations of one iteration: the allocations of
    /// the measured iterations divided by their number, rounded to the
    /// nearest whole number, with a half rounded up.
    pub fn max_allocs(mut self, ceiling: u64) -> Self {
        self.max_allocs = Some(ceiling);
        self
    }

    /// States the most bytes that one iteration requests from the allocator,
    /// rounded down.
    pub fn max_bytes(mut self, ceiling: u64) -> Self {
        self.max_bytes = Some(ceiling);
        self
    }

    /// Runs the warm-up iterations, then `iterations` measured iterations of
    /// `body`, and times each measured iteration.
    pub fn run(mut self, iterations: usize, mut body: impl FnMut()) -> Self {
        self.ran = true;
        for _ in 0..self.warmup {
            body();
        }
        let clock = self.seat.clock();
        let mut latencies = Vec::with_capacity(iterations);
        let before = alloc::thread_counts();
        for _ in 0..iterations {
            let started = clock.now();
            body();
            latencies.push(clock.now().saturating_sub(started));
        }
        let after = alloc::thread_counts();
        self.measured = Some(measured(latencies, before.zip(after).map(difference)));
        self
    }

    /// Measures a body whose input `setup` builds before each iteration.
    ///
    /// A benchmark whose operation consumes its input builds a new one for
    /// every iteration, and [`Contract::run`] would measure the build and the
    /// operation together. This builds every measured input first, outside
    /// the measurement, and measures only the bodies. Every measured input
    /// exists at once, so a large input and a long run cost that much memory.
    pub fn measuring<T>(
        mut self,
        iterations: usize,
        mut setup: impl FnMut() -> T,
        mut body: impl FnMut(T),
    ) -> Self {
        self.ran = true;
        for _ in 0..self.warmup {
            body(setup());
        }
        let inputs: Vec<T> = (0..iterations).map(|_| setup()).collect();
        let clock = self.seat.clock();
        let mut latencies = Vec::with_capacity(iterations);
        let before = alloc::thread_counts();
        for input in inputs {
            let started = clock.now();
            body(input);
            latencies.push(clock.now().saturating_sub(started));
        }
        let after = alloc::thread_counts();
        self.measured = Some(measured(latencies, before.zip(after).map(difference)));
        self
    }

    /// Runs `body` on `workers` scoped threads, which make `iterations`
    /// measured iterations in all, and times every iteration on its worker.
    ///
    /// The workers start together. A latency is one iteration's duration on
    /// its worker, and the allocation counts are the sums over the workers.
    /// The body borrows from the caller, because the workers end before the
    /// call returns.
    ///
    /// # Panics
    ///
    /// Panics when `workers` is 0.
    pub fn run_parallel(
        mut self,
        workers: usize,
        iterations: usize,
        body: impl Fn() + Sync,
    ) -> Self {
        assert!(workers > 0, "bench: run_parallel with no worker");
        self.ran = true;
        for _ in 0..self.warmup {
            body();
        }
        let clock = self.seat.clock();
        let start = Barrier::new(workers);
        let shares = (0..workers)
            .map(|worker| iterations / workers + usize::from(worker < iterations % workers));
        let results: Vec<Worker> = std::thread::scope(|scope| {
            let handles: Vec<_> = shares
                .map(|share| {
                    let (body, start) = (&body, &start);
                    scope.spawn(move || {
                        let mut latencies = Vec::with_capacity(share);
                        start.wait();
                        let before = alloc::thread_counts();
                        for _ in 0..share {
                            let started = clock.now();
                            body();
                            latencies.push(clock.now().saturating_sub(started));
                        }
                        let after = alloc::thread_counts();
                        (latencies, before.zip(after).map(difference))
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| {
                    handle
                        .join()
                        .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
                })
                .collect()
        });
        let mut latencies = Vec::with_capacity(iterations);
        let mut counts = Some((0, 0));
        for (worker, delta) in results {
            latencies.extend(worker);
            counts = counts.zip(delta).map(|((a, b), (c, d))| (a + c, b + d));
        }
        self.measured = Some(measured(latencies, counts));
        self
    }

    /// Reports each stated ceiling as one call of its assertion: a failure
    /// for a ceiling that the run crossed, and a pass otherwise.
    ///
    /// A contract that measured nothing, or a run of no iteration, states no
    /// verdict, and ends the test through the seat's `fail`. An allocation
    /// ceiling without the counting allocator ends with a fault.
    #[track_caller]
    pub fn check(self) {
        self.seat.helper();
        let Some(run) = self.measured.as_ref().filter(|run| run.iterations > 0) else {
            self.seat
                .fail(&format!("{}: nothing was measured", self.msg));
            return;
        };
        if let Some(ceiling) = self.max_latency {
            self.duration("bench-max-latency", ceiling, percentile(&run.latencies));
        }
        if let Some(ceiling) = self.max_mean {
            self.duration("bench-max-mean", ceiling, mean(&run.latencies));
        }
        if let Some(ceiling) = self.max_allocs {
            let per = run
                .counts
                .map(|(allocations, _)| (2 * allocations + run.iterations) / (2 * run.iterations));
            self.count("bench-max-allocs", ceiling, per);
        }
        if let Some(ceiling) = self.max_bytes {
            let per = run.counts.map(|(_, bytes)| bytes / run.iterations);
            self.count("bench-max-bytes", ceiling, per);
        }
    }

    #[track_caller]
    fn duration(&self, assertion: &'static str, ceiling: Duration, measured: Duration) {
        let run = Run::of(self.seat);
        if measured > ceiling {
            run.fail(
                Mode::Fatal,
                assertion,
                &self.msg,
                vec![
                    ("want", nanoseconds(ceiling)),
                    ("got", nanoseconds(measured)),
                ],
            );
            return;
        }
        run.pass(Mode::Fatal, assertion, &self.msg);
    }

    #[track_caller]
    fn count(&self, assertion: &'static str, ceiling: u64, measured: Option<u64>) {
        let run = Run::of(self.seat);
        let Some(measured) = measured else {
            run.fault(
                Mode::Fatal,
                assertion,
                &self.msg,
                "no allocator counts allocations in this process: install \
                 dokimi_assert::bench::CountingAllocator as the global allocator",
            );
            return;
        };
        if measured > ceiling {
            run.fail(
                Mode::Fatal,
                assertion,
                &self.msg,
                vec![
                    ("want", Value::from(ceiling)),
                    ("got", Value::from(measured)),
                ],
            );
            return;
        }
        run.pass(Mode::Fatal, assertion, &self.msg);
    }
}

fn difference(((a, b), (c, d)): ((u64, u64), (u64, u64))) -> (u64, u64) {
    (c - a, d - b)
}

fn measured(mut latencies: Vec<Duration>, counts: Option<(u64, u64)>) -> Measured {
    latencies.sort_unstable();
    Measured {
        iterations: u64::try_from(latencies.len()).unwrap_or(u64::MAX),
        latencies,
        counts,
    }
}

fn mean(sorted: &[Duration]) -> Duration {
    let total: Duration = sorted.iter().sum();
    total / u32::try_from(sorted.len()).unwrap_or(u32::MAX)
}

/// Returns the p99 of a sorted list of one sample or more, or the slowest
/// sample below [`P99_MINIMUM`] samples.
fn percentile(sorted: &[Duration]) -> Duration {
    let last = sorted.len() - 1;
    if sorted.len() < P99_MINIMUM {
        return sorted[last];
    }
    // The 99th percentile is the sample at ceil(0.99 n), counted from 1.
    let at = (99 * sorted.len()).div_ceil(100) - 1;
    sorted[at.min(last)]
}
