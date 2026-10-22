//! Shapes: a language-neutral description of the values of a type, which
//! the definition maps to one generator.
//!
//! A shape is a JSON object whose `shape` key names its id. A shape file is
//! a root shape that may also state `definitions`, the shapes that a `ref`
//! names, and `source`, the language and the type that it was read from,
//! which no reader compares. Two implementations that read one shape decode
//! the same value from the same choices, so two types with one shape produce
//! the same values from one seed in every language.

mod nodes;
pub(crate) mod reader;
pub(crate) mod time;
mod zone;

use super::generator::Generator;
use crate::json::{self, Json, object};
use crate::value::Value;
use std::fmt;

/// The levels of objects and arrays that a shape file may nest.
const MAX_DEPTH: usize = 128;

/// The largest integer that a JSON reader in JavaScript keeps exact.
const SAFE: i128 = (1 << 53) - 1;

/// The shape of a type: the description of the type's values that a shape
/// file states.
///
/// A type states its shape in [`Arbitrary::shape`](super::Arbitrary::shape)
/// with the constructors of this type and the constraints of its use:
///
/// ```
/// use dokimi_assert::prop::Shape;
///
/// let quantity = Shape::int(32, false).min(1).max(99);
/// let lines = Shape::list(Shape::string().max_size(8)).min_size(1);
/// let note = Shape::optional(Shape::string());
/// let order = Shape::record([("quantity", quantity), ("lines", lines), ("note", note)]);
/// # let _ = order;
/// ```
///
/// A constraint applies to the shape when the shape takes it, and otherwise
/// to the content of a list, a fixed-list, a set or an optional, so
/// `Shape::list(Shape::int(8, false)).max(9)` bounds the elements. A
/// constraint that applies to no part of the shape makes the shape fail to
/// read.
#[derive(Debug, Clone, PartialEq)]
#[must_use]
pub struct Shape {
    node: Result<Json, ShapeError>,
}

impl Shape {
    fn of(node: Json) -> Self {
        Self { node: Ok(node) }
    }

    /// Returns a shape that fails to read for `reason`.
    pub(crate) fn refused(reason: &str) -> Self {
        Self {
            node: Err(ShapeError::new(reason)),
        }
    }

    fn kind(kind: &str) -> Self {
        Self::of(object([("shape", kind.into())]))
    }

    /// Returns the shape of `content` inside a shape of the id `kind`, under
    /// `key`.
    fn around(kind: &str, members: Vec<(&str, Self)>) -> Self {
        let mut node = vec![("shape".to_owned(), Json::from(kind))];
        for (key, content) in members {
            match content.node {
                Ok(content) => node.push((key.to_owned(), content)),
                Err(refused) => return Self { node: Err(refused) },
            }
        }
        Self::of(Json::Object(node))
    }

    /// Returns the shape of a list of pairs of a name and a shape, which a
    /// record's fields and an enum's variants state.
    fn named<N: Into<String>>(
        kind: &str,
        key: &str,
        pairs: impl IntoIterator<Item = (N, Option<Self>)>,
    ) -> Self {
        let mut items = Vec::new();
        for (name, content) in pairs {
            let content = match content.map(|content| content.node) {
                None => Json::Null,
                Some(Ok(content)) => content,
                Some(Err(refused)) => return Self { node: Err(refused) },
            };
            items.push(Json::Array(vec![Json::String(name.into()), content]));
        }
        Self::of(Json::Object(vec![
            ("shape".to_owned(), Json::from(kind)),
            (key.to_owned(), Json::Array(items)),
        ]))
    }

    /// Returns the shape of a boolean.
    pub fn boolean() -> Self {
        Self::kind("bool")
    }

    /// Returns the shape of an integer of `width` bits, 8, 16, 32, 64 or 128,
    /// signed or unsigned, over the width's whole range.
    pub fn int(width: u32, signed: bool) -> Self {
        Self::of(object([
            ("shape", "int".into()),
            ("width", Json::Int(i128::from(width))),
            ("signed", signed.into()),
        ]))
    }

    /// Returns the shape of a float of `width` bits, 32 or 64, over the finite
    /// values of the width.
    pub fn float(width: u32) -> Self {
        Self::of(object([
            ("shape", "float".into()),
            ("width", Json::Int(i128::from(width))),
        ]))
    }

    /// Returns the shape of one character of the default alphabet.
    pub fn character() -> Self {
        Self::kind("char")
    }

    /// Returns the shape of text over the default alphabet.
    pub fn string() -> Self {
        Self::kind("string")
    }

    /// Returns the shape of a byte string.
    pub fn bytes() -> Self {
        Self::kind("bytes")
    }

    /// Returns the shape of a list of `of`'s values.
    pub fn list(of: Self) -> Self {
        Self::around("list", vec![("of", of)])
    }

    /// Returns the shape of a list of exactly `size` of `of`'s values.
    pub fn fixed_list(of: Self, size: usize) -> Self {
        let node = Self::around("fixed-list", vec![("of", of)]);
        node.with("size", Json::from(size))
    }

    /// Returns the shape of a set of `of`'s values: a list of distinct values.
    pub fn set(of: Self) -> Self {
        Self::around("set", vec![("of", of)])
    }

    /// Returns the shape of a map from `key`'s values to `of`'s values.
    pub fn map(key: Self, of: Self) -> Self {
        Self::around("map", vec![("key", key), ("of", of)])
    }

    /// Returns the shape of a value of `of`, or of no value.
    pub fn optional(of: Self) -> Self {
        Self::around("optional", vec![("of", of)])
    }

    /// Returns the shape of a record of named fields, in order.
    pub fn record<N: Into<String>>(fields: impl IntoIterator<Item = (N, Self)>) -> Self {
        Self::named(
            "record",
            "fields",
            fields.into_iter().map(|(name, of)| (name, Some(of))),
        )
    }

    /// Returns the shape of an enum of named variants, in order, each with
    /// the shape of its payload or without a payload.
    pub fn enumeration<N: Into<String>>(
        variants: impl IntoIterator<Item = (N, Option<Self>)>,
    ) -> Self {
        Self::named("enum", "variants", variants)
    }

    /// Returns the shape of one of `values`, in order.
    pub fn literal(values: impl IntoIterator<Item = Value>) -> Self {
        Self::of(object([
            ("shape", "literal".into()),
            (
                "values",
                Json::Array(values.into_iter().map(|value| value.to_literal()).collect()),
            ),
        ]))
    }

    /// Returns the shape of the definition named `name`.
    pub fn reference(name: &str) -> Self {
        Self::of(object([("shape", "ref".into()), ("name", name.into())]))
    }

    /// Returns the shape of a UUID.
    pub fn uuid() -> Self {
        Self::kind("uuid")
    }

    /// Returns the shape of an IP address of either version.
    pub fn ip_address() -> Self {
        Self::kind("ip-address")
    }

    /// Returns the shape of a decimal, which [`Shape::scale`] completes.
    pub fn decimal() -> Self {
        Self::kind("decimal")
    }

    /// Returns the shape of an instant at nanoseconds.
    pub fn instant() -> Self {
        Self::timed("instant")
    }

    /// Returns the shape of a date.
    pub fn date() -> Self {
        Self::kind("date")
    }

    /// Returns the shape of a time of day at nanoseconds.
    pub fn time_of_day() -> Self {
        Self::timed("time-of-day")
    }

    /// Returns the shape of a date and a time of day without a zone, at
    /// nanoseconds.
    pub fn local_date_time() -> Self {
        Self::timed("local-date-time")
    }

    /// Returns the shape of a signed duration at nanoseconds.
    pub fn duration() -> Self {
        Self::timed("duration")
    }

    /// Returns the shape of an offset from UTC.
    pub fn offset() -> Self {
        Self::kind("offset")
    }

    /// Returns the shape of a time zone of the definition's list.
    pub fn zone() -> Self {
        Self::kind("zone")
    }

    /// Returns the shape of an instant in a time zone, at nanoseconds.
    pub fn zoned_date_time() -> Self {
        Self::timed("zoned-date-time")
    }

    /// Returns the shape of a wall time in a time zone, at nanoseconds.
    pub fn wall_time() -> Self {
        Self::timed("wall-time")
    }

    fn timed(kind: &str) -> Self {
        Self::of(object([("shape", kind.into()), ("unit", "ns".into())]))
    }

    /// Returns the shape with `key` set to `value` on the shape itself.
    fn with(self, key: &str, value: Json) -> Self {
        let node = self.node.map(|node| put(&node, key, value));
        Self { node }
    }

    /// Returns the shape with a constraint placed where it applies.
    fn constrain(self, key: &str, value: Result<Json, ShapeError>) -> Self {
        let node = self
            .node
            .and_then(|node| value.and_then(|value| place(&node, key, value)));
        Self { node }
    }

    /// Returns the shape with the least value `bound`: an integer, a float,
    /// or the text of a decimal, a date, a time of day or an instant.
    pub fn min(self, bound: impl Into<Value>) -> Self {
        self.constrain("min", bound_json(bound.into()))
    }

    /// Returns the shape with the greatest value `bound`, as [`Shape::min`]
    /// states it.
    pub fn max(self, bound: impl Into<Value>) -> Self {
        self.constrain("max", bound_json(bound.into()))
    }

    /// Returns the shape with at least `n` elements, characters or bytes.
    pub fn min_size(self, n: usize) -> Self {
        self.constrain("min_size", Ok(Json::from(n)))
    }

    /// Returns the shape with at most `n` elements, characters or bytes.
    pub fn max_size(self, n: usize) -> Self {
        self.constrain("max_size", Ok(Json::from(n)))
    }

    /// Returns the shape of text that `pattern`, of the portable subset,
    /// matches in full.
    pub fn pattern(self, pattern: &str) -> Self {
        self.constrain("pattern", Ok(Json::from(pattern)))
    }

    /// Returns the shape of text over `characters`, in their stated order.
    pub fn alphabet(self, characters: &str) -> Self {
        self.constrain("alphabet", Ok(Json::from(characters)))
    }

    /// Returns the shape of a float that admits NaN.
    pub fn allow_nan(self) -> Self {
        self.constrain("allow_nan", Ok(Json::Bool(true)))
    }

    /// Returns the shape of a float that admits the infinities, unbounded on
    /// both sides unless a bound states otherwise.
    pub fn allow_infinity(self) -> Self {
        self.constrain("allow_infinity", Ok(Json::Bool(true)))
    }

    /// Returns the shape of a time at the resolution `unit`: `s`, `ms`, `us`
    /// or `ns`.
    pub fn unit(self, unit: &str) -> Self {
        self.constrain("unit", Ok(Json::from(unit)))
    }

    /// Returns the shape of a decimal with `digits` digits after the point.
    pub fn scale(self, digits: u32) -> Self {
        self.constrain("scale", Ok(Json::Int(i128::from(digits))))
    }

    /// Returns the shape of an IP address of `version`, 4 or 6.
    pub fn version(self, version: u8) -> Self {
        self.constrain("version", Ok(Json::Int(i128::from(version))))
    }

    /// Returns the JSON of the shape, or the constraint that it refused.
    pub(crate) fn json(&self) -> Result<&Json, ShapeError> {
        self.node.as_ref().map_err(Clone::clone)
    }
}

/// Returns the object `node` with the member `key` set to `value`, in place
/// of an earlier member of the key. Every node of a shape is an object.
pub(crate) fn put(node: &Json, key: &str, value: Json) -> Json {
    let mut members = node
        .as_object()
        .expect("every node of a shape is an object")
        .to_vec();
    match members.iter_mut().find(|(held, _)| held == key) {
        Some((_, held)) => *held = value,
        None => members.push((key.to_owned(), value)),
    }
    Json::Object(members)
}

/// Returns `node` with a constraint placed on the shape when the shape takes
/// it, and otherwise on the content of a list, a fixed-list, a set or an
/// optional.
fn place(node: &Json, key: &str, value: Json) -> Result<Json, ShapeError> {
    let kind = node
        .get("shape")
        .and_then(Json::as_str)
        .unwrap_or_default()
        .to_owned();
    let takes = reader::keys(&kind)
        .is_some_and(|(required, optional)| required.contains(&key) || optional.contains(&key));
    if takes {
        return Ok(put(node, key, value));
    }
    if matches!(kind.as_str(), "list" | "fixed-list" | "set" | "optional") {
        let placed = place(node.get("of").unwrap_or(&Json::Null), key, value)?;
        return Ok(put(node, "of", placed));
    }
    Err(ShapeError::new(format!(
        "the constraint {key} applies to no part of the {kind} shape"
    )))
}

/// Returns the JSON of a bound: an integer within ±(2^53 − 1) as a number and
/// beyond as its decimal text, a float as a number or by name, and text as
/// it is.
fn bound_json(bound: Value) -> Result<Json, ShapeError> {
    match bound {
        Value::Int(held) if held.abs() <= SAFE => Ok(Json::Int(held)),
        Value::Int(held) => Ok(Json::String(held.to_string())),
        Value::Uint(held) => Ok(Json::String(held.to_string())),
        Value::Float(held) if held.is_nan() => Ok("NaN".into()),
        Value::Float(held) if held.is_infinite() => {
            Ok(if held > 0.0 { "Inf" } else { "-Inf" }.into())
        }
        Value::Float(held) => Ok(Json::Float(held)),
        Value::String(text) => Ok(Json::String(text)),
        other => Err(ShapeError::new(format!(
            "the bound {other} is no number and no text"
        ))),
    }
}

/// A shape that does not read, or a value that is no value of a type.
///
/// A shape does not read for a shape that the vocabulary does not have, a
/// key that a shape does not take, a parameter that does not read, a ref to
/// no definition, or a definition without a finite value. The text states
/// where in the shape file the fault is, such as
/// `shape.lines.of.width is 7, not 32 or 64`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapeError {
    reason: String,
}

impl ShapeError {
    /// Returns the error of `reason`, one clause that states what is wrong,
    /// such as the refusal of [`Arbitrary::from_value`] for a value that is
    /// no value of its type.
    ///
    /// [`Arbitrary::from_value`]: super::Arbitrary::from_value
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }

    /// Returns the clause that states what is wrong.
    pub(crate) fn reason(&self) -> &str {
        &self.reason
    }
}

impl fmt::Display for ShapeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "prop: {}", self.reason)
    }
}

impl std::error::Error for ShapeError {}

/// Returns the generator of a shape file, which is JSON text.
///
/// A record decodes to [`Value::Record`] and a variant to [`Value::Variant`].
/// Every other shape decodes to the value that the definition states for it:
/// an instant to a record of its seconds and its units within the second, a
/// date to its days since 1970-01-01, a decimal to its unscaled integer, and
/// a UUID and an IP address to their bytes.
///
/// ```
/// use dokimi_assert::prop;
///
/// let orders = prop::of_shape(r#"{"shape": "record", "fields": [
///     ["id", {"shape": "int", "width": 32, "signed": false}],
///     ["note", {"shape": "optional", "of": {"shape": "string", "max_size": 8}}]
/// ]}"#)?;
/// # let _ = orders;
/// # Ok::<(), prop::ShapeError>(())
/// ```
///
/// # Errors
///
/// Returns [`ShapeError`] for text that is no JSON, and for a shape that does
/// not read.
pub fn of_shape(text: &str) -> Result<Generator<Value>, ShapeError> {
    let document: Json = json::parse(text, MAX_DEPTH)
        .map_err(|malformed| ShapeError::new(format!("the shape file is no JSON: {malformed}")))?;
    reader::read(&document, &[])
}
