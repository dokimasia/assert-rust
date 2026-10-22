//! [`Arbitrary`] for the date and time types of jiff, behind the feature
//! `jiff`.
//!
//! A [`Timestamp`] reads as an instant, a [`Date`] as a date, a [`Time`] as a
//! time of day, a [`DateTime`] as a local date and time, a [`SignedDuration`]
//! as a duration, an [`Offset`] as an offset, a [`TimeZone`] as a zone and a
//! [`Zoned`] as an instant in a zone. A zone resolves through jiff's
//! time-zone database.

use super::{Arbitrary, Definitions, integer_of, mismatch};
use crate::prop::shape::{Shape, ShapeError};
use crate::value::Value;
use ::jiff::civil::{Date, DateTime, Time};
use ::jiff::tz::{Offset, TimeZone};
use ::jiff::{SignedDuration, Timestamp, Zoned};

/// The last whole second that a [`Timestamp`] states. jiff keeps every
/// timestamp within the dates that every offset can show.
const LAST_TIMESTAMP: &str = "9999-12-30T22:00:00Z";

/// The seconds of a day.
const SECONDS_PER_DAY: i64 = 86_400;

/// The day and the time from which a date, a time of day and a local date
/// and time count.
const EPOCH: DateTime = DateTime::constant(1970, 1, 1, 0, 0, 0, 0);

/// Returns the duration of a count of nanoseconds.
fn duration_of(value: &Value, expected: &str) -> Result<SignedDuration, ShapeError> {
    SignedDuration::try_from_nanos_i128(integer_of(value)?).ok_or_else(|| mismatch(expected, value))
}

/// Returns the timestamp of a count of nanoseconds since the epoch.
///
/// `Timestamp::from_nanosecond` of jiff-core 0.1.1 checks only that the
/// seconds fit an `i64`, so the range of a timestamp is checked first.
fn timestamp_of(value: &Value) -> Result<Timestamp, ShapeError> {
    let nanos = integer_of(value)?;
    (Timestamp::MIN.as_nanosecond()..=Timestamp::MAX.as_nanosecond())
        .contains(&nanos)
        .then(|| Timestamp::from_nanosecond(nanos).ok())
        .flatten()
        .ok_or_else(|| mismatch("an instant that a Timestamp states", value))
}

/// Returns the zone of a zone's name.
fn zone_of(value: &Value) -> Result<TimeZone, ShapeError> {
    match value {
        Value::String(name) => {
            TimeZone::get(name).map_err(|_| mismatch("a zone of jiff's time-zone database", value))
        }
        other => Err(mismatch("the name of a zone", other)),
    }
}

/// Returns the name of a zone, and null for a zone without one, such as a
/// fixed offset.
fn zone_name(zone: &TimeZone) -> Value {
    zone.iana_name().map_or(Value::Null, Value::from)
}

/// A timestamp reads as an instant up to 9999-12-30T22:00:00Z, the last
/// whole second of a timestamp.
impl Arbitrary for Timestamp {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::instant().max(LAST_TIMESTAMP)
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        timestamp_of(&value)
    }

    fn to_value(&self) -> Value {
        Value::Int(self.as_nanosecond())
    }
}

impl Arbitrary for Date {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::date()
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        let refused = || mismatch("a date that a Date states", &value);
        let seconds = i64::try_from(integer_of(&value)?)
            .ok()
            .and_then(|days| days.checked_mul(SECONDS_PER_DAY))
            .ok_or_else(refused)?;
        EPOCH
            .date()
            .checked_add(SignedDuration::from_secs(seconds))
            .map_err(|_| refused())
    }

    fn to_value(&self) -> Value {
        Value::from(self.duration_since(EPOCH.date()).as_secs() / SECONDS_PER_DAY)
    }
}

impl Arbitrary for Time {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::time_of_day()
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        let since = duration_of(&value, "a time of day")?;
        if since.is_negative() || since.as_secs() >= SECONDS_PER_DAY {
            return Err(mismatch("a time of day", &value));
        }
        Ok(Time::midnight() + since)
    }

    fn to_value(&self) -> Value {
        Value::Int(self.duration_since(Time::midnight()).as_nanos())
    }
}

impl Arbitrary for DateTime {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::local_date_time()
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        let since = duration_of(&value, "a local date and time")?;
        EPOCH
            .checked_add(since)
            .map_err(|_| mismatch("a local date and time that a DateTime states", &value))
    }

    fn to_value(&self) -> Value {
        Value::Int(self.duration_since(EPOCH).as_nanos())
    }
}

impl Arbitrary for SignedDuration {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::duration()
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        duration_of(&value, "a duration that a SignedDuration states")
    }

    fn to_value(&self) -> Value {
        Value::Int(self.as_nanos())
    }
}

impl Arbitrary for Offset {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::offset()
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        i32::try_from(integer_of(&value)?)
            .ok()
            .and_then(|seconds| Offset::from_seconds(seconds).ok())
            .ok_or_else(|| mismatch("an offset that an Offset states", &value))
    }

    fn to_value(&self) -> Value {
        Value::from(self.seconds())
    }
}

/// A zone reads as a zone of the definition's list. A zone without a name,
/// such as a fixed offset, has the value null, which no zone shape states.
impl Arbitrary for TimeZone {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::zone()
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        zone_of(&value)
    }

    fn to_value(&self) -> Value {
        zone_name(self)
    }
}

/// An instant in a zone reads as a zoned date and time. An instant after
/// 9999-12-30T22:00:00.999999999Z is no timestamp, so [`prop::of`] refuses
/// the cases that draw one.
///
/// [`prop::of`]: crate::prop::of
impl Arbitrary for Zoned {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::zoned_date_time()
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        let mut fields = super::registry::record_fields(value, &["instant", "zone"])?.into_iter();
        let instant = timestamp_of(&fields.next().unwrap_or(Value::Null))?;
        Ok(instant.to_zoned(zone_of(&fields.next().unwrap_or(Value::Null))?))
    }

    fn to_value(&self) -> Value {
        Value::Record(vec![
            ("instant".to_owned(), self.timestamp().to_value()),
            ("zone".to_owned(), zone_name(self.time_zone())),
        ])
    }
}
