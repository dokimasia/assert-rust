//! The spec of `bench`, the performance ceilings of a contract.

use crate::helpers::{panic_of, records};
use dokimi_assert::bench::Contract;
use dokimi_assert::{Clock, Controlled, Failure, Recorder, Value};
use std::cell::Cell;
use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// Returns a recorder on a controlled clock, with the clock.
fn controlled() -> (Recorder, Arc<Controlled>) {
    let clock = Arc::new(Controlled::new());
    (
        Recorder::new().with_clock(Arc::clone(&clock) as Arc<dyn Clock>),
        clock,
    )
}

/// Returns the duration of `millis` milliseconds in nanoseconds, as a record
/// states it.
fn nanos(millis: i128) -> Value {
    Value::Int(millis * 1_000_000)
}

mod contract {
    use super::*;

    #[test]
    fn states_its_ceilings_in_its_debug_text() {
        let seat = Recorder::new();
        let contract = Contract::new(&seat, "c").warmup(2).max_allocs(3);
        assert_eq!(
            format!("{contract:?}"),
            r#"Contract { msg: "c", warmup: 2, max_latency: None, max_mean: None, max_allocs: Some(3), max_bytes: None, measured: None, .. }"#
        );
    }
}

mod check {
    use super::*;

    #[test]
    fn fails_a_contract_that_measured_nothing() {
        let seat = Recorder::new();
        Contract::new(&seat, "a lookup stays quick")
            .max_latency(Duration::from_secs(1))
            .check();
        assert_eq!(seat.message(), "a lookup stays quick: nothing was measured");
        assert_eq!(seat.failures(), [] as [Failure; 0]);
        assert_eq!(records(&seat), [] as [serde_json::Value; 0]);
    }

    #[test]
    fn fails_a_run_of_no_iteration() {
        let seat = Recorder::new();
        Contract::new(&seat, "c")
            .max_mean(Duration::from_secs(1))
            .run(0, || {})
            .check();
        assert_eq!(seat.message(), "c: nothing was measured");
    }

    #[test]
    fn states_one_verdict_for_each_ceiling() {
        let seat = Recorder::new();
        Contract::new(&seat, "c")
            .max_latency(Duration::from_secs(5))
            .max_mean(Duration::from_secs(5))
            .max_allocs(10)
            .max_bytes(1_000)
            .run(10, || {})
            .check();
        assert!(!seat.failed(), "{}", seat.message());
        let assertions: Vec<String> = records(&seat)
            .iter()
            .map(|record| record["assertion"].as_str().unwrap_or_default().to_owned())
            .collect();
        assert_eq!(
            assertions,
            [
                "bench-max-latency",
                "bench-max-mean",
                "bench-max-allocs",
                "bench-max-bytes"
            ]
        );
    }
}

mod warmup {
    use super::*;

    #[test]
    fn runs_iterations_that_no_ceiling_covers() {
        let (seat, clock) = controlled();
        let calls = Cell::new(0);
        Contract::new(&seat, "c")
            .warmup(3)
            .max_mean(Duration::from_millis(1))
            .run(2, || {
                calls.set(calls.get() + 1);
                if calls.get() <= 3 {
                    clock.advance(Duration::from_millis(10));
                }
            })
            .check();
        assert!(!seat.failed(), "{}", seat.message());
        assert_eq!(calls.get(), 5);
    }

    #[test]
    fn panics_after_the_contract_ran_its_body() {
        let seat = Recorder::new();
        let ran = Contract::new(&seat, "c").run(1, || {});
        assert_eq!(
            panic_of(|| drop(ran.warmup(1))).as_deref(),
            Some("bench: warmup after the contract has run its body")
        );
    }
}

mod max_latency {
    use super::*;

    #[test]
    fn states_the_slowest_iteration_of_a_run_of_fewer_than_100() {
        let (seat, clock) = controlled();
        let calls = Cell::new(0);
        Contract::new(&seat, "c")
            .max_latency(Duration::from_millis(5))
            .run(10, || {
                calls.set(calls.get() + 1);
                clock.advance(Duration::from_millis(if calls.get() == 5 { 7 } else { 1 }));
            })
            .check();
        let failure = &seat.failures()[0];
        assert_eq!(failure.assertion(), Some("bench-max-latency"));
        assert_eq!(failure.detail(), [("want", nanos(5)), ("got", nanos(7))]);
    }

    #[test]
    fn states_the_99th_percentile_of_a_run_of_100() {
        let (seat, clock) = controlled();
        let calls = Cell::new(0);
        Contract::new(&seat, "c")
            .max_latency(Duration::from_millis(1))
            .run(100, || {
                calls.set(calls.get() + 1);
                clock.advance(Duration::from_millis(if calls.get() == 50 {
                    50
                } else {
                    1
                }));
            })
            .check();
        assert!(!seat.failed(), "{}", seat.message());
    }
}

mod max_mean {
    use super::*;

    #[test]
    fn states_the_mean_of_the_iterations() {
        let (seat, clock) = controlled();
        let calls = Cell::new(0);
        Contract::new(&seat, "c")
            .max_mean(Duration::from_millis(1))
            .run(4, || {
                calls.set(calls.get() + 1);
                clock.advance(Duration::from_millis(if calls.get() == 4 { 5 } else { 1 }));
            })
            .check();
        let failure = &seat.failures()[0];
        assert_eq!(failure.assertion(), Some("bench-max-mean"));
        assert_eq!(failure.detail(), [("want", nanos(1)), ("got", nanos(2))]);
    }
}

mod max_allocs {
    use super::*;

    #[test]
    fn states_the_allocations_of_one_iteration() {
        let seat = Recorder::new();
        Contract::new(&seat, "c")
            .max_allocs(0)
            .run(10, || {
                black_box(vec![1_u8; 4]);
            })
            .check();
        let failure = &seat.failures()[0];
        assert_eq!(failure.assertion(), Some("bench-max-allocs"));
        assert_eq!(
            failure.detail(),
            [("want", Value::Int(0)), ("got", Value::Int(1))]
        );
    }
}

mod max_bytes {
    use super::*;

    #[test]
    fn states_the_bytes_that_one_iteration_requests() {
        let seat = Recorder::new();
        Contract::new(&seat, "c")
            .max_bytes(32)
            .run(10, || {
                black_box(vec![1_u8; 64]);
            })
            .check();
        let failure = &seat.failures()[0];
        assert_eq!(failure.assertion(), Some("bench-max-bytes"));
        assert_eq!(
            failure.detail(),
            [("want", Value::Int(32)), ("got", Value::Int(64))]
        );
    }
}

mod measuring {
    use super::*;

    #[test]
    fn leaves_the_setup_out_of_the_measurement() {
        let seat = Recorder::new();
        Contract::new(&seat, "c")
            .max_allocs(0)
            .max_bytes(0)
            .measuring(10, || vec![1_u8; 64], |input| drop(black_box(input)))
            .check();
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn runs_the_warmup_with_its_setup() {
        let seat = Recorder::new();
        let setups = Cell::new(0);
        let _ = Contract::new(&seat, "c").warmup(2).measuring(
            3,
            || setups.set(setups.get() + 1),
            |()| {},
        );
        assert_eq!(setups.get(), 5);
    }
}

mod run_parallel {
    use super::*;

    #[test]
    fn makes_the_iterations_across_the_workers() {
        let seat = Recorder::new();
        let calls = AtomicUsize::new(0);
        Contract::new(&seat, "c")
            .warmup(1)
            .max_mean(Duration::from_secs(5))
            .run_parallel(3, 7, || {
                calls.fetch_add(1, Ordering::SeqCst);
            })
            .check();
        assert!(!seat.failed(), "{}", seat.message());
        assert_eq!(calls.load(Ordering::SeqCst), 8);
    }

    #[test]
    fn sums_the_allocations_of_every_worker() {
        let seat = Recorder::new();
        Contract::new(&seat, "c")
            .max_allocs(0)
            .run_parallel(2, 10, || {
                black_box(vec![1_u8; 8]);
            })
            .check();
        assert_eq!(
            seat.failures()[0].detail(),
            [("want", Value::Int(0)), ("got", Value::Int(1))]
        );
    }

    #[test]
    fn panics_without_a_worker() {
        let seat = Recorder::new();
        assert_eq!(
            panic_of(|| drop(Contract::new(&seat, "c").run_parallel(0, 1, || {}))).as_deref(),
            Some("bench: run_parallel with no worker")
        );
    }

    #[test]
    fn raises_the_panic_of_a_worker_on_the_calling_thread() {
        let seat = Recorder::new();
        let raised = panic_of(|| {
            drop(Contract::new(&seat, "c").run_parallel(2, 2, || panic!("the body fails")));
        });
        assert_eq!(raised.as_deref(), Some("the body fails"));
    }
}
