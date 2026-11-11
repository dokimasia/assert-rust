// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `prop::engine::trace`, a case stated as its draws and its
//! steps.

use crate::helpers::pairs;
use dokimi_assert::prop::engine::trace::{Refusal, TraceError};
use dokimi_assert::{Recorder, prop};
use std::sync::{Mutex, PoisonError};

/// The draw entries of the digits 7 and 3 of a pair, before the entry of
/// the pair.
const DIGITS: &str = r#"{"label": "a", "value": {"type": "int", "value": 7}},
    {"label": "b", "value": {"type": "int", "value": 3}}"#;

/// Returns the pair that the first case of a run of the draws `draws`
/// decodes under `pair`, and the recorder of the run.
fn first_pair(draws: &str) -> (Option<Vec<u8>>, Recorder) {
    let first = Mutex::new(None);
    let seat = Recorder::new();
    prop::for_all(
        &seat,
        "the pair decodes",
        prop::draws(draws).hermetic().seed(1).cases(1),
        |c| {
            let pair = c.draw(&pairs(), "pair");
            first
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get_or_insert(pair);
        },
    );
    (
        first.into_inner().unwrap_or_else(PoisonError::into_inner),
        seat,
    )
}

mod trace_error {
    use super::*;

    #[test]
    fn states_the_entry_the_name_and_the_refusal() {
        let error = TraceError {
            entry: 2,
            name: "n".to_owned(),
            refusal: Refusal::Value,
        };
        assert_eq!(error.to_string(), "prop: trace entry 2 (n): value");
    }
}

mod refusal {
    use super::*;

    #[test]
    fn names_each_refusal_as_a_vector_states_it() {
        let names: Vec<&str> = [Refusal::Label, Refusal::Value, Refusal::Step]
            .into_iter()
            .map(Refusal::name)
            .collect();
        assert_eq!(names, ["label", "value", "step"]);
    }
}

mod tracing {
    use super::*;

    #[test]
    fn takes_the_entry_of_a_composite_after_the_entries_of_its_draws() {
        let (pair, seat) = first_pair(&format!(
            r#"[{DIGITS}, {{"label": "pair", "value": {{"type": "list", "of": "int", "value": [7, 3]}}}}]"#
        ));
        assert!(!seat.failed(), "{}", seat.message());
        assert_eq!(pair, Some(vec![7, 3]));
    }

    #[test]
    fn takes_the_entry_of_a_composite_by_its_label_alone() {
        let (pair, seat) = first_pair(&format!(
            r#"[{DIGITS}, {{"label": "pair", "value": {{"type": "list", "of": "int", "value": [1, 1]}}}}]"#
        ));
        assert!(!seat.failed(), "{}", seat.message());
        assert_eq!(pair, Some(vec![7, 3]));
    }

    #[test]
    fn refuses_an_entry_of_another_label_at_the_end_of_a_composite() {
        let (_, seat) = first_pair(&format!(
            r#"[{DIGITS}, {{"label": "other", "value": {{"type": "list", "of": "int", "value": [7, 3]}}}}]"#
        ));
        assert_eq!(
            seat.message(),
            r#"prop::for_all: draws[2].label: the draw labelled "pair" takes the entry labelled "other""#
        );
    }

    #[test]
    fn takes_no_entry_for_a_composite_past_the_last_entry() {
        let (pair, seat) = first_pair(&format!("[{DIGITS}]"));
        assert!(!seat.failed(), "{}", seat.message());
        assert_eq!(pair, Some(vec![7, 3]));
    }
}
