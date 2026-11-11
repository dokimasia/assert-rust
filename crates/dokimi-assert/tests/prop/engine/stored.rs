// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `prop::engine::stored`: the run of a stored case from its
//! choices, and from the counterexample of its entry as a trace when the
//! choices decode to another counterexample.

use crate::helpers::{Scratch, pairs, records};
use dokimi_assert::prop::{self, Case};
use dokimi_assert::stateful::{self, Action, Machine};
use dokimi_assert::{Recorder, Seat, check};
use std::sync::Arc;

/// The contract of the property of every spec.
const CONTRACT: &str = "the property holds";

/// The choices of a case of no choices, which take their targets.
const NO_CHOICES: &str = "prop1:";

/// The choices that decode to an `n` of 1 and of 5.
const ONE: &str = "prop1:AAE";
const FIVE: &str = "prop1:AAU";

/// The choices of the machine of [`booms`] that keep `tick` alone and take
/// one step of it.
const TICK: &str = "prop1:AAEAAAABAAAAAA";

/// Returns an entry of the property of the format `store`, whose choices are
/// `choices` and whose counterexample is `counterexample`.
fn entry(store: u8, choices: &str, counterexample: &str) -> String {
    format!(
        r#"{{"store": {store}, "definition": "16.3.0", "property": "{CONTRACT}",
        "identity": {{"assertion": "true", "file": "stored.rs", "line": 1}},
        "choices": "{choices}", "counterexample": {counterexample},
        "found": "2026-11-10"}}"#
    )
}

/// Runs `body` as the property with the seed 1, over a store that contains
/// `entry` alone, and returns the recorder.
fn run_stored(entry: &str, body: impl Fn(&Case<'_>) + Send + Sync) -> Recorder {
    let dir = Scratch::new();
    std::fs::write(dir.join("entry.json"), entry).expect("the store accepts a file");
    let seat = Recorder::new();
    prop::for_all(
        &seat,
        CONTRACT,
        prop::hermetic().seed(1).store(dir.join("")),
        body,
    );
    seat
}

/// The body of a property whose `n`, from 0 to 9, stays below 3.
fn below_three(c: &Case<'_>) {
    let n = c.draw(&prop::integer(0..=9_u8), "n");
    check::is_true(c, n < 3, "n stays below 3");
}

/// The body of a machine whose step `boom` fails the case, and whose step
/// `tick` does nothing.
fn booms(c: &Case<'_>) {
    let machine = Machine::<i64>::new([
        Action::new("tick", async |_: &Case<'_>, _: usize| {}),
        Action::new("boom", async |c: &Case<'_>, _: usize| {
            c.fail("the step fails");
        }),
    ]);
    stateful::steps(c, &machine, stateful::max(5));
}

/// Returns the text of the faults that a run noted.
fn noted(seat: &Recorder) -> String {
    seat.faults()
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Returns the outcome and the valid cases of the detail of the first call
/// record of `seat`.
fn ended(seat: &Recorder) -> (String, u64) {
    let detail = &records(seat)[0]["detail"];
    (
        detail["outcome"].as_str().unwrap_or_default().to_owned(),
        detail["cases"].as_u64().unwrap_or(u64::MAX),
    )
}

mod stored {
    use super::*;

    #[test]
    fn replays_the_counterexample_of_a_case_whose_choices_decode_to_other_values() {
        let seat = run_stored(
            &entry(
                1,
                ONE,
                r#"[{"label": "n", "value": {"type": "int", "value": 5}}]"#,
            ),
            below_three,
        );
        assert_eq!(ended(&seat), ("counterexample".to_owned(), 0));
        let notes = noted(&seat);
        assert!(
            notes.contains(
                "the stored case decodes to other values than it records, and the run replayed its counterexample"
            ),
            "{notes}"
        );
    }

    #[test]
    fn replays_the_steps_of_an_entry_of_format_2() {
        let seat = run_stored(&entry(2, NO_CHOICES, r#"[{"step": "boom"}]"#), booms);
        assert_eq!(ended(&seat), ("counterexample".to_owned(), 0));
        let detail = &records(&seat)[0]["detail"];
        assert_eq!(
            detail["counterexample"],
            serde_json::json!([{"step": "boom"}])
        );
    }

    #[test]
    fn replays_the_steps_of_an_entry_whose_choices_take_another_step() {
        let seat = run_stored(&entry(2, TICK, r#"[{"step": "boom"}]"#), booms);
        assert_eq!(ended(&seat), ("counterexample".to_owned(), 0));
        let detail = &records(&seat)[0]["detail"];
        assert_eq!(
            detail["counterexample"],
            serde_json::json!([{"step": "boom"}])
        );
    }

    #[test]
    fn replays_a_concurrent_step_of_an_entry_of_format_2() {
        let seat = run_stored(
            &entry(2, NO_CHOICES, r#"[{"step": "boom", "client": 1}]"#),
            |c| {
                let machine =
                    Machine::<i64>::new([Action::new("boom", async |c: &Case<'_>, _: usize| {
                        c.fail("the step fails");
                    })]);
                stateful::steps(c, &machine, stateful::clients(2));
            },
        );
        assert_eq!(ended(&seat), ("counterexample".to_owned(), 0));
        let notes = noted(&seat);
        assert!(
            notes.ends_with("and the run replayed its counterexample"),
            "{notes}"
        );
    }

    #[test]
    fn replays_a_composite_whose_entry_records_no_value() {
        let seat = run_stored(
            &entry(
                1,
                NO_CHOICES,
                r#"[{"label": "a", "value": {"type": "int", "value": 7}},
                {"label": "b", "value": {"type": "int", "value": 3}}, {"label": "pair"}]"#,
            ),
            |c| {
                let pair = c.draw(&pairs(), "pair");
                check::not_equal(c, pair, vec![7_u8, 3], "the pair is not 7 and 3");
            },
        );
        assert_eq!(ended(&seat), ("counterexample".to_owned(), 0));
    }

    #[test]
    fn ends_the_run_with_the_fault_of_the_case_of_the_trace() {
        let seat = run_stored(
            &entry(
                1,
                ONE,
                r#"[{"label": "n", "value": {"type": "int", "value": 5}}]"#,
            ),
            |c| {
                let n = c.draw(&prop::integer(0..=9_u8), "n");
                if n == 5 {
                    c.report_fault(Arc::new(std::io::Error::other("the case faults")), true);
                }
            },
        );
        assert_eq!(records(&seat)[0]["verdict"], "error");
        assert!(
            seat.message().ends_with("the case faults"),
            "{}",
            seat.message()
        );
    }

    #[test]
    fn counts_a_failing_case_of_the_choices_whatever_it_decodes_to() {
        let seat = run_stored(
            &entry(
                1,
                FIVE,
                r#"[{"label": "n", "value": {"type": "int", "value": 4}}]"#,
            ),
            below_three,
        );
        assert_eq!(ended(&seat), ("counterexample".to_owned(), 0));
        let notes = noted(&seat);
        assert!(
            notes.ends_with(
                "the stored case decodes to other values than it records, and the run tested those"
            ),
            "{notes}"
        );
    }

    #[test]
    fn notes_nothing_for_a_case_that_states_the_counterexample_of_its_entry() {
        let seat = run_stored(
            &entry(
                1,
                FIVE,
                r#"[{"label": "n", "value": {"type": "int", "value": 5}}]"#,
            ),
            below_three,
        );
        assert_eq!(ended(&seat), ("counterexample".to_owned(), 0));
        assert_eq!(noted(&seat), "");
    }

    #[test]
    fn tests_the_choices_when_the_body_refuses_an_entry_of_the_counterexample() {
        let seat = run_stored(
            &entry(
                1,
                ONE,
                r#"[{"label": "m", "value": {"type": "int", "value": 5}}]"#,
            ),
            below_three,
        );
        assert_ne!(ended(&seat).1, 0, "the run goes on past the stored case");
        let notes = noted(&seat);
        assert!(
            notes.ends_with(
                r#"the run tested those: counterexample[0].label: the draw labelled "n" takes the entry labelled "m""#
            ),
            "{notes}"
        );
    }

    #[test]
    fn tests_the_choices_when_a_draw_entry_of_the_counterexample_has_no_value() {
        let seat = run_stored(
            &entry(
                1,
                ONE,
                r#"[{"label": "n"}, {"label": "n", "value": {"type": "int", "value": 2}}]"#,
            ),
            below_three,
        );
        let notes = noted(&seat);
        assert!(
            notes.ends_with(
                r#"the run tested those: counterexample[0].value: the entry of the draw labelled "n" has no value"#
            ),
            "{notes}"
        );
    }

    #[test]
    fn tests_the_choices_when_the_case_of_the_trace_leaves_an_entry_untaken() {
        let seat = run_stored(
            &entry(
                1,
                ONE,
                r#"[{"label": "n", "value": {"type": "int", "value": 1}}, {"label": "n", "value": {"type": "int", "value": 2}}]"#,
            ),
            below_three,
        );
        let notes = noted(&seat);
        assert!(
            notes.ends_with(
                "the run tested those: counterexample[1]: the case leaves the entry untaken"
            ),
            "{notes}"
        );
    }
}
