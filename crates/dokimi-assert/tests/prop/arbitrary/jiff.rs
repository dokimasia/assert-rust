// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `prop::arbitrary::jiff`, the date and time types of jiff.

use super::{converts, generates, refusal, shape};
use ::jiff::civil::{Date, DateTime, Time};
use ::jiff::tz::{Offset, TimeZone};
use ::jiff::{SignedDuration, Timestamp, Zoned};
use dokimi_assert::Value;
use dokimi_assert::prop::Arbitrary;
use serde_json::json;

#[test]
fn states_the_shape_of_each_type() {
    let tests = [
        (
            shape::<Timestamp>(),
            json!({"shape": "instant", "unit": "ns", "max": "9999-12-30T22:00:00Z"}),
        ),
        (shape::<Date>(), json!({"shape": "date"})),
        (
            shape::<Time>(),
            json!({"shape": "time-of-day", "unit": "ns"}),
        ),
        (
            shape::<DateTime>(),
            json!({"shape": "local-date-time", "unit": "ns"}),
        ),
        (
            shape::<SignedDuration>(),
            json!({"shape": "duration", "unit": "ns"}),
        ),
        (shape::<Offset>(), json!({"shape": "offset"})),
        (shape::<TimeZone>(), json!({"shape": "zone"})),
        (
            shape::<Zoned>(),
            json!({"shape": "zoned-date-time", "unit": "ns"}),
        ),
    ];
    for (got, want) in tests {
        assert_eq!(got, want);
    }
}

#[test]
fn converts_each_type_to_its_native_value_and_back() {
    let second = Value::Int(1_000_000_000);
    converts(&Timestamp::from_second(1).expect("a timestamp"), &second);
    converts(&Date::constant(1970, 1, 2), &Value::Int(1));
    converts(&Time::constant(0, 0, 1, 0), &second);
    converts(&DateTime::constant(1970, 1, 1, 0, 0, 1, 0), &second);
    converts(&SignedDuration::from_secs(-1), &Value::Int(-1_000_000_000));
    converts(&Offset::constant(1), &Value::Int(3600));
    let zone = TimeZone::get("Europe/Amsterdam").expect("the database has the zone");
    converts(&zone, &Value::from("Europe/Amsterdam"));
    let zoned = Timestamp::from_second(1)
        .expect("a timestamp")
        .to_zoned(zone);
    converts(
        &zoned,
        &Value::Record(vec![
            ("instant".to_owned(), second),
            ("zone".to_owned(), Value::from("Europe/Amsterdam")),
        ]),
    );
}

#[test]
fn states_a_zone_without_a_name_as_null() {
    assert_eq!(TimeZone::fixed(Offset::constant(2)).to_value(), Value::Null);
}

#[test]
fn refuses_a_value_outside_each_type() {
    let huge = || Value::Int(i128::MAX);
    let tests = [
        (
            refusal::<Timestamp>(huge()),
            "an instant that a Timestamp states",
        ),
        (refusal::<Date>(huge()), "a date that a Date states"),
        (
            refusal::<Date>(Value::Int(10_000_000)),
            "a date that a Date states",
        ),
        (refusal::<Time>(Value::Int(-1)), "a time of day"),
        (
            refusal::<Time>(Value::Int(86_400_000_000_000)),
            "a time of day",
        ),
        (refusal::<Time>(huge()), "a time of day"),
        (refusal::<DateTime>(huge()), "a local date and time"),
        (
            refusal::<DateTime>(Value::Int(i128::from(i64::MAX) * 1_000_000_000)),
            "a local date and time that a DateTime states",
        ),
        (
            refusal::<SignedDuration>(huge()),
            "a duration that a SignedDuration states",
        ),
        (
            refusal::<Offset>(Value::Int(100_000)),
            "an offset that an Offset states",
        ),
        (
            refusal::<TimeZone>(Value::from("Mars/Olympus")),
            "a zone of jiff's time-zone database",
        ),
        (refusal::<TimeZone>(Value::Int(1)), "the name of a zone"),
        (refusal::<Zoned>(Value::Int(1)), "a record"),
    ];
    for (got, want) in tests {
        assert!(got.ends_with(want), "{got} ends with {want}");
    }
}

#[test]
fn generates_values_that_convert_back() {
    generates::<Timestamp>();
    generates::<Date>();
    generates::<Time>();
    generates::<DateTime>();
    generates::<SignedDuration>();
    generates::<Offset>();
    generates::<TimeZone>();
    generates::<Zoned>();
}
