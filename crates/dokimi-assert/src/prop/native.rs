// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The native form of the values of a shape, which [`Arbitrary`] takes and
//! returns, and the value that a shape's generator decodes.
//!
//! The two forms differ for a time and a decimal. The generator of a time
//! shape decodes its value in the shape's unit, and the native form states
//! nanoseconds, so a type converts its values without the unit of its use. A
//! decimal decodes to its unscaled integer, and its native form is the text
//! of its digits, which states its scale.
//!
//! [`Arbitrary`]: super::Arbitrary

use super::registry::record_fields;
use super::shape::ShapeError;
use super::shape::time::{SECONDS_PER_DAY, per_second};
use crate::json::Json;
use crate::value::Value;

/// The nanoseconds of a second.
const NANOS_PER_SECOND: i128 = 1_000_000_000;

/// Which form a walk turns a value into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Form {
    /// The native form, from the value that a generator decodes.
    Native,
    /// The value that a generator decodes, from the native form.
    Decoded,
}

/// The definitions of a shape file whose values a walk turns from one form
/// into the other.
pub(crate) struct Walk<'d> {
    pub(crate) definitions: &'d [(String, Json)],
}

impl Walk<'_> {
    /// Returns the native form of a value that a generator of `shape`
    /// decoded.
    pub(crate) fn native(&self, shape: &Json, value: Value) -> Result<Value, ShapeError> {
        self.walk(shape, value, Form::Native)
    }

    /// Returns the value that a generator of `shape` decodes, of a value in
    /// the native form.
    pub(crate) fn decoded(&self, shape: &Json, value: Value) -> Result<Value, ShapeError> {
        self.walk(shape, value, Form::Decoded)
    }

    fn walk(&self, shape: &Json, value: Value, form: Form) -> Result<Value, ShapeError> {
        let kind = shape
            .get("shape")
            .and_then(Json::as_str)
            .unwrap_or_default();
        match (kind, value) {
            ("record", Value::Record(fields)) => {
                let shapes = shape
                    .get("fields")
                    .and_then(Json::as_array)
                    .unwrap_or_default();
                let mut walked = Vec::with_capacity(fields.len());
                for (name, field) in fields {
                    let of = field_shape(shapes, &name).ok_or_else(|| {
                        ShapeError::new(format!("the record has no field {name}"))
                    })?;
                    walked.push((name, self.walk(of, field, form)?));
                }
                Ok(Value::Record(walked))
            }
            ("enum", Value::Variant { name, payload }) => {
                let shapes = shape
                    .get("variants")
                    .and_then(Json::as_array)
                    .unwrap_or_default();
                let payload = match (payload, field_shape(shapes, &name)) {
                    (Some(payload), Some(of)) => Some(Box::new(self.walk(of, *payload, form)?)),
                    (payload, _) => payload,
                };
                Ok(Value::Variant { name, payload })
            }
            ("list" | "set" | "fixed-list", Value::List(items)) => {
                let of = shape.get("of").unwrap_or(&Json::Null);
                items
                    .into_iter()
                    .map(|item| self.walk(of, item, form))
                    .collect::<Result<_, _>>()
                    .map(Value::List)
            }
            ("map", Value::Map(entries)) => {
                let key = shape.get("key").unwrap_or(&Json::Null);
                let of = shape.get("of").unwrap_or(&Json::Null);
                entries
                    .into_iter()
                    .map(|(at, held)| Ok((self.walk(key, at, form)?, self.walk(of, held, form)?)))
                    .collect::<Result<_, ShapeError>>()
                    .map(Value::Map)
            }
            ("optional", Value::Null) => Ok(Value::Null),
            ("optional", value) => self.walk(shape.get("of").unwrap_or(&Json::Null), value, form),
            ("ref", value) => {
                // A ref to a registered generator names no definition, and
                // its values are native in both forms.
                let name = shape.get("name").and_then(Json::as_str).unwrap_or_default();
                match self.definitions.iter().find(|(held, _)| held == name) {
                    Some((_, definition)) => self.walk(definition, value, form),
                    None => Ok(value),
                }
            }
            ("instant", value) => instant(step(shape), &value, form),
            ("time-of-day" | "duration", value) => scaled(step(shape), &value, form),
            ("local-date-time", value) => local(step(shape), &value, form),
            ("zoned-date-time", value) => zoned(shape, value, form, "instant", instant),
            ("wall-time", value) => zoned(shape, value, form, "local-date-time", local),
            ("decimal", value) => decimal(shape, value, form),
            (_, value) => Ok(value),
        }
    }
}

/// Returns the shape of the field or the variant named `name` among a list
/// of names with shapes.
fn field_shape<'s>(shapes: &'s [Json], name: &str) -> Option<&'s Json> {
    shapes.iter().find_map(|pair| match pair.as_array() {
        Some([Json::String(held), of]) if held == name => Some(of),
        _ => None,
    })
}

/// Returns the nanoseconds of one unit of a time shape, whose unit the reader
/// of the shape file checked.
fn step(shape: &Json) -> i128 {
    let per = shape
        .get("unit")
        .and_then(Json::as_str)
        .and_then(per_second)
        .expect("the reader checks the unit of each time shape");
    NANOS_PER_SECOND / per
}

/// Returns the integer of a value.
fn integer(value: &Value) -> Result<i128, ShapeError> {
    match value {
        Value::Int(held) => Ok(*held),
        other => Err(ShapeError::new(format!("the value {other} is no int"))),
    }
}

/// Returns the two integer parts of a record of a two-choice time.
fn parts(value: &Value, names: [&str; 2]) -> Result<(i128, i128), ShapeError> {
    let fields = record_fields(value.clone(), &names)?;
    Ok((integer(&fields[0])?, integer(&fields[1])?))
}

/// Returns a count of nanoseconds in units of `step` nanoseconds, or the
/// refusal of a count finer than the unit.
fn in_units(nanos: i128, step: i128) -> Result<i128, ShapeError> {
    if nanos % step != 0 {
        return Err(ShapeError::new(format!(
            "{nanos} nanoseconds are finer than a unit of {step}"
        )));
    }
    Ok(nanos / step)
}

/// Turns a time of one choice, in units, into nanoseconds or back.
fn scaled(step: i128, value: &Value, form: Form) -> Result<Value, ShapeError> {
    let held = integer(value)?;
    match form {
        Form::Native => Ok(Value::Int(held * step)),
        Form::Decoded => in_units(held, step).map(Value::Int),
    }
}

/// Turns an instant, its seconds and its units, into nanoseconds or back.
fn instant(step: i128, value: &Value, form: Form) -> Result<Value, ShapeError> {
    match form {
        Form::Native => {
            let (seconds, units) = parts(value, ["seconds", "units"])?;
            Ok(Value::Int(seconds * NANOS_PER_SECOND + units * step))
        }
        Form::Decoded => {
            let nanos = integer(value)?;
            let units = in_units(nanos.rem_euclid(NANOS_PER_SECOND), step)?;
            Ok(Value::Record(vec![
                (
                    "seconds".to_owned(),
                    Value::Int(nanos.div_euclid(NANOS_PER_SECOND)),
                ),
                ("units".to_owned(), Value::Int(units)),
            ]))
        }
    }
}

/// Turns a local date and time, its date and its time of day, into
/// nanoseconds or back.
fn local(step: i128, value: &Value, form: Form) -> Result<Value, ShapeError> {
    let day = SECONDS_PER_DAY * NANOS_PER_SECOND;
    match form {
        Form::Native => {
            let (date, time) = parts(value, ["date", "time-of-day"])?;
            Ok(Value::Int(date * day + time * step))
        }
        Form::Decoded => {
            let nanos = integer(value)?;
            let time = in_units(nanos.rem_euclid(day), step)?;
            Ok(Value::Record(vec![
                ("date".to_owned(), Value::Int(nanos.div_euclid(day))),
                ("time-of-day".to_owned(), Value::Int(time)),
            ]))
        }
    }
}

/// Turns a time in a zone, a record of the time and the zone's name, into
/// its native form or back, with the time turned by `turn`.
fn zoned(
    shape: &Json,
    value: Value,
    form: Form,
    part: &str,
    turn: fn(i128, &Value, Form) -> Result<Value, ShapeError>,
) -> Result<Value, ShapeError> {
    let step = step(shape);
    match value {
        Value::Record(fields) => match <[(String, Value); 2]>::try_from(fields) {
            Ok([(first, time), (second, zone)]) if first == part && second == "zone" => {
                Ok(Value::Record(vec![
                    (first, turn(step, &time, form)?),
                    (second, zone),
                ]))
            }
            _ => Err(ShapeError::new(format!(
                "the value is no record of {part} and zone"
            ))),
        },
        other => Err(ShapeError::new(format!("the value {other} is no record"))),
    }
}

/// Turns a decimal, its unscaled integer, into the text of its digits or
/// back. The reader of the shape file checked the decimal's scale.
fn decimal(shape: &Json, value: Value, form: Form) -> Result<Value, ShapeError> {
    let scale = shape
        .get("scale")
        .and_then(Json::as_int)
        .and_then(|scale| usize::try_from(scale).ok())
        .expect("the reader checks the scale of each decimal shape");
    match (form, value) {
        (Form::Native, value) => Ok(Value::String(decimal_text(integer(&value)?, scale))),
        (Form::Decoded, Value::String(text)) => unscaled(&text, scale).map(Value::Int),
        (Form::Decoded, other) => Err(ShapeError::new(format!(
            "the value {other} is no decimal text"
        ))),
    }
}

/// Returns the text of a decimal's digits: its unscaled integer with the
/// point before the last `scale` digits.
fn decimal_text(unscaled: i128, scale: usize) -> String {
    let sign = if unscaled < 0 { "-" } else { "" };
    let digits = format!("{:0>width$}", unscaled.unsigned_abs(), width = scale + 1);
    if scale == 0 {
        return format!("{sign}{digits}");
    }
    let (whole, fraction) = digits.split_at(digits.len() - scale);
    format!("{sign}{whole}.{fraction}")
}

/// Returns the unscaled integer of a decimal's text at a scale, or the
/// refusal of text that is no decimal or has more digits than the scale.
fn unscaled(text: &str, scale: usize) -> Result<i128, ShapeError> {
    let refused = || ShapeError::new(format!("{text:?} is no decimal of scale {scale}"));
    let (negative, digits) = text
        .strip_prefix('-')
        .map_or((false, text), |rest| (true, rest));
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    let plain = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
    if whole.is_empty() || !plain(whole) || !plain(fraction) || fraction.len() > scale {
        return Err(refused());
    }
    let magnitude: i128 = format!("{whole}{fraction:0<scale$}")
        .parse()
        .map_err(|_| refused())?;
    Ok(if negative { -magnitude } else { magnitude })
}
