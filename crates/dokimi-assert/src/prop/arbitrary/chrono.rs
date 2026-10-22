// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! [`Arbitrary`] for the date and time types of chrono, behind the feature
//! `chrono`.
//!
//! A [`DateTime<Utc>`] reads as an instant, a [`NaiveDate`] as a date, a
//! [`NaiveTime`] as a time of day, a [`NaiveDateTime`] as a local date and
//! time, a [`TimeDelta`] as a duration and a [`FixedOffset`] as an offset.

use super::{Arbitrary, Definitions, integer_of, mismatch};
use crate::prop::shape::{Shape, ShapeError};
use crate::value::Value;
use ::chrono::{
    DateTime, Datelike, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, TimeDelta, Timelike, Utc,
};

/// The nanoseconds of a second.
const NANOS_PER_SECOND: i128 = 1_000_000_000;

/// The days from 0001-01-01, which chrono counts as day 1, to 1970-01-01.
const EPOCH_DAYS_FROM_CE: i32 = 719_163;

/// Returns the whole seconds and the nanoseconds within the second of a
/// count of nanoseconds, or `None` beyond the seconds of an `i64`.
fn split(nanos: i128) -> Option<(i64, u32)> {
    let seconds = i64::try_from(nanos.div_euclid(NANOS_PER_SECOND)).ok()?;
    let within = u32::try_from(nanos.rem_euclid(NANOS_PER_SECOND)).ok()?;
    Some((seconds, within))
}

/// Returns the instant of a count of nanoseconds since the epoch.
fn instant_of(value: &Value) -> Result<DateTime<Utc>, ShapeError> {
    split(integer_of(value)?)
        .and_then(|(seconds, within)| DateTime::from_timestamp(seconds, within))
        .ok_or_else(|| mismatch("an instant that a DateTime states", value))
}

/// Returns the nanoseconds since the epoch of an instant.
fn nanos_of(instant: &DateTime<Utc>) -> Value {
    Value::Int(
        i128::from(instant.timestamp()) * NANOS_PER_SECOND
            + i128::from(instant.timestamp_subsec_nanos()),
    )
}

impl Arbitrary for DateTime<Utc> {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::instant()
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        instant_of(&value)
    }

    fn to_value(&self) -> Value {
        nanos_of(self)
    }
}

impl Arbitrary for NaiveDate {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::date()
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        i32::try_from(integer_of(&value)?)
            .ok()
            .and_then(|days| days.checked_add(EPOCH_DAYS_FROM_CE))
            .and_then(NaiveDate::from_num_days_from_ce_opt)
            .ok_or_else(|| mismatch("a date that a NaiveDate states", &value))
    }

    fn to_value(&self) -> Value {
        Value::from(self.num_days_from_ce() - EPOCH_DAYS_FROM_CE)
    }
}

impl Arbitrary for NaiveTime {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::time_of_day()
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        split(integer_of(&value)?)
            .and_then(|(seconds, within)| {
                let seconds = u32::try_from(seconds).ok()?;
                NaiveTime::from_num_seconds_from_midnight_opt(seconds, within)
            })
            .ok_or_else(|| mismatch("a time of day", &value))
    }

    fn to_value(&self) -> Value {
        Value::Int(
            i128::from(self.num_seconds_from_midnight()) * NANOS_PER_SECOND
                + i128::from(self.nanosecond()),
        )
    }
}

impl Arbitrary for NaiveDateTime {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::local_date_time()
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        instant_of(&value)
            .map(|instant| instant.naive_utc())
            .map_err(|_| mismatch("a local date and time that a NaiveDateTime states", &value))
    }

    fn to_value(&self) -> Value {
        nanos_of(&self.and_utc())
    }
}

impl Arbitrary for TimeDelta {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::duration()
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        split(integer_of(&value)?)
            .and_then(|(seconds, within)| TimeDelta::new(seconds, within))
            .ok_or_else(|| mismatch("a duration that a TimeDelta states", &value))
    }

    fn to_value(&self) -> Value {
        Value::Int(
            i128::from(self.num_seconds()) * NANOS_PER_SECOND + i128::from(self.subsec_nanos()),
        )
    }
}

impl Arbitrary for FixedOffset {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::offset()
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        i32::try_from(integer_of(&value)?)
            .ok()
            .and_then(FixedOffset::east_opt)
            .ok_or_else(|| mismatch("an offset that a FixedOffset states", &value))
    }

    fn to_value(&self) -> Value {
        Value::from(self.local_minus_utc())
    }
}
