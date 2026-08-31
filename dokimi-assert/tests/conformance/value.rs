//! The seven types a corpus case can state, as one Rust type.
//!
//! The corpus states its arguments as typed literals so the same case can be read by
//! every implementation. Rust is statically typed, so a case's arguments arrive as one
//! enum rather than as whatever the language calls dynamic.
//!
//! The derived [`PartialEq`] is exactly what the standard asks equality to be. Two values
//! of different types are different variants and never compare. `f64` says NaN is unequal
//! to itself and that the two zeroes are equal. `Vec` and `BTreeMap` compare by their
//! contents. Nothing here had to be written to make that true.

use serde_json::Value as Json;
use std::collections::BTreeMap;

/// A value a corpus case can state.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// The absent value.
    Null,
    /// A boolean.
    Bool(bool),
    /// A whole number.
    Int(i64),
    /// A floating-point number.
    Float(f64),
    /// Text.
    Str(String),
    /// An ordered sequence.
    List(Vec<Value>),
    /// A mapping, ordered so two equal maps print the same way.
    Map(BTreeMap<String, Value>),
}

impl Value {
    /// The value as text, when it is text.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(held) => Some(held),
            _ => None,
        }
    }

    /// The value as a number, when it is one.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            #[expect(clippy::cast_precision_loss, reason = "a corpus literal is small")]
            Self::Int(held) => Some(*held as f64),
            Self::Float(held) => Some(*held),
            _ => None,
        }
    }
}

impl dokimi_assert::matcher::Container for Value {
    fn size(&self) -> usize {
        match self {
            Self::Str(held) => held.chars().count(),
            Self::List(held) => held.len(),
            Self::Map(held) => held.len(),
            // Nothing else has a length. The corpus never asks, because
            // a case that did would be asking about a Rust type error.
            _ => 0,
        }
    }
}

impl dokimi_assert::matcher::containment::Holds<Value> for Value {
    fn holds(&self, needle: &Value) -> bool {
        match (self, needle) {
            (Self::Str(text), Self::Str(part)) => text.contains(part.as_str()),
            (Self::List(items), _) => items.contains(needle),
            (Self::Map(entries), Self::Str(key)) => entries.contains_key(key),
            _ => false,
        }
    }
}

/// Read one typed literal.
///
/// The encoding names the type, so an int and a float that print the same are still
/// different values. Reading the JSON number alone would lose that.
pub fn decode(raw: &Json) -> Result<Value, String> {
    let named = raw
        .get("type")
        .and_then(Json::as_str)
        .ok_or("a literal names no type")?;
    let held = raw.get("value");

    let want = |what: &str| format!("a {named} literal carries no {what}");

    match named {
        "null" => Ok(Value::Null),
        "bool" => held
            .and_then(Json::as_bool)
            .map(Value::Bool)
            .ok_or_else(|| want("bool")),
        "int" => held
            .and_then(Json::as_i64)
            .map(Value::Int)
            .ok_or_else(|| want("int")),
        // JSON has no NaN or infinity, so a float literal accepts the
        // three names for them as strings. Both are cases the standard
        // states, so neither can be skipped.
        "float" => match held {
            Some(Json::String(named)) => match named.as_str() {
                "NaN" => Ok(Value::Float(f64::NAN)),
                "Inf" => Ok(Value::Float(f64::INFINITY)),
                "-Inf" => Ok(Value::Float(f64::NEG_INFINITY)),
                other => Err(format!("a float literal names an unknown value {other:?}")),
            },
            _ => held
                .and_then(Json::as_f64)
                .map(Value::Float)
                .ok_or_else(|| want("float")),
        },
        "string" => held
            .and_then(Json::as_str)
            .map(|text| Value::Str(text.to_owned()))
            .ok_or_else(|| want("string")),
        "list" => held
            .and_then(Json::as_array)
            .ok_or_else(|| want("list"))?
            .iter()
            .map(|item| decode_element(item, raw.get("of").and_then(Json::as_str)))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::List),
        "map" => {
            let entries = held.and_then(Json::as_object).ok_or_else(|| want("map"))?;
            let of = raw.get("of").and_then(Json::as_str);
            entries
                .iter()
                .map(|(key, item)| Ok((key.clone(), decode_element(item, of)?)))
                .collect::<Result<BTreeMap<_, _>, String>>()
                .map(Value::Map)
        }
        other => Err(format!("a literal names an unknown type {other:?}")),
    }
}

/// Read an element of a list or map, whose type the container states once.
fn decode_element(raw: &Json, of: Option<&str>) -> Result<Value, String> {
    if raw.is_object() && raw.get("type").is_some() {
        return decode(raw);
    }
    let named = of.ok_or("a container states neither its element type nor a typed element")?;
    decode(&serde_json::json!({ "type": named, "value": raw }))
}
