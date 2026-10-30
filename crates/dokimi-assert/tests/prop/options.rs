// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `prop::options`, the settings of a property.

use crate::helpers::{Results, panic_of, records};
use dokimi_assert::prop::{self, Case, Options, RunResult};
use dokimi_assert::{Recorder, Standard, check};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

/// Returns the debug text of `options`.
fn stated<T>(options: &Options<T>) -> String {
    format!("{options:?}")
}

mod options {
    use super::*;

    #[test]
    fn states_the_defaults_of_the_definition() {
        assert_eq!(
            stated(&Options::<()>::default()),
            "Options { cases: 100, seed: None, replay: None, shrink: 2000, shrink_time: 30s, max_choices: 8192, \
             store: None, explain: true, workers: 1, hermetic: false, using: {}, .. }"
        );
    }

    #[test]
    fn starts_a_chain_with_the_function_of_each_setting() {
        let tests: [(Options, &str); 13] = [
            (prop::cases(5), "cases: 5"),
            (prop::seed(7), "seed: Some(7)"),
            (prop::replay("prop1:AA"), r#"replay: Some("prop1:AA")"#),
            (prop::shrink(3), "shrink: 3"),
            (prop::shrink_time(Duration::from_secs(1)), "shrink_time: 1s"),
            (prop::max_choices(9), "max_choices: 9"),
            (prop::store("dir"), r#"store: Some("dir")"#),
            (prop::explain(false), "explain: false"),
            (prop::workers(2), "workers: 2"),
            (prop::hermetic(), "hermetic: true"),
            (prop::using(&prop::just(1_u8)), r#"using: {"u8"}"#),
            (prop::on_result(|_| {}), "cases: 100"),
            (
                prop::require("small", 0.5).draws("[]").around(|run| run()),
                "cases: 100",
            ),
        ];
        for (give, want) in tests {
            assert!(
                stated(&give).contains(want),
                "{} states {want}",
                stated(&give)
            );
        }
    }

    #[test]
    fn keeps_every_setting_in_a_clone() {
        let options = prop::cases::<u8>(5)
            .seed(7)
            .example([1])
            .using(&prop::integer(0..=3_u8));
        assert_eq!(stated(&options.clone()), stated(&options));
    }

    #[test]
    fn panics_for_a_count_of_0() {
        assert_eq!(
            panic_of(|| _ = prop::cases::<()>(0)).as_deref(),
            Some("prop: cases(0) tests nothing")
        );
        assert_eq!(
            panic_of(|| _ = prop::max_choices::<()>(0)).as_deref(),
            Some("prop: max_choices(0) admits no choice")
        );
        assert_eq!(
            panic_of(|| _ = prop::workers::<()>(0)).as_deref(),
            Some("prop: workers(0) runs nothing")
        );
    }

    #[test]
    fn panics_for_a_share_outside_0_to_1() {
        assert_eq!(
            panic_of(|| _ = prop::require::<()>("small", 2.0)).as_deref(),
            Some(r#"prop: require("small", 2) states no share"#)
        );
    }
}

mod explain {
    use super::*;

    #[test]
    fn leaves_the_draws_of_a_counterexample_unexplained_when_off() {
        let seat = Recorder::new();
        prop::for_all(
            &seat,
            "n stays small",
            prop::explain(false).hermetic().seed(1).store(""),
            |c| {
                let n = c.draw(&prop::integer(0..=9_u8), "n");
                check::is_true(c, n < 3, "n stays below 3");
            },
        );
        let draw = &records(&seat)[0]["detail"]["counterexample"][0];
        assert_eq!(draw["any-value-fails"], serde_json::Value::Null);
    }
}

mod around {
    use super::*;

    #[test]
    fn runs_each_run_of_the_body_inside_the_wrapper() {
        let wrapped = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&wrapped);
        let seat = Recorder::new();
        let options = prop::around(move |run| {
            counted.fetch_add(1, Ordering::SeqCst);
            run();
        });
        prop::for_all(
            &seat,
            "the body runs",
            options.hermetic().seed(1).cases(5).store(""),
            |c| {
                let _ = c.draw(&prop::integer(0..=9_u8), "n");
            },
        );
        assert!(!seat.failed(), "{}", seat.message());
        assert!(wrapped.load(Ordering::SeqCst) >= 5);
    }
}

mod on_result {
    use super::*;

    /// The contract of every property of the module.
    const CONTRACT: &str = "n stays below 3";

    /// Returns the settings of the module's runs under `options`.
    fn hermetic<T>(options: Options<T>) -> Options<T> {
        options.hermetic().seed(1).store("")
    }

    /// Draws `n` and fails when `n` is 3 or more.
    fn drawing(c: &Case<'_>) {
        let n = c.draw(&prop::integer(0..=9_u8), "n");
        check::is_true(c, n < 3, CONTRACT);
    }

    #[test]
    fn hands_the_result_before_an_aborting_seat_receives_the_failure() {
        let results = Results::default();
        let options = hermetic(prop::on_result(results.keep()));
        let raised = panic_of(|| prop::for_all(&Standard::new(), CONTRACT, options, drawing))
            .expect("the seat panics with the failure");
        assert!(
            raised.starts_with("n stays below 3: counterexample after "),
            "{raised}"
        );
        let outcomes: Vec<_> = results.kept().iter().map(|result| result.outcome).collect();
        assert_eq!(outcomes, ["counterexample"]);
    }

    #[test]
    fn hands_the_result_to_the_function_of_the_later_option() {
        let (first, later) = (Results::default(), Results::default());
        let options = prop::on_result(first.keep()).on_result(later.keep());
        prop::for_all(&Recorder::new(), CONTRACT, hermetic(options), drawing);
        assert_eq!((first.kept().len(), later.kept().len()), (0, 1));
    }

    #[test]
    fn hands_the_result_on_the_thread_of_the_call() {
        let threads = Arc::new(Mutex::new(Vec::new()));
        let kept = Arc::clone(&threads);
        let options = prop::workers(4).on_result(move |_| {
            kept.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(std::thread::current().id());
        });
        prop::for_all(&Recorder::new(), CONTRACT, hermetic(options), drawing);
        assert_eq!(
            *threads.lock().unwrap_or_else(PoisonError::into_inner),
            [std::thread::current().id()]
        );
    }

    #[test]
    fn hands_the_result_of_a_property_form() {
        let results = Results::default();
        prop::is_true(
            &Recorder::new(),
            |n: &u8| *n < 3,
            CONTRACT,
            hermetic(prop::on_result(results.keep())),
        );
        let outcomes: Vec<_> = results.kept().iter().map(|result| result.outcome).collect();
        assert_eq!(outcomes, ["counterexample"]);
    }

    #[test]
    fn hands_the_result_of_each_call_of_the_fuzz_bridge() {
        let results = Results::default();
        let seat = Recorder::new();
        let options = prop::on_result(results.keep()).hermetic().store("");
        let target = prop::fuzz(&seat, CONTRACT, options, |c| {
            _ = c.draw(&prop::integer(0..=9_u8), "n");
        });
        target(&[1]);
        let counts: Vec<_> = results
            .kept()
            .iter()
            .map(|result| (result.outcome, result.cases))
            .collect();
        assert_eq!(counts, [("passed", 0), ("passed", 1)]);
    }

    #[test]
    fn does_not_hand_a_result_for_a_run_that_a_fault_ends() {
        let results = Results::default();
        let seat = Recorder::new();
        let options = prop::on_result(results.keep()).around(|_| panic!("the wrapper fails"));
        prop::for_all(&seat, CONTRACT, hermetic(options), drawing);
        assert!(
            seat.message()
                .ends_with("around: the wrapper panicked: the wrapper fails"),
            "{}",
            seat.message()
        );
        assert_eq!(results.kept(), [] as [RunResult; 0]);
    }

    #[test]
    fn does_not_hand_a_result_for_a_call_that_a_fault_ends_before_its_run() {
        let results = Results::default();
        let seat = Recorder::new();
        let options = prop::on_result(results.keep()).replay("nonsense");
        prop::for_all(&seat, CONTRACT, hermetic(options), drawing);
        assert!(
            seat.message().starts_with("prop::for_all: replay: "),
            "{}",
            seat.message()
        );
        assert_eq!(results.kept(), [] as [RunResult; 0]);
    }

    #[test]
    fn passes_on_a_panic_of_the_function_without_a_verdict() {
        let seat = Recorder::new();
        let options = hermetic(prop::on_result(|_| panic!("the function fails")));
        let raised = panic_of(|| prop::for_all(&seat, CONTRACT, options, drawing));
        assert_eq!(raised.as_deref(), Some("the function fails"));
        let calls: Vec<_> = records(&seat)
            .into_iter()
            .filter(|record| record["parent"].is_null())
            .collect();
        assert_eq!(calls, [] as [serde_json::Value; 0]);
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn keeps_the_function_in_a_clone() {
        let results = Results::default();
        let options = prop::on_result(results.keep());
        prop::for_all(
            &Recorder::new(),
            CONTRACT,
            hermetic(options.clone()),
            drawing,
        );
        assert_eq!(results.kept().len(), 1);
    }
}

mod examples {
    use super::*;

    #[test]
    fn runs_the_stated_cases_of_a_form_first() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let kept = Arc::clone(&seen);
        let seat = Recorder::new();
        let options = prop::examples([7_u8, 8])
            .example([9])
            .hermetic()
            .seed(1)
            .cases(1)
            .store("");
        prop::is_true(
            &seat,
            move |n: &u8| {
                kept.lock().expect("no case panicked").push(*n);
                true
            },
            "every byte passes",
            options,
        );
        assert_eq!(&seen.lock().expect("no case panicked")[..3], [7, 8, 9]);
        let using = prop::using(&prop::just(4_u8))
            .hermetic()
            .seed(1)
            .cases(1)
            .store("");
        prop::is_true(
            &seat,
            |n: &u8| *n == 4,
            "the generator of the setting makes 4",
            using,
        );
        let example = prop::example([5_u8]).hermetic().seed(1).cases(1).store("");
        prop::is_true(&seat, |_: &u8| true, "the example runs", example);
        assert!(!seat.failed(), "{}", seat.message());
    }
}
