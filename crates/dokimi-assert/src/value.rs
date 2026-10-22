// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The values that a failure's detail and a call record state.

use crate::files::Tree;
use crate::json::{Json, object};
use std::fmt::{self, Debug, Display, Write as _};

/// A value of a failure's detail or of a call record, as a typed literal of
/// the definition states it.
///
/// An assertion over a parameter of a known type states the parameter as
/// its literal: a `&str` as [`Value::String`], a count as [`Value::Int`]
/// and a tolerance as [`Value::Float`]. An assertion that is generic over
/// `T: Debug` states the caller's value as [`Value::Opaque`], with the text
/// of `{:?}`, because Rust cannot read the structure of a generic value.
///
/// Two values are equal when they have the same variant and equal contents.
/// A float compares as `f64` compares, so `Float(NAN)` is unequal to itself.
///
/// ```
/// use dokimi_assert::{Recorder, Value, check};
///
/// let r = Recorder::new();
/// check::length(&r, "abc", 2, "the code has two characters");
/// let failure = &r.failures()[0];
/// assert_eq!(failure.want(), Some(&Value::Int(2)));
/// assert_eq!(failure.got(), Some(&Value::Int(3)));
/// ```
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Value {
    /// The absence of a value, such as `None`.
    Null,
    /// A boolean.
    Bool(bool),
    /// An integer of any width up to 128 bits.
    Int(i128),
    /// An unsigned 128-bit integer above the range of `i128`. A smaller one
    /// is an [`Value::Int`].
    Uint(u128),
    /// A float. A value of an `f32` widens to `f64` without loss.
    Float(f64),
    /// Text.
    String(String),
    /// A byte string.
    Bytes(Vec<u8>),
    /// A sequence, in order.
    List(Vec<Value>),
    /// A map, as its entries in the order the map lists them.
    Map(Vec<(Value, Value)>),
    /// A record, as its fields in declaration order.
    Record(Vec<(String, Value)>),
    /// One variant of an enum, with its payload when the variant has one.
    Variant {
        /// The name of the variant.
        name: String,
        /// The payload of a variant that has one.
        payload: Option<Box<Value>>,
    },
    /// A tree of files, as a comparison of trees states it.
    Tree(Tree),
    /// A value outside the typed literals, as its `Debug` renders it.
    Opaque(String),
}

/// The most levels of objects and arrays that a literal nests. A store
/// entry's root object, its counterexample and the draw take three of a
/// file's 64.
const LITERAL_DEPTH: usize = 61;

/// The largest integer that a JSON reader in JavaScript keeps exact.
const SAFE: i128 = (1 << 53) - 1;

impl Value {
    /// Returns the opaque value of `value`, with the text of its `Debug`.
    pub(crate) fn debug<T: Debug + ?Sized>(value: &T) -> Self {
        Self::Opaque(format!("{value:?}"))
    }

    /// Returns the typed literal of the value, or its opaque literal when the
    /// typed literal would nest more than 61 levels.
    pub(crate) fn to_literal(&self) -> Json {
        let literal = self.literal();
        if literal.depth() > LITERAL_DEPTH {
            return object([("type", "opaque".into()), ("text", self.to_string().into())]);
        }
        literal
    }

    fn literal(&self) -> Json {
        match self {
            Self::Null => object([("type", "null".into())]),
            Self::Bool(held) => object([("type", "bool".into()), ("value", (*held).into())]),
            Self::Int(held) => object([("type", "int".into()), ("value", int(*held))]),
            Self::Uint(held) => {
                object([("type", "int".into()), ("value", held.to_string().into())])
            }
            Self::Float(held) => object([("type", "float".into()), ("value", float(*held))]),
            Self::String(text) => {
                object([("type", "string".into()), ("value", text.as_str().into())])
            }
            Self::Bytes(bytes) => object([("type", "bytes".into()), ("value", hex(bytes).into())]),
            Self::List(items) => list(items),
            Self::Map(entries) => object([
                ("type", "map".into()),
                (
                    "entries",
                    Json::Array(
                        entries
                            .iter()
                            .map(|(key, value)| Json::Array(vec![key.literal(), value.literal()]))
                            .collect(),
                    ),
                ),
            ]),
            Self::Record(fields) => object([
                ("type", "record".into()),
                (
                    "fields",
                    Json::Array(
                        fields
                            .iter()
                            .map(|(name, value)| {
                                Json::Array(vec![name.as_str().into(), value.literal()])
                            })
                            .collect(),
                    ),
                ),
            ]),
            Self::Variant { name, payload } => {
                let mut members = vec![
                    ("type".to_owned(), Json::from("variant")),
                    ("name".to_owned(), Json::from(name.as_str())),
                ];
                if let Some(payload) = payload {
                    members.push(("payload".to_owned(), payload.literal()));
                }
                Json::Object(members)
            }
            Self::Tree(tree) => tree.literal(),
            Self::Opaque(text) => {
                object([("type", "opaque".into()), ("text", text.as_str().into())])
            }
        }
    }

    /// Reads a typed literal of the definition's encoding.
    ///
    /// A list with `of` and a `value` of `null`, and a map with `key`, `of`
    /// and a `value` of `null`, state an absent container, which reads as
    /// [`Value::Null`].
    pub(crate) fn from_literal(literal: &Json) -> Result<Self, LiteralError> {
        let kind = literal
            .get("type")
            .and_then(Json::as_str)
            .ok_or(LiteralError("a literal without a type"))?;
        let value = literal.get("value");
        match kind {
            "null" => Ok(Self::Null),
            "bool" => value
                .and_then(Json::as_bool)
                .map(Self::Bool)
                .ok_or(LiteralError("a bool without a boolean value")),
            "int" => value.map_or(Err(LiteralError("an int without a value")), read_integer),
            "float" => value
                .map_or(Err(LiteralError("a float without a value")), read_float)
                .map(Self::Float),
            "string" => value
                .and_then(Json::as_str)
                .map(|text| Self::String(text.to_owned()))
                .ok_or(LiteralError("a string without a text value")),
            "bytes" => value
                .and_then(Json::as_str)
                .ok_or(LiteralError("bytes without a hexadecimal value"))
                .and_then(unhex)
                .map(Self::Bytes),
            "list" => read_list(literal),
            "map" => read_map(literal),
            "record" => read_record(literal),
            "variant" => read_variant(literal),
            "opaque" => literal
                .get("text")
                .and_then(Json::as_str)
                .map(|text| Self::Opaque(text.to_owned()))
                .ok_or(LiteralError("an opaque literal without a text")),
            _ => Err(LiteralError("a literal of a type that Rust does not read")),
        }
    }

    /// Returns the value that a field of a run's detail states, from the
    /// JSON of its call record: an object as a record, and a typed literal as
    /// its value.
    pub(crate) fn from_detail(json: &Json) -> Self {
        match json {
            Json::Null => Self::Null,
            Json::Bool(held) => Self::Bool(*held),
            Json::Int(held) => Self::Int(*held),
            Json::Float(held) => Self::Float(*held),
            Json::String(text) => Self::String(text.clone()),
            Json::Array(items) => Self::List(items.iter().map(Self::from_detail).collect()),
            Json::Object(members) => {
                if json.get("type").is_some() {
                    if let Ok(value) = Self::from_literal(json) {
                        return value;
                    }
                }
                Self::Record(
                    members
                        .iter()
                        .map(|(name, value)| (name.clone(), Self::from_detail(value)))
                        .collect(),
                )
            }
        }
    }
}

/// Why a JSON value is no typed literal that the library reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LiteralError(pub(crate) &'static str);

impl Display for LiteralError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// Returns the JSON of an integer: a number within ±(2^53 − 1), and its
/// decimal text beyond.
fn int(held: i128) -> Json {
    if (-SAFE..=SAFE).contains(&held) {
        return Json::Int(held);
    }
    Json::String(held.to_string())
}

/// Returns the JSON of a float, with NaN and the infinities by name.
fn float(held: f64) -> Json {
    if held.is_nan() {
        return "NaN".into();
    }
    if held.is_infinite() {
        return if held > 0.0 { "Inf" } else { "-Inf" }.into();
    }
    Json::Float(held)
}

/// Returns `bytes` in lowercase hexadecimal, as a typed literal states them.
pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(2 * bytes.len()), |mut text, byte| {
            // Writing to a String cannot fail.
            let _ = write!(text, "{byte:02x}");
            text
        })
}

fn unhex(text: &str) -> Result<Vec<u8>, LiteralError> {
    let lowercase = text
        .bytes()
        .all(|digit| digit.is_ascii_digit() || (b'a'..=b'f').contains(&digit));
    (0..text.len())
        .step_by(2)
        .map(|at| {
            text.get(at..at + 2)
                .and_then(|pair| u8::from_str_radix(pair, 16).ok())
        })
        .collect::<Option<_>>()
        .filter(|_| lowercase)
        .ok_or(LiteralError(
            "bytes whose value is no lowercase hexadecimal text",
        ))
}

/// Returns the scalar type of a value that a list's `of` states, with the
/// value's plain JSON.
fn scalar(value: &Value) -> Option<(&'static str, Json)> {
    match value {
        Value::Bool(held) => Some(("bool", Json::Bool(*held))),
        Value::Int(held) => Some(("int", int(*held))),
        Value::Float(held) => Some(("float", float(*held))),
        Value::String(text) => Some(("string", text.as_str().into())),
        _ => None,
    }
}

/// Returns the literal of a list: its plain values under `of` when every
/// element is a scalar of one type, and the literal of each element
/// otherwise.
fn list(items: &[Value]) -> Json {
    let scalars: Option<Vec<(&str, Json)>> = items.iter().map(scalar).collect();
    match scalars {
        Some(scalars)
            if scalars
                .first()
                .is_some_and(|(of, _)| scalars.iter().all(|(kind, _)| kind == of)) =>
        {
            let of = scalars[0].0;
            object([
                ("type", "list".into()),
                ("of", of.into()),
                (
                    "value",
                    Json::Array(scalars.into_iter().map(|(_, plain)| plain).collect()),
                ),
            ])
        }
        _ => object([
            ("type", "list".into()),
            (
                "items",
                Json::Array(items.iter().map(Value::literal).collect()),
            ),
        ]),
    }
}

fn read_int(value: &Json) -> Result<i128, LiteralError> {
    match value {
        Json::Int(held) => Ok(*held),
        Json::String(text) => text
            .parse()
            .map_err(|_| LiteralError("an int whose text is no integer")),
        _ => Err(LiteralError("an int whose value is no integer")),
    }
}

/// Reads an int of any width up to 128 bits, signed or unsigned.
fn read_integer(value: &Json) -> Result<Value, LiteralError> {
    match (read_int(value), value) {
        (Ok(held), _) => Ok(Value::Int(held)),
        (Err(refused), Json::String(text)) => text.parse().map(Value::Uint).map_err(|_| refused),
        (Err(refused), _) => Err(refused),
    }
}

fn read_float(value: &Json) -> Result<f64, LiteralError> {
    match value {
        Json::String(text) => match text.as_str() {
            "NaN" => Ok(f64::NAN),
            "Inf" => Ok(f64::INFINITY),
            "-Inf" => Ok(f64::NEG_INFINITY),
            _ => Err(LiteralError("a float whose text is not NaN, Inf or -Inf")),
        },
        number => number
            .as_f64()
            .ok_or(LiteralError("a float whose value is no number")),
    }
}

/// Reads the plain JSON value of a scalar of type `of`.
fn read_scalar(of: &str, value: &Json) -> Result<Value, LiteralError> {
    match of {
        "bool" => value.as_bool().map(Value::Bool).ok_or(LiteralError(
            "a list of bool with an element that is no boolean",
        )),
        "int" => read_int(value).map(Value::Int),
        "float" => read_float(value).map(Value::Float),
        "string" => value
            .as_str()
            .map(|text| Value::String(text.to_owned()))
            .ok_or(LiteralError(
                "a list of string with an element that is no text",
            )),
        _ => Err(LiteralError(
            "a scalar type that the encoding does not state",
        )),
    }
}

fn read_list(literal: &Json) -> Result<Value, LiteralError> {
    if let Some(items) = literal.get("items") {
        let items = items
            .as_array()
            .ok_or(LiteralError("a list whose items are no array"))?;
        return items
            .iter()
            .map(Value::from_literal)
            .collect::<Result<_, _>>()
            .map(Value::List);
    }
    let of = literal
        .get("of")
        .and_then(Json::as_str)
        .ok_or(LiteralError("a list without items or of"))?;
    match literal.get("value") {
        Some(Json::Null) => Ok(Value::Null),
        Some(Json::Array(values)) => values
            .iter()
            .map(|value| read_scalar(of, value))
            .collect::<Result<_, _>>()
            .map(Value::List),
        _ => Err(LiteralError("a list whose value is no array")),
    }
}

fn read_map(literal: &Json) -> Result<Value, LiteralError> {
    if let Some(entries) = literal.get("entries") {
        let entries = entries
            .as_array()
            .ok_or(LiteralError("a map whose entries are no array"))?;
        return entries
            .iter()
            .map(|entry| match entry.as_array() {
                Some([key, value]) => Ok((Value::from_literal(key)?, Value::from_literal(value)?)),
                _ => Err(LiteralError("a map entry that is no pair")),
            })
            .collect::<Result<_, _>>()
            .map(Value::Map);
    }
    let of = literal
        .get("of")
        .and_then(Json::as_str)
        .ok_or(LiteralError("a map without entries or of"))?;
    match literal.get("value") {
        Some(Json::Null) => Ok(Value::Null),
        Some(Json::Object(members)) => members
            .iter()
            .map(|(key, value)| Ok((Value::String(key.clone()), read_scalar(of, value)?)))
            .collect::<Result<_, _>>()
            .map(Value::Map),
        _ => Err(LiteralError("a map whose value is no object")),
    }
}

fn read_record(literal: &Json) -> Result<Value, LiteralError> {
    let fields = literal
        .get("fields")
        .and_then(Json::as_array)
        .ok_or(LiteralError("a record without fields"))?;
    let mut read: Vec<(String, Value)> = Vec::with_capacity(fields.len());
    for field in fields {
        let Some([Json::String(name), value]) = field.as_array() else {
            return Err(LiteralError("a record field that is no name and value"));
        };
        if read.iter().any(|(held, _)| held == name) {
            return Err(LiteralError("a record that names a field twice"));
        }
        read.push((name.clone(), Value::from_literal(value)?));
    }
    Ok(Value::Record(read))
}

fn read_variant(literal: &Json) -> Result<Value, LiteralError> {
    let name = literal
        .get("name")
        .and_then(Json::as_str)
        .ok_or(LiteralError("a variant without a name"))?;
    let payload = literal
        .get("payload")
        .map(Value::from_literal)
        .transpose()?
        .map(Box::new);
    Ok(Value::Variant {
        name: name.to_owned(),
        payload,
    })
}

/// Renders the value as a failure's sentence shows it: text in quotes with
/// Rust's escapes, a float with its fraction, and an opaque value as its
/// text.
impl Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => f.write_str("None"),
            Self::Bool(held) => write!(f, "{held}"),
            Self::Int(held) => write!(f, "{held}"),
            Self::Uint(held) => write!(f, "{held}"),
            Self::Float(held) => write!(f, "{held:?}"),
            Self::String(text) => write!(f, "{text:?}"),
            Self::Bytes(bytes) => write!(f, "b\"{}\"", bytes.escape_ascii()),
            Self::List(items) => {
                f.write_char('[')?;
                for (at, item) in items.iter().enumerate() {
                    if at > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{item}")?;
                }
                f.write_char(']')
            }
            Self::Map(entries) => {
                f.write_char('{')?;
                for (at, (key, value)) in entries.iter().enumerate() {
                    if at > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{key}: {value}")?;
                }
                f.write_char('}')
            }
            Self::Record(fields) => {
                f.write_str("{ ")?;
                for (at, (name, value)) in fields.iter().enumerate() {
                    if at > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{name}: {value}")?;
                }
                f.write_str(" }")
            }
            Self::Variant { name, payload } => match payload {
                Some(payload) => write!(f, "{name}({payload})"),
                None => f.write_str(name),
            },
            Self::Tree(tree) => write!(f, "{tree:?}"),
            Self::Opaque(text) => f.write_str(text),
        }
    }
}

macro_rules! from_integer {
    ($($t:ty)*) => {$(
        impl From<$t> for Value {
            fn from(held: $t) -> Self {
                Self::Int(i128::from(held))
            }
        }
    )*};
}

from_integer!(i8 i16 i32 i64 i128 u8 u16 u32 u64);

impl From<u128> for Value {
    fn from(held: u128) -> Self {
        i128::try_from(held).map_or(Self::Uint(held), Self::Int)
    }
}

impl From<isize> for Value {
    fn from(held: isize) -> Self {
        // isize is at most 64 bits on every platform that Rust supports.
        Self::Int(i128::try_from(held).unwrap_or(i128::MAX))
    }
}

impl From<usize> for Value {
    fn from(held: usize) -> Self {
        // usize is at most 64 bits on every platform that Rust supports.
        Self::Int(i128::try_from(held).unwrap_or(i128::MAX))
    }
}

impl From<f32> for Value {
    fn from(held: f32) -> Self {
        Self::Float(f64::from(held))
    }
}

impl From<f64> for Value {
    fn from(held: f64) -> Self {
        Self::Float(held)
    }
}

impl From<bool> for Value {
    fn from(held: bool) -> Self {
        Self::Bool(held)
    }
}

impl From<&str> for Value {
    fn from(text: &str) -> Self {
        Self::String(text.to_owned())
    }
}

impl From<String> for Value {
    fn from(text: String) -> Self {
        Self::String(text)
    }
}

impl From<char> for Value {
    fn from(c: char) -> Self {
        Self::String(c.to_string())
    }
}

impl From<&[u8]> for Value {
    fn from(bytes: &[u8]) -> Self {
        Self::Bytes(bytes.to_vec())
    }
}

impl From<Vec<u8>> for Value {
    fn from(bytes: Vec<u8>) -> Self {
        Self::Bytes(bytes)
    }
}

impl From<Vec<Value>> for Value {
    fn from(items: Vec<Value>) -> Self {
        Self::List(items)
    }
}

impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(held: Option<T>) -> Self {
        held.map_or(Self::Null, Into::into)
    }
}
