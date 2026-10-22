//! The spec of `prop::options`, the settings of a property.

use crate::helpers::{panic_of, records};
use dokimi_assert::prop::{self, Options};
use dokimi_assert::{Recorder, check};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
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
        let tests: [(Options, &str); 12] = [
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

mod examples {
    use super::*;

    #[test]
    fn runs_the_stated_cases_of_a_form_first() {
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
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
