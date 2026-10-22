//! The spec of `value`, the values that a failure and a call record state.

use crate::helpers::{Scratch, records};
use dokimi_assert::files::{self, Tree};
use dokimi_assert::prop::engine::choice::Choice;
use dokimi_assert::prop::engine::harness;
use dokimi_assert::prop::{self, Generator};
use dokimi_assert::{Recorder, Value, check};
use serde_json::json;
use std::fmt::Debug;

/// Returns the typed literal of the detail field `field` of the call record
/// of `call`, made on a new recorder.
fn literal(field: &str, call: impl FnOnce(&Recorder)) -> serde_json::Value {
    let seat = Recorder::new();
    call(&seat);
    records(&seat)[0]["detail"][field].clone()
}

/// Returns the typed literal of the simplest value of `generator`, as the
/// counterexample of a property that every value fails states it.
fn drawn<T: Debug + 'static>(generator: &Generator<T>) -> serde_json::Value {
    let seat = Recorder::new();
    prop::for_all(
        &seat,
        "no value passes",
        prop::hermetic().seed(1).store("").shrink(0),
        |c| {
            let _ = c.draw(generator, "value");
            check::is_true(c, false, "no value passes");
        },
    );
    records(&seat)[0]["detail"]["counterexample"][0]["value"].clone()
}

/// Returns the value that a literal shape over the one typed literal
/// `literal` decodes to, or the reason why the literal does not read.
fn read(literal: &serde_json::Value) -> Result<Value, String> {
    let file = json!({"shape": "literal", "values": [literal]}).to_string();
    match prop::of_shape(&file) {
        Ok(generator) => Ok(harness::decode(&generator, vec![Choice::Integer(0)])
            .1
            .expect("the literal decodes")),
        Err(refused) => {
            let text = refused.to_string();
            Err(text
                .split_once(", which is ")
                .map_or(text.clone(), |(_, reason)| reason.to_owned()))
        }
    }
}

mod literal {
    use super::*;

    #[test]
    fn writes_each_scalar_with_its_type() {
        assert_eq!(
            literal("got", |t| check::length(t, "ab", 3, "c")),
            json!({"type": "int", "value": 2})
        );
        assert_eq!(
            literal("got", |t| check::has_prefix(t, "ab", "b", "c")),
            json!({"type": "string", "value": "ab"})
        );
        assert_eq!(
            literal("tolerance", |t| check::close_to(t, 2.0, 0.0, 0.5, "c")),
            json!({"type": "float", "value": 0.5})
        );
        assert_eq!(
            literal("reason", |t| check::matches(t, "a", "b", "c")),
            json!({"type": "null"})
        );
        assert_eq!(
            literal("got", |t| check::equal(t, Some(1), None, "c")),
            json!({"type": "opaque", "text": "Some(1)"})
        );
    }

    #[test]
    fn writes_an_integer_beyond_two_to_the_53_as_its_decimal_text() {
        assert_eq!(
            literal("want", |t| check::length(t, "", 9_007_199_254_740_992, "c")),
            json!({"type": "int", "value": "9007199254740992"})
        );
        assert_eq!(
            literal("want", |t| check::length(t, "", 9_007_199_254_740_991, "c")),
            json!({"type": "int", "value": 9_007_199_254_740_991_i64})
        );
    }

    #[test]
    fn writes_an_unsigned_integer_beyond_i128_as_its_decimal_text() {
        assert_eq!(
            drawn(&prop::integer(u128::MAX..=u128::MAX)),
            json!({"type": "int", "value": "340282366920938463463374607431768211455"})
        );
    }

    #[test]
    fn writes_a_list_of_one_scalar_type_as_plain_values_under_of() {
        let tests = [
            (
                drawn(&prop::list(prop::boolean(), 1..=1)),
                json!({"type": "list", "of": "bool", "value": [false]}),
            ),
            (
                drawn(&prop::list(prop::float(0.0..=1.0_f64), 1..=1)),
                json!({"type": "list", "of": "float", "value": [0.0]}),
            ),
            (
                drawn(&prop::list(prop::string(..=0), 1..=1)),
                json!({"type": "list", "of": "string", "value": [""]}),
            ),
            (
                drawn(&prop::list(prop::integer(0..=1_u8), 1..=1)),
                json!({"type": "list", "of": "int", "value": [0]}),
            ),
        ];
        for (got, want) in tests {
            assert_eq!(got, want);
        }
    }

    #[test]
    fn writes_a_value_that_nests_beyond_61_levels_as_its_opaque_text() {
        let deep = (0..62).fold(Value::Null, |inner, _| Value::List(vec![inner]));
        let text = deep.to_string();
        assert_eq!(
            drawn(&prop::just(deep)),
            json!({"type": "opaque", "text": text})
        );
    }

    #[test]
    fn writes_nan_and_the_infinities_by_name() {
        let float = |x: f64| literal("got", |t| check::close_to(t, x, 0.0, 0.0, "c"));
        assert_eq!(float(f64::NAN), json!({"type": "float", "value": "NaN"}));
        assert_eq!(
            float(f64::INFINITY),
            json!({"type": "float", "value": "Inf"})
        );
        assert_eq!(
            float(f64::NEG_INFINITY),
            json!({"type": "float", "value": "-Inf"})
        );
    }
}

mod from_literal {
    use super::*;

    #[test]
    fn reads_each_scalar_literal() {
        let tests = [
            (
                "bytes",
                json!({"type": "bytes", "value": "0aff"}),
                Value::Bytes(vec![0x0a, 0xff]),
            ),
            (
                "an int as text",
                json!({"type": "int", "value": "-170141183460469231731687303715884105728"}),
                Value::Int(i128::MIN),
            ),
            (
                "an int beyond i128",
                json!({"type": "int", "value": "340282366920938463463374607431768211455"}),
                Value::Uint(u128::MAX),
            ),
            (
                "an infinity",
                json!({"type": "float", "value": "-Inf"}),
                Value::Float(f64::NEG_INFINITY),
            ),
            (
                "an opaque value",
                json!({"type": "opaque", "text": "Point"}),
                Value::Opaque("Point".to_owned()),
            ),
            ("a null", json!({"type": "null"}), Value::Null),
        ];
        for (name, give, want) in tests {
            assert_eq!(read(&give), Ok(want), "{name}");
        }
        assert!(
            matches!(read(&json!({"type": "float", "value": "NaN"})), Ok(Value::Float(x)) if x.is_nan())
        );
        assert_eq!(
            read(&json!({"type": "float", "value": "Inf"})),
            Ok(Value::Float(f64::INFINITY))
        );
    }

    #[test]
    fn reads_each_container_literal() {
        let pair = || Value::Map(vec![(Value::from("a"), Value::Int(1))]);
        let tests = [
            (
                "a list of bools",
                json!({"type": "list", "of": "bool", "value": [true]}),
                Value::List(vec![Value::Bool(true)]),
            ),
            (
                "a list of floats",
                json!({"type": "list", "of": "float", "value": [0.5, "Inf"]}),
                Value::List(vec![Value::Float(0.5), Value::Float(f64::INFINITY)]),
            ),
            (
                "a list of text",
                json!({"type": "list", "of": "string", "value": ["a"]}),
                Value::List(vec![Value::from("a")]),
            ),
            (
                "an absent list",
                json!({"type": "list", "of": "int", "value": null}),
                Value::Null,
            ),
            (
                "a list of items",
                json!({"type": "list", "items": [{"type": "null"}]}),
                Value::List(vec![Value::Null]),
            ),
            (
                "a map of entries",
                json!({"type": "map", "entries": [[{"type": "string", "value": "a"}, {"type": "int", "value": 1}]]}),
                pair(),
            ),
            (
                "a map of plain values",
                json!({"type": "map", "of": "int", "value": {"a": 1}}),
                pair(),
            ),
            (
                "an absent map",
                json!({"type": "map", "of": "int", "value": null}),
                Value::Null,
            ),
            (
                "a variant",
                json!({"type": "variant", "name": "Some", "payload": {"type": "int", "value": 1}}),
                Value::Variant {
                    name: "Some".to_owned(),
                    payload: Some(Box::new(Value::Int(1))),
                },
            ),
        ];
        for (name, give, want) in tests {
            assert_eq!(read(&give), Ok(want), "{name}");
        }
    }

    #[test]
    fn refuses_a_scalar_literal_that_does_not_state_a_value() {
        let tests = [
            ("no type", json!({"value": 1}), "a literal without a type"),
            (
                "an unknown type",
                json!({"type": "tuple"}),
                "a literal of a type that Rust does not read",
            ),
            (
                "a bool of no boolean",
                json!({"type": "bool", "value": 1}),
                "a bool without a boolean value",
            ),
            (
                "an int of other text",
                json!({"type": "int", "value": "x"}),
                "an int whose text is no integer",
            ),
            (
                "an int of another kind",
                json!({"type": "int", "value": true}),
                "an int whose value is no integer",
            ),
            (
                "a float without a value",
                json!({"type": "float"}),
                "a float without a value",
            ),
            (
                "a float of other text",
                json!({"type": "float", "value": "x"}),
                "a float whose text is not NaN, Inf or -Inf",
            ),
            (
                "a float of another kind",
                json!({"type": "float", "value": true}),
                "a float whose value is no number",
            ),
            (
                "a string of no text",
                json!({"type": "string", "value": 1}),
                "a string without a text value",
            ),
            (
                "bytes of no text",
                json!({"type": "bytes", "value": 1}),
                "bytes without a hexadecimal value",
            ),
            (
                "bytes in capitals",
                json!({"type": "bytes", "value": "0A"}),
                "bytes whose value is no lowercase hexadecimal text",
            ),
            (
                "an odd number of digits",
                json!({"type": "bytes", "value": "abc"}),
                "bytes whose value is no lowercase hexadecimal text",
            ),
            (
                "an opaque value without a text",
                json!({"type": "opaque"}),
                "an opaque literal without a text",
            ),
        ];
        for (name, give, want) in tests {
            assert_eq!(read(&give), Err(want.to_owned()), "{name}");
        }
    }

    #[test]
    fn refuses_a_compound_literal_that_does_not_state_a_value() {
        let tests = [
            (
                "a list of other bools",
                json!({"type": "list", "of": "bool", "value": [1]}),
                "a list of bool with an element that is no boolean",
            ),
            (
                "a list of other text",
                json!({"type": "list", "of": "string", "value": [1]}),
                "a list of string with an element that is no text",
            ),
            (
                "a list of another scalar",
                json!({"type": "list", "of": "char", "value": ["a"]}),
                "a scalar type that the encoding does not state",
            ),
            (
                "a list of items of no array",
                json!({"type": "list", "items": 1}),
                "a list whose items are no array",
            ),
            (
                "a list without items",
                json!({"type": "list"}),
                "a list without items or of",
            ),
            (
                "a list of a value of no array",
                json!({"type": "list", "of": "int", "value": 1}),
                "a list whose value is no array",
            ),
            (
                "a map of entries of no array",
                json!({"type": "map", "entries": 1}),
                "a map whose entries are no array",
            ),
            (
                "a map entry of no pair",
                json!({"type": "map", "entries": [1]}),
                "a map entry that is no pair",
            ),
            (
                "a map without entries",
                json!({"type": "map"}),
                "a map without entries or of",
            ),
            (
                "a map of a value of no object",
                json!({"type": "map", "of": "int", "value": 1}),
                "a map whose value is no object",
            ),
            (
                "a record without fields",
                json!({"type": "record"}),
                "a record without fields",
            ),
            (
                "a field of no pair",
                json!({"type": "record", "fields": [1]}),
                "a record field that is no name and value",
            ),
            (
                "a field twice",
                json!({"type": "record", "fields": [["a", {"type": "null"}], ["a", {"type": "null"}]]}),
                "a record that names a field twice",
            ),
            (
                "a variant without a name",
                json!({"type": "variant"}),
                "a variant without a name",
            ),
        ];
        for (name, give, want) in tests {
            assert_eq!(read(&give), Err(want.to_owned()), "{name}");
        }
    }
}

mod from_detail {
    use super::*;

    #[test]
    fn reads_a_literal_that_rust_does_not_read_as_the_record_of_its_members() {
        let dir = Scratch::new();
        let want = Tree::from([("a.txt", files::text("a\n"))]);
        let seat = Recorder::new();
        prop::for_all(
            &seat,
            "the tree is written",
            prop::hermetic().seed(1).store("").shrink(0),
            |c| {
                let _ = c.draw(&prop::boolean(), "flag");
                files::equal(c, dir.join(""), &want, "the tree is written");
            },
        );
        let failure = &seat.failures()[0];
        let Some((_, Value::Record(case))) =
            failure.detail().iter().find(|(name, _)| *name == "failure")
        else {
            panic!("the detail states the failure of the case: {failure:?}");
        };
        let Some((_, Value::Record(detail))) = case.iter().find(|(name, _)| name == "detail")
        else {
            panic!("the failure of the case states its detail: {case:?}");
        };
        let Some((_, Value::Record(tree))) = detail.iter().find(|(name, _)| name == "want") else {
            panic!("the detail of the case states the tree that it wants: {detail:?}");
        };
        assert_eq!(tree[0], ("type".to_owned(), Value::from("tree")));
    }
}

mod display {
    use super::*;

    #[test]
    fn renders_text_in_quotes_with_escapes() {
        assert_eq!(Value::from("a\"b\n").to_string(), r#""a\"b\n""#);
    }

    #[test]
    fn renders_an_unsigned_integer_as_its_digits() {
        assert_eq!(
            Value::Uint(u128::MAX).to_string(),
            "340282366920938463463374607431768211455"
        );
    }

    #[test]
    fn renders_a_float_with_its_fraction() {
        assert_eq!(Value::Float(2.0).to_string(), "2.0");
        assert_eq!(Value::Float(f64::NAN).to_string(), "NaN");
    }

    #[test]
    fn renders_containers_as_rust_writes_their_literals() {
        let record = Value::Record(vec![
            ("id".to_owned(), Value::Int(1)),
            ("tags".to_owned(), Value::List(vec![Value::from("a")])),
        ]);
        assert_eq!(record.to_string(), r#"{ id: 1, tags: ["a"] }"#);
        let map = Value::Map(vec![(Value::from("k"), Value::Bool(true))]);
        assert_eq!(map.to_string(), r#"{"k": true}"#);
        assert_eq!(Value::Bytes(b"a\x00".to_vec()).to_string(), r#"b"a\x00""#);
        assert_eq!(Value::Null.to_string(), "None");
        let variant = Value::Variant {
            name: "Some".to_owned(),
            payload: Some(Box::new(Value::Int(1))),
        };
        assert_eq!(variant.to_string(), "Some(1)");
        let bare = Value::Variant {
            name: "Pending".to_owned(),
            payload: None,
        };
        assert_eq!(bare.to_string(), "Pending");
        assert_eq!(
            Value::Opaque("Point { x: 1 }".to_owned()).to_string(),
            "Point { x: 1 }"
        );
    }

    #[test]
    fn renders_a_tree_as_the_expressions_of_its_entries() {
        let tree = Tree::from([
            ("a.txt", files::text("a\n")),
            ("keys", files::directory().with_mode(0o700)),
        ]);
        assert_eq!(
            Value::Tree(tree).to_string(),
            r#"{"a.txt": text("a\n"), "keys": directory().with_mode(0o700)}"#
        );
    }
}

mod from {
    use super::*;

    #[test]
    fn converts_the_values_of_rust_types() {
        assert_eq!(Value::from(3_u8), Value::Int(3));
        assert_eq!(Value::from(-3_isize), Value::Int(-3));
        assert_eq!(Value::from(3_usize), Value::Int(3));
        assert_eq!(Value::from(1.5_f32), Value::Float(1.5));
        assert_eq!(Value::from(0.25_f64), Value::Float(0.25));
        assert_eq!(Value::from(true), Value::Bool(true));
        assert_eq!(Value::from('x'), Value::from("x"));
        assert_eq!(Value::from(String::from("y")), Value::from("y"));
        assert_eq!(Value::from(&b"ab"[..]), Value::Bytes(b"ab".to_vec()));
        assert_eq!(Value::from(b"ab".to_vec()), Value::Bytes(b"ab".to_vec()));
        assert_eq!(Value::from(None::<i32>), Value::Null);
        assert_eq!(Value::from(Some(true)), Value::Bool(true));
        assert_eq!(
            Value::from(vec![Value::Null]),
            Value::List(vec![Value::Null])
        );
    }
}
