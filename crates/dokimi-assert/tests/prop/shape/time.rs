// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `prop::shape::time`: the time shapes that make more than one
//! choice, and the text of a time shape's bounds.

use crate::helpers::{panic_of, records};
use dokimi_assert::prop::engine::choice::Choice;
use dokimi_assert::prop::engine::harness;
use dokimi_assert::prop::{self, Generator};
use dokimi_assert::{Recorder, Value};

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

/// Returns `true` when a shape of the id `kind` reads with the bound `min`.
fn admits(kind: &str, min: &str) -> bool {
    let unit = if kind == "date" {
        ""
    } else {
        r#", "unit": "ns""#
    };
    prop::of_shape(&format!(r#"{{"shape": "{kind}"{unit}, "min": "{min}"}}"#)).is_ok()
}

/// Returns the outcome of a run under a cap of one choice, whose cases draw
/// a flag when `flagged` is `true` and then a value of `generator`.
fn capped(generator: &Generator<Value>, flagged: bool) -> serde_json::Value {
    let seat = Recorder::new();
    let options = prop::hermetic().seed(1).cases(5).max_choices(1);
    prop::for_all(&seat, "the value is drawn", options, |c| {
        if flagged {
            let _ = c.draw(&prop::boolean(), "flag");
        }
        let _ = c.draw(generator, "value");
    });
    records(&seat)[0]["detail"]["outcome"].clone()
}

/// Returns a record of two parts.
fn parts(names: [&str; 2], values: [Value; 2]) -> Value {
    let [first, second] = values;
    Value::Record(vec![
        (names[0].to_owned(), first),
        (names[1].to_owned(), second),
    ])
}

mod node {
    use super::*;

    #[test]
    fn names_its_kind_in_the_refusal_of_a_flag() {
        for kind in ["instant", "local-date-time", "zoned-date-time", "wall-time"] {
            let generator = read(&format!(r#"{{"shape": "{kind}", "unit": "s"}}"#));
            let message = panic_of(|| drop(generator.max_leaves(3)));
            assert_eq!(
                message,
                Some(format!(
                    "prop: max_leaves applies to no generator of the kind {kind}"
                ))
            );
        }
    }
}

mod instant {
    use super::*;

    #[test]
    fn refuses_to_invert_a_value_that_is_no_instant_of_the_shape() {
        let seconds = read(
            r#"{"shape": "instant", "unit": "s", "min": "1970-01-01T00:00:00Z", "max": "1970-01-02T00:00:00Z"}"#,
        );
        let instant = |seconds: i128, units: i128| {
            parts(
                ["seconds", "units"],
                [Value::Int(seconds), Value::Int(units)],
            )
        };
        let tests = [
            ("no record", Value::Int(1), "the value is no record"),
            (
                "other parts",
                parts(["a", "b"], [Value::Int(1), Value::Int(0)]),
                "the value is no record of the shape's two parts",
            ),
            (
                "an instant outside the bounds",
                instant(-1, 0),
                "the value is outside the bounds",
            ),
            (
                "units of an instant at seconds",
                instant(1, 5),
                "an instant at seconds has no units",
            ),
        ];
        for (name, give, want) in tests {
            assert_eq!(inverted(&seconds, &give), Err(want.to_owned()), "{name}");
        }
        assert_eq!(
            inverted(&seconds, &instant(7, 0)),
            Ok(vec![Choice::Integer(7)])
        );
    }

    #[test]
    fn rejects_a_case_whose_units_pass_the_cap_on_choices() {
        let instant = read(r#"{"shape": "instant", "unit": "ms"}"#);
        assert_eq!(capped(&instant, false), "rejected");
    }
}

mod local {
    use super::*;

    #[test]
    fn refuses_to_invert_a_date_outside_the_bounds() {
        let local = read(
            r#"{"shape": "local-date-time", "unit": "s", "min": "1970-01-01T00:00:00", "max": "1970-01-02T00:00:00"}"#,
        );
        let value = parts(["date", "time-of-day"], [Value::Int(5), Value::Int(0)]);
        assert_eq!(
            inverted(&local, &value),
            Err("the value is outside the bounds".to_owned())
        );
    }
}

mod zoned {
    use super::*;

    #[test]
    fn refuses_to_invert_a_value_that_is_no_instant_in_a_zone_of_the_list() {
        let zoned = read(r#"{"shape": "zoned-date-time", "unit": "s"}"#);
        let instant = parts(["seconds", "units"], [Value::Int(0), Value::Int(0)]);
        let tests = [
            ("no record", Value::Int(1), "the value is no record"),
            (
                "other parts",
                parts(["a", "zone"], [instant.clone(), Value::from("UTC")]),
                "the value is no record of a value and a zone",
            ),
            (
                "an unknown zone",
                parts(
                    ["instant", "zone"],
                    [instant.clone(), Value::from("Mars/Olympus")],
                ),
                "the value's zone is no zone of the list",
            ),
        ];
        for (name, give, want) in tests {
            assert_eq!(inverted(&zoned, &give), Err(want.to_owned()), "{name}");
        }
        let utc = parts(["instant", "zone"], [instant, Value::from("UTC")]);
        assert!(inverted(&zoned, &utc).is_ok());
    }

    #[test]
    fn rejects_a_case_whose_zone_passes_the_cap_on_choices() {
        let zoned = read(r#"{"shape": "zoned-date-time", "unit": "s"}"#);
        assert_eq!(capped(&zoned, true), "rejected");
    }
}

mod wall {
    use super::*;

    #[test]
    fn inverts_a_wall_time_in_a_zone_with_offset_changes() {
        let wall = read(r#"{"shape": "wall-time", "unit": "s"}"#);
        let local = parts(["date", "time-of-day"], [Value::Int(0), Value::Int(0)]);
        let value = parts(
            ["local-date-time", "zone"],
            [local, Value::from("Europe/Amsterdam")],
        );
        let choices = inverted(&wall, &value).expect("the wall time inverts");
        assert_eq!(choices[..2], [Choice::Integer(1), Choice::Integer(0)]);
    }
}

mod date {
    use super::*;

    #[test]
    fn reads_a_date_of_the_proleptic_gregorian_calendar() {
        let tests = [
            ("a leap day", "2024-02-29", true),
            ("the last day of a short month", "2023-04-30", true),
            ("a leap day of a common year", "2023-02-29", false),
            ("the 31st of a short month", "2023-04-31", false),
            ("a 13th month", "2023-13-01", false),
            ("a day 0", "2023-01-00", false),
            ("the year 0", "0000-01-01", false),
            ("a fourth part", "2023-01-01-01", false),
            ("a year of two digits", "23-01-01", false),
        ];
        for (name, give, want) in tests {
            assert_eq!(admits("date", give), want, "{name}");
        }
    }
}

mod time {
    use super::*;

    #[test]
    fn reads_a_time_of_day_with_up_to_nine_digits_of_a_fraction() {
        let tests = [
            ("nine digits", "10:00:00.123456789", true),
            ("ten digits", "10:00:00.1234567890", false),
            ("a point without digits", "10:00:00.", false),
            ("a fraction of no digits", "10:00:00.a", false),
            ("a fourth part", "10:00:00:00", false),
            ("hour 24", "24:00:00", false),
            ("minute 60", "10:60:00", false),
            ("second 60", "10:00:60", false),
        ];
        for (name, give, want) in tests {
            assert_eq!(admits("time-of-day", give), want, "{name}");
        }
    }
}
