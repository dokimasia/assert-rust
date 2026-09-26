//! An allocation ceiling on a call in a test.
//!
//! [`bench::Contract::max_allocs`](crate::bench::Contract::max_allocs) states a ceiling on
//! a benchmark iteration, which is checked when benchmarks run. This states one on a call,
//! which the ordinary test run checks.

use super::report::{Mode, fail};
use crate::bench;
use crate::seat::Seat;

/// How many calls [`max_allocs`] counts, after the call that warms the body.
const COUNTED_CALLS: u64 = 100;

/// Report when the body makes more than `ceiling` heap allocations per call.
///
/// One call warms the body first, and the count is the average over the 100 calls after
/// it, rounded down. It counts the calling thread's allocations, so a test running beside
/// it on another thread does not change the count.
///
/// Needs [`CountingAllocator`](crate::bench::CountingAllocator) installed as the test
/// binary's global allocator. Without it the body still runs, and the assertion reports
/// that nothing counted rather than passing.
#[track_caller]
pub fn max_allocs<F: FnMut()>(seat: &dyn Seat, mode: Mode, mut body: F, ceiling: u64, msg: &str) {
    seat.helper();

    body();
    let before = bench::thread_allocations();
    for _ in 0..COUNTED_CALLS {
        body();
    }
    let after = bench::thread_allocations();

    let (Some(before), Some(after)) = (before, after) else {
        fail(
            seat,
            mode,
            "max-allocs",
            msg,
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

    let per_call = (after - before) / COUNTED_CALLS;
    if per_call > ceiling {
        fail(
            seat,
            mode,
            "max-allocs",
            msg,
            vec![
                ("want", format!("{ceiling}").into()),
                ("got", format!("{per_call}").into()),
            ],
        );
    }
}
