// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The predicates of the property vectors, stated as data.
//!
//! A predicate is a JSON object with a `kind` and its parameters, and
//! `"not": true` negates it. Equal means equal under the definition's
//! equality: a float by its bits, a map by its entries in any order. Numbers
//! compare by their exact values, as the definition's reference compares
//! them: an integer of any size with a float, and a sum of integers without
//! rounding.

use super::choices::{integer, number};
use crate::literal;
use dokimi_assert::Value;
use serde_json::Value as Json;
use std::cmp::Ordering;
use std::fmt;
use std::sync::Arc;

/// A predicate over a value.
pub type Predicate = Arc<dyn Fn(&Value) -> bool + Send + Sync>;

/// A predicate spec that the vocabulary does not define, or that lacks a
/// parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PredicateError(pub String);

impl fmt::Display for PredicateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PredicateError {}

/// Returns `true` when two values are equal under the definition's equality.
#[must_use]
pub fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Float(x), Value::Float(y)) => {
            (x.is_nan() && y.is_nan()) || x.to_bits() == y.to_bits()
        }
        (Value::List(x), Value::List(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| same(a, b))
        }
        (Value::Map(x), Value::Map(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.iter().any(|(l, w)| same(k, l) && same(v, w)))
        }
        (Value::Record(x), Value::Record(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|((m, a), (n, b))| m == n && same(a, b))
        }
        (
            Value::Variant {
                name: m,
                payload: a,
            },
            Value::Variant {
                name: n,
                payload: b,
            },
        ) => {
            m == n
                && match (a, b) {
                    (Some(a), Some(b)) => same(a, b),
                    (None, None) => true,
                    _ => false,
                }
        }
        _ => a == b,
    }
}

/// A number with its exact value: an integer of any size that a value
/// states, or a float.
#[derive(Debug, Clone, Copy)]
enum Number {
    Integer(i128),
    Unsigned(u128),
    Float(f64),
}

/// 2^127, the first float above every `i128`.
const SIGNED_LIMIT: f64 = 170_141_183_460_469_231_731_687_303_715_884_105_728.0;

/// 2^128, the first float above every `u128`.
const UNSIGNED_LIMIT: f64 = 340_282_366_920_938_463_463_374_607_431_768_211_456.0;

impl Number {
    /// Returns the number that a value states. A bool is a number only where
    /// `bools` is set, as it is under Python's `<` and `sum`.
    fn of(value: &Value, bools: bool) -> Option<Self> {
        match value {
            Value::Int(held) => Some(Self::Integer(*held)),
            Value::Uint(held) => {
                Some(i128::try_from(*held).map_or(Self::Unsigned(*held), Self::Integer))
            }
            Value::Float(held) => Some(Self::Float(*held)),
            Value::Bool(held) if bools => Some(Self::Integer(i128::from(*held))),
            _ => None,
        }
    }

    /// Returns the number that a predicate's parameter states: a float as a
    /// JSON float or by name, and an integer as a JSON integer or as decimal
    /// text.
    fn parameter(written: &Json) -> Option<Self> {
        match written {
            Json::Number(held) if held.is_f64() => held.as_f64().map(Self::Float),
            _ => integer(written)
                .map(Self::Integer)
                .or_else(|| number(written).map(Self::Float)),
        }
    }

    /// Returns the float that is nearest to the number.
    #[expect(
        clippy::cast_precision_loss,
        reason = "Python's float of an int is the nearest float, as this cast is"
    )]
    fn float(self) -> f64 {
        match self {
            Self::Integer(held) => held as f64,
            Self::Unsigned(held) => held as f64,
            Self::Float(held) => held,
        }
    }

    /// Returns the sum of two numbers as Python adds them: two integers
    /// exactly, and a float with anything as floats.
    fn add(self, other: Self) -> Self {
        match (self, other) {
            (Self::Integer(a), Self::Integer(b)) => a
                .checked_add(b)
                .map_or_else(|| Self::Float(self.float() + other.float()), Self::Integer),
            _ => Self::Float(self.float() + other.float()),
        }
    }

    /// Returns the order of two numbers by their exact values, or `None` when
    /// either is NaN.
    fn compare(self, other: Self) -> Option<Ordering> {
        match (self, other) {
            (Self::Integer(a), Self::Integer(b)) => Some(a.cmp(&b)),
            (Self::Unsigned(a), Self::Unsigned(b)) => Some(a.cmp(&b)),
            (Self::Unsigned(_), Self::Integer(_)) => Some(Ordering::Greater),
            (Self::Integer(_), Self::Unsigned(_)) => Some(Ordering::Less),
            (Self::Float(a), Self::Float(b)) => a.partial_cmp(&b),
            (Self::Float(_), _) => other.compare(self).map(Ordering::reverse),
            (Self::Integer(a), Self::Float(b)) => integer_float(a, b),
            (Self::Unsigned(a), Self::Float(b)) => unsigned_float(a, b),
        }
    }
}

/// Returns the exact order of an integer and a float.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the floor of a float inside the range of i128 is an integer that i128 states"
)]
fn integer_float(a: i128, b: f64) -> Option<Ordering> {
    if b.is_nan() {
        return None;
    }
    if b >= SIGNED_LIMIT {
        return Some(Ordering::Less);
    }
    if b < -SIGNED_LIMIT {
        return Some(Ordering::Greater);
    }
    let floor = b.floor();
    Some(a.cmp(&(floor as i128)).then(if b > floor {
        Ordering::Less
    } else {
        Ordering::Equal
    }))
}

/// Returns the exact order of an unsigned integer above `i128::MAX` and a
/// float.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a float in [2^127, 2^128) is an integer that u128 states"
)]
fn unsigned_float(a: u128, b: f64) -> Option<Ordering> {
    if b.is_nan() {
        return None;
    }
    if b >= UNSIGNED_LIMIT {
        return Some(Ordering::Less);
    }
    if b < SIGNED_LIMIT {
        return Some(Ordering::Greater);
    }
    Some(a.cmp(&(b as u128)))
}

/// Returns the order of two values as Python's `<` orders them: numbers by
/// their values, a bool as an integer, and text, bools and lists in their
/// own order. Values that `<` does not order are equal.
#[must_use]
pub fn order(a: &Value, b: &Value) -> Ordering {
    if let (Some(x), Some(y)) = (Number::of(a, true), Number::of(b, true)) {
        return x.compare(y).unwrap_or(Ordering::Equal);
    }
    match (a, b) {
        (Value::String(a), Value::String(b)) => a.cmp(b),
        (Value::List(a), Value::List(b)) => {
            for (x, y) in a.iter().zip(b) {
                let found = order(x, y);
                if found != Ordering::Equal {
                    return found;
                }
            }
            a.len().cmp(&b.len())
        }
        _ => Ordering::Equal,
    }
}

/// Returns a number parameter.
fn threshold(spec: &Json, key: &str) -> Result<Number, PredicateError> {
    Number::parameter(&spec[key])
        .ok_or_else(|| PredicateError(format!("prop: the predicate lacks the number {key}")))
}

/// Returns an integer parameter.
fn whole(spec: &Json, key: &str) -> Result<i128, PredicateError> {
    integer(&spec[key])
        .ok_or_else(|| PredicateError(format!("prop: the predicate lacks the integer {key}")))
}

/// Returns `true` when a value is a number, and no bool, above or at `n`.
fn at_least(value: &Value, n: Number) -> bool {
    Number::of(value, false)
        .and_then(|held| held.compare(n))
        .is_some_and(Ordering::is_ge)
}

/// Returns `true` when a value is a number, and no bool, above `n`.
fn above(value: &Value, n: Number) -> bool {
    Number::of(value, false)
        .and_then(|held| held.compare(n))
        .is_some_and(Ordering::is_gt)
}

/// Returns the number of elements of a collection.
fn length(value: &Value) -> Option<usize> {
    match value {
        Value::List(items) => Some(items.len()),
        Value::String(text) => Some(text.chars().count()),
        Value::Bytes(bytes) => Some(bytes.len()),
        Value::Map(entries) => Some(entries.len()),
        _ => None,
    }
}

/// Returns the predicate that a spec states.
///
/// # Errors
///
/// Returns [`PredicateError`] for a spec that names no predicate, or lacks a
/// parameter.
pub fn predicate(spec: &Json) -> Result<Predicate, PredicateError> {
    let kind = spec["kind"].as_str().unwrap_or_default();
    let holds: Predicate = match kind {
        "always" => Arc::new(|_| true),
        "never" => Arc::new(|_| false),
        "equals" => {
            let wanted =
                literal::decode(&spec["value"]).map_err(|bad| PredicateError(bad.to_string()))?;
            Arc::new(move |value| same(value, &wanted))
        }
        "at-least" => {
            let n = threshold(spec, "n")?;
            Arc::new(move |value| at_least(value, n))
        }
        "divisible-by" => {
            let n = whole(spec, "n")?;
            if n == 0 {
                return Err(PredicateError(
                    "prop: divisible-by states n of 0".to_owned(),
                ));
            }
            Arc::new(move |value| match Number::of(value, false) {
                Some(Number::Integer(held)) => held.rem_euclid(n) == 0,
                Some(Number::Unsigned(held)) => held % n.unsigned_abs() == 0,
                _ => false,
            })
        }
        "sum-above" => {
            let n = threshold(spec, "n")?;
            Arc::new(move |value| match value {
                Value::List(items) => items
                    .iter()
                    .filter_map(|item| Number::of(item, true))
                    .fold(Number::Integer(0), Number::add)
                    .compare(n)
                    .is_some_and(Ordering::is_gt),
                _ => false,
            })
        }
        "length-at-least" => {
            let n = usize::try_from(whole(spec, "n")?)
                .map_err(|_| PredicateError("prop: length-at-least states n below 0".to_owned()))?;
            Arc::new(move |value| length(value).is_some_and(|held| held >= n))
        }
        "contains" => {
            let wanted =
                literal::decode(&spec["value"]).map_err(|bad| PredicateError(bad.to_string()))?;
            Arc::new(move |value| match (value, &wanted) {
                (Value::List(items), _) => items.iter().any(|item| same(item, &wanted)),
                (Value::String(text), Value::String(needle)) => text.contains(needle.as_str()),
                (Value::Bytes(bytes), Value::Bytes(needle)) => {
                    needle.is_empty()
                        || bytes
                            .windows(needle.len())
                            .any(|window| window == needle.as_slice())
                }
                _ => false,
            })
        }
        "not-sorted" => Arc::new(|value| match value {
            Value::List(items) => items
                .windows(2)
                .any(|pair| order(&pair[1], &pair[0]) == Ordering::Less),
            _ => false,
        }),
        "has-duplicate" => Arc::new(|value| match value {
            Value::List(items) => items
                .iter()
                .enumerate()
                .any(|(at, item)| items[..at].iter().any(|earlier| same(earlier, item))),
            _ => false,
        }),
        "indexed-above" => {
            let n = threshold(spec, "n")?;
            Arc::new(move |value| {
                let Value::List(items) = value else {
                    return false;
                };
                let Some((Value::Int(index), earlier)) = items.split_last() else {
                    return false;
                };
                usize::try_from(*index)
                    .ok()
                    .and_then(|at| earlier.get(at))
                    .is_some_and(|element| above(element, n))
            })
        }
        other => {
            return Err(PredicateError(format!(
                "prop: {other:?} names no predicate"
            )));
        }
    };
    if spec.get("not") == Some(&Json::Bool(true)) {
        return Ok(Arc::new(move |value| !holds(value)));
    }
    Ok(holds)
}
