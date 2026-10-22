//! The spec of `json`, the strict reader and the writer of JSON text.
//!
//! The library writes JSON in each call record and in a golden file of JSON,
//! and reads JSON from that golden file and from the text that the golden
//! assertion compares.

use crate::helpers::{Scratch, definition};
use dokimi_assert::{Recorder, check, golden};

/// Returns the message of the fault of a comparison of `got` with the field
/// `field` of a golden file that contains `golden`.
fn fault(golden_text: &str, got: &str) -> String {
    let dir = Scratch::new();
    let path = dir.join("golden.json");
    std::fs::write(&path, golden_text).expect("the scratch directory accepts a file");
    let seat = Recorder::new();
    golden::matches_json_field(&seat, &path, "field", got, false, &[]);
    seat.message()
}

/// Returns `true` when the field `field` of a golden file that contains
/// `golden` matches `got`.
fn matches(golden_text: &str, got: &str) -> bool {
    let dir = Scratch::new();
    let path = dir.join("golden.json");
    std::fs::write(&path, golden_text).expect("the scratch directory accepts a file");
    let seat = Recorder::new();
    golden::matches_json_field(&seat, &path, "field", got, false, &[]);
    !seat.failed()
}

mod write {
    use super::*;

    #[test]
    fn writes_a_call_record_on_one_line_without_spaces() {
        let seat = Recorder::new();
        check::length(&seat, "ab", 2, "the code has two characters");
        let line = &seat.records()[0];
        let start = format!(
            r#"{{"definition":"{}","seq":1,"assertion":"length","#,
            definition()
        );
        assert!(line.starts_with(&start), "{line}");
        assert!(!line.contains('\n'), "{line}");
    }

    #[test]
    fn writes_a_control_character_as_an_escape() {
        let seat = Recorder::new();
        check::is_true(&seat, true, "a\u{1}b\"c");
        let line = &seat.records()[0];
        assert!(line.contains(r#""contract":"a\u0001b\"c""#), "{line}");
    }

    #[test]
    fn writes_the_short_escape_of_each_control_character_that_has_one() {
        let seat = Recorder::new();
        check::is_true(&seat, true, "\\\n\r\t\u{8}\u{c}");
        let line = &seat.records()[0];
        assert!(line.contains(r#""contract":"\\\n\r\t\b\f""#), "{line}");
    }

    #[test]
    fn writes_a_float_that_reads_back_to_its_bits() {
        for x in [0.1_f64, 1e300, 5e-324, 1.0] {
            let seat = Recorder::new();
            check::close_to(&seat, x, 0.0, -1.0, "c");
            let record: serde_json::Value =
                serde_json::from_str(&seat.records()[0]).expect("a call record is JSON");
            let back = record["detail"]["got"]["value"]
                .as_f64()
                .expect("the record states a number");
            assert_eq!(back.to_bits(), x.to_bits(), "{x:?}");
        }
    }

    #[test]
    fn lays_out_a_golden_object_over_lines_indented_by_two_spaces() {
        let dir = Scratch::new();
        let path = dir.join("golden.json");
        let seat = Recorder::new();
        golden::matches_json_field(&seat, &path, "field", r#"{"a":[1,{}],"b":[]}"#, true, &[]);
        let written = std::fs::read_to_string(&path).expect("the update writes the golden file");
        assert_eq!(
            written,
            "{\n  \"field\": {\n    \"a\": [\n      1,\n      {}\n    ],\n    \"b\": []\n  }\n}\n"
        );
    }
}

mod read {
    use super::*;

    #[test]
    fn reads_an_integer_beyond_the_range_of_i64() {
        assert!(matches(
            r#"{"field": 18446744073709551615}"#,
            "18446744073709551615"
        ));
    }

    #[test]
    fn reads_minus_zero_as_a_float() {
        assert!(matches(r#"{"field": -0.0}"#, "-0"));
    }

    #[test]
    fn reads_an_escaped_surrogate_pair_as_one_character() {
        assert!(matches(r#"{"field": "\ud83d\ude00"}"#, "\"\u{1F600}\""));
    }

    #[test]
    fn refuses_a_name_that_repeats_within_an_object() {
        let message = fault(r#"{"field": 1, "field": 2}"#, "1");
        assert!(
            message.contains("a name that repeats within an object"),
            "{message}"
        );
    }

    #[test]
    fn refuses_a_nesting_deeper_than_512_levels() {
        let deep = format!("{}{}", "[".repeat(513), "]".repeat(513));
        let message = fault(r#"{"field": 1}"#, &deep);
        assert!(
            message.contains("a nesting deeper than the limit"),
            "{message}"
        );
        let shallow = format!("{}{}", "[".repeat(511), "]".repeat(511));
        assert!(!fault(r#"{"field": 1}"#, &shallow).contains("deeper"));
    }

    #[test]
    fn refuses_a_lone_surrogate() {
        let message = fault(r#"{"field": "\ud83d"}"#, "1");
        assert!(message.contains("a lone surrogate"), "{message}");
    }

    #[test]
    fn refuses_an_escape_with_a_sign_in_place_of_a_digit() {
        let message = fault(r#"{"field": "\u+123"}"#, "1");
        assert!(
            message.contains("an escape without four hexadecimal digits"),
            "{message}"
        );
    }

    #[test]
    fn refuses_text_that_is_no_json() {
        for text in ["NaN", "1 2", "01", "[1,]", "\"a\u{1}\""] {
            let message = fault(r#"{"field": 1}"#, text);
            assert!(message.contains("is no JSON"), "{text}: {message}");
        }
    }

    #[test]
    fn states_where_and_why_the_reader_stopped() {
        let deep = "{\"a\":".repeat(513);
        let tests = [
            ("no text", "", "the end of the text at byte 0"),
            (
                "no comma between elements",
                "[1 2]",
                "no comma or bracket after an element at byte 3",
            ),
            (
                "a member without a colon",
                r#"{"a" 1}"#,
                "no colon after a name at byte 5",
            ),
            (
                "no comma between members",
                r#"{"a":1 "b":2}"#,
                "no comma or brace after a member at byte 7",
            ),
            (
                "a member without a name",
                "{1:2}",
                "no name of a member at byte 1",
            ),
            (
                "a sign without digits",
                "-",
                "no digit in a number at byte 1",
            ),
            (
                "a point without digits",
                "1.",
                "no digit after a decimal point at byte 2",
            ),
            (
                "an exponent without digits",
                "1e",
                "no digit in an exponent at byte 2",
            ),
            (
                "a string without its end",
                "\"abc",
                "a string without its closing quote at byte 4",
            ),
            ("a word of no value", "tru", "no JSON value at byte 0"),
            (
                "objects nested too deep",
                &deep,
                "a nesting deeper than the limit at byte 2560",
            ),
        ];
        for (name, give, want) in tests {
            let message = fault(r#"{"field": 1}"#, give);
            assert!(message.contains(want), "{name}: {message}");
        }
    }

    #[test]
    fn reads_every_escape_of_a_string() {
        let text = r#""\"\\\/\b\f\n\r\t\u00e9""#;
        assert!(matches(&format!(r#"{{"field": {text}}}"#), text));
        let message = fault(r#"{"field": "\x"}"#, "1");
        assert!(message.contains("an unknown escape"), "{message}");
    }

    #[test]
    fn refuses_a_low_surrogate_without_its_high_surrogate() {
        let message = fault(r#"{"field": "\udc00"}"#, "1");
        assert!(message.contains("a lone surrogate"), "{message}");
        let message = fault(r#"{"field": "\ud83d\u0041"}"#, "1");
        assert!(message.contains("a lone surrogate"), "{message}");
    }

    #[test]
    fn reads_a_number_with_an_exponent_or_beyond_128_bits_as_a_float() {
        assert!(matches(r#"{"field": 1E+5}"#, "1e5"));
        assert!(matches(r#"{"field": 2.5e-1}"#, "0.25"));
        assert!(matches(
            r#"{"field": 100000000000000000000000000000000000000000}"#,
            "1e41"
        ));
    }
}
