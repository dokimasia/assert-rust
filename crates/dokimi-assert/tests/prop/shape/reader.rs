// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `prop::shape::reader`, the reader of shape files.

use dokimi_assert::Value;
use dokimi_assert::prop::engine::choice::Choice;
use dokimi_assert::prop::engine::harness;
use dokimi_assert::prop::{self, Generator};

/// Returns the generator of a shape file that reads.
fn read(text: &str) -> Generator<Value> {
    prop::of_shape(text).expect("the shape file reads")
}

/// Returns the reason of the refusal of a shape file, after its prefix
/// `prop: `.
fn refusal(text: &str) -> String {
    let refused = prop::of_shape(text).expect_err("the shape file does not read");
    refused.to_string().replacen("prop: ", "", 1)
}

/// Returns the values of the first 30 random cases of the seed 5.
fn sampled(generator: &Generator<Value>) -> Vec<Value> {
    (0..30)
        .filter_map(|index| harness::generate(generator, 5, index).1)
        .collect()
}

/// Returns the integer of an int value.
fn int(value: &Value) -> i128 {
    match value {
        Value::Int(held) => *held,
        other => panic!("{other} is no int"),
    }
}

mod of_shape {
    use super::*;

    #[test]
    fn refuses_a_file_whose_shape_does_not_read() {
        let tests = [
            ("a document of another kind", "[]", "[] is not a shape"),
            (
                "an unknown id",
                r#"{"shape": "tuple"}"#,
                r#"shape is "tuple", which is no shape"#,
            ),
            (
                "an id that is no text",
                r#"{"shape": 5}"#,
                "shape is 5, which is no shape",
            ),
            (
                "a key of no shape",
                r#"{"shape": "bool", "min": 1}"#,
                "shape.min does not apply to the bool shape",
            ),
            (
                "a missing key",
                r#"{"shape": "list"}"#,
                "shape states no of, which the list shape needs",
            ),
            (
                "a field that is no shape",
                r#"{"shape": "record", "fields": [["a", 1]]}"#,
                "shape.a is 1, not a shape",
            ),
        ];
        for (name, give, want) in tests {
            assert_eq!(refusal(give), want, "{name}");
        }
    }

    #[test]
    fn refuses_definitions_and_a_source_that_do_not_read() {
        let tests = [
            (
                "definitions that are no map",
                r#"{"shape": "bool", "definitions": []}"#,
                "definitions is not a map of names to shapes",
            ),
            (
                "a definition that is no shape",
                r#"{"shape": "bool", "definitions": {"x": 1}}"#,
                "definitions.x is 1, not a shape",
            ),
            (
                "a source without a type",
                r#"{"shape": "bool", "source": {"language": "rust"}}"#,
                r#"source is {"language":"rust"}, not a language and a type"#,
            ),
            (
                "a source that is no object",
                r#"{"shape": "bool", "source": "rust"}"#,
                r#"source is "rust", not a language and a type"#,
            ),
        ];
        for (name, give, want) in tests {
            assert_eq!(refusal(give), want, "{name}");
        }
    }

    #[test]
    fn refuses_a_ref_without_a_definition() {
        assert_eq!(
            refusal(r#"{"shape": "ref", "name": "y"}"#),
            r#"a ref names "y", which is no definition"#
        );
        assert_eq!(
            refusal(r#"{"shape": "ref", "name": 5}"#),
            r#"a ref names "5", which is no definition"#
        );
    }

    #[test]
    fn refuses_a_definition_without_a_finite_value() {
        let give = r#"{"shape": "ref", "name": "a", "definitions": {
            "a": {"shape": "record", "fields": [["next", {"shape": "ref", "name": "b"}]]},
            "b": {"shape": "record", "fields": [["back", {"shape": "ref", "name": "a"}]]}}}"#;
        assert_eq!(
            refusal(give),
            "definitions.a has no finite value, because it refers back without an optional, a list, a set, a map or an enum that can exit"
        );
    }

    #[test]
    fn refuses_names_that_do_not_read() {
        let tests = [
            (
                "no pair",
                r#"{"shape": "record", "fields": []}"#,
                "shape.fields is not a list of pairs",
            ),
            (
                "a pair without a shape",
                r#"{"shape": "record", "fields": [["a"]]}"#,
                r#"shape.fields has ["a"], not a name and a shape"#,
            ),
            (
                "a name twice",
                r#"{"shape": "record", "fields": [["a", {"shape": "bool"}], ["a", {"shape": "bool"}]]}"#,
                r#"shape.fields names "a" twice or not at all"#,
            ),
            (
                "an empty name",
                r#"{"shape": "enum", "variants": [["", null]]}"#,
                r#"shape.variants names "" twice or not at all"#,
            ),
            (
                "a variant without a payload in a definition that refers back",
                r#"{"shape": "ref", "name": "t", "definitions": {"t": {"shape": "enum", "variants": [["a"], ["b", {"shape": "ref", "name": "t"}]]}}}"#,
                r#"definitions.t.variants has ["a"], not a name and a shape"#,
            ),
        ];
        for (name, give, want) in tests {
            assert_eq!(refusal(give), want, "{name}");
        }
    }

    #[test]
    fn refuses_an_int_that_does_not_read() {
        let tests = [
            (
                "a width of no int",
                r#"{"shape": "int", "width": 7, "signed": true}"#,
                "shape states width 7 and signed true",
            ),
            (
                "a signedness of no boolean",
                r#"{"shape": "int", "width": 8, "signed": "yes"}"#,
                r#"shape states width 8 and signed "yes""#,
            ),
            (
                "a width that is text",
                r#"{"shape": "int", "width": "8", "signed": true}"#,
                r#"shape.width is "8", not an integer"#,
            ),
            (
                "empty bounds",
                r#"{"shape": "int", "width": 8, "signed": false, "min": 5, "max": 3}"#,
                "shape bounds [5, 3] are empty or outside [0, 255]",
            ),
            (
                "a safe bound as text",
                r#"{"shape": "int", "width": 8, "signed": false, "min": "5"}"#,
                r#"shape bound "5" is no integer"#,
            ),
            (
                "a bound in no canonical form",
                r#"{"shape": "int", "width": 64, "signed": true, "min": "+9007199254740993"}"#,
                r#"shape bound "+9007199254740993" is no integer"#,
            ),
            (
                "a wide bound that is no integer",
                r#"{"shape": "int", "width": 128, "signed": false, "min": "x"}"#,
                r#"shape bound "x" is no integer"#,
            ),
            (
                "a wide bound of another kind",
                r#"{"shape": "int", "width": 128, "signed": false, "min": true}"#,
                "shape bound true is no integer",
            ),
            (
                "empty wide bounds",
                r#"{"shape": "int", "width": 128, "signed": false, "min": 5, "max": 3}"#,
                "shape bounds [5, 3] are empty",
            ),
        ];
        for (name, give, want) in tests {
            assert_eq!(refusal(give), want, "{name}");
        }
    }

    #[test]
    fn reads_an_integer_bound_beyond_the_safe_range_as_its_text() {
        let low = read(
            r#"{"shape": "int", "width": 64, "signed": true, "min": "-9223372036854775808", "max": "-9007199254740993"}"#,
        );
        assert!(
            sampled(&low)
                .iter()
                .all(|value| int(value) <= -9_007_199_254_740_993)
        );
        let high = read(
            r#"{"shape": "int", "width": 128, "signed": false, "min": "340282366920938463463374607431768211454", "max": "340282366920938463463374607431768211455"}"#,
        );
        let values = sampled(&high);
        assert!(
            values.iter().all(|value| matches!(
                value,
                Value::Uint(u128::MAX | 340_282_366_920_938_463_463_374_607_431_768_211_454)
            )),
            "{values:?}"
        );
        let small = read(r#"{"shape": "int", "width": 128, "signed": false, "max": 3}"#);
        assert!(
            sampled(&small)
                .iter()
                .all(|value| (0..=3).contains(&int(value)))
        );
    }

    #[test]
    fn refuses_a_float_that_does_not_read() {
        let tests = [
            (
                "a width of no float",
                r#"{"shape": "float", "width": 16}"#,
                "shape.width is 16, not 32 or 64",
            ),
            (
                "a flag of no boolean",
                r#"{"shape": "float", "width": 64, "allow_nan": 1}"#,
                "shape.allow_nan is 1, not a boolean",
            ),
            (
                "a bound of other text",
                r#"{"shape": "float", "width": 64, "min": "x"}"#,
                r#"shape.min is "x", not a number"#,
            ),
            (
                "a bound of another kind",
                r#"{"shape": "float", "width": 64, "min": true}"#,
                "shape.min is true, not a number",
            ),
            (
                "the infinities between finite bounds",
                r#"{"shape": "float", "width": 64, "allow_infinity": true, "min": 0, "max": 1}"#,
                "shape allows the infinities and bounds out both",
            ),
            (
                "empty bounds",
                r#"{"shape": "float", "width": 64, "min": 2, "max": 1}"#,
                "shape: float bounds [2.0, 1.0] are empty",
            ),
            (
                "a bound of another width",
                r#"{"shape": "float", "width": 32, "min": 0.1}"#,
                "shape: float bound 0.1 is not a value of width 32",
            ),
        ];
        for (name, give, want) in tests {
            assert_eq!(refusal(give), want, "{name}");
        }
    }

    #[test]
    fn reads_an_infinite_bound_by_its_name() {
        let below = read(
            r#"{"shape": "float", "width": 64, "allow_infinity": true, "min": "-Inf", "max": -1}"#,
        );
        assert!(
            sampled(&below)
                .iter()
                .all(|value| matches!(value, Value::Float(x) if *x <= -1.0))
        );
        let above = read(
            r#"{"shape": "float", "width": 64, "allow_infinity": true, "min": 1, "max": "Inf"}"#,
        );
        assert!(
            sampled(&above)
                .iter()
                .all(|value| matches!(value, Value::Float(x) if *x >= 1.0))
        );
        assert_eq!(
            refusal(r#"{"shape": "float", "width": 64, "min": "NaN"}"#),
            "shape: float bounds [NaN, 1.7976931348623157e308] are empty"
        );
    }

    #[test]
    fn refuses_sizes_that_do_not_read() {
        let tests = [
            (
                "a size below 0",
                r#"{"shape": "list", "of": {"shape": "bool"}, "min_size": -1}"#,
                "shape.min_size is -1, below 0",
            ),
            (
                "a maximum below the minimum",
                r#"{"shape": "bytes", "min_size": 3, "max_size": 1}"#,
                "shape: max_size is below min_size",
            ),
            (
                "a fixed-list without a size",
                r#"{"shape": "fixed-list", "of": {"shape": "bool"}, "size": null}"#,
                "shape is a fixed-list and states no size",
            ),
        ];
        for (name, give, want) in tests {
            assert_eq!(refusal(give), want, "{name}");
        }
    }

    #[test]
    fn refuses_text_that_does_not_read() {
        let tests = [
            (
                "an empty alphabet",
                r#"{"shape": "char", "alphabet": ""}"#,
                r#"shape.alphabet is "", not characters"#,
            ),
            (
                "an alphabet that repeats",
                r#"{"shape": "string", "alphabet": "aa"}"#,
                "shape.alphabet repeats a character",
            ),
            (
                "a pattern with an alphabet",
                r#"{"shape": "string", "pattern": "a", "alphabet": "ab"}"#,
                "shape states a pattern and an alphabet or a size",
            ),
            (
                "a pattern that is no text",
                r#"{"shape": "string", "pattern": 1}"#,
                "shape.pattern is 1, not a pattern",
            ),
            (
                "a pattern outside the subset",
                r#"{"shape": "string", "pattern": "("}"#,
                r#"shape: pattern "(" at 1: a group is not closed"#,
            ),
        ];
        for (name, give, want) in tests {
            assert_eq!(refusal(give), want, "{name}");
        }
    }

    #[test]
    fn refuses_literals_and_addresses_that_do_not_read() {
        let tests = [
            (
                "no literal",
                r#"{"shape": "literal", "values": []}"#,
                "shape.values is not a list of literals",
            ),
            (
                "a literal that does not read",
                r#"{"shape": "literal", "values": [{"type": "int"}]}"#,
                r#"shape.values has {"type":"int"}, which is an int without a value"#,
            ),
            (
                "an unknown version",
                r#"{"shape": "ip-address", "version": 5}"#,
                "shape.version is 5, not 4 or 6",
            ),
        ];
        for (name, give, want) in tests {
            assert_eq!(refusal(give), want, "{name}");
        }
    }

    #[test]
    fn refuses_a_decimal_that_does_not_read() {
        let tests = [
            (
                "no scale",
                r#"{"shape": "decimal", "scale": null}"#,
                "shape is a decimal and states no scale",
            ),
            (
                "a scale beyond 32 bits",
                r#"{"shape": "decimal", "scale": 4294967296}"#,
                "shape.scale is too large",
            ),
            (
                "a bound of other text",
                r#"{"shape": "decimal", "scale": 2, "min": "1.x"}"#,
                r#"shape.min is "1.x", not a decimal"#,
            ),
            (
                "a bound of no text",
                r#"{"shape": "decimal", "scale": 2, "min": 1}"#,
                "shape.min is 1, not a decimal",
            ),
            (
                "a bound without digits",
                r#"{"shape": "decimal", "scale": 2, "max": "-.5"}"#,
                r#"shape.max is "-.5", not a decimal"#,
            ),
            (
                "empty bounds",
                r#"{"shape": "decimal", "scale": 2, "min": "2", "max": "1"}"#,
                "shape bounds [200, 100] are empty or outside [-9223372036854775808, 9223372036854775807]",
            ),
        ];
        for (name, give, want) in tests {
            assert_eq!(refusal(give), want, "{name}");
        }
    }

    #[test]
    fn rounds_a_decimal_bound_with_more_digits_inward() {
        let decimal = read(r#"{"shape": "decimal", "scale": 2, "min": "-1.234", "max": "1.239"}"#);
        assert!(harness::invert(&decimal, &Value::Int(-123)).is_ok());
        assert!(harness::invert(&decimal, &Value::Int(123)).is_ok());
        assert!(harness::invert(&decimal, &Value::Int(-124)).is_err());
        assert!(harness::invert(&decimal, &Value::Int(124)).is_err());
    }

    #[test]
    fn refuses_a_time_that_does_not_read() {
        let tests = [
            (
                "an unknown unit",
                r#"{"shape": "instant", "unit": "m"}"#,
                r#"shape.unit is "m", which is none of s, ms, us and ns"#,
            ),
            (
                "an instant without a zone",
                r#"{"shape": "instant", "unit": "s", "min": "2000-01-01T00:00:00"}"#,
                r#"shape bound "2000-01-01T00:00:00" is no instant in UTC"#,
            ),
            (
                "an instant finer than its unit",
                r#"{"shape": "instant", "unit": "s", "min": "2000-01-01T00:00:00.5Z"}"#,
                r#"shape bound "2000-01-01T00:00:00.5Z" is finer than its unit"#,
            ),
            (
                "instants in reverse",
                r#"{"shape": "instant", "unit": "s", "min": "2000-01-01T00:00:00Z", "max": "1999-01-01T00:00:00Z"}"#,
                "shape bounds (946684800, 0) to (915148800, 0) are empty or out of range",
            ),
            (
                "a local date and time in UTC",
                r#"{"shape": "local-date-time", "unit": "s", "min": "2000-01-01T00:00:00Z"}"#,
                r#"shape bound "2000-01-01T00:00:00Z" is no local date and time"#,
            ),
            (
                "a local date and time finer than its unit",
                r#"{"shape": "local-date-time", "unit": "s", "min": "2000-01-01T00:00:00.5"}"#,
                r#"shape bound "2000-01-01T00:00:00.5" is finer than its unit"#,
            ),
            (
                "no time of day",
                r#"{"shape": "time-of-day", "unit": "s", "min": "25:00:00"}"#,
                r#"shape bound "25:00:00" is no time of day"#,
            ),
            (
                "a time of day finer than its unit",
                r#"{"shape": "time-of-day", "unit": "s", "min": "10:00:00.5"}"#,
                r#"shape bound "10:00:00.5" is finer than its unit"#,
            ),
            (
                "no date",
                r#"{"shape": "date", "min": "2023-02-29"}"#,
                r#"shape bound "2023-02-29" is no date"#,
            ),
        ];
        for (name, give, want) in tests {
            assert_eq!(refusal(give), want, "{name}");
        }
    }

    #[test]
    fn builds_a_definition_once_for_each_ref_to_it() {
        let pair = read(
            r#"{"shape": "record", "fields": [["a", {"shape": "ref", "name": "x"}], ["b", {"shape": "ref", "name": "x"}]],
            "definitions": {"x": {"shape": "bool"}}}"#,
        );
        let (_, value) = harness::decode(&pair, vec![Choice::Integer(0), Choice::Integer(1)]);
        let want = Value::Record(vec![
            ("a".to_owned(), Value::Bool(false)),
            ("b".to_owned(), Value::Bool(true)),
        ]);
        assert_eq!(value, Some(want));
    }

    #[test]
    fn reads_definitions_that_refer_to_each_other() {
        let nested = read(
            r#"{"shape": "ref", "name": "a", "definitions": {
            "a": {"shape": "optional", "of": {"shape": "ref", "name": "b"}},
            "b": {"shape": "list", "of": {"shape": "ref", "name": "a"}}}}"#,
        );
        assert_eq!(sampled(&nested).len(), 30);
    }
}
