// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `prop::engine::tree`, the case tree of a run, which finds a
//! repeated case, a body that diverges and an exhausted domain.

use crate::helpers::records;
use dokimi_assert::prop;
use dokimi_assert::{Recorder, check};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Returns the detail of the record of a property whose body `body` receives
/// the number of the call.
fn detail(body: impl Fn(&prop::Case<'_>, usize) + Sync) -> serde_json::Value {
    let calls = AtomicUsize::new(0);
    let seat = Recorder::new();
    prop::for_all(
        &seat,
        "the body is a function of its choices",
        prop::hermetic().seed(2).store(""),
        |c| {
            body(c, calls.fetch_add(1, Ordering::SeqCst));
        },
    );
    records(&seat)[0]["detail"].clone()
}

mod walker {
    use super::*;

    #[test]
    fn ends_the_run_at_a_first_case_without_a_request() {
        let detail = detail(|c, call| {
            if call > 0 {
                let _ = c.draw(&prop::boolean(), "flag");
            }
        });
        assert_eq!(detail["outcome"], "vacuous", "{detail}");
        assert_eq!(detail["cases"], 1, "{detail}");
    }

    #[test]
    fn finds_an_end_where_an_earlier_case_made_a_request() {
        let detail = detail(|c, call| {
            if call == 0 {
                let _ = c.draw(&prop::boolean(), "flag");
            }
        });
        assert_eq!(detail["outcome"], "flaky", "{detail}");
        assert_eq!(detail["divergence"]["what"], "request", "{detail}");
        assert_eq!(
            detail["divergence"]["replayed"],
            serde_json::Value::Null,
            "{detail}"
        );
    }

    #[test]
    fn keys_every_nan_as_one_value() {
        let detail = detail(|c, _| {
            let x = c.draw(&prop::float(0.0..=1.0_f64).allow_nan(), "x");
            check::is_true(
                c,
                x.is_nan() || (0.0..=1.0).contains(&x),
                "x is in range or NaN",
            );
        });
        assert_eq!(detail["outcome"], "passed", "{detail}");
    }
}

mod tree {
    use super::*;

    #[test]
    fn keeps_running_cases_once_it_holds_two_to_the_20_nodes() {
        let seat = Recorder::new();
        prop::for_all(
            &seat,
            "every case runs",
            prop::hermetic().seed(3).cases(140).store(""),
            |c| {
                let mut rand = c.rand();
                for _ in 0..8000 {
                    let _ = rand.next_u64();
                }
            },
        );
        let detail = &records(&seat)[0]["detail"];
        assert_eq!(detail["outcome"], "passed", "{detail}");
        assert_eq!(detail["cases"], 140, "{detail}");
    }
}
