// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `prop::arbitrary::time`, the date and time types of time.

use super::{converts, generates, refusal, shape};
use ::time::{
    Date, Duration, Month, OffsetDateTime, PrimitiveDateTime, Time, UtcDateTime, UtcOffset,
};
use dokimi_assert::Value;
use serde_json::json;

#[test]
fn states_the_shape_of_each_type() {
    let tests = [
        (
            shape::<UtcDateTime>(),
            json!({"shape": "instant", "unit": "ns"}),
        ),
        (
            shape::<OffsetDateTime>(),
            json!({"shape": "instant", "unit": "ns"}),
        ),
        (shape::<Date>(), json!({"shape": "date"})),
        (
            shape::<Time>(),
            json!({"shape": "time-of-day", "unit": "ns"}),
        ),
        (
            shape::<PrimitiveDateTime>(),
            json!({"shape": "local-date-time", "unit": "ns"}),
        ),
        (
            shape::<Duration>(),
            json!({"shape": "duration", "unit": "ns"}),
        ),
        (shape::<UtcOffset>(), json!({"shape": "offset"})),
    ];
    for (got, want) in tests {
        assert_eq!(got, want);
    }
}

#[test]
fn converts_each_type_to_its_native_value_and_back() {
    let second = Value::Int(1_000_000_000);
    let instant = UtcDateTime::from_unix_timestamp(1).expect("an instant");
    converts(&instant, &second);
    converts(&OffsetDateTime::from(instant), &second);
    converts(
        &Date::from_calendar_date(1970, Month::January, 2).expect("a date"),
        &Value::Int(1),
    );
    converts(
        &Time::from_hms_nano(1, 2, 3, 4).expect("a time"),
        &Value::Int(3_723_000_000_004),
    );
    converts(
        &PrimitiveDateTime::new(instant.date(), instant.time()),
        &second,
    );
    converts(&Duration::milliseconds(-1500), &Value::Int(-1_500_000_000));
    converts(
        &UtcOffset::from_whole_seconds(3600).expect("an offset"),
        &Value::Int(3600),
    );
}

#[test]
fn refuses_a_value_outside_each_type() {
    let huge = || Value::Int(i128::MAX);
    let tests = [
        (
            refusal::<UtcDateTime>(huge()),
            "an instant that a UtcDateTime states",
        ),
        (
            refusal::<OffsetDateTime>(huge()),
            "an instant that a UtcDateTime states",
        ),
        (refusal::<Date>(huge()), "a date that a Date states"),
        (refusal::<Time>(Value::Int(-1)), "a time of day"),
        (
            refusal::<Time>(Value::Int(86_400_000_000_000)),
            "a time of day",
        ),
        (
            refusal::<PrimitiveDateTime>(huge()),
            "a local date and time that a PrimitiveDateTime states",
        ),
        (
            refusal::<Duration>(huge()),
            "a duration within the nanoseconds of an i64",
        ),
        (
            refusal::<UtcOffset>(Value::Int(100_000)),
            "an offset that a UtcOffset states",
        ),
    ];
    for (got, want) in tests {
        assert!(got.ends_with(want), "{got} ends with {want}");
    }
}

#[test]
fn generates_values_that_convert_back() {
    generates::<UtcDateTime>();
    generates::<OffsetDateTime>();
    generates::<Date>();
    generates::<Time>();
    generates::<PrimitiveDateTime>();
    generates::<Duration>();
    generates::<UtcOffset>();
}
