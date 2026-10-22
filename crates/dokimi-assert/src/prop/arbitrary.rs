// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! Inputs derived from types: the trait that states the shape of a type, and
//! its implementations for the types of the standard library.
//!
//! A property form generates its input from the shape of the input's type,
//! so two types with one shape produce the same values from one seed in every
//! language of the definition. `#[derive(Arbitrary)]`, behind the feature
//! `derive`, implements the trait for a struct and an enum. The features
//! `chrono`, `jiff`, `rust_decimal`, `time` and `uuid` implement it for the
//! types of those crates.

#[cfg(feature = "chrono")]
mod chrono;
#[cfg(feature = "jiff")]
mod jiff;
#[cfg(feature = "rust_decimal")]
mod rust_decimal;
#[cfg(feature = "time")]
mod time;
#[cfg(feature = "uuid")]
mod uuid;

use super::generator::Generator;
use super::registry::{self, convert_from, convert_to};
use super::scope::Generators;
use super::shape::{Shape, ShapeError};
use crate::value::Value;
use std::any::TypeId;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::hash::Hash;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The nanoseconds of a second.
const NANOS_PER_SECOND: i128 = 1_000_000_000;

/// A type whose values a property generates from the type's shape.
///
/// [`Arbitrary::shape`] states the shape, and [`Arbitrary::from_value`] builds
/// a value from the value that the shape's generator decodes. That value is
/// in the native form of its shape:
///
/// - A record is a [`Value::Record`] of its fields, and an enum a
///   [`Value::Variant`].
/// - An instant is a [`Value::Int`] of nanoseconds since
///   1970-01-01T00:00:00Z, a time of day of nanoseconds since midnight, a
///   local date and time of nanoseconds since 1970-01-01T00:00:00 of its
///   calendar, and a duration of nanoseconds. A date is an int of days since
///   1970-01-01, and an offset of seconds east of UTC.
/// - An instant in a zone is a record of its instant and the name of its
///   zone, and a wall time in a zone a record of its local date and time and
///   the name of its zone.
/// - A decimal is a [`Value::String`] of its digits, such as `-12.34`.
/// - A UUID and an IP address are [`Value::Bytes`] in network order.
/// - Every other shape is the value of its generator: an int, a float, a
///   bool, text, bytes, a list or a map.
///
/// ```
/// use dokimi_assert::Value;
/// use dokimi_assert::prop::{Arbitrary, Definitions, Shape, ShapeError};
///
/// #[derive(Debug, Clone, PartialEq)]
/// struct Percent(u8);
///
/// impl Arbitrary for Percent {
///     fn shape(_: &mut Definitions) -> Shape {
///         Shape::int(8, false).max(100)
///     }
///
///     fn from_value(value: Value) -> Result<Self, ShapeError> {
///         u8::from_value(value).map(Percent)
///     }
///
///     fn to_value(&self) -> Value {
///         self.0.to_value()
///     }
/// }
/// ```
pub trait Arbitrary: Sized + 'static {
    /// Returns the shape of the type. A type that can refer to itself states
    /// its shape through [`Definitions::define`], and states the shape of
    /// each part through [`Definitions::shape`].
    fn shape(definitions: &mut Definitions) -> Shape;

    /// Builds a value from the value that a generator of the type's shape
    /// decodes, in the native form of the shape.
    ///
    /// # Errors
    ///
    /// Returns [`ShapeError`] for a value that is no value of the type.
    fn from_value(value: Value) -> Result<Self, ShapeError>;

    /// Returns the value that [`Arbitrary::from_value`] turns back into
    /// `self`.
    fn to_value(&self) -> Value;

    /// Returns the shape of a `Vec` of the type: a list of the type's shape,
    /// and bytes for `Vec<u8>`.
    #[doc(hidden)]
    fn list_shape(definitions: &mut Definitions) -> Shape {
        Shape::list(definitions.shape::<Self>())
    }

    /// Builds a `Vec` of the type from the value of its list shape.
    ///
    /// # Errors
    ///
    /// Returns [`ShapeError`] for a value that is no list of the type.
    #[doc(hidden)]
    fn list_from_value(value: Value) -> Result<Vec<Self>, ShapeError> {
        match value {
            Value::List(items) => items.into_iter().map(convert_from).collect(),
            other => Err(mismatch("a list", &other)),
        }
    }

    /// Returns the value of a `Vec` of the type.
    #[doc(hidden)]
    fn list_to_value(items: &[Self]) -> Value {
        Value::List(items.iter().map(convert_to).collect())
    }
}

/// The definitions of the types that refer to themselves, which a shape file
/// states beside its root shape, and the types whose shapes are being
/// stated.
#[derive(Debug, Default)]
pub struct Definitions {
    pub(crate) shapes: Vec<(String, Shape)>,
    open: Vec<(TypeId, bool)>,
    defined: Vec<TypeId>,
    /// The registered and the stated generators that the shape refers to, by
    /// the name of their type.
    pub(crate) externals: Vec<(String, Generator<Value>)>,
    /// The generators of the run that reads the type, or `None` for a read
    /// of the test process.
    pub(crate) using: Option<Arc<Generators>>,
}

impl Definitions {
    /// Returns the definitions of a read under `using`, the generators of a
    /// run, or of a read of the test process for `None`.
    pub(crate) fn under(using: Option<Arc<Generators>>) -> Self {
        Self {
            using,
            ..Self::default()
        }
    }

    /// Returns the shape of `T` at a place where `T` occurs: a ref to the
    /// generator that the run states for `T`, the shape that a registration
    /// states for `T`, or else `T`'s own shape.
    pub fn shape<T: Arbitrary>(&mut self) -> Shape {
        if let Some(external) = self.using.as_ref().and_then(|using| using.external::<T>()) {
            let name = std::any::type_name::<T>();
            self.externals.push((name.to_owned(), external));
            return Shape::reference(name);
        }
        match registry::shape_of_type::<T>(self) {
            Some(shape) => shape,
            None => T::shape(self),
        }
    }

    /// Returns the shape that `build` states for `T`.
    ///
    /// When `T` occurs again while `build` runs, the shape of `T` becomes a
    /// definition named after `T`, and every place where `T` occurs a ref to
    /// it. A type without such a recursion has its shape inline.
    pub fn define<T: 'static>(&mut self, build: impl FnOnce(&mut Self) -> Shape) -> Shape {
        let id = TypeId::of::<T>();
        let name = std::any::type_name::<T>();
        if self.defined.contains(&id) {
            return Shape::reference(name);
        }
        if let Some(open) = self.open.iter_mut().find(|(held, _)| *held == id) {
            open.1 = true;
            return Shape::reference(name);
        }
        self.open.push((id, false));
        let shape = build(self);
        let recursive = self.open.pop().is_some_and(|(_, recursive)| recursive);
        if !recursive {
            return shape;
        }
        self.shapes.push((name.to_owned(), shape));
        self.defined.push(id);
        Shape::reference(name)
    }
}

/// Returns the refusal of a value of another form than a shape decodes.
pub(crate) fn mismatch(expected: &str, value: &Value) -> ShapeError {
    ShapeError::new(format!("the value {value} is not {expected}"))
}

/// Returns the integer of a value, or the refusal of another value.
fn integer_of(value: &Value) -> Result<i128, ShapeError> {
    match value {
        Value::Int(held) => Ok(*held),
        other => Err(mismatch("an int", other)),
    }
}

impl Arbitrary for bool {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::boolean()
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        match value {
            Value::Bool(held) => Ok(held),
            other => Err(mismatch("a bool", &other)),
        }
    }

    fn to_value(&self) -> Value {
        Value::Bool(*self)
    }
}

macro_rules! integers {
    ($($t:ty: $width:expr, $signed:expr;)*) => {$(
        impl Arbitrary for $t {
            fn shape(_: &mut Definitions) -> Shape {
                Shape::int($width, $signed)
            }

            fn from_value(value: Value) -> Result<Self, ShapeError> {
                let held = integer_of(&value)?;
                <$t>::try_from(held).map_err(|_| mismatch(concat!("an int of ", stringify!($t)), &value))
            }

            fn to_value(&self) -> Value {
                Value::from(*self)
            }
        }
    )*};
}

integers! {
    i8: 8, true;
    i16: 16, true;
    i32: 32, true;
    i64: 64, true;
    isize: usize::BITS, true;
    u16: 16, false;
    u32: 32, false;
    u64: 64, false;
    usize: usize::BITS, false;
}

/// A byte reads as an unsigned 8-bit integer, and a `Vec` of bytes reads as
/// bytes.
impl Arbitrary for u8 {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::int(8, false)
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        let held = integer_of(&value)?;
        u8::try_from(held).map_err(|_| mismatch("an int of u8", &value))
    }

    fn to_value(&self) -> Value {
        Value::from(*self)
    }

    fn list_shape(_: &mut Definitions) -> Shape {
        Shape::bytes()
    }

    fn list_from_value(value: Value) -> Result<Vec<Self>, ShapeError> {
        match value {
            Value::Bytes(bytes) => Ok(bytes),
            other => Err(mismatch("bytes", &other)),
        }
    }

    fn list_to_value(items: &[Self]) -> Value {
        Value::Bytes(items.to_vec())
    }
}

impl Arbitrary for i128 {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::int(128, true)
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        integer_of(&value)
    }

    fn to_value(&self) -> Value {
        Value::Int(*self)
    }
}

impl Arbitrary for u128 {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::int(128, false)
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        match value {
            Value::Uint(held) => Ok(held),
            Value::Int(held) => {
                u128::try_from(held).map_err(|_| mismatch("an int of u128", &value))
            }
            other => Err(mismatch("an int of u128", &other)),
        }
    }

    fn to_value(&self) -> Value {
        Value::from(*self)
    }
}

impl Arbitrary for f64 {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::float(64)
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        match value {
            Value::Float(held) => Ok(held),
            other => Err(mismatch("a float", &other)),
        }
    }

    fn to_value(&self) -> Value {
        Value::Float(*self)
    }
}

impl Arbitrary for f32 {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::float(32)
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "a value of a float of width 32 is an f32, which the cast states exactly"
    )]
    fn from_value(value: Value) -> Result<Self, ShapeError> {
        match value {
            Value::Float(held) => Ok(held as f32),
            other => Err(mismatch("a float", &other)),
        }
    }

    fn to_value(&self) -> Value {
        Value::from(*self)
    }
}

impl Arbitrary for char {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::character()
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        if let Value::String(text) = &value {
            let mut chars = text.chars();
            if let (Some(c), None) = (chars.next(), chars.next()) {
                return Ok(c);
            }
        }
        Err(mismatch("one character", &value))
    }

    fn to_value(&self) -> Value {
        Value::from(*self)
    }
}

impl Arbitrary for String {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::string()
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        match value {
            Value::String(text) => Ok(text),
            other => Err(mismatch("text", &other)),
        }
    }

    fn to_value(&self) -> Value {
        Value::from(self.as_str())
    }
}

impl<T: Arbitrary> Arbitrary for Vec<T> {
    fn shape(definitions: &mut Definitions) -> Shape {
        T::list_shape(definitions)
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        T::list_from_value(value)
    }

    fn to_value(&self) -> Value {
        T::list_to_value(self)
    }
}

impl<T: Arbitrary, const N: usize> Arbitrary for [T; N] {
    fn shape(definitions: &mut Definitions) -> Shape {
        Shape::fixed_list(definitions.shape::<T>(), N)
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        let Value::List(items) = value else {
            return Err(mismatch("a list", &value));
        };
        let items: Vec<T> = items
            .into_iter()
            .map(convert_from)
            .collect::<Result<_, _>>()?;
        items.try_into().map_err(|items: Vec<T>| {
            ShapeError::new(format!(
                "a list of {} items is no array of {N}",
                items.len()
            ))
        })
    }

    fn to_value(&self) -> Value {
        Value::List(self.iter().map(convert_to).collect())
    }
}

impl<T: Arbitrary> Arbitrary for Option<T> {
    fn shape(definitions: &mut Definitions) -> Shape {
        Shape::optional(definitions.shape::<T>())
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        match value {
            Value::Null => Ok(None),
            other => convert_from(other).map(Some),
        }
    }

    fn to_value(&self) -> Value {
        self.as_ref().map_or(Value::Null, convert_to)
    }
}

/// A box reads as its content.
impl<T: Arbitrary> Arbitrary for Box<T> {
    fn shape(definitions: &mut Definitions) -> Shape {
        definitions.shape::<T>()
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        convert_from(value).map(Box::new)
    }

    fn to_value(&self) -> Value {
        convert_to(&**self)
    }
}

/// Returns the entries of a map's value.
fn entries_of<K: Arbitrary, V: Arbitrary>(value: Value) -> Result<Vec<(K, V)>, ShapeError> {
    match value {
        Value::Map(entries) => entries
            .into_iter()
            .map(|(key, held)| Ok((convert_from(key)?, convert_from(held)?)))
            .collect(),
        other => Err(mismatch("a map", &other)),
    }
}

impl<K: Arbitrary + Eq + Hash, V: Arbitrary, S: std::hash::BuildHasher + Default + 'static>
    Arbitrary for HashMap<K, V, S>
{
    fn shape(definitions: &mut Definitions) -> Shape {
        Shape::map(definitions.shape::<K>(), definitions.shape::<V>())
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        entries_of(value).map(|entries| entries.into_iter().collect())
    }

    fn to_value(&self) -> Value {
        Value::Map(
            self.iter()
                .map(|(key, held)| (convert_to(key), convert_to(held)))
                .collect(),
        )
    }
}

impl<K: Arbitrary + Ord, V: Arbitrary> Arbitrary for BTreeMap<K, V> {
    fn shape(definitions: &mut Definitions) -> Shape {
        Shape::map(definitions.shape::<K>(), definitions.shape::<V>())
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        entries_of(value).map(|entries| entries.into_iter().collect())
    }

    fn to_value(&self) -> Value {
        Value::Map(
            self.iter()
                .map(|(key, held)| (convert_to(key), convert_to(held)))
                .collect(),
        )
    }
}

/// Returns the elements of a set's value.
fn elements_of<T: Arbitrary>(value: Value) -> Result<Vec<T>, ShapeError> {
    match value {
        Value::List(items) => items.into_iter().map(convert_from).collect(),
        other => Err(mismatch("a list", &other)),
    }
}

impl<T: Arbitrary + Eq + Hash, S: std::hash::BuildHasher + Default + 'static> Arbitrary
    for HashSet<T, S>
{
    fn shape(definitions: &mut Definitions) -> Shape {
        Shape::set(definitions.shape::<T>())
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        elements_of(value).map(|items| items.into_iter().collect())
    }

    fn to_value(&self) -> Value {
        Value::List(self.iter().map(convert_to).collect())
    }
}

impl<T: Arbitrary + Ord> Arbitrary for BTreeSet<T> {
    fn shape(definitions: &mut Definitions) -> Shape {
        Shape::set(definitions.shape::<T>())
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        elements_of(value).map(|items| items.into_iter().collect())
    }

    fn to_value(&self) -> Value {
        Value::List(self.iter().map(convert_to).collect())
    }
}

/// Returns the fields of a record's value, by position, for a tuple of
/// `count` fields named `0`, `1`, `2` and on.
fn tuple_fields(value: Value, count: usize) -> Result<Vec<Value>, ShapeError> {
    let names: Vec<String> = (0..count).map(|at| at.to_string()).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    registry::record_fields(value, &names)
}

macro_rules! tuples {
    ($($count:expr => ($($name:ident: $at:tt),+);)*) => {$(
        /// A tuple reads as a record whose fields are named by position.
        impl<$($name: Arbitrary),+> Arbitrary for ($($name,)+) {
            fn shape(definitions: &mut Definitions) -> Shape {
                Shape::record([$((stringify!($at), definitions.shape::<$name>())),+])
            }

            fn from_value(value: Value) -> Result<Self, ShapeError> {
                let mut fields = tuple_fields(value, $count)?.into_iter();
                Ok(($(convert_from::<$name>(fields.next().unwrap_or(Value::Null))?,)+))
            }

            fn to_value(&self) -> Value {
                Value::Record(vec![$((stringify!($at).to_owned(), convert_to(&self.$at))),+])
            }
        }
    )*};
}

tuples! {
    1 => (A: 0);
    2 => (A: 0, B: 1);
    3 => (A: 0, B: 1, C: 2);
    4 => (A: 0, B: 1, C: 2, D: 3);
    5 => (A: 0, B: 1, C: 2, D: 3, E: 4);
    6 => (A: 0, B: 1, C: 2, D: 3, E: 4, F: 5);
}

/// A duration reads as a duration at nanoseconds from 0, because a
/// `Duration` has no negative values.
impl Arbitrary for Duration {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::duration().min(0)
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        let nanos = integer_of(&value)?;
        u64::try_from(nanos)
            .map(Duration::from_nanos)
            .map_err(|_| mismatch("a duration of 0 or more nanoseconds", &value))
    }

    fn to_value(&self) -> Value {
        Value::from(self.as_nanos())
    }
}

/// A time of the system clock reads as an instant at nanoseconds.
impl Arbitrary for SystemTime {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::instant()
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        let nanos = integer_of(&value)?;
        let magnitude = Duration::new(
            u64::try_from((nanos / NANOS_PER_SECOND).unsigned_abs()).unwrap_or(u64::MAX),
            u32::try_from((nanos % NANOS_PER_SECOND).unsigned_abs()).unwrap_or_default(),
        );
        let time = if nanos < 0 {
            UNIX_EPOCH.checked_sub(magnitude)
        } else {
            UNIX_EPOCH.checked_add(magnitude)
        };
        time.ok_or_else(|| mismatch("an instant that the system clock states", &value))
    }

    fn to_value(&self) -> Value {
        let nanos = match self.duration_since(UNIX_EPOCH) {
            Ok(after) => i128::try_from(after.as_nanos()).unwrap_or(i128::MAX),
            Err(before) => -i128::try_from(before.duration().as_nanos()).unwrap_or(i128::MAX),
        };
        Value::Int(nanos)
    }
}

impl Arbitrary for IpAddr {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::ip_address()
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        match value {
            Value::Bytes(bytes) => match <[u8; 4]>::try_from(bytes.as_slice()) {
                Ok(octets) => Ok(IpAddr::from(octets)),
                Err(_) => <[u8; 16]>::try_from(bytes.as_slice())
                    .map(IpAddr::from)
                    .map_err(|_| {
                        mismatch("an address of 4 or 16 bytes", &Value::Bytes(bytes.clone()))
                    }),
            },
            other => Err(mismatch("an address", &other)),
        }
    }

    fn to_value(&self) -> Value {
        match self {
            IpAddr::V4(address) => Value::Bytes(address.octets().to_vec()),
            IpAddr::V6(address) => Value::Bytes(address.octets().to_vec()),
        }
    }
}

impl Arbitrary for Ipv4Addr {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::ip_address().version(4)
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        match IpAddr::from_value(value)? {
            IpAddr::V4(address) => Ok(address),
            IpAddr::V6(address) => Err(mismatch(
                "an address of version 4",
                &IpAddr::V6(address).to_value(),
            )),
        }
    }

    fn to_value(&self) -> Value {
        Value::Bytes(self.octets().to_vec())
    }
}

impl Arbitrary for Ipv6Addr {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::ip_address().version(6)
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        match IpAddr::from_value(value)? {
            IpAddr::V6(address) => Ok(address),
            IpAddr::V4(address) => Err(mismatch(
                "an address of version 6",
                &IpAddr::V4(address).to_value(),
            )),
        }
    }

    fn to_value(&self) -> Value {
        Value::Bytes(self.octets().to_vec())
    }
}

/// A [`Value`] has no shape of its own: a property over values states their
/// generator with [`Options::using`](super::Options::using), such as one that
/// [`of_shape`](super::of_shape) returns.
impl Arbitrary for Value {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::refused("a Value has no shape of its own, so its generator comes from prop::using")
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        Ok(value)
    }

    fn to_value(&self) -> Value {
        self.clone()
    }
}
