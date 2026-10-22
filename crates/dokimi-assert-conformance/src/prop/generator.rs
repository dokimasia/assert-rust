//! The generators of the property vectors, built from their JSON.
//!
//! A vector states a generator as a JSON object whose `gen` key names its
//! id. [`build`] turns that object into a generator of the library, whose
//! values are the typed literals that a vector states. A `map` names the
//! function of a subject kind, a `filter` states its predicate as data, and
//! `self` is one position of the recursive value whose extension contains it.

use super::choices::{integer, number};
use super::predicate::{self, PredicateError, order};
use crate::literal;
use dokimi_assert::Value;
use dokimi_assert::prop::engine::collection::Sizes;
use dokimi_assert::prop::engine::harness;
use dokimi_assert::prop::{self, Generator};
use serde_json::Value as Json;
use std::fmt;

/// The values that one recursive value may draw from its base when the spec
/// states no `max_leaves`.
const DEFAULT_MAX_LEAVES: usize = 100;

/// A spec that names no generator, or misstates a parameter or a literal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecError(pub String);

impl fmt::Display for SpecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SpecError {}

impl From<PredicateError> for SpecError {
    fn from(error: PredicateError) -> Self {
        Self(error.to_string())
    }
}

fn refused(what: impl fmt::Display) -> SpecError {
    SpecError(format!("prop: {what}"))
}

/// Returns the integer parameter `key` of a spec.
fn int(spec: &Json, key: &str) -> Result<i128, SpecError> {
    integer(&spec[key]).ok_or_else(|| refused(format!("{} needs the integer {key}", spec["gen"])))
}

/// Returns a count parameter, or `None` when the spec states none.
fn count(spec: &Json, key: &str) -> Result<Option<usize>, SpecError> {
    match spec.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(_) => usize::try_from(int(spec, key)?)
            .map(Some)
            .map_err(|_| refused(format!("{key} is below 0"))),
    }
}

/// Returns `min_size`, 0 by default, and `max_size`, unbounded by default.
fn sizes(spec: &Json) -> Result<Sizes, SpecError> {
    let min = count(spec, "min_size")?.unwrap_or(0);
    let max = count(spec, "max_size")?;
    Sizes::new(min, max).ok_or_else(|| refused(format!("sizes [{min}, {max:?}] are empty")))
}

/// Returns a boolean parameter, false when the spec states none.
fn flag(spec: &Json, key: &str) -> Result<bool, SpecError> {
    match spec.get(key) {
        None => Ok(false),
        Some(Json::Bool(held)) => Ok(*held),
        Some(other) => Err(refused(format!("{key} is {other}, not a boolean"))),
    }
}

/// Returns the typed literals of the values parameter.
fn values(spec: &Json) -> Result<Vec<Value>, SpecError> {
    spec["values"]
        .as_array()
        .ok_or_else(|| refused("values is not a list"))?
        .iter()
        .map(|value| literal::decode(value).map_err(refused))
        .collect()
}

/// Returns the generator that a spec states. `scope` is the position of the
/// recursive value whose extension contains the spec, which a `self` inside
/// it refers to.
///
/// # Errors
///
/// Returns [`SpecError`] for a spec that names no generator of the
/// vocabulary, lacks a parameter, or states a parameter that the generator
/// cannot take.
pub fn build(spec: &Json, scope: Option<&Generator<Value>>) -> Result<Generator<Value>, SpecError> {
    let kind = spec["gen"]
        .as_str()
        .ok_or_else(|| refused(format!("{spec} names no generator")))?;
    let child = |key: &str| build(&spec[key], scope);
    Ok(match kind {
        "self" => scope
            .cloned()
            .ok_or_else(|| refused("self appears outside a recursive generator's extension"))?,
        "integer" => integer_of(int(spec, "min")?, int(spec, "max")?)?,
        "duration" => {
            harness::duration_nanos(int(spec, "min")?, int(spec, "max")?).map_err(refused)?
        }
        "float" => float_of(spec)?,
        "boolean" => boolean_of(spec)?,
        "just" => prop::just(literal::decode(&spec["value"]).map_err(refused)?),
        "sampled-from" => {
            let values = values(spec)?;
            if values.is_empty() {
                return Err(refused("sampled-from needs at least one value"));
            }
            prop::sampled_from(values)
        }
        "string" => string_of(spec)?,
        "string-matching" => {
            let pattern = spec["pattern"]
                .as_str()
                .ok_or_else(|| refused("pattern is not a string"))?;
            check_pattern(pattern)?;
            prop::string_matching(pattern).valued()
        }
        "bytes" => {
            let sizes = sizes(spec)?;
            match sizes.max_size() {
                Some(max) => prop::bytes(sizes.min_size()..=max),
                None => prop::bytes(sizes.min_size()..),
            }
            .valued()
        }
        "permutation" => prop::permutation(values(spec)?).valued(),
        "recursive" => recursive_of(spec)?,
        "one-of" => {
            let of = spec["of"]
                .as_array()
                .filter(|of| !of.is_empty())
                .ok_or_else(|| refused("one-of needs a list of generators"))?;
            prop::one_of(
                of.iter()
                    .map(|alternative| build(alternative, scope))
                    .collect::<Result<Vec<_>, _>>()?,
            )
        }
        "optional" => prop::optional(child("of")?).valued(),
        "list" => {
            let sizes = sizes(spec)?;
            let list = match sizes.max_size() {
                Some(max) => prop::list(child("of")?, sizes.min_size()..=max),
                None => prop::list(child("of")?, sizes.min_size()..),
            };
            if flag(spec, "unique")? {
                list.unique().valued()
            } else {
                list.valued()
            }
        }
        "dict" => harness::pairs(child("keys")?, child("values")?, sizes(spec)?),
        "filter" => {
            let keep = predicate::predicate(&spec["keep"])?;
            child("of")?.filter(move |value| keep(value))
        }
        "map" => {
            let subject = spec["subject"]
                .as_str()
                .ok_or_else(|| refused("map names no subject"))?;
            let function = function(subject)
                .ok_or_else(|| refused(format!("the subject {subject:?} has no function")))?;
            child("of")?.map(function)
        }
        other => return Err(refused(format!("{other:?} is not a generator"))),
    })
}

/// Returns the generator of the integers in `[lo, hi]`, as a signed or an
/// unsigned 64-bit integer.
fn integer_of(lo: i128, hi: i128) -> Result<Generator<Value>, SpecError> {
    if lo > hi {
        return Err(refused(format!("integer bounds [{lo}, {hi}] are empty")));
    }
    if let (Ok(lo), Ok(hi)) = (i64::try_from(lo), i64::try_from(hi)) {
        return Ok(prop::integer(lo..=hi).valued());
    }
    match (u64::try_from(lo), u64::try_from(hi)) {
        (Ok(lo), Ok(hi)) => Ok(prop::integer(lo..=hi).valued()),
        _ => Err(refused(format!(
            "integer bounds [{lo}, {hi}] are inside neither the signed nor the unsigned 64-bit range"
        ))),
    }
}

/// Returns the generator of a float of the stated width.
fn float_of(spec: &Json) -> Result<Generator<Value>, SpecError> {
    let lo = number(&spec["min"]).ok_or_else(|| refused("float needs min"))?;
    let hi = number(&spec["max"]).ok_or_else(|| refused("float needs max"))?;
    let width = match spec.get("width") {
        None => 64,
        Some(_) => int(spec, "width")?,
    };
    let allow_nan = flag(spec, "allow_nan")?;
    let generator = match width {
        64 => {
            let generator = prop::float(lo..=hi);
            if allow_nan {
                generator.allow_nan()
            } else {
                generator
            }
            .valued()
        }
        32 => {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "a bound of width 32 is a value that an f32 states exactly"
            )]
            let (lo, hi) = (lo as f32, hi as f32);
            let generator = prop::float(lo..=hi);
            if allow_nan {
                generator.allow_nan()
            } else {
                generator
            }
            .valued()
        }
        other => return Err(refused(format!("float width {other} is neither 32 nor 64"))),
    };
    Ok(generator)
}

/// Returns the generator of a boolean, true with the probability `p`.
fn boolean_of(spec: &Json) -> Result<Generator<Value>, SpecError> {
    let generator = match spec.get("p") {
        None | Some(Json::Null) => prop::boolean(),
        Some(Json::Array(parts)) if parts.len() == 2 => {
            let part = |at: usize| {
                parts[at]
                    .as_u64()
                    .ok_or_else(|| refused("p is not two integers"))
            };
            prop::boolean().odds(part(0)?, part(1)?)
        }
        Some(other) => {
            return Err(refused(format!(
                "p is {other}, want [numerator, denominator]"
            )));
        }
    };
    Ok(generator.valued())
}

/// Returns the generator of a string over the default alphabet or the
/// stated characters.
fn string_of(spec: &Json) -> Result<Generator<Value>, SpecError> {
    let sizes = sizes(spec)?;
    let generator = match sizes.max_size() {
        Some(max) => prop::string(sizes.min_size()..=max),
        None => prop::string(sizes.min_size()..),
    };
    let generator = match spec.get("alphabet") {
        None | Some(Json::Null) => generator,
        Some(Json::String(characters)) if !characters.is_empty() => generator.alphabet(characters),
        Some(other) => {
            return Err(refused(format!(
                "alphabet is {other}, not a non-empty string"
            )));
        }
    };
    Ok(generator.valued())
}

/// Refuses a pattern outside the portable subset, which the library's
/// generator would refuse with a panic.
fn check_pattern(pattern: &str) -> Result<(), SpecError> {
    let seat = dokimi_assert::Recorder::new();
    dokimi_assert::check::matches(&seat, "", pattern, "the pattern parses");
    match seat
        .failures()
        .first()
        .and_then(|failure| failure.field("reason").cloned())
    {
        Some(Value::String(reason)) => Err(refused(reason)),
        _ => Ok(()),
    }
}

/// Returns the generator of a recursive value, binding each self in its
/// extension to its position.
fn recursive_of(spec: &Json) -> Result<Generator<Value>, SpecError> {
    let max_leaves = count(spec, "max_leaves")?.unwrap_or(DEFAULT_MAX_LEAVES);
    if max_leaves < 1 {
        return Err(refused(format!("max_leaves is {max_leaves}, below 1")));
    }
    let base = build(&spec["base"], None)?;
    let extend = spec["extend"].clone();
    build(&extend, Some(&prop::just(Value::Null)))?;
    let recursive = prop::recursive(base, move |position| {
        build(&extend, Some(&position)).unwrap_or_else(|_| prop::just(Value::Null))
    });
    Ok(recursive.max_leaves(max_leaves))
}

/// Returns the function of a subject kind that takes one input.
#[must_use]
pub fn function(subject: &str) -> Option<fn(Value) -> Value> {
    Some(match subject {
        "identity" => |x| x,
        "is-non-negative" => |x| Value::Bool(matches!(x, Value::Int(n) if n >= 0)),
        "returns-null" => |_| Value::Null,
        "drops-the-first" => |x| match x {
            Value::List(items) => Value::List(items.into_iter().skip(1).collect()),
            Value::String(text) => Value::String(text.chars().skip(1).collect()),
            other => other,
        },
        "prepends-zero" => |x| match x {
            Value::List(mut items) => {
                items.insert(0, Value::Int(0));
                Value::List(items)
            }
            other => other,
        },
        "sorts" => |x| match x {
            Value::List(mut items) => {
                items.sort_by(order);
                Value::List(items)
            }
            other => other,
        },
        "wraps-in-a-and-b" => |x| match x {
            Value::String(text) => Value::String(format!("a{text}b")),
            other => other,
        },
        _ => return None,
    })
}
