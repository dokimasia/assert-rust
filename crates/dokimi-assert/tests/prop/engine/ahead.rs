//! The spec of `prop::engine::ahead`, the executor that runs the generated
//! cases of a run on more than one worker.

use crate::helpers::records;
use dokimi_assert::prop::Case;
use dokimi_assert::{Recorder, check, prop};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

/// Returns the detail of a run of a property that fails for an `n` of 900 or
/// more, on `workers` workers.
fn detail(workers: usize) -> serde_json::Value {
    let seat = Recorder::new();
    prop::for_all(
        &seat,
        "n stays below 900",
        prop::hermetic().seed(5).workers(workers),
        |c| {
            let n = c.draw(&prop::integer(0..=1000_u32), "n");
            check::is_true(c, n < 900, "n stays below 900");
        },
    );
    records(&seat)[0]["detail"].clone()
}

mod executor {
    use super::*;

    #[test]
    fn reports_on_more_than_one_worker_what_a_run_on_one_reports() {
        let one = detail(1);
        assert_eq!(one["outcome"], "counterexample", "{one}");
        assert_eq!(detail(4), one);
    }

    #[test]
    fn ends_the_run_with_the_fault_of_an_edge_case_on_one_worker() {
        // A case of one choice has no prefix case, so on one worker the
        // third call runs edge case 0, which draws the least value.
        let drawn = Arc::new(AtomicI64::new(0));
        let seen = Arc::clone(&drawn);
        let calls = AtomicUsize::new(0);
        let options = prop::hermetic().seed(5).around(move |run| {
            let call = calls.fetch_add(1, Ordering::SeqCst);
            run();
            let n = drawn.load(Ordering::SeqCst);
            assert_ne!(call, 2, "the wrapper refuses the edge case of n = {n}");
        });
        let seat = Recorder::new();
        prop::for_all(
            &seat,
            "n stays in its bounds",
            options,
            move |c: &Case<'_>| {
                let n = c.draw(&prop::integer(-5..=1000_i64), "n");
                seen.store(n, Ordering::SeqCst);
                check::is_true(c, n <= 1000, "n stays in its bounds");
            },
        );
        let message = seat.message();
        assert!(
            message.contains("the wrapper refuses the edge case of n = -5"),
            "{message}"
        );
    }
}
