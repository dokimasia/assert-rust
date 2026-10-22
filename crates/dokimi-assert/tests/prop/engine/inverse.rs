//! The spec of `prop::engine::inverse`, which runs a generator backwards to
//! the choices that decode to a value.

use dokimi_assert::Value;
use dokimi_assert::prop::engine::choice::Choice;
use dokimi_assert::prop::engine::harness;
use dokimi_assert::prop::{self, Generator};

/// Returns the choices that decode to `value`, or the reason of the refusal
/// after its prefix `prop: `.
fn inverted<T: 'static>(generator: &Generator<T>, value: &Value) -> Result<Vec<Choice>, String> {
    harness::invert(generator, value)
        .map_err(|refused| refused.to_string().replacen("prop: ", "", 1))
}

/// Returns the text of a string as a value.
fn text(value: &str) -> Value {
    Value::from(value)
}

mod invert {
    use super::*;

    #[test]
    fn refuses_a_collection_outside_its_sizes() {
        let lists = prop::list(prop::boolean(), 1..=2);
        let long = Value::List(vec![Value::Bool(true); 3]);
        assert_eq!(
            inverted(&lists, &long),
            Err("the items are outside the collection's sizes".to_owned())
        );
    }

    #[test]
    fn refuses_a_value_that_the_steps_decode_to_no_value() {
        let even = prop::integer(0..=9_u8).filter(|n| n % 2 == 0);
        assert_eq!(
            inverted(&even, &Value::Int(3)),
            Err("the value decodes to no value".to_owned())
        );
    }

    #[test]
    fn refuses_a_value_whose_steps_the_case_records_otherwise() {
        let nested = prop::recursive(prop::integer(0..=9_u8).valued(), |inner| {
            prop::list(inner, 1..=2).valued()
        });
        let wide = Value::List(vec![Value::Int(2), Value::List(vec![Value::Int(1)])]);
        assert_eq!(
            inverted(&nested.max_leaves(1), &wide),
            Err("the value is not a value that the generator produces".to_owned())
        );
    }
}

mod matched {
    use super::*;

    #[test]
    fn takes_the_first_alternative_that_matches_the_whole_text() {
        let words = prop::string_matching("^(?:ab|a)b$");
        let choices = inverted(&words, &text("ab")).expect("the pattern matches the text");
        assert_eq!(choices[0], Choice::Integer(1));
        assert_eq!(
            inverted(&words, &text("abc")),
            Err("the pattern does not match the value in full".to_owned())
        );
    }

    #[test]
    fn repeats_a_quantifier_as_often_as_the_rest_of_the_pattern_allows() {
        let words = prop::string_matching("^(?:a|)*b{2,3}$");
        assert!(inverted(&words, &text("aabb")).is_ok());
        assert!(inverted(&words, &text("b")).is_err());
    }

    #[test]
    fn finds_a_character_in_any_interval_of_a_class() {
        let words = prop::string_matching("^[a-cx-z]$");
        assert!(inverted(&words, &text("y")).is_ok());
        assert!(inverted(&words, &text("m")).is_err());
    }
}
