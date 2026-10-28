// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `matcher/heap`: the ceiling on the heap that a call keeps
//! allocated.
//!
//! The binary of the specs installs the counting allocator, so every reading
//! of this spec counts.

use crate::helpers::{failure_of, passes};
use dokimi_assert::{Recorder, Value, check, soft};
use std::cell::RefCell;
use std::hint::black_box;

/// The entries that the filters read: 256 of 1 KiB each.
const ENTRIES: usize = 256;

/// The ceiling of the specs: a quarter of the entries.
const CEILING: u64 = 256 * 1024 / 4;

/// Copies each entry and drops it before the probe of the next one.
fn stream(probe: &dyn Fn()) {
    for _ in 0..ENTRIES {
        probe();
        black_box(vec![0_u8; 1024]);
    }
}

/// Copies every entry and keeps the copies until it returns.
fn buffer(probe: &dyn Fn()) {
    let mut kept = Vec::with_capacity(ENTRIES);
    for _ in 0..ENTRIES {
        probe();
        kept.push(vec![0_u8; 1024]);
    }
    black_box(kept);
}

mod max_live_bytes {
    use super::*;

    #[test]
    fn passes_a_body_that_keeps_one_entry_at_each_probe() {
        passes(|t| check::max_live_bytes(t, stream, CEILING, "c"));
    }

    #[test]
    fn fails_a_body_that_keeps_every_entry() {
        let failure = failure_of(|t| check::max_live_bytes(t, buffer, CEILING, "c"));
        let detail = failure.detail();
        assert_eq!(detail[0], ("want", Value::from(CEILING)));
        assert_eq!(detail[2], ("probes", Value::from(256_u64)));
        let ("got", Value::Int(got)) = &detail[1] else {
            panic!("the second field is got, a count of bytes: {detail:?}");
        };
        assert!(*got > i128::from(CEILING), "got is {got} bytes");
    }

    #[test]
    fn does_not_read_a_buffer_that_is_freed_before_the_probe() {
        passes(|t| {
            check::max_live_bytes(
                t,
                |probe| {
                    black_box(vec![0_u8; 4 * 65_536]);
                    probe();
                },
                CEILING,
                "c",
            );
        });
    }

    #[test]
    fn does_not_read_the_call_that_warms_the_body() {
        let cache = RefCell::new(Vec::new());
        passes(|t| {
            check::max_live_bytes(
                t,
                |probe| {
                    if cache.borrow().is_empty() {
                        *cache.borrow_mut() = vec![0_u8; 4 * 65_536];
                    }
                    probe();
                },
                CEILING,
                "c",
            );
        });
        assert_eq!(cache.borrow().len(), 4 * 65_536);
    }

    #[test]
    fn ends_the_call_with_a_fault_for_a_body_that_never_calls_the_probe() {
        let seat = Recorder::new();
        check::max_live_bytes(&seat, |_| {}, CEILING, "c");
        assert_eq!(seat.message(), "the callable called the probe 0 times");
    }

    #[test]
    fn runs_a_body_over_its_ceiling_ten_times_after_warming_it() {
        let mut calls = 0;
        failure_of(|t| {
            check::max_live_bytes(
                t,
                |probe| {
                    calls += 1;
                    buffer(probe);
                },
                CEILING,
                "c",
            );
        });
        assert_eq!(calls, 11);
    }

    #[test]
    fn calls_a_body_that_meets_its_ceiling_twice() {
        let mut calls = 0;
        passes(|t| {
            check::max_live_bytes(
                t,
                |probe| {
                    calls += 1;
                    probe();
                },
                CEILING,
                "c",
            );
        });
        assert_eq!(calls, 2);
    }

    #[test]
    fn records_a_failure_of_the_recording_surface() {
        let seat = Recorder::new();
        soft::max_live_bytes(&seat, buffer, CEILING, "c");
        assert_eq!(seat.failures().len(), 1);
        assert_eq!(seat.messages().len(), 1);
    }
}
