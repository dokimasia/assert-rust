// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `matcher/allocations`: allocation ceilings on a call.
//!
//! The binary of the specs installs the counting allocator, so every ceiling
//! of this spec counts.

use crate::helpers::{failure_of, passes};
use dokimi_assert::{Value, check, soft};
use std::cell::Cell;
use std::hint::black_box;

/// Returns a body that allocates once on every call whose number, counted
/// from 1, is a multiple of `every`.
fn allocating_every(every: u64) -> impl FnMut() {
    let mut calls = 0;
    move || {
        calls += 1;
        if calls % every == 0 {
            black_box(Box::new(calls));
        }
    }
}

mod max_allocs {
    use super::*;

    #[test]
    fn passes_a_body_that_allocates_nothing() {
        let still = || {
            black_box(1 + 1);
        };
        passes(|t| check::max_allocs(t, still, 0, "c"));
    }

    #[test]
    fn passes_a_body_at_its_ceiling() {
        let once = || {
            black_box(vec![1_u8; 8]);
        };
        passes(|t| check::max_allocs(t, once, 1, "c"));
    }

    #[test]
    fn admits_one_allocation_in_all_for_a_ceiling_of_0() {
        passes(|t| check::max_allocs(t, allocating_every(101), 0, "c"));
    }

    #[test]
    fn states_the_ceiling_with_the_count_per_call_and_the_total() {
        let twice = || {
            black_box((vec![1_u8], vec![2_u8]));
        };
        let failure = failure_of(|t| check::max_allocs(t, twice, 1, "c"));
        assert_eq!(failure.assertion(), Some("max-allocs"));
        assert_eq!(
            failure.detail(),
            [
                ("want", Value::Int(1)),
                ("got", Value::Int(2)),
                ("total", Value::Int(200))
            ]
        );
    }

    #[test]
    fn rounds_a_half_up() {
        let failure = failure_of(|t| check::max_allocs(t, allocating_every(2), 0, "c"));
        assert_eq!(
            failure.detail()[1..],
            [("got", Value::Int(1)), ("total", Value::Int(50))]
        );
        passes(|t| check::max_allocs(t, allocating_every(2), 1, "c"));
    }

    #[test]
    fn warms_the_body_before_each_of_ten_counts() {
        let calls = Cell::new(0);
        let counting = || {
            calls.set(calls.get() + 1);
            black_box(Box::new(calls.get()));
        };
        let _ = failure_of(|t| soft::max_allocs(t, counting, 0, "c"));
        assert_eq!(calls.get(), 10 * 101);
    }

    #[test]
    fn passes_a_ceiling_that_the_tenth_count_meets() {
        let calls = Cell::new(0);
        let passes_last = allocating_in_counts(&calls, |count| if count < 9 { 2 } else { 1 });
        passes(|t| check::max_allocs(t, passes_last, 0, "c"));
        assert_eq!(calls.get(), 10 * 101);
    }

    #[test]
    fn states_the_lowest_total_after_ten_counts_that_exceed_the_ceiling() {
        let calls = Cell::new(0);
        let twice = allocating_in_counts(&calls, |_| 2);
        let failure = failure_of(|t| soft::max_allocs(t, twice, 0, "c"));
        assert_eq!(
            failure.detail(),
            [
                ("want", Value::Int(0)),
                ("got", Value::Int(0)),
                ("total", Value::Int(2))
            ]
        );
    }
}

/// Returns a body that allocates `allocations(count)` times in the first
/// counted call of each count, numbered from 0, and counts its calls in
/// `calls`. A count is one call that warms the body and 100 counted calls.
fn allocating_in_counts<'a>(
    calls: &'a Cell<u64>,
    allocations: impl Fn(u64) -> u64 + 'a,
) -> impl FnMut() + 'a {
    move || {
        let call = calls.get();
        calls.set(call + 1);
        if call % 101 == 1 {
            for _ in 0..allocations(call / 101) {
                black_box(Box::new(call));
            }
        }
    }
}

mod max_allocs_with_setup {
    use super::*;

    #[test]
    fn counts_the_allocations_of_the_body_alone() {
        let setup = || black_box(vec![1_u8; 64]);
        passes(|t| check::max_allocs_with_setup(t, setup, |input| drop(black_box(input)), 0, "c"));
    }

    #[test]
    fn states_the_ceiling_with_the_count_per_call_and_the_total() {
        let grow = |mut input: Vec<u8>| {
            input.extend_from_slice(&[2; 64]);
            drop(black_box(input));
        };
        let failure = failure_of(|t| {
            check::max_allocs_with_setup(t, || black_box(vec![1_u8; 4]), grow, 0, "c");
        });
        assert_eq!(failure.assertion(), Some("max-allocs-with-setup"));
        assert_eq!(
            failure.detail(),
            [
                ("want", Value::Int(0)),
                ("got", Value::Int(1)),
                ("total", Value::Int(100))
            ]
        );
    }
}
