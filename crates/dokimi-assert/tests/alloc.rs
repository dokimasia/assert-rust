//! The spec of `alloc`, the global allocator that counts what each thread
//! allocates.

use dokimi_assert::bench::{Contract, CountingAllocator};
use dokimi_assert::{Recorder, Value};
use std::hint::black_box;

/// Returns the detail field `got` of the one failure of `seat`.
fn got(seat: &Recorder) -> Value {
    let failures = seat.failures();
    assert_eq!(
        failures.len(),
        1,
        "the contract reports one failure: {failures:?}"
    );
    failures[0].got().cloned().unwrap_or(Value::Null)
}

mod counting_allocator {
    use super::*;

    #[test]
    fn names_the_allocator_in_its_debug_text() {
        let allocator = CountingAllocator::new();
        assert_eq!(format!("{allocator:?}"), "CountingAllocator");
    }

    #[test]
    fn counts_each_allocation_of_an_iteration() {
        let seat = Recorder::new();
        let allocating = || {
            black_box(Box::new([0_u8; 64]));
        };
        Contract::new(&seat, "c")
            .max_allocs(0)
            .run(10, allocating)
            .check();
        assert_eq!(got(&seat), Value::Int(1));
    }

    #[test]
    fn counts_the_bytes_that_an_allocation_requests() {
        let seat = Recorder::new();
        let allocating = || {
            black_box(Box::new([0_u8; 64]));
        };
        Contract::new(&seat, "c")
            .max_bytes(0)
            .run(10, allocating)
            .check();
        assert_eq!(got(&seat), Value::Int(64));
    }

    #[test]
    fn counts_the_growth_of_a_reallocation() {
        let seat = Recorder::new();
        let grow = |mut grown: Vec<u8>| {
            grown.reserve_exact(24);
            black_box(grown);
        };
        Contract::new(&seat, "c")
            .max_bytes(0)
            .measuring(10, || black_box(Vec::<u8>::with_capacity(8)), grow)
            .check();
        assert_eq!(got(&seat), Value::Int(24 - 8));
    }

    #[test]
    fn counts_the_allocations_of_the_calling_thread_alone() {
        let seat = Recorder::new();
        let elsewhere = || {
            std::thread::scope(|scope| {
                scope.spawn(|| {
                    for _ in 0..64 {
                        black_box(vec![1_u64; 32]);
                    }
                });
            });
        };
        Contract::new(&seat, "c")
            .max_allocs(0)
            .run(3, elsewhere)
            .check();
        let Value::Int(counted) = got(&seat) else {
            panic!("the count is an integer");
        };
        assert!(
            counted < 64,
            "the calling thread counted {counted} allocations"
        );
    }
}
