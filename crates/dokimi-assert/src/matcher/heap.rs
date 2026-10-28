// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The ceiling on the heap that a call keeps allocated.

use crate::alloc;
use crate::seat::Seat;
use crate::value::Value;
use crate::verdict::{Mode, Run};
use std::cell::Cell;

/// The most runs that [`max_live_bytes`] takes. The assertion fails when none
/// of them meets the ceiling.
const RUNS: usize = 10;

/// Reports whether `body` keeps at most `ceiling` bytes allocated by the
/// calling thread at each call of the probe that it receives.
///
/// The assertion calls the body once with a probe that reads nothing to warm
/// it. A run then reads the live bytes of the thread, and calls the body
/// with a probe that reads them at each call. The excess of a run is its
/// largest reading less the first reading, or 0. A run whose excess exceeds
/// the ceiling is taken again, ten runs at most, and the failure states the
/// smallest excess and the probes of its run. A body that calls the probe 0
/// times in its first run ends the call with a fault.
#[track_caller]
pub(crate) fn max_live_bytes(
    seat: &dyn Seat,
    mode: Mode,
    mut body: impl FnMut(&dyn Fn()),
    ceiling: u64,
    msg: &str,
) {
    seat.helper();
    let run = Run::of(seat);
    if alloc::thread_live().is_none() {
        run.fault(mode, "max-live-bytes", msg, alloc::UNCOUNTED);
        return;
    }
    body(&|| {});
    let mut lowest: Option<(u64, u64)> = None;
    for index in 0..RUNS {
        let (excess, probes) = excess(&mut body);
        if index == 0 && probes == 0 {
            run.fault(
                mode,
                "max-live-bytes",
                msg,
                "the callable called the probe 0 times",
            );
            return;
        }
        if excess <= ceiling {
            run.pass(mode, "max-live-bytes", msg);
            return;
        }
        if lowest.is_none_or(|(least, _)| excess < least) {
            lowest = Some((excess, probes));
        }
    }
    let (got, probes) = lowest.unwrap_or_default();
    run.fail(
        mode,
        "max-live-bytes",
        msg,
        vec![
            ("want", Value::from(ceiling)),
            ("got", Value::from(got)),
            ("probes", Value::from(probes)),
        ],
    );
}

/// Runs `body` once, and returns its excess and the calls of its probe.
fn excess(body: &mut impl FnMut(&dyn Fn())) -> (u64, u64) {
    let largest = Cell::new(0_u64);
    let probes = Cell::new(0_u64);
    let first = reading();
    let probe = || {
        probes.set(probes.get() + 1);
        let excess = u64::try_from(reading().saturating_sub(first)).unwrap_or(0);
        largest.set(largest.get().max(excess));
    };
    body(&probe);
    (largest.get(), probes.get())
}

/// Returns the live bytes of the calling thread. The caller has checked that
/// the counting allocator counts them.
fn reading() -> i64 {
    alloc::thread_live().unwrap_or_default()
}
