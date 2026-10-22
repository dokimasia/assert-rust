// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `prop::generators`, the generators of the definition's
//! vocabulary.

use crate::helpers::{panic_of, records};
use dokimi_assert::prop::engine::choice::Choice;
use dokimi_assert::prop::engine::harness;
use dokimi_assert::prop::{self, Generator};
use dokimi_assert::{Collector, Recorder, Value, check};
use std::fmt::Debug;
use std::ops::Bound;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Returns the value that `generator` decodes from `choices`.
fn decoded<T: 'static>(generator: &Generator<T>, choices: Vec<Choice>) -> Option<T> {
    harness::decode(generator, choices).1
}

/// Returns the values of the first 30 random cases of the seed 3.
fn sampled<T: 'static>(generator: &Generator<T>) -> Vec<T> {
    (0..30)
        .filter_map(|index| harness::generate(generator, 3, index).1)
        .collect()
}

/// Returns the choices that decode to `value`, or the reason of the refusal,
/// the text of the refusal after its prefix `prop: `.
fn inverted<T: 'static>(generator: &Generator<T>, value: &Value) -> Result<Vec<Choice>, String> {
    harness::invert(generator, value)
        .map_err(|refused| refused.to_string().replacen("prop: ", "", 1))
}

/// Asserts that `value` inverts to choices that decode to `want`.
fn round_trips<T: PartialEq + Debug + 'static>(generator: &Generator<T>, value: &Value, want: &T) {
    let choices = inverted(generator, value).expect("the generator produces the value");
    assert_eq!(decoded(generator, choices).as_ref(), Some(want));
}

/// Returns an integer choice.
fn int(value: i128) -> Choice {
    Choice::Integer(value)
}

/// A call that a case expects to panic.
type Panicking = Box<dyn Fn()>;

mod integer {
    use super::*;

    #[test]
    fn decodes_the_integers_of_each_form_of_range() {
        assert!(
            sampled(&prop::integer(3..7_u8))
                .iter()
                .all(|n| (3..7).contains(n))
        );
        assert!(
            sampled(&prop::integer(-3..=3_i16))
                .iter()
                .all(|n| (-3..=3).contains(n))
        );
        let above = prop::integer((Bound::Excluded(250_u8), Bound::Unbounded));
        assert!(sampled(&above).iter().all(|n| *n > 250));
        let wide = prop::integer((Bound::Excluded(0_u128), Bound::Excluded(5_u128)));
        assert!(sampled(&wide).iter().all(|n| (1..5).contains(n)));
        assert_eq!(sampled(&prop::integer::<i128>(..)).len(), 30);
    }

    #[test]
    fn panics_for_a_range_without_a_value() {
        let tests: [(&str, Panicking); 3] = [
            ("empty", Box::new(|| drop(prop::integer(5..5_i32)))),
            (
                "past the largest",
                Box::new(|| drop(prop::integer((Bound::Excluded(u8::MAX), Bound::Unbounded)))),
            ),
            (
                "past the smallest",
                Box::new(|| drop(prop::integer(..i128::MIN))),
            ),
        ];
        for (name, give) in tests {
            assert_eq!(
                panic_of(give).as_deref(),
                Some("prop::integer: the range states no value"),
                "{name}"
            );
        }
    }

    #[test]
    fn inverts_a_value_to_its_choice() {
        round_trips(&prop::integer(0..=9_u8), &Value::Int(7), &7);
        round_trips(
            &prop::integer::<u128>(..),
            &Value::Uint(u128::MAX),
            &u128::MAX,
        );
        round_trips(&prop::integer::<i128>(..), &Value::Int(-5), &-5);
        assert_eq!(
            inverted(&prop::integer(0..=9_u8), &Value::from("7")),
            Err("the value is no integer".to_owned())
        );
        assert_eq!(
            inverted(&prop::integer::<i128>(..), &Value::Bool(true)),
            Err("the value is no integer".to_owned())
        );
        assert!(inverted(&prop::integer(0..=9_u8), &Value::Int(10)).is_err());
    }

    #[test]
    fn refuses_to_invert_a_wide_integer_outside_either_part_of_its_bounds() {
        let wide = prop::integer(0..=(1_i128 << 64) + 5);
        let outside = Err("the value is outside the bounds".to_owned());
        assert_eq!(inverted(&wide, &Value::Int(-1)), outside);
        assert_eq!(inverted(&wide, &Value::Int((1 << 64) + 10)), outside);
        round_trips(&wide, &Value::Int((1 << 64) + 5), &((1 << 64) + 5));
    }
}

mod float {
    use super::*;

    #[test]
    fn decodes_the_floats_of_each_form_of_range() {
        assert!(
            sampled(&prop::float(0.0..1.0_f64))
                .iter()
                .all(|x| (0.0..1.0).contains(x))
        );
        let above = prop::float((Bound::Excluded(0.0_f32), Bound::Included(1.0_f32)));
        assert!(sampled(&above).iter().all(|x| *x > 0.0 && *x <= 1.0));
        assert!(
            sampled(&prop::float::<f64>(..))
                .iter()
                .all(|x| x.is_finite())
        );
    }

    #[test]
    fn admits_nan_after_allow_nan() {
        let nan = prop::float::<f64>(..).allow_nan();
        let choices = inverted(&nan, &Value::Float(f64::NAN)).expect("the generator produces NaN");
        assert!(decoded(&nan, choices).is_some_and(f64::is_nan));
    }

    #[test]
    fn panics_for_a_range_without_a_value() {
        let raised = panic_of(|| drop(prop::float(1.0..1.0_f64))).expect("the range is empty");
        assert!(
            raised.starts_with("prop::float: the range from 1.0 to "),
            "{raised}"
        );
    }

    #[test]
    fn inverts_a_value_to_its_choice() {
        round_trips(&prop::float(0.0..=1.0_f64), &Value::Float(0.5), &0.5);
        assert_eq!(
            inverted(&prop::float(0.0..=1.0_f64), &Value::Int(1)),
            Err("the value is no float".to_owned())
        );
    }
}

mod boolean {
    use super::*;

    #[test]
    fn decodes_true_from_1_and_false_from_0() {
        assert_eq!(decoded(&prop::boolean(), vec![int(1)]), Some(true));
        assert_eq!(decoded(&prop::boolean(), vec![int(0)]), Some(false));
    }

    #[test]
    fn draws_true_at_the_stated_odds() {
        let never = prop::boolean().odds(0, 1);
        assert!(sampled(&never).iter().all(|held| !held));
        let always = prop::boolean().odds(1, 1);
        assert!(sampled(&always).iter().all(|held| *held));
    }

    #[test]
    fn panics_for_odds_that_are_no_probability() {
        assert_eq!(
            panic_of(|| drop(prop::boolean().odds(5, 4))).as_deref(),
            Some("prop: odds(5, 4) is no probability")
        );
    }

    #[test]
    fn inverts_a_value_to_its_choice() {
        round_trips(&prop::boolean(), &Value::Bool(true), &true);
        assert_eq!(
            inverted(&prop::boolean(), &Value::Int(1)),
            Err("the value is no boolean".to_owned())
        );
    }
}

mod just {
    use super::*;

    /// A value without a typed literal.
    #[derive(Debug, Clone, PartialEq)]
    pub(super) struct Token;

    #[test]
    fn decodes_its_value_without_a_choice() {
        assert_eq!(decoded(&prop::just(4_u8), Vec::new()), Some(4));
    }

    #[test]
    fn inverts_its_value_and_refuses_any_other() {
        round_trips(&prop::just(4_u8), &Value::Int(4), &4);
        assert_eq!(
            inverted(&prop::just(4_u8), &Value::Int(5)),
            Err("the value is not the generator's value".to_owned())
        );
        assert!(
            inverted(&prop::just(Token), &Value::Null)
                .is_err_and(|refused| refused.contains("just"))
        );
    }
}

mod sampled_from {
    use super::*;

    #[test]
    fn decodes_the_value_at_the_chosen_index() {
        assert_eq!(
            decoded(&prop::sampled_from(["a", "b", "c"]), vec![int(2)]),
            Some("c")
        );
    }

    #[test]
    fn inverts_a_value_to_its_index() {
        round_trips(&prop::sampled_from(["a", "b"]), &Value::from("b"), &"b");
        assert_eq!(
            inverted(&prop::sampled_from(["a", "b"]), &Value::from("z")),
            Err("the value is none of the values".to_owned())
        );
        assert!(inverted(&prop::sampled_from([just::Token]), &Value::Null).is_err());
    }

    #[test]
    fn panics_for_no_value() {
        assert_eq!(
            panic_of(|| drop(prop::sampled_from(Vec::<u8>::new()))).as_deref(),
            Some("prop::sampled_from: no value to sample from")
        );
    }
}

mod one_of {
    use super::*;

    #[test]
    fn decodes_a_value_of_the_chosen_generator() {
        let either = prop::one_of([prop::just(1_u8), prop::integer(5..=9_u8)]);
        assert_eq!(decoded(&either, vec![int(1), int(7)]), Some(7));
    }

    #[test]
    fn inverts_a_value_through_the_first_generator_that_produces_it() {
        let either = prop::one_of([prop::just(1_u8), prop::integer(5..=9_u8)]);
        round_trips(&either, &Value::Int(6), &6);
        assert!(inverted(&either, &Value::Int(3)).is_err());
    }

    #[test]
    fn panics_for_no_generator() {
        assert_eq!(
            panic_of(|| drop(prop::one_of(Vec::<Generator<u8>>::new()))).as_deref(),
            Some("prop::one_of: no generator to choose from")
        );
    }
}

mod optional {
    use super::*;

    #[test]
    fn decodes_none_from_an_absent_choice() {
        let maybe = prop::optional(prop::integer(0..=9_u8));
        assert_eq!(decoded(&maybe, vec![int(0)]), Some(None));
        assert_eq!(decoded(&maybe, vec![int(1), int(4)]), Some(Some(4)));
    }

    #[test]
    fn inverts_null_and_a_value() {
        let maybe = prop::optional(prop::integer(0..=9_u8));
        round_trips(&maybe, &Value::Null, &None);
        round_trips(&maybe, &Value::Int(3), &Some(3));
    }
}

mod list {
    use super::*;

    #[test]
    fn decodes_the_elements_until_a_flag_stops_it() {
        let digits = prop::list(prop::integer(0..=9_u8), ..);
        assert_eq!(
            decoded(&digits, vec![int(1), int(4), int(1), int(5), int(0)]),
            Some(vec![4, 5])
        );
    }

    #[test]
    fn keeps_unique_elements_after_unique() {
        let distinct = prop::list(prop::integer(0..=3_u8), ..=10).unique();
        for list in sampled(&distinct) {
            let mut sorted = list.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(sorted.len(), list.len());
        }
        let floats = prop::list(prop::float(0.0..=1.0_f64), ..=4).unique();
        assert!(sampled(&floats).iter().all(|list| list.len() <= 4));
    }

    #[test]
    fn compares_elements_without_a_typed_literal_by_their_equality() {
        /// A byte whose values have no typed literal.
        #[derive(Debug, Clone, PartialEq)]
        struct Tag(u8);
        let tags = prop::list(prop::integer(0..=2_u8).map(Tag), ..=5).unique();
        for list in sampled(&tags) {
            let distinct = list
                .iter()
                .enumerate()
                .all(|(at, tag)| !list[..at].contains(tag));
            assert!(distinct, "{list:?}");
        }
    }

    #[test]
    fn inverts_a_list_and_refuses_duplicates_of_a_unique_list() {
        let digits = prop::list(prop::integer(0..=9_u8), ..);
        round_trips(
            &digits,
            &Value::List(vec![Value::Int(1), Value::Int(2)]),
            &vec![1, 2],
        );
        assert_eq!(
            inverted(&digits, &Value::Int(1)),
            Err("the value is no list".to_owned())
        );
        let distinct = prop::list(prop::integer(0..=9_u8), ..).unique();
        assert!(inverted(&distinct, &Value::List(vec![Value::Int(1), Value::Int(1)])).is_err());
    }

    #[test]
    fn panics_for_sizes_without_a_length() {
        assert_eq!(
            panic_of(|| drop(prop::list(prop::boolean(), 3..3))).as_deref(),
            Some("prop::list: the sizes state no length")
        );
    }
}

mod dict {
    use super::*;

    #[test]
    fn decodes_entries_with_distinct_keys() {
        let map = prop::dict(prop::integer(0..=2_u8), prop::boolean(), ..=5);
        assert!(sampled(&map).iter().all(|map| map.len() <= 3));
    }

    #[test]
    fn inverts_a_map_and_refuses_another_value() {
        let map = prop::dict(prop::integer(0..=2_u8), prop::boolean(), ..);
        let entries = Value::Map(vec![(Value::Int(1), Value::Bool(true))]);
        let choices = inverted(&map, &entries).expect("the generator produces the map");
        assert_eq!(decoded(&map, choices).map(|map| map.len()), Some(1));
        assert_eq!(
            inverted(&map, &Value::Int(1)),
            Err("the value is no map".to_owned())
        );
        let twice = Value::Map(vec![
            (Value::Int(1), Value::Bool(true)),
            (Value::Int(1), Value::Bool(false)),
        ]);
        assert!(inverted(&map, &twice).is_err());
    }

    #[test]
    fn states_the_literal_of_a_drawn_map() {
        let t = Collector::new();
        let map = prop::dict(prop::integer(0..=2_u8), prop::boolean(), 1..=1);
        prop::for_all(
            &t,
            "a map has one entry",
            prop::hermetic().seed(1).cases(5),
            |c| {
                let drawn = c.draw(&map, "map");
                check::length(c, &drawn, 1, "the map has one entry");
            },
        );
    }
}

mod string {
    use super::*;

    #[test]
    fn decodes_the_characters_of_the_default_alphabet() {
        let text = prop::string(..);
        assert_eq!(
            decoded(&text, vec![Choice::Sequence(vec![0, 10])]),
            Some("0a".to_owned())
        );
    }

    #[test]
    fn decodes_the_characters_of_a_stated_alphabet() {
        let text = prop::string(..).alphabet("xy");
        assert_eq!(
            decoded(&text, vec![Choice::Sequence(vec![1, 0])]),
            Some("yx".to_owned())
        );
        round_trips(&text, &Value::from("xyx"), &"xyx".to_owned());
        assert_eq!(
            inverted(&text, &Value::from("z")),
            Err("the value has a character outside the alphabet".to_owned())
        );
        assert_eq!(
            inverted(&text, &Value::Int(1)),
            Err("the value is no string".to_owned())
        );
    }

    #[test]
    fn panics_for_an_alphabet_without_a_character_or_with_a_repeat() {
        assert_eq!(
            panic_of(|| drop(prop::string(..).alphabet(""))).as_deref(),
            Some("prop: alphabet(\"\") has no character")
        );
        assert_eq!(
            panic_of(|| drop(prop::string(..).alphabet("aba"))).as_deref(),
            Some("prop: alphabet repeats the character 'a'")
        );
    }

    #[test]
    fn inverts_a_string_of_the_default_alphabet() {
        round_trips(&prop::string(..), &Value::from("abc"), &"abc".to_owned());
    }
}

mod string_matching {
    use super::*;

    #[test]
    fn decodes_the_strings_that_the_pattern_matches() {
        let pattern = prop::string_matching("(ab|c)[x-z]{1,2}");
        for text in sampled(&pattern) {
            assert!(text.starts_with("ab") || text.starts_with('c'), "{text}");
        }
    }

    #[test]
    fn rejects_a_case_whose_repeat_passes_the_cap_on_choices() {
        let seat = Recorder::new();
        let pattern = prop::string_matching("[a-z]{5}");
        let options = prop::hermetic().seed(1).cases(5).max_choices(2);
        prop::for_all(&seat, "the text is drawn", options, |c| {
            let _ = c.draw(&pattern, "text");
        });
        assert_eq!(records(&seat)[0]["detail"]["outcome"], "rejected");
    }

    #[test]
    fn inverts_a_matching_string_and_refuses_another() {
        let pattern = prop::string_matching("a[0-9]+");
        round_trips(&pattern, &Value::from("a42"), &"a42".to_owned());
        assert_eq!(
            inverted(&pattern, &Value::from("b")),
            Err("the pattern does not match the value in full".to_owned())
        );
        assert_eq!(
            inverted(&pattern, &Value::Int(1)),
            Err("the value is no string".to_owned())
        );
    }

    #[test]
    fn panics_for_a_pattern_outside_the_portable_subset() {
        let raised =
            panic_of(|| drop(prop::string_matching("(?i)a"))).expect("the pattern is refused");
        assert!(raised.starts_with("prop::string_matching: "), "{raised}");
    }
}

mod bytes {
    use super::*;

    #[test]
    fn decodes_a_byte_of_each_element() {
        assert_eq!(
            decoded(&prop::bytes(..), vec![Choice::Sequence(vec![0, 255])]),
            Some(vec![0, 255])
        );
    }

    #[test]
    fn inverts_bytes_and_refuses_another_value() {
        round_trips(&prop::bytes(..), &Value::Bytes(vec![1, 2]), &vec![1, 2]);
        assert_eq!(
            inverted(&prop::bytes(..), &Value::Int(1)),
            Err("the value is no byte string".to_owned())
        );
    }
}

mod duration {
    use super::*;

    #[test]
    fn decodes_the_durations_of_each_form_of_range() {
        let second = Duration::from_secs(1);
        let within = prop::duration((Bound::Excluded(Duration::ZERO), Bound::Excluded(second)));
        assert!(
            sampled(&within)
                .iter()
                .all(|held| *held > Duration::ZERO && *held < second)
        );
        assert!(
            sampled(&prop::duration(..))
                .iter()
                .all(|held| held.as_nanos() <= u128::from(u64::MAX))
        );
        assert!(
            sampled(&prop::duration(second..=second))
                .iter()
                .all(|held| *held == second)
        );
    }

    #[test]
    fn panics_for_a_range_without_a_duration_of_64_bit_nanoseconds() {
        let message = Some("prop::duration: the range states no duration of 64-bit nanoseconds");
        assert_eq!(
            panic_of(|| drop(prop::duration(..Duration::ZERO))).as_deref(),
            message
        );
        assert_eq!(
            panic_of(|| drop(prop::duration(..=Duration::MAX))).as_deref(),
            message
        );
    }
}

mod permutation {
    use super::*;

    #[test]
    fn decodes_an_ordering_of_the_values() {
        assert_eq!(
            decoded(&prop::permutation([1, 2, 3]), vec![int(2), int(1)]),
            Some(vec![3, 2, 1])
        );
    }

    #[test]
    fn inverts_an_ordering_and_refuses_any_other_value() {
        let orders = prop::permutation([1_u8, 2, 3]);
        let reversed = Value::List(vec![Value::Int(3), Value::Int(2), Value::Int(1)]);
        round_trips(&orders, &reversed, &vec![3, 2, 1]);
        let refused = Err("the value is no permutation of the values".to_owned());
        assert_eq!(inverted(&orders, &Value::Int(1)), refused);
        assert_eq!(
            inverted(&orders, &Value::List(vec![Value::Int(1)])),
            refused
        );
        assert!(
            inverted(
                &orders,
                &Value::List(vec![Value::Int(1), Value::Int(1), Value::Int(1)])
            )
            .is_err()
        );
        assert!(inverted(&prop::permutation([just::Token]), &Value::List(Vec::new())).is_err());
        let other = Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(4)]);
        assert_eq!(inverted(&orders, &other), refused);
    }
}

mod recursive {
    use super::*;

    /// A tree of bytes.
    #[derive(Debug, Clone, PartialEq)]
    enum Tree {
        Leaf(u8),
        Node(Vec<Tree>),
    }

    /// Returns the number of leaves of a tree.
    fn leaves(tree: &Tree) -> usize {
        match tree {
            Tree::Leaf(_) => 1,
            Tree::Node(children) => children.iter().map(leaves).sum(),
        }
    }

    /// Returns the generator of trees.
    fn trees() -> Generator<Tree> {
        prop::recursive(prop::integer(..).map(Tree::Leaf), |tree| {
            prop::list(tree, ..=3).map(Tree::Node)
        })
    }

    #[test]
    fn decodes_the_base_or_the_extension() {
        assert_eq!(decoded(&trees(), vec![int(0), int(7)]), Some(Tree::Leaf(7)));
        let node = decoded(&trees(), vec![int(1), int(1), int(0), int(2), int(0)]);
        assert_eq!(node, Some(Tree::Node(vec![Tree::Leaf(2)])));
    }

    #[test]
    fn takes_the_base_at_every_position_after_max_leaves() {
        let choices = vec![
            int(1),
            int(1),
            int(0),
            int(5),
            int(1),
            int(1),
            int(7),
            int(0),
        ];
        let limited = decoded(&trees().max_leaves(1), choices.clone());
        assert_eq!(
            limited,
            Some(Tree::Node(vec![Tree::Leaf(5), Tree::Leaf(7)]))
        );
        let unlimited = decoded(&trees(), choices).expect("the choices decode a tree");
        assert!(leaves(&unlimited) >= 1);
        assert_ne!(Some(unlimited), limited);
    }

    #[test]
    fn states_a_tree_without_a_typed_literal_as_opaque() {
        let seat = Recorder::new();
        let tree = trees();
        prop::for_all(
            &seat,
            "the tree has no leaf",
            prop::hermetic().seed(1).store(""),
            |c| {
                let drawn = c.draw(&tree, "tree");
                check::is_true(c, leaves(&drawn) == 0, "the tree has no leaf");
            },
        );
        let value = &records(&seat)[0]["detail"]["counterexample"][0]["value"];
        assert_eq!(
            *value,
            serde_json::json!({"type": "opaque", "text": "Leaf(0)"})
        );
    }

    #[test]
    fn panics_for_max_leaves_of_0() {
        assert_eq!(
            panic_of(|| drop(trees().max_leaves(0))).as_deref(),
            Some("prop: max_leaves(0) admits no value")
        );
    }

    #[test]
    fn inverts_a_value_of_the_base_or_of_the_extension() {
        let numbers = prop::recursive(prop::integer(0..=9_u8), |inner| {
            inner.map(|n| n.saturating_add(10))
        });
        round_trips(&numbers, &Value::Int(4), &4);
        assert!(inverted(&numbers, &Value::Int(14)).is_err());
    }

    #[test]
    fn rejects_a_position_whose_recursive_generator_is_gone() {
        let kept = Arc::new(Mutex::new(None));
        let saved = Arc::clone(&kept);
        let numbers = prop::recursive(prop::integer(0..=9_u8), move |inner| {
            *saved.lock().expect("no draw panicked") = Some(inner.clone());
            inner
        });
        drop(numbers);
        let position = kept
            .lock()
            .expect("no draw panicked")
            .take()
            .expect("the extension kept its position");
        assert_eq!(decoded(&position, vec![int(0), int(1)]), None);
        assert_eq!(
            inverted(&position, &Value::Int(1)),
            Err("the recursive generator is gone".to_owned())
        );
        assert_eq!(sampled(&position), [] as [u8; 0]);
        let message = panic_of(|| drop(position.max_leaves(1)));
        assert_eq!(
            message.as_deref(),
            Some("prop: max_leaves applies to no generator of the kind self")
        );
    }

    #[test]
    fn inverts_a_value_of_a_position_inside_its_extension() {
        let nested = prop::recursive(prop::integer(0..=9_u8).valued(), |inner| {
            prop::list(inner, 1..=2).valued()
        });
        assert_eq!(format!("{nested:?}"), r#"Generator("recursive")"#);
        let value = Value::List(vec![Value::Int(1), Value::List(vec![Value::Int(2)])]);
        let choices = inverted(&nested, &value).expect("the value is a list of a base and a list");
        assert_eq!(decoded(&nested, choices), Some(value));
    }
}

mod composite {
    use super::*;

    #[test]
    fn decodes_the_value_that_its_function_returns_from_its_draws() {
        let pair = prop::composite(|c| {
            let n = c.draw(&prop::integer(0..=9_u8), "n");
            (n, c.draw(&prop::integer(n..=9_u8), "m"))
        });
        for (n, m) in sampled(&pair) {
            assert!(n <= m);
        }
    }

    #[test]
    fn ends_the_decode_at_a_draw_that_ends_the_case() {
        let rejected = prop::composite(|c| {
            c.assume(false);
            1_u8
        });
        assert_eq!(decoded(&rejected, Vec::new()), None);
    }

    #[test]
    fn passes_on_a_panic_of_its_function() {
        let failing = prop::composite(|_| -> u8 { panic!("the function fails") });
        assert_eq!(
            panic_of(|| _ = decoded(&failing, Vec::new())).as_deref(),
            Some("the function fails")
        );
    }
}

mod flags {
    use super::*;

    #[test]
    fn panics_for_a_flag_of_another_kind_of_generator() {
        let kinds: Vec<(&str, Panicking)> = vec![
            (
                "integer",
                Box::new(|| drop(prop::integer(0..=1_u8).max_leaves(1))),
            ),
            (
                "int",
                Box::new(|| drop(prop::integer::<i128>(..).max_leaves(1))),
            ),
            (
                "float",
                Box::new(|| drop(prop::float::<f64>(..).max_leaves(1))),
            ),
            ("boolean", Box::new(|| drop(prop::boolean().max_leaves(1)))),
            ("just", Box::new(|| drop(prop::just(1).max_leaves(1)))),
            (
                "sampled-from",
                Box::new(|| drop(prop::sampled_from([1]).max_leaves(1))),
            ),
            (
                "one-of",
                Box::new(|| drop(prop::one_of([prop::just(1)]).max_leaves(1))),
            ),
            (
                "optional",
                Box::new(|| drop(prop::optional(prop::just(1)).max_leaves(1))),
            ),
            (
                "list",
                Box::new(|| drop(prop::list(prop::just(1), ..).max_leaves(1))),
            ),
            (
                "dict",
                Box::new(|| drop(prop::dict(prop::just(1), prop::just(1), ..).max_leaves(1))),
            ),
            ("string", Box::new(|| drop(prop::string(..).max_leaves(1)))),
            (
                "string-matching",
                Box::new(|| drop(prop::string_matching("a").max_leaves(1))),
            ),
            ("bytes", Box::new(|| drop(prop::bytes(..).max_leaves(1)))),
            (
                "duration",
                Box::new(|| drop(prop::duration(..).max_leaves(1))),
            ),
            (
                "permutation",
                Box::new(|| drop(prop::permutation([1]).max_leaves(1))),
            ),
            (
                "map",
                Box::new(|| drop(prop::just(1).map(|n| n).max_leaves(1))),
            ),
            (
                "filter",
                Box::new(|| drop(prop::just(1).filter(|_| true).max_leaves(1))),
            ),
            (
                "bind",
                Box::new(|| drop(prop::just(1).bind(prop::just).max_leaves(1))),
            ),
            (
                "composite",
                Box::new(|| drop(prop::composite(|_| 1).max_leaves(1))),
            ),
        ];
        for (kind, give) in kinds {
            assert_eq!(
                panic_of(give).as_deref(),
                Some(
                    format!("prop: max_leaves applies to no generator of the kind {kind}").as_str()
                ),
                "{kind}"
            );
        }
    }
}
