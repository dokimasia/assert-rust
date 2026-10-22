// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The time shapes that make more than one choice, and the text forms of
//! the bounds of every time shape.
//!
//! An instant is its seconds since 1970-01-01T00:00:00Z and the units within
//! the second, because one 64-bit integer cannot state the nanoseconds of the
//! years 1 to 9999. A local date and time is its date and its time of day. An
//! instant in a zone and a wall time in a zone take their zone first, and
//! then a value near one of the zone's offset changes in one case of four.

use super::zone::{Zone, zones};
use crate::prop::case::Case;
use crate::prop::engine::case::{Request, Stop};
use crate::prop::engine::choice::{Bounds, Choice, IntegerBounds};
use crate::prop::engine::draw::{Draw, HALF, QUARTER};
use crate::prop::engine::inverse::{CannotInvert, Emitted, Planned, planned};
use crate::prop::generator::Node;
use crate::prop::generators::{index, position, second_part};
use crate::value::Value;

/// The seconds of a day.
pub(crate) const SECONDS_PER_DAY: i128 = 86_400;

/// The first and the last second of the date and time shapes:
/// 0001-01-01T00:00:00Z and 9999-12-31T23:59:59Z, since the epoch.
pub(crate) const FIRST_SECOND: i128 = -62_135_596_800;
pub(crate) const LAST_SECOND: i128 = 253_402_300_799;

/// The first and the last day of the date shapes, since 1970-01-01.
pub(crate) const FIRST_DAY: i128 = FIRST_SECOND.div_euclid(SECONDS_PER_DAY);
pub(crate) const LAST_DAY: i128 = LAST_SECOND.div_euclid(SECONDS_PER_DAY);

/// The first and the last year of the date and time shapes.
const FIRST_YEAR: i128 = 1;
const LAST_YEAR: i128 = 9999;

/// The digits of a fraction of a second, at most nanoseconds.
const MAX_FRACTION_DIGITS: usize = 9;

/// The bounds of a two-choice shape: the first and the second part of one
/// end.
pub(crate) type Pair = (i128, i128);

/// Returns the units in a second of a unit's name: `s`, `ms`, `us` or `ns`.
pub(crate) fn per_second(unit: &str) -> Option<i128> {
    match unit {
        "s" => Some(1),
        "ms" => Some(1_000),
        "us" => Some(1_000_000),
        "ns" => Some(1_000_000_000),
        _ => None,
    }
}

/// Returns the value of an instant: its seconds and its units within the
/// second.
fn instant_value(seconds: i128, units: i128) -> Value {
    Value::Record(vec![
        ("seconds".to_owned(), Value::Int(seconds)),
        ("units".to_owned(), Value::Int(units)),
    ])
}

/// Returns the value of a local date and time: its date and its time of day.
fn local_value(day: i128, time: i128) -> Value {
    Value::Record(vec![
        ("date".to_owned(), Value::Int(day)),
        ("time-of-day".to_owned(), Value::Int(time)),
    ])
}

/// Returns the two parts of a record of a two-choice shape, which must be
/// integers with the two names.
fn parts(value: &Value, names: [&str; 2]) -> Result<(i128, i128), CannotInvert> {
    match value {
        Value::Record(fields) => match fields.as_slice() {
            [(first, Value::Int(a)), (second, Value::Int(b))]
                if *first == names[0] && *second == names[1] =>
            {
                Ok((*a, *b))
            }
            _ => Err(CannotInvert::refused(
                "the value is no record of the shape's two parts",
            )),
        },
        _ => Err(CannotInvert::refused("the value is no record")),
    }
}

/// Returns the two fields of a record of a value in a zone: the value and
/// the zone's name.
fn zoned_parts<'v>(value: &'v Value, part: &str) -> Result<(&'v Value, &'v str), CannotInvert> {
    match value {
        Value::Record(fields) => match fields.as_slice() {
            [(first, inner), (second, Value::String(zone))]
                if first == part && second == "zone" =>
            {
                Ok((inner, zone.as_str()))
            }
            _ => Err(CannotInvert::refused(
                "the value is no record of a value and a zone",
            )),
        },
        _ => Err(CannotInvert::refused("the value is no record")),
    }
}

/// An instant: its seconds, then the units within the second, as one span.
/// At a unit of a second, the instant makes no second choice, and its units
/// are 0.
pub(crate) struct Instant {
    pub(crate) per: i128,
    pub(crate) lo: Pair,
    pub(crate) hi: Pair,
}

impl Instant {
    fn steps(&self, value: &Value) -> Result<Vec<Planned>, CannotInvert> {
        let (seconds, units) = parts(value, ["seconds", "units"])?;
        let first = planned(
            Bounds::Integer(IntegerBounds::of(self.lo.0, self.hi.0)),
            Choice::Integer(seconds),
        )?;
        if self.per == 1 {
            if units != 0 {
                return Err(CannotInvert::refused("an instant at seconds has no units"));
            }
            return Ok(vec![first]);
        }
        let bounds = second_part(seconds, self.lo, self.hi, self.per - 1);
        Ok(vec![
            first,
            planned(Bounds::Integer(bounds), Choice::Integer(units))?,
        ])
    }
}

impl Node<Value> for Instant {
    fn id(&self) -> &'static str {
        "instant"
    }

    fn decode(&self, case: &Case<'_>) -> Result<Value, Stop> {
        let _span = case.span("instant", None);
        let seconds = case.integer(IntegerBounds::of(self.lo.0, self.hi.0), None, true)?;
        let units = if self.per > 1 {
            case.integer(
                second_part(seconds, self.lo, self.hi, self.per - 1),
                None,
                true,
            )?
        } else {
            0
        };
        Ok(instant_value(seconds, units))
    }

    fn emit(&self, value: &Value) -> Result<Emitted, CannotInvert> {
        Ok((self.steps(value)?, value.clone()))
    }
}

/// A date and a time of day without a zone, as one span.
pub(crate) struct Local {
    pub(crate) per: i128,
    pub(crate) lo: Pair,
    pub(crate) hi: Pair,
}

impl Local {
    /// Returns the last unit of a day.
    fn top(&self) -> i128 {
        SECONDS_PER_DAY * self.per - 1
    }

    fn steps(&self, value: &Value) -> Result<Vec<Planned>, CannotInvert> {
        let (day, time) = parts(value, ["date", "time-of-day"])?;
        let first = planned(
            Bounds::Integer(IntegerBounds::of(self.lo.0, self.hi.0)),
            Choice::Integer(day),
        )?;
        let bounds = second_part(day, self.lo, self.hi, self.top());
        Ok(vec![
            first,
            planned(Bounds::Integer(bounds), Choice::Integer(time))?,
        ])
    }
}

impl Node<Value> for Local {
    fn id(&self) -> &'static str {
        "local-date-time"
    }

    fn decode(&self, case: &Case<'_>) -> Result<Value, Stop> {
        let _span = case.span("local-date-time", None);
        let day = case.integer(IntegerBounds::of(self.lo.0, self.hi.0), None, true)?;
        let time = case.integer(second_part(day, self.lo, self.hi, self.top()), None, true)?;
        Ok(local_value(day, time))
    }

    fn emit(&self, value: &Value) -> Result<Emitted, CannotInvert> {
        Ok((self.steps(value)?, value.clone()))
    }
}

/// Returns the zone of the list that an index chooses.
fn zone_of(case: &Case<'_>) -> Result<&'static Zone, Stop> {
    let listed = zones();
    let at = case.integer(
        IntegerBounds::of(0, index(listed.len()) - 1),
        Some(0),
        false,
    )?;
    Ok(&listed[position(at)])
}

/// Returns a coin with probability `p`: one integer choice in `[0, 1]`.
fn coin(case: &Case<'_>, p: crate::prop::engine::draw::Ratio) -> Result<bool, Stop> {
    let request = Request {
        bounds: Bounds::Integer(IntegerBounds::of(0, 1)),
        draw: Draw::Boolean(p),
        edge: None,
        reuse: false,
    };
    Ok(case.choose(request)?.as_integer() == Some(1))
}

/// Returns whether a value of a zone comes from one of its offset changes.
/// A zone without a change, such as UTC, makes no choice here.
fn near(case: &Case<'_>, zone: &Zone) -> Result<bool, Stop> {
    if zone.changes.is_empty() {
        return Ok(false);
    }
    coin(case, QUARTER)
}

/// Returns one of a zone's changes, and a step from -1 to 1 units.
fn change_and_step(case: &Case<'_>, zone: &Zone) -> Result<(usize, i128), Stop> {
    let bounds = IntegerBounds::of(0, index(zone.changes.len()) - 1);
    let at = case.integer(bounds, Some(0), false)?;
    let step = case.integer(IntegerBounds::of(-1, 1), None, true)?;
    Ok((position(at), step))
}

/// Returns the steps of a zone, and of the choice that takes the whole
/// range, for a zone that has changes.
fn zone_steps(name: &str) -> Result<Vec<Planned>, CannotInvert> {
    let listed = zones();
    let at = listed
        .iter()
        .position(|zone| zone.name == name)
        .ok_or_else(|| CannotInvert::refused("the value's zone is no zone of the list"))?;
    let mut steps = vec![Planned::new(
        Bounds::Integer(IntegerBounds::of(0, index(listed.len()) - 1)),
        Choice::Integer(index(at)),
    )];
    if !listed[at].changes.is_empty() {
        steps.push(Planned::new(
            Bounds::Integer(IntegerBounds::of(0, 1)),
            Choice::Integer(0),
        ));
    }
    Ok(steps)
}

/// An instant in a zone: the zone, then an instant near one of its changes
/// or from the whole range.
pub(crate) struct Zoned {
    pub(crate) instant: Instant,
}

impl Node<Value> for Zoned {
    fn id(&self) -> &'static str {
        "zoned-date-time"
    }

    fn decode(&self, case: &Case<'_>) -> Result<Value, Stop> {
        let per = self.instant.per;
        let _span = case.span("zoned-date-time", None);
        let zone = zone_of(case)?;
        let value = if near(case, zone)? {
            let (at, step) = change_and_step(case, zone)?;
            let total = zone.changes[at].at * per + step;
            instant_value(total.div_euclid(per), total.rem_euclid(per))
        } else {
            self.instant.decode(case)?
        };
        Ok(Value::Record(vec![
            ("instant".to_owned(), value),
            ("zone".to_owned(), Value::from(zone.name.as_str())),
        ]))
    }

    /// Returns the steps of the zone, the whole range and the instant: a
    /// value at an offset change has one sequence of choices as any other.
    fn emit(&self, value: &Value) -> Result<Emitted, CannotInvert> {
        let (inner, zone) = zoned_parts(value, "instant")?;
        let mut steps = zone_steps(zone)?;
        steps.extend(self.instant.steps(inner)?);
        Ok((steps, value.clone()))
    }
}

/// A wall time in a zone, which the code under test resolves.
///
/// The zone comes first. A wall time near a change is the change's instant
/// in the offset before it or after it, moved by one unit at most, which
/// reaches the wall times that a gap skips and a fold repeats.
pub(crate) struct Wall {
    pub(crate) local: Local,
}

impl Node<Value> for Wall {
    fn id(&self) -> &'static str {
        "wall-time"
    }

    fn decode(&self, case: &Case<'_>) -> Result<Value, Stop> {
        let per = self.local.per;
        let _span = case.span("wall-time", None);
        let zone = zone_of(case)?;
        let value = if near(case, zone)? {
            let (at, step) = change_and_step(case, zone)?;
            let change = zone.changes[at];
            let offset = if coin(case, HALF)? {
                change.after
            } else {
                change.before
            };
            let total = (change.at + offset) * per + step;
            let day = SECONDS_PER_DAY * per;
            local_value(total.div_euclid(day), total.rem_euclid(day))
        } else {
            self.local.decode(case)?
        };
        Ok(Value::Record(vec![
            ("local-date-time".to_owned(), value),
            ("zone".to_owned(), Value::from(zone.name.as_str())),
        ]))
    }

    fn emit(&self, value: &Value) -> Result<Emitted, CannotInvert> {
        let (inner, zone) = zoned_parts(value, "local-date-time")?;
        let mut steps = zone_steps(zone)?;
        steps.extend(self.local.steps(inner)?);
        Ok((steps, value.clone()))
    }
}

/// Returns the number of ASCII digits, of an exact count.
fn digits(text: &str, count: usize) -> Option<i128> {
    (text.len() == count && text.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()
}

/// Returns the days since 1970-01-01 of a date of the proleptic Gregorian
/// calendar in the years 1 to 9999, or `None` for a date that does not
/// exist.
fn days(year: i128, month: i128, day: i128) -> Option<i128> {
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let length = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return None,
    };
    if !(FIRST_YEAR..=LAST_YEAR).contains(&year) || !(1..=length).contains(&day) {
        return None;
    }
    // Howard Hinnant's days_from_civil, with the year starting in March.
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let of_era = year - era * 400;
    let of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let of_cycle = of_era * 365 + of_era / 4 - of_era / 100 + of_year;
    Some(era * 146_097 + of_cycle - 719_468)
}

/// Returns the days since 1970-01-01 of a date written as `YYYY-MM-DD`.
pub(crate) fn date(text: &str) -> Option<i128> {
    let mut parts = text.split('-');
    let (year, month, day) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    days(digits(year, 4)?, digits(month, 2)?, digits(day, 2)?)
}

/// Returns the seconds since midnight and the fraction's digits of a time of
/// day written as `hh:mm:ss`, with up to nine digits of a fraction after a
/// point.
pub(crate) fn time(text: &str) -> Option<(i128, &str)> {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    if text.contains('.')
        && (fraction.is_empty()
            || fraction.len() > MAX_FRACTION_DIGITS
            || !fraction.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return None;
    }
    let mut parts = whole.split(':');
    let (hour, minute, second) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let (hour, minute, second) = (digits(hour, 2)?, digits(minute, 2)?, digits(second, 2)?);
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    Some((hour * 3600 + minute * 60 + second, fraction))
}

/// A date and a time of day, as a shape's bound writes them.
pub(crate) struct DateTime<'t> {
    /// The days since 1970-01-01.
    pub(crate) day: i128,
    /// The seconds since midnight.
    pub(crate) seconds: i128,
    /// The digits of the fraction of the second.
    pub(crate) fraction: &'t str,
    /// Whether the text ends in `Z`: an instant in UTC.
    pub(crate) utc: bool,
}

/// Returns the date and the time of day of text written as
/// `YYYY-MM-DDThh:mm:ss`, with up to nine digits of a fraction, and a `Z`
/// for an instant in UTC.
pub(crate) fn date_time(text: &str) -> Option<DateTime<'_>> {
    let (on, at) = text.split_once('T')?;
    let (at, utc) = match at.strip_suffix('Z') {
        Some(at) => (at, true),
        None => (at, false),
    };
    let (seconds, fraction) = time(at)?;
    Some(DateTime {
        day: date(on)?,
        seconds,
        fraction,
        utc,
    })
}

/// Returns the units within a second that a fraction's digits state at a
/// unit of `per` in a second, or `None` when the digits are finer than the
/// unit.
pub(crate) fn fraction_units(fraction: &str, per: i128) -> Option<i128> {
    if fraction.is_empty() {
        return Some(0);
    }
    let numerator = fraction.parse::<i128>().ok()? * per;
    let denominator = 10_i128.pow(u32::try_from(fraction.len()).ok()?);
    (numerator % denominator == 0).then_some(numerator / denominator)
}
