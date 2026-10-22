// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `prop::shape::nodes`: the generators of the structural shapes
//! and the budget of a recursive shape.

use crate::helpers::{panic_of, records};
use dokimi_assert::prop::engine::choice::Choice;
use dokimi_assert::prop::engine::harness;
use dokimi_assert::prop::{self, Generator};
use dokimi_assert::{Recorder, Value, check};
use serde_json::json;

/// Returns the generator of a shape file that reads.
fn read(text: &str) -> Generator<Value> {
    prop::of_shape(text).expect("the shape file reads")
}

/// Returns the reason why `generator` cannot produce `value`, after its prefix
/// `prop: `.
fn inverted(generator: &Generator<Value>, value: &Value) -> Result<Vec<Choice>, String> {
    harness::invert(generator, value)
        .map_err(|refused| refused.to_string().replacen("prop: ", "", 1))
}

/// Returns the message of the panic of the flag `max_leaves` on `generator`.
fn flag_refusal(generator: Generator<Value>) -> Option<String> {
    panic_of(|| drop(generator.max_leaves(3)))
}

/// The shape of a recursive enum: a node of one or more trees, or a leaf.
const TREE: &str = r#"{"shape": "ref", "name": "t", "definitions": {"t": {"shape": "enum", "variants": [
    ["node", {"shape": "list", "of": {"shape": "ref", "name": "t"}, "min_size": 1}],
    ["leaf", null]]}}}"#;

/// Returns the number of variants that a tree of `TREE` nests.
fn depth(tree: &Value) -> usize {
    match tree {
        Value::Variant {
            payload: Some(children),
            ..
        } => match children.as_ref() {
            Value::List(items) => 1 + items.iter().map(depth).max().unwrap_or(0),
            other => panic!("{other} is no list of trees"),
        },
        _ => 1,
    }
}

mod node {
    use super::*;

    #[test]
    fn names_its_kind_in_the_refusal_of_a_flag() {
        let tests = [
            (
                r#"{"shape": "ref", "name": "m", "definitions": {"m": {"shape": "optional", "of": {"shape": "ref", "name": "m"}}}}"#,
                "shape",
            ),
            (
                r#"{"shape": "ref", "name": "x", "definitions": {"x": {"shape": "bool"}}}"#,
                "ref",
            ),
            (
                r#"{"shape": "optional", "of": {"shape": "bool"}}"#,
                "optional",
            ),
            (r#"{"shape": "list", "of": {"shape": "bool"}}"#, "list"),
            (
                r#"{"shape": "map", "key": {"shape": "bool"}, "of": {"shape": "bool"}}"#,
                "dict",
            ),
            (
                r#"{"shape": "record", "fields": [["a", {"shape": "bool"}]]}"#,
                "record",
            ),
            (r#"{"shape": "enum", "variants": [["A", null]]}"#, "enum"),
            (r#"{"shape": "ip-address"}"#, "ip-address"),
        ];
        for (give, want) in tests {
            let message = flag_refusal(read(give));
            assert_eq!(
                message,
                Some(format!(
                    "prop: max_leaves applies to no generator of the kind {want}"
                ))
            );
        }
    }
}

mod reference {
    use super::*;

    #[test]
    fn states_the_value_of_its_definition_as_its_literal() {
        let generator =
            read(r#"{"shape": "ref", "name": "x", "definitions": {"x": {"shape": "bool"}}}"#);
        let seat = Recorder::new();
        prop::for_all(
            &seat,
            "a flag is set",
            prop::hermetic().seed(1).store("").shrink(0),
            |c| {
                let flag = c.draw(&generator, "flag");
                check::is_true(c, flag == Value::Bool(true), "a flag is set");
            },
        );
        let record = &records(&seat)[0];
        assert_eq!(
            record["detail"]["counterexample"][0]["value"],
            json!({"type": "bool", "value": false}),
            "{record}"
        );
    }
}

mod list {
    use super::*;

    #[test]
    fn refuses_to_invert_a_value_that_is_no_list() {
        let generator = read(r#"{"shape": "list", "of": {"shape": "bool"}}"#);
        assert_eq!(
            inverted(&generator, &Value::Int(1)),
            Err("the value is no list".to_owned())
        );
    }
}

mod map {
    use super::*;

    #[test]
    fn decodes_a_recursive_map_within_the_budget() {
        let generator = read(
            r#"{"shape": "ref", "name": "m", "definitions": {"m": {"shape": "map",
            "key": {"shape": "int", "width": 8, "signed": false}, "of": {"shape": "ref", "name": "m"}}}}"#,
        );
        let values: Vec<Value> = (0..20)
            .filter_map(|index| harness::generate(&generator, 9, index).1)
            .collect();
        assert_eq!(values.len(), 20);
    }
}

mod record {
    use super::*;

    #[test]
    fn refuses_to_invert_a_value_that_is_no_record_of_its_fields() {
        let generator = read(r#"{"shape": "record", "fields": [["a", {"shape": "bool"}]]}"#);
        assert_eq!(
            inverted(&generator, &Value::Int(1)),
            Err("the value is no record".to_owned())
        );
        let other = Value::Record(vec![("b".to_owned(), Value::Bool(true))]);
        assert_eq!(
            inverted(&generator, &other),
            Err("the value is no record of the shape's fields".to_owned())
        );
    }
}

mod enumeration {
    use super::*;

    #[test]
    fn inverts_a_variant_with_and_without_a_payload() {
        let generator =
            read(r#"{"shape": "enum", "variants": [["A", null], ["B", {"shape": "bool"}]]}"#);
        let unit = Value::Variant {
            name: "A".to_owned(),
            payload: None,
        };
        assert_eq!(inverted(&generator, &unit), Ok(vec![Choice::Integer(0)]));
        let carried = Value::Variant {
            name: "B".to_owned(),
            payload: Some(Box::new(Value::Bool(true))),
        };
        assert_eq!(
            inverted(&generator, &carried),
            Ok(vec![Choice::Integer(1), Choice::Integer(1)])
        );
    }

    #[test]
    fn refuses_to_invert_a_value_that_is_no_variant_of_the_enum() {
        let generator =
            read(r#"{"shape": "enum", "variants": [["A", null], ["B", {"shape": "bool"}]]}"#);
        let tests = [
            ("no variant", Value::Int(1), "the value is no variant"),
            (
                "an unknown variant",
                Value::Variant {
                    name: "C".to_owned(),
                    payload: None,
                },
                "the value is no variant of the enum",
            ),
            (
                "a payload of a unit variant",
                Value::Variant {
                    name: "A".to_owned(),
                    payload: Some(Box::new(Value::Bool(true))),
                },
                "the payload of the value is not the variant's",
            ),
        ];
        for (name, give, want) in tests {
            assert_eq!(inverted(&generator, &give), Err(want.to_owned()), "{name}");
        }
    }

    #[test]
    fn takes_the_exit_of_a_recursive_enum_once_the_budget_is_used() {
        let generator = read(TREE);
        let (_, tree) = harness::decode(&generator, vec![Choice::Integer(0); 1000]);
        assert_eq!(tree.as_ref().map(depth), Some(100));
    }
}

mod address {
    use super::*;

    #[test]
    fn inverts_an_address_of_either_version() {
        let generator = read(r#"{"shape": "ip-address"}"#);
        let choices =
            inverted(&generator, &Value::Bytes(vec![127, 0, 0, 1])).expect("the address inverts");
        assert_eq!(choices[0], Choice::Integer(0));
        let choices =
            inverted(&generator, &Value::Bytes(vec![0; 16])).expect("the address inverts");
        assert_eq!(choices[0], Choice::Integer(1));
    }

    #[test]
    fn refuses_to_invert_a_value_that_is_no_address() {
        let generator = read(r#"{"shape": "ip-address"}"#);
        assert_eq!(
            inverted(&generator, &Value::Int(1)),
            Err("the value is no address".to_owned())
        );
        assert_eq!(
            inverted(&generator, &Value::Bytes(vec![1; 5])),
            Err("the value is neither 4 nor 16 bytes".to_owned())
        );
    }
}
