// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The typed literals of the definition, decoded to the values that a driver
//! hands an assertion, and compared with the values of a record.

use dokimi_assert::Value;
use dokimi_assert::files::{self, Entry, Tree};
use serde_json::Value as Json;
use std::fmt;

/// A typed literal that states no type of the definition, or a value that
/// its type does not allow.
#[derive(Debug, Clone, PartialEq)]
pub struct Malformed(pub Json);

impl fmt::Display for Malformed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "a malformed typed literal: {}", self.0)
    }
}

impl std::error::Error for Malformed {}

/// Returns the canonical text of a value, which two values share exactly
/// when the definition compares them equal: a float compares by its bits,
/// with every NaN one value, an int differs from a float of the same value,
/// and a map compares its entries in any order.
#[must_use]
pub fn canonical(value: &Value) -> String {
    let joined = |items: Vec<String>| items.join(",");
    match value {
        Value::Null => "null".to_owned(),
        Value::Bool(held) => format!("bool:{held}"),
        Value::Int(held) => format!("int:{held}"),
        Value::Uint(held) => format!("int:{held}"),
        Value::Float(held) if held.is_nan() => "float:NaN".to_owned(),
        Value::Float(held) => format!("float:{:x}", held.to_bits()),
        Value::String(text) => format!("string:{text:?}"),
        Value::Bytes(bytes) => format!("bytes:{bytes:?}"),
        Value::List(items) => format!("list:[{}]", joined(items.iter().map(canonical).collect())),
        Value::Map(entries) => {
            let mut pairs: Vec<String> = entries
                .iter()
                .map(|(key, held)| format!("{}={}", canonical(key), canonical(held)))
                .collect();
            pairs.sort();
            format!("map:{{{}}}", joined(pairs))
        }
        Value::Record(fields) => format!(
            "record:{{{}}}",
            joined(
                fields
                    .iter()
                    .map(|(name, held)| format!("{name:?}={}", canonical(held)))
                    .collect()
            )
        ),
        Value::Variant { name, payload } => format!(
            "variant:{name:?}({})",
            payload.as_deref().map(canonical).unwrap_or_default()
        ),
        other => format!("{other:?}"),
    }
}

/// Returns the value that a typed literal states.
///
/// A reference decodes to its value. The overlay of Rust declines
/// `by-identity`, so no case that applies to Rust compares references by
/// identity.
///
/// # Errors
///
/// Returns [`Malformed`] for a literal of a type that the definition does
/// not define, or with a value that its type does not allow.
pub fn decode(literal: &Json) -> Result<Value, Malformed> {
    read(literal).ok_or_else(|| Malformed(literal.clone()))
}

fn read(literal: &Json) -> Option<Value> {
    let value = &literal["value"];
    match literal["type"].as_str()? {
        "null" => Some(Value::Null),
        "bool" => value.as_bool().map(Value::Bool),
        "int" => int(value).map(Value::Int),
        "float" => float(value).map(Value::Float),
        "string" => value.as_str().map(|text| Value::String(text.to_owned())),
        "bytes" => value.as_str().and_then(unhex).map(Value::Bytes),
        "list" => list(literal),
        "map" => map(literal),
        "record" => record(literal),
        "variant" => variant(literal),
        "tree" => read_tree(literal).map(Value::Tree),
        "reference" => read(value),
        "opaque" => literal["text"]
            .as_str()
            .map(|text| Value::Opaque(text.to_owned())),
        _ => None,
    }
}

fn int(value: &Json) -> Option<i128> {
    match value {
        Json::String(text) => text.parse().ok(),
        Json::Number(number) => number.as_i128(),
        _ => None,
    }
}

fn float(value: &Json) -> Option<f64> {
    match value.as_str() {
        Some("NaN") => Some(f64::NAN),
        Some("Inf") => Some(f64::INFINITY),
        Some("-Inf") => Some(f64::NEG_INFINITY),
        Some(_) => None,
        None => value.as_f64(),
    }
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(text.get(at..at + 2)?, 16).ok())
        .collect()
}

fn scalar(of: &str, value: &Json) -> Option<Value> {
    match of {
        "bool" => value.as_bool().map(Value::Bool),
        "int" => int(value).map(Value::Int),
        "float" => float(value).map(Value::Float),
        "string" => value.as_str().map(|text| Value::String(text.to_owned())),
        _ => None,
    }
}

fn list(literal: &Json) -> Option<Value> {
    if let Some(items) = literal["items"].as_array() {
        return items
            .iter()
            .map(read)
            .collect::<Option<_>>()
            .map(Value::List);
    }
    let of = literal["of"].as_str()?;
    match &literal["value"] {
        Json::Null => Some(Value::Null),
        Json::Array(values) => values
            .iter()
            .map(|value| scalar(of, value))
            .collect::<Option<_>>()
            .map(Value::List),
        _ => None,
    }
}

fn map(literal: &Json) -> Option<Value> {
    if let Some(entries) = literal["entries"].as_array() {
        return entries
            .iter()
            .map(|entry| Some((read(&entry[0])?, read(&entry[1])?)))
            .collect::<Option<_>>()
            .map(Value::Map);
    }
    let of = literal["of"].as_str()?;
    match &literal["value"] {
        Json::Null => Some(Value::Null),
        Json::Object(members) => members
            .iter()
            .map(|(key, value)| Some((Value::String(key.clone()), scalar(of, value)?)))
            .collect::<Option<_>>()
            .map(Value::Map),
        _ => None,
    }
}

fn record(literal: &Json) -> Option<Value> {
    literal["fields"]
        .as_array()?
        .iter()
        .map(|field| Some((field[0].as_str()?.to_owned(), read(&field[1])?)))
        .collect::<Option<_>>()
        .map(Value::Record)
}

fn variant(literal: &Json) -> Option<Value> {
    let name = literal["name"].as_str()?.to_owned();
    let payload = match literal.get("payload") {
        Some(payload) => Some(Box::new(read(payload)?)),
        None => None,
    };
    Some(Value::Variant { name, payload })
}

/// Returns the tree that a tree literal of an input states, built with the
/// constructors of `dokimi_assert::files`.
///
/// # Errors
///
/// Returns [`Malformed`] for a literal that is no tree literal of an input:
/// an entry without a path or without exactly one of `text`, `bytes`,
/// `directory` and `link`, a mode beyond the nine permission bits or on a
/// link, and an executable file of bytes, for which the API has no
/// constructor.
pub fn tree(literal: &Json) -> Result<Tree, Malformed> {
    read_tree(literal).ok_or_else(|| Malformed(literal.clone()))
}

/// Returns the paths and the entries that a tree literal of an input
/// states, in its order.
///
/// # Errors
///
/// Returns [`Malformed`] for the literals that [`tree`] refuses.
pub fn entries(literal: &Json) -> Result<Vec<(String, Entry)>, Malformed> {
    read_entries(literal).ok_or_else(|| Malformed(literal.clone()))
}

fn read_tree(literal: &Json) -> Option<Tree> {
    read_entries(literal).map(|entries| entries.into_iter().collect())
}

fn read_entries(literal: &Json) -> Option<Vec<(String, Entry)>> {
    if literal["type"] != "tree" {
        return None;
    }
    literal["entries"].as_array()?.iter().map(entry).collect()
}

fn entry(literal: &Json) -> Option<(String, Entry)> {
    let path = literal["path"].as_str()?.to_owned();
    let executable = literal
        .get("executable")
        .map_or(Some(false), Json::as_bool)?;
    let kinds = ["text", "bytes", "directory", "link"]
        .iter()
        .filter(|kind| literal.get(**kind).is_some())
        .count();
    if kinds != 1 {
        return None;
    }
    let entry = if let Some(text) = literal.get("text") {
        let text = text.as_str()?;
        if executable {
            files::executable(text)
        } else {
            files::text(text)
        }
    } else if let Some(bytes) = literal.get("bytes") {
        if executable {
            return None;
        }
        files::bytes(unhex(bytes.as_str()?)?)
    } else if let Some(target) = literal.get("link") {
        files::link(target.as_str()?)
    } else if literal["directory"] == true {
        files::directory()
    } else {
        return None;
    };
    let Some(mode) = literal.get("mode") else {
        return Some((path, entry));
    };
    let mode = u32::try_from(mode.as_u64()?)
        .ok()
        .filter(|mode| *mode <= 0o777)?;
    if literal.get("link").is_some() {
        return None;
    }
    Some((path, entry.with_mode(mode)))
}

/// How the driver handed a case its values, which decides the text of an
/// opaque value in the record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rendering {
    /// The driver handed the assertion [`Value`]s, so an opaque field states
    /// the `Debug` of a `Value`.
    Values,
    /// The driver handed each list of the case as a slice of [`Value`]s, so
    /// an opaque field of a list states the `Debug` of the slice.
    Slices,
    /// A subject produced native values, such as an `i64` or a `Vec<i64>`, so
    /// an opaque field states their `Debug`.
    Native,
}

/// Returns the `Debug` text of the native Rust value that `value` stands
/// for: an integer as `1`, a list as `[1, 2]`.
fn native(value: &Value) -> String {
    match value {
        Value::Int(held) => held.to_string(),
        Value::Float(held) => format!("{held:?}"),
        Value::Bool(held) => held.to_string(),
        Value::String(text) => format!("{text:?}"),
        Value::List(items) => format!(
            "[{}]",
            items.iter().map(native).collect::<Vec<_>>().join(", ")
        ),
        Value::Null => "None".to_owned(),
        other => format!("{other:?}"),
    }
}

/// A value of a record that differs from the value that a case states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Difference {
    /// The value that the record states, as its `Debug` renders it.
    pub stated: String,
    /// The value that the case states, under the rendering of the driver.
    pub expected: String,
}

impl fmt::Display for Difference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the record states {}, and the case states {}",
            self.stated, self.expected
        )
    }
}

/// Returns how the record's value `got` differs from the case's value
/// `want`, or `None` when the two agree.
///
/// A typed value compares with the case's value as the definition compares
/// literals: a map compares its entries in any order, and a float compares
/// its bits, so NaN equals NaN. An opaque value agrees when its text is the
/// `Debug` text of the case's value under the rendering of the driver.
#[must_use]
pub fn differs(got: &Value, want: &Value, rendering: Rendering) -> Option<Difference> {
    if let Value::Opaque(text) = got {
        let expected = match (rendering, want) {
            (Rendering::Slices, Value::List(items)) => format!("{items:?}"),
            (Rendering::Values | Rendering::Slices, _) => format!("{want:?}"),
            (Rendering::Native, _) => native(want),
        };
        return (*text != expected).then(|| Difference {
            stated: text.clone(),
            expected,
        });
    }
    (!same(got, want)).then(|| Difference {
        stated: format!("{got:?}"),
        expected: format!("{want:?}"),
    })
}

fn same(got: &Value, want: &Value) -> bool {
    match (got, want) {
        (Value::Float(a), Value::Float(b)) => {
            a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
        }
        (Value::List(a), Value::List(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| same(x, y))
        }
        (Value::Map(a), Value::Map(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, v)| b.iter().any(|(l, w)| same(k, l) && same(v, w)))
        }
        (Value::Record(a), Value::Record(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|((m, x), (n, y))| m == n && same(x, y))
        }
        _ => got == want,
    }
}
