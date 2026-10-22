// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `prop::arbitrary::chrono`, the date and time types of chrono.

use super::{converts, generates, refusal, shape};
use ::chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, TimeDelta, Utc};
use dokimi_assert::Value;
use serde_json::json;

#[test]
fn states_the_shape_of_each_type() {
    let tests = [
        (
            shape::<DateTime<Utc>>(),
            json!({"shape": "instant", "unit": "ns"}),
        ),
        (shape::<NaiveDate>(), json!({"shape": "date"})),
        (
            shape::<NaiveTime>(),
            json!({"shape": "time-of-day", "unit": "ns"}),
        ),
        (
            shape::<NaiveDateTime>(),
            json!({"shape": "local-date-time", "unit": "ns"}),
        ),
        (
            shape::<TimeDelta>(),
            json!({"shape": "duration", "unit": "ns"}),
        ),
        (shape::<FixedOffset>(), json!({"shape": "offset"})),
    ];
    for (got, want) in tests {
        assert_eq!(got, want);
    }
}

#[test]
fn converts_each_type_to_its_native_value_and_back() {
    let second = Value::Int(1_000_000_000);
    let instant = DateTime::from_timestamp(1, 0).expect("an instant");
    converts(&instant, &second);
    converts(
        &NaiveDate::from_ymd_opt(1970, 1, 2).expect("a date"),
        &Value::Int(1),
    );
    converts(&NaiveTime::from_hms_opt(0, 0, 1).expect("a time"), &second);
    converts(&instant.naive_utc(), &second);
    converts(&TimeDelta::milliseconds(-1500), &Value::Int(-1_500_000_000));
    converts(
        &FixedOffset::east_opt(3600).expect("an offset"),
        &Value::Int(3600),
    );
}

#[test]
fn refuses_a_value_outside_each_type() {
    let huge = || Value::Int(i128::MAX);
    let tests = [
        (
            refusal::<DateTime<Utc>>(huge()),
            "an instant that a DateTime states",
        ),
        (
            refusal::<DateTime<Utc>>(Value::Int(i128::from(i64::MAX) * 1_000_000_000)),
            "an instant that a DateTime states",
        ),
        (
            refusal::<NaiveDate>(huge()),
            "a date that a NaiveDate states",
        ),
        (refusal::<NaiveTime>(Value::Int(-1)), "a time of day"),
        (
            refusal::<NaiveTime>(Value::Int(86_400_000_000_000)),
            "a time of day",
        ),
        (
            refusal::<NaiveDateTime>(huge()),
            "a local date and time that a NaiveDateTime states",
        ),
        (
            refusal::<TimeDelta>(huge()),
            "a duration that a TimeDelta states",
        ),
        (
            refusal::<FixedOffset>(Value::Int(100_000)),
            "an offset that a FixedOffset states",
        ),
    ];
    for (got, want) in tests {
        assert!(got.ends_with(want), "{got} ends with {want}");
    }
}

#[test]
fn generates_values_that_convert_back() {
    generates::<DateTime<Utc>>();
    generates::<NaiveDate>();
    generates::<NaiveTime>();
    generates::<NaiveDateTime>();
    generates::<TimeDelta>();
    generates::<FixedOffset>();
}
