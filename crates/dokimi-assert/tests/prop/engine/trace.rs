// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `prop::engine::trace`, a case stated as its draws and its
//! steps.

use crate::helpers::{one_byte, pairs};
use dokimi_assert::Recorder;
use dokimi_assert::prop::engine::trace::{Refusal, TraceError};
use dokimi_assert::prop::{self, Case, Generator};
use std::sync::{Mutex, PoisonError};

/// The draw entries of the digits 7 and 3 of a pair, before the entry of
/// the pair.
const DIGITS: &str = r#"{"label": "a", "value": {"type": "int", "value": 7}},
    {"label": "b", "value": {"type": "int", "value": 3}}"#;

/// The typed literal of the pair 7 and 3.
const PAIR: &str = r#"{"type": "list", "of": "int", "value": [7, 3]}"#;

/// Returns what `body` returns in the first case of a run of the draws
/// `draws`, and the recorder of the run.
fn first_case<T: Send>(
    draws: &str,
    body: impl Fn(&Case<'_>) -> T + Send + Sync,
) -> (Option<T>, Recorder) {
    let first = Mutex::new(None);
    let seat = Recorder::new();
    prop::for_all(
        &seat,
        "the draws decode",
        prop::draws(draws).hermetic().seed(1).cases(1),
        |c| {
            let returned = body(c);
            first
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get_or_insert(returned);
        },
    );
    (
        first.into_inner().unwrap_or_else(PoisonError::into_inner),
        seat,
    )
}

/// Returns the one-of of a byte string of one byte and the pair.
fn byte_or_pair() -> Generator<Vec<u8>> {
    prop::one_of([one_byte(), pairs()])
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
        let (pair, seat) = first_case(
            &format!(r#"[{DIGITS}, {{"label": "pair", "value": {PAIR}}}]"#),
            |c| c.draw(&pairs(), "pair"),
        );
        assert!(!seat.failed(), "{}", seat.message());
        assert_eq!(pair, Some(vec![7, 3]));
    }

    #[test]
    fn takes_the_entry_of_a_composite_by_its_label_alone() {
        let (pair, seat) = first_case(
            &format!(
                r#"[{DIGITS}, {{"label": "pair", "value": {{"type": "list", "of": "int", "value": [1, 1]}}}}]"#
            ),
            |c| c.draw(&pairs(), "pair"),
        );
        assert!(!seat.failed(), "{}", seat.message());
        assert_eq!(pair, Some(vec![7, 3]));
    }

    #[test]
    fn refuses_an_entry_of_another_label_at_the_end_of_a_composite() {
        let (_, seat) = first_case(
            &format!(r#"[{DIGITS}, {{"label": "other", "value": {PAIR}}}]"#),
            |c| c.draw(&pairs(), "pair"),
        );
        assert_eq!(
            seat.message(),
            r#"prop::for_all: draws[2].label: the draw labelled "pair" takes the entry labelled "other""#
        );
    }

    #[test]
    fn takes_no_entry_for_a_composite_past_the_last_entry() {
        let (pair, seat) = first_case(&format!("[{DIGITS}]"), |c| c.draw(&pairs(), "pair"));
        assert!(!seat.failed(), "{}", seat.message());
        assert_eq!(pair, Some(vec![7, 3]));
    }

    #[test]
    fn takes_the_entry_of_a_one_of_through_a_composite_after_the_entries_of_its_draws() {
        let (pair, seat) = first_case(
            &format!(r#"[{DIGITS}, {{"label": "pair", "value": {PAIR}}}]"#),
            |c| c.draw(&byte_or_pair(), "pair"),
        );
        assert!(!seat.failed(), "{}", seat.message());
        assert_eq!(pair, Some(vec![7, 3]));
    }

    #[test]
    fn takes_the_entry_of_an_alternative_without_a_composite_when_the_draw_starts() {
        let (drawn, seat) = first_case(
            r#"[{"label": "pair", "value": {"type": "bytes", "value": "07"}},
            {"label": "x", "value": {"type": "int", "value": 5}}]"#,
            |c| {
                let pair = c.draw(&prop::one_of([pairs(), one_byte()]), "pair");
                (pair, c.draw(&prop::integer(0..=9_u8), "x"))
            },
        );
        assert!(!seat.failed(), "{}", seat.message());
        assert_eq!(drawn, Some((vec![7], 5)));
    }

    #[test]
    fn takes_the_entries_of_a_list_of_composites_in_the_order_of_the_record() {
        let (pairs, seat) = first_case(
            &format!(
                r#"[{{"label": "a", "value": {{"type": "int", "value": 1}}}},
                {{"label": "b", "value": {{"type": "int", "value": 2}}}}, {DIGITS},
                {{"label": "pairs", "value": {{"type": "list", "items": [
                    {{"type": "list", "of": "int", "value": [1, 2]}}, {PAIR}]}}}}]"#
            ),
            |c| c.draw(&prop::list(pairs(), ..=3), "pairs"),
        );
        assert!(!seat.failed(), "{}", seat.message());
        assert_eq!(pairs, Some(vec![vec![1, 2], vec![7, 3]]));
    }

    #[test]
    fn takes_the_entry_of_a_present_optional_composite_after_its_draws() {
        let (maybe, seat) = first_case(
            &format!(r#"[{DIGITS}, {{"label": "maybe", "value": {PAIR}}}]"#),
            |c| c.draw(&prop::optional(pairs()), "maybe"),
        );
        assert!(!seat.failed(), "{}", seat.message());
        assert_eq!(maybe, Some(Some(vec![7, 3])));
    }

    #[test]
    fn takes_the_entry_of_an_absent_optional_composite_when_the_draw_starts() {
        let (maybe, seat) = first_case(
            r#"[{"label": "maybe", "value": {"type": "null"}},
            {"label": "x", "value": {"type": "int", "value": 5}}]"#,
            |c| {
                let maybe = c.draw(&prop::optional(pairs()), "maybe");
                (maybe, c.draw(&prop::integer(0..=9_u8), "x"))
            },
        );
        assert!(!seat.failed(), "{}", seat.message());
        assert_eq!(maybe, Some((None, 5)));
    }

    #[test]
    fn takes_the_entry_of_a_recursive_through_a_composite_after_its_draws() {
        let nested = prop::recursive(pairs(), |position| {
            prop::list(position, 1..=2).map(|lists| lists.concat())
        });
        let (pair, seat) = first_case(
            &format!(r#"[{DIGITS}, {{"label": "pair", "value": {PAIR}}}]"#),
            |c| c.draw(&nested, "pair"),
        );
        assert!(!seat.failed(), "{}", seat.message());
        assert_eq!(pair, Some(vec![7, 3]));
    }

    #[test]
    fn does_not_test_the_recorded_value_of_a_filter_of_a_composite() {
        let long = pairs().filter(|pair| pair.len() >= 2);
        let (pair, seat) = first_case(
            &format!(
                r#"[{DIGITS}, {{"label": "pair", "value": {{"type": "list", "of": "int", "value": [7]}}}}]"#
            ),
            |c| c.draw(&long, "pair"),
        );
        assert!(!seat.failed(), "{}", seat.message());
        assert_eq!(pair, Some(vec![7, 3]));
    }

    #[test]
    fn does_not_compare_the_composites_of_a_unique_list() {
        let (pairs, seat) = first_case(
            &format!(
                r#"[{DIGITS}, {DIGITS},
                {{"label": "pairs", "value": {{"type": "list", "items": [{PAIR}, {PAIR}]}}}}]"#
            ),
            |c| c.draw(&prop::list(pairs(), ..=3).unique(), "pairs"),
        );
        assert!(!seat.failed(), "{}", seat.message());
        assert_eq!(pairs, Some(vec![vec![7, 3]]));
    }

    #[test]
    fn refuses_the_value_of_a_dict_through_a_composite() {
        let maps = prop::dict(prop::integer(0..=9_u8), pairs(), ..=2);
        let (_, seat) = first_case(
            &format!(
                r#"[{DIGITS}, {{"label": "map", "value": {{"type": "map", "entries": [
                    [{{"type": "int", "value": 5}}, {PAIR}]]}}}}]"#
            ),
            |c| c.draw(&maps, "map"),
        );
        assert_eq!(
            seat.message(),
            r#"prop::for_all: draws[2].value: the generator of the draw labelled "map" does not produce the value of the entry"#
        );
    }

    #[test]
    fn refuses_the_next_entry_when_a_step_entry_comes_before_the_entry_of_the_draw() {
        let (_, seat) = first_case(
            &format!(r#"[{DIGITS}, {{"step": "put"}}, {{"label": "pair", "value": {PAIR}}}]"#),
            |c| c.draw(&byte_or_pair(), "pair"),
        );
        assert_eq!(
            seat.message(),
            r#"prop::for_all: draws[0].label: the draw labelled "pair" takes the entry labelled "a""#
        );
    }

    #[test]
    fn refuses_the_next_entry_when_no_entry_has_the_label_of_the_draw() {
        let (_, seat) = first_case(&format!("[{DIGITS}]"), |c| c.draw(&byte_or_pair(), "pair"));
        assert_eq!(
            seat.message(),
            r#"prop::for_all: draws[0].label: the draw labelled "pair" takes the entry labelled "a""#
        );
    }

    #[test]
    fn refuses_the_next_entry_when_a_later_entry_runs_backwards_without_a_composite() {
        let (_, seat) = first_case(
            r#"[{"label": "a", "value": {"type": "int", "value": 7}},
            {"label": "pair", "value": {"type": "bytes", "value": "07"}}]"#,
            |c| c.draw(&byte_or_pair(), "pair"),
        );
        assert_eq!(
            seat.message(),
            r#"prop::for_all: draws[0].label: the draw labelled "pair" takes the entry labelled "a""#
        );
    }

    #[test]
    fn refuses_an_entry_that_the_composite_inside_the_draw_leaves() {
        let (_, seat) = first_case(
            &format!(
                r#"[{DIGITS}, {{"label": "b", "value": {{"type": "int", "value": 4}}}},
                {{"label": "pair", "value": {PAIR}}}]"#
            ),
            |c| c.draw(&byte_or_pair(), "pair"),
        );
        assert_eq!(
            seat.message(),
            r#"prop::for_all: draws[2].label: the draw labelled "pair" takes the entry labelled "b""#
        );
    }

    #[test]
    fn refuses_the_entry_of_a_draw_that_a_draw_inside_it_takes() {
        let inner = prop::composite(|c| vec![c.draw(&prop::integer(0..=9_u8), "pair")]);
        let (_, seat) = first_case(
            r#"[{"label": "pair", "value": {"type": "int", "value": 5}},
            {"label": "pair", "value": {"type": "list", "of": "int", "value": [5]}}]"#,
            |c| c.draw(&prop::one_of([inner.clone()]), "pair"),
        );
        assert_eq!(
            seat.message(),
            r#"prop::for_all: draws[0].label: the draw labelled "pair" takes the entry labelled "pair""#
        );
    }
}
