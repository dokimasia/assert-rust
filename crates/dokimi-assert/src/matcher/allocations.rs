// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! Allocation ceilings on a call in a test.

use crate::alloc;
use crate::seat::Seat;
use crate::value::Value;
use crate::verdict::{Mode, Run};

/// The calls that one count counts, after the call that warms the callable.
const RUNS: u64 = 100;

/// The most counts that a ceiling takes. The assertion fails when none of
/// them meets the ceiling.
const COUNTS: usize = 10;

/// The most allocations that the counted calls can make and still meet a
/// ceiling of 0. It admits an allocation that the process makes while a
/// count runs.
const ZERO_SLACK: u64 = 1;

/// Returns `total`, the allocations of [`RUNS`] calls, per call: the average
/// rounded to the nearest whole number, with a half rounded up.
pub(crate) fn per_call(total: u64) -> u64 {
    (2 * total + RUNS) / (2 * RUNS)
}

/// Returns `true` when `total` meets `ceiling`: at most [`ZERO_SLACK`]
/// allocations for a ceiling of 0, and an average that [`per_call`] rounds to
/// at most `ceiling` for any other.
pub(crate) fn meets(ceiling: u64, total: u64) -> bool {
    if ceiling == 0 {
        return total <= ZERO_SLACK;
    }
    per_call(total) <= ceiling
}

/// Returns the allocations of the calling thread, or `None` without the
/// counting allocator.
fn reading() -> Option<u64> {
    alloc::thread_allocations()
}

/// Reports whether `body` makes at most `ceiling` heap allocations per call.
///
/// A count calls the body once to warm it and counts the allocations of the
/// next 100 calls on the calling thread. A ceiling of 0 admits one
/// allocation in all, and any other ceiling admits an average per call that
/// rounds to at most the ceiling. A count that exceeds the ceiling is taken
/// again, ten times at most, and the failure states the count with the
/// lowest total.
#[track_caller]
pub(crate) fn max_allocs(
    seat: &dyn Seat,
    mode: Mode,
    mut body: impl FnMut(),
    ceiling: u64,
    msg: &str,
) {
    seat.helper();
    let run = Run::of(seat);
    let mut lowest = u64::MAX;
    for _ in 0..COUNTS {
        body();
        let Some(before) = reading() else {
            run.fault(mode, "max-allocs", msg, alloc::UNCOUNTED);
            return;
        };
        for _ in 0..RUNS {
            body();
        }
        let total = reading().unwrap_or(before) - before;
        if meets(ceiling, total) {
            run.pass(mode, "max-allocs", msg);
            return;
        }
        lowest = lowest.min(total);
    }
    run.fail(
        mode,
        "max-allocs",
        msg,
        vec![
            ("want", Value::from(ceiling)),
            ("got", Value::from(per_call(lowest))),
            ("total", Value::from(lowest)),
        ],
    );
}

/// Reports whether `body` makes at most `ceiling` heap allocations per call
/// on an input that `setup` builds before each call. The allocations of
/// `setup` are not counted. The counts and the failure are those of
/// [`max_allocs`].
#[track_caller]
pub(crate) fn max_allocs_with_setup<T>(
    seat: &dyn Seat,
    mode: Mode,
    mut setup: impl FnMut() -> T,
    mut body: impl FnMut(T),
    ceiling: u64,
    msg: &str,
) {
    seat.helper();
    let run = Run::of(seat);
    let mut lowest = u64::MAX;
    for _ in 0..COUNTS {
        body(setup());
        if reading().is_none() {
            run.fault(mode, "max-allocs-with-setup", msg, alloc::UNCOUNTED);
            return;
        }
        let mut total = 0;
        for _ in 0..RUNS {
            let input = setup();
            let before = reading().unwrap_or_default();
            body(input);
            total += reading().unwrap_or(before) - before;
        }
        if meets(ceiling, total) {
            run.pass(mode, "max-allocs-with-setup", msg);
            return;
        }
        lowest = lowest.min(total);
    }
    run.fail(
        mode,
        "max-allocs-with-setup",
        msg,
        vec![
            ("want", Value::from(ceiling)),
            ("got", Value::from(per_call(lowest))),
            ("total", Value::from(lowest)),
        ],
    );
}
