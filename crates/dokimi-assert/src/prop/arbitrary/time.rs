//! [`Arbitrary`] for the date and time types of time, behind the feature
//! `time`.
//!
//! A [`UtcDateTime`] reads as an instant, and so does an [`OffsetDateTime`],
//! whose values are in UTC. A [`Date`] reads as a date, a [`Time`] as a time
//! of day, a [`PrimitiveDateTime`] as a local date and time, a [`Duration`]
//! as a duration and a [`UtcOffset`] as an offset.

use super::{Arbitrary, Definitions, integer_of, mismatch};
use crate::prop::shape::{Shape, ShapeError};
use crate::value::Value;
use ::time::{Date, Duration, OffsetDateTime, PrimitiveDateTime, Time, UtcDateTime, UtcOffset};

/// The nanoseconds of a second, a minute and an hour.
const NANOS_PER_SECOND: i128 = 1_000_000_000;
const NANOS_PER_MINUTE: i128 = 60 * NANOS_PER_SECOND;
const NANOS_PER_HOUR: i128 = 60 * NANOS_PER_MINUTE;

/// The Julian day of 1970-01-01.
const EPOCH_JULIAN_DAY: i32 = 2_440_588;

/// Returns the instant of a count of nanoseconds since the epoch.
fn instant_of(value: &Value) -> Result<UtcDateTime, ShapeError> {
    UtcDateTime::from_unix_timestamp_nanos(integer_of(value)?)
        .map_err(|_| mismatch("an instant that a UtcDateTime states", value))
}

impl Arbitrary for UtcDateTime {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::instant()
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        instant_of(&value)
    }

    fn to_value(&self) -> Value {
        Value::Int(self.unix_timestamp_nanos())
    }
}

/// A date and time with an offset reads as its instant, and a value that a
/// generator decodes has the offset UTC.
impl Arbitrary for OffsetDateTime {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::instant()
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        instant_of(&value).map(OffsetDateTime::from)
    }

    fn to_value(&self) -> Value {
        Value::Int(self.unix_timestamp_nanos())
    }
}

impl Arbitrary for Date {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::date()
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        i32::try_from(integer_of(&value)?)
            .ok()
            .and_then(|days| days.checked_add(EPOCH_JULIAN_DAY))
            .and_then(|day| Date::from_julian_day(day).ok())
            .ok_or_else(|| mismatch("a date that a Date states", &value))
    }

    fn to_value(&self) -> Value {
        Value::from(self.to_julian_day() - EPOCH_JULIAN_DAY)
    }
}

impl Arbitrary for Time {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::time_of_day()
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        let nanos = integer_of(&value)?;
        let part =
            |unit: i128, of: i128| u8::try_from(nanos.div_euclid(unit) % of).unwrap_or_default();
        let within = u32::try_from(nanos.rem_euclid(NANOS_PER_SECOND)).unwrap_or_default();
        (0..24 * NANOS_PER_HOUR)
            .contains(&nanos)
            .then(|| {
                Time::from_hms_nano(
                    part(NANOS_PER_HOUR, 24),
                    part(NANOS_PER_MINUTE, 60),
                    part(NANOS_PER_SECOND, 60),
                    within,
                )
                .ok()
            })
            .flatten()
            .ok_or_else(|| mismatch("a time of day", &value))
    }

    fn to_value(&self) -> Value {
        let (hour, minute, second, within) = self.as_hms_nano();
        Value::Int(
            i128::from(hour) * NANOS_PER_HOUR
                + i128::from(minute) * NANOS_PER_MINUTE
                + i128::from(second) * NANOS_PER_SECOND
                + i128::from(within),
        )
    }
}

impl Arbitrary for PrimitiveDateTime {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::local_date_time()
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        instant_of(&value)
            .map(|instant| PrimitiveDateTime::new(instant.date(), instant.time()))
            .map_err(|_| {
                mismatch(
                    "a local date and time that a PrimitiveDateTime states",
                    &value,
                )
            })
    }

    fn to_value(&self) -> Value {
        Value::Int(self.assume_utc().unix_timestamp_nanos())
    }
}

impl Arbitrary for Duration {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::duration()
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        i64::try_from(integer_of(&value)?)
            .map(Duration::nanoseconds)
            .map_err(|_| mismatch("a duration within the nanoseconds of an i64", &value))
    }

    fn to_value(&self) -> Value {
        Value::Int(self.whole_nanoseconds())
    }
}

impl Arbitrary for UtcOffset {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::offset()
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        i32::try_from(integer_of(&value)?)
            .ok()
            .and_then(|seconds| UtcOffset::from_whole_seconds(seconds).ok())
            .ok_or_else(|| mismatch("an offset that a UtcOffset states", &value))
    }

    fn to_value(&self) -> Value {
        Value::from(self.whole_seconds())
    }
}
