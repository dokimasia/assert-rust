// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `prop::property`, the run of a property and its record.

use crate::helpers::{Scratch, panic_of, records};
use dokimi_assert::prop::{self, Arbitrary, Case, Definitions, Shape, ShapeError};
use dokimi_assert::{Recorder, Value, check};
use std::sync::atomic::{AtomicUsize, Ordering};

/// An identifier, whose values have no typed literal.
#[derive(Debug, Clone, PartialEq)]
struct Id(u8);

impl Arbitrary for Id {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::int(8, false)
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        u8::from_value(value).map(Id)
    }

    fn to_value(&self) -> Value {
        self.0.to_value()
    }
}

/// The contract of every property of the spec.
const CONTRACT: &str = "the property holds";

/// Returns the recorder of a run of `body` with the seed 1 under `options`.
fn run(options: prop::Options, body: impl Fn(&Case<'_>) + Send + Sync) -> Recorder {
    let seat = Recorder::new();
    prop::for_all(&seat, CONTRACT, options.hermetic().seed(1).store(""), body);
    seat
}

/// Draws `n` and `m` and fails when `n` is 3 or more.
fn drawing(c: &Case<'_>) {
    let n = c.draw(&prop::integer(0..=9_u8), "n");
    let _ = c.draw(&prop::integer(0..=9_u8), "m");
    check::is_true(c, n < 3, "n stays below 3");
}

mod draws {
    use super::*;

    #[test]
    fn runs_the_case_of_the_entries_first() {
        let entries = r#"[{"label": "n", "value": {"type": "int", "value": 7}}, {"label": "m", "value": {"type": "int", "value": 1}}]"#;
        let seat = run(prop::draws(entries).shrink(0), drawing);
        let detail = &records(&seat)[0]["detail"];
        assert_eq!(detail["cases"], 0);
        assert_eq!(detail["counterexample"][0]["value"]["value"], 7);
    }

    #[test]
    fn ends_the_call_with_a_fault_at_entries_that_it_cannot_read() {
        let tests = [
            (
                "{}",
                "prop::for_all: draws: the entries are not a JSON array of objects",
            ),
            (
                "[1]",
                "prop::for_all: draws: the entries are not a JSON array of objects",
            ),
            (
                r#"[{"step": "a", "client": "x"}]"#,
                r#"prop::for_all: draws[0].client: the client "x" is not an int of 0 or more"#,
            ),
            (
                r#"[{"step": "a", "client": -1}]"#,
                "prop::for_all: draws[0].client: the client -1 is not an int of 0 or more",
            ),
            (
                r#"[{"label": "n"}]"#,
                "prop::for_all: draws[0]: the entry has neither a step nor a label and a value",
            ),
        ];
        for (give, want) in tests {
            assert_eq!(run(prop::draws(give), drawing).message(), want, "{give}");
        }
        let refused = run(
            prop::draws(r#"[{"label": "n", "value": {"type": "nothing"}}]"#),
            drawing,
        )
        .message();
        assert!(
            refused
                .starts_with("prop::for_all: draws[0].value: the value is not a typed literal: "),
            "{refused}"
        );
    }

    #[test]
    fn ends_the_call_with_a_fault_at_an_entry_that_the_body_cannot_follow() {
        let tests = [
            (
                r#"[{"label": "n", "value": {"type": "int", "value": 70}}]"#,
                r#"prop::for_all: draws[0].value: the generator of the draw labelled "n" does not produce the value of the entry"#,
            ),
            (
                r#"[{"label": "m", "value": {"type": "int", "value": 1}}]"#,
                r#"prop::for_all: draws[0].label: the draw labelled "n" takes the entry labelled "m""#,
            ),
            (
                r#"[{"step": "go"}]"#,
                r#"prop::for_all: draws[0].label: the draw labelled "n" takes the step entry of "go""#,
            ),
        ];
        for (give, want) in tests {
            assert_eq!(run(prop::draws(give), drawing).message(), want, "{give}");
        }
    }
}

mod replay {
    use super::*;

    #[test]
    fn runs_the_one_case_of_the_token() {
        let failing = run(prop::replay("prop1:AAUAAQ"), drawing);
        let detail = &records(&failing)[0]["detail"];
        assert_eq!(detail["outcome"], "counterexample");
        assert_eq!(detail["choices"], "prop1:AAUAAQ");
        let passing = run(prop::replay("prop1:AAEAAQ"), drawing);
        assert_eq!(records(&passing)[0]["detail"]["cases"], 1);
    }

    #[test]
    fn ends_the_call_with_the_fault_of_the_case_of_the_token() {
        let options = prop::replay("prop1:AAUAAQ").around(|_| panic!("the wrapper fails"));
        let message = run(options, drawing).message();
        assert!(
            message.ends_with("around: the wrapper panicked: the wrapper fails"),
            "{message}"
        );
    }

    #[test]
    fn ends_the_call_with_a_fault_at_a_text_that_is_no_token() {
        let refused = run(prop::replay("nonsense"), drawing).message();
        assert!(
            refused.starts_with("prop::for_all: replay: the text is not a replay token: "),
            "{refused}"
        );
    }
}

mod around {
    use super::*;

    #[test]
    fn fails_the_case_of_a_body_that_fails_inside_the_wrapper() {
        let seat = run(prop::around(|run| run()), drawing);
        assert_eq!(records(&seat)[0]["detail"]["outcome"], "counterexample");
    }

    #[test]
    fn ends_the_call_with_a_fault_at_a_wrapper_that_breaks_its_contract() {
        let tests: [(prop::Options, &str); 3] = [
            (
                prop::around(|_| {}),
                "prop::for_all: around: the wrapper returned without calling run",
            ),
            (
                prop::around(|run| {
                    run();
                    run();
                }),
                "prop::for_all: around: the wrapper called run a second time",
            ),
            (
                prop::around(|_| panic!("the wrapper fails")),
                "prop::for_all: around: the wrapper panicked: the wrapper fails",
            ),
        ];
        for (give, want) in tests {
            assert_eq!(run(give, drawing).message(), want);
        }
    }
}

mod claim {
    use super::*;

    #[test]
    fn ends_the_call_with_a_fault_at_a_second_property_of_one_contract_and_store() {
        let dir = Scratch::new();
        let seat = Recorder::new();
        let options = || prop::hermetic().seed(1).store(dir.join("store"));
        prop::for_all(&seat, CONTRACT, options(), |c| {
            _ = c.draw(&prop::boolean(), "flag");
        });
        prop::for_all(&seat, CONTRACT, options(), |c| {
            _ = c.draw(&prop::boolean(), "flag");
        });
        assert!(
            seat.message().ends_with(&format!(
                "two properties of the test have the contract {CONTRACT:?}, and they would share their stored cases"
            )),
            "{}",
            seat.message()
        );
    }
}

mod for_all {
    use super::*;

    #[test]
    fn reports_a_pass_with_the_detail_of_its_run() {
        let seat = run(prop::cases(10), |c| {
            _ = c.draw(&prop::integer(0..=99_u8), "n");
        });
        let record = records(&seat).remove(0);
        assert_eq!(record["verdict"], "pass");
        assert_eq!(record["detail"]["cases"], 10);
    }

    #[test]
    fn runs_the_cases_on_more_than_one_worker() {
        let one = run(prop::Options::default(), drawing);
        let four = run(prop::workers(4), drawing);
        assert_eq!(records(&four)[0]["detail"], records(&one)[0]["detail"]);
    }
}

mod fuzz {
    use super::*;

    #[test]
    fn runs_the_case_of_each_input_of_the_fuzzer() {
        let seat = Recorder::new();
        let calls = AtomicUsize::new(0);
        let target = prop::fuzz(&seat, CONTRACT, prop::hermetic().store(""), |c| {
            calls.fetch_add(1, Ordering::SeqCst);
            let _ = c.draw(&prop::integer(0..=9_u8), "n");
        });
        target(&[0, 1, 2]);
        target(&[]);
        assert!(calls.load(Ordering::SeqCst) >= 2);
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn panics_with_the_sentence_of_a_failing_input() {
        let seat = Recorder::new();
        let target = prop::fuzz(&seat, CONTRACT, prop::hermetic().store(""), |c| {
            let n = c.draw(&prop::integer(0..=255_u8), "n");
            check::is_true(c, n < 3, "n stays below 3");
        });
        let raised = panic_of(|| target(&[200])).expect("the failing input panics");
        assert!(
            raised.starts_with("the property holds: counterexample after "),
            "{raised}"
        );
    }

    #[test]
    fn faults_every_input_of_a_property_that_ended_with_a_fault() {
        let seat = Recorder::new();
        let target = prop::fuzz(&seat, CONTRACT, prop::hermetic().replay("nonsense"), |_| {});
        assert!(
            seat.message().starts_with("prop::fuzz: replay: "),
            "{}",
            seat.message()
        );
        let raised = panic_of(|| target(&[1])).expect("an input of a faulted property panics");
        assert!(
            raised.ends_with("prop::fuzz: the property ended with a fault"),
            "{raised}"
        );
    }

    #[test]
    fn ends_the_call_with_a_fault_at_a_second_target_of_one_contract_and_store() {
        let dir = Scratch::new();
        let seat = Recorder::new();
        let options = || prop::hermetic().store(dir.join("store"));
        let _first = prop::fuzz(&seat, CONTRACT, options(), |c| {
            _ = c.draw(&prop::boolean(), "flag");
        });
        let _second = prop::fuzz(&seat, CONTRACT, options(), |c| {
            _ = c.draw(&prop::boolean(), "flag");
        });
        assert!(
            seat.message()
                .ends_with("and they would share their stored cases"),
            "{}",
            seat.message()
        );
    }

    #[test]
    fn ends_the_call_with_a_fault_at_a_damaged_store() {
        let dir = Scratch::new();
        std::fs::create_dir_all(dir.join("store")).expect("the scratch accepts a directory");
        std::fs::write(dir.join("store/damaged.json"), "not json")
            .expect("the store accepts a file");
        let seat = Recorder::new();
        let _target = prop::fuzz(
            &seat,
            CONTRACT,
            prop::hermetic().store(dir.join("store")),
            |c| {
                _ = c.draw(&prop::boolean(), "flag");
            },
        );
        assert!(
            seat.message()
                .ends_with("the store has damaged files: damaged.json"),
            "{}",
            seat.message()
        );
    }

    #[test]
    fn ends_the_call_with_a_fault_of_a_stored_case() {
        let dir = Scratch::new();
        let seat = Recorder::new();
        let store = || prop::hermetic().seed(1).store(dir.join("store"));
        prop::for_all(&seat, CONTRACT, store(), |c| {
            let n = c.draw(&prop::integer(0..=9_u8), "n");
            check::is_true(c, n < 3, "n stays below 3");
        });
        let replayed = Recorder::new();
        let failing = std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    let _target = prop::fuzz(
                        &replayed,
                        CONTRACT,
                        store().around(|_| panic!("the wrapper fails")),
                        |_| {},
                    );
                })
                .join()
        });
        assert!(failing.is_ok());
        assert!(
            replayed
                .message()
                .ends_with("around: the wrapper panicked: the wrapper fails"),
            "{}",
            replayed.message()
        );
    }

    #[test]
    fn reports_a_failing_stored_case_with_its_token() {
        let dir = Scratch::new();
        let store = || prop::hermetic().seed(1).store(dir.join("store"));
        let first = Recorder::new();
        prop::for_all(&first, CONTRACT, store(), drawing);
        let stored = records(&first)[0]["detail"]["choices"].clone();
        let replayed = Recorder::new();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                let _target = prop::fuzz(&replayed, CONTRACT, store(), drawing);
            });
        });
        let detail = &records(&replayed)[0]["detail"];
        assert_eq!(detail["outcome"], "counterexample", "{detail}");
        assert_eq!(detail["choices"], stored, "{detail}");
    }

    #[test]
    fn records_the_verdict_skip_for_a_stored_case_that_skips() {
        let dir = Scratch::new();
        let store = || prop::hermetic().seed(1).store(dir.join("store"));
        let first = Recorder::new();
        prop::for_all(&first, CONTRACT, store(), drawing);
        let replayed = Recorder::new();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                let target = prop::fuzz(&replayed, CONTRACT, store(), |c| {
                    let _ = c.draw(&prop::integer(0..=9_u8), "n");
                    c.skip("the case needs a capability");
                });
                target(&[1]);
            });
        });
        let record = &records(&replayed)[0];
        assert_eq!(record["verdict"], "skip", "{record}");
        assert_eq!(
            record["detail"]["reason"], "the case needs a capability",
            "{record}"
        );
    }

    #[test]
    fn panics_with_the_fault_of_an_input() {
        let seat = Recorder::new();
        let target = prop::fuzz(
            &seat,
            CONTRACT,
            prop::hermetic()
                .store("")
                .around(|_| panic!("the wrapper fails")),
            |c| {
                _ = c.draw(&prop::boolean(), "flag");
            },
        );
        let raised = panic_of(|| target(&[1])).expect("the faulted input panics");
        assert!(
            raised.ends_with("around: the wrapper panicked: the wrapper fails"),
            "{raised}"
        );
    }
}

mod save {
    use super::*;

    #[test]
    fn writes_no_entry_of_a_counterexample_of_values() {
        let dir = Scratch::new();
        let seat = Recorder::new();
        let generator = prop::integer(0..=255_u8).map(Id);
        let options = prop::examples([Id(200)])
            .using(&generator)
            .hermetic()
            .seed(1)
            .store(dir.join("store"));
        prop::is_true(
            &seat,
            |id: &Id| id.0 < 100,
            "the id stays below 100",
            options,
        );
        assert_eq!(records(&seat)[0]["detail"]["outcome"], "counterexample");
        assert!(
            std::fs::read_dir(dir.join("store")).is_err(),
            "the run writes no entry"
        );
    }
}
