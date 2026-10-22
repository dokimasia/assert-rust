// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The equality by which uniqueness, examples and the inverse compare
//! decoded values.
//!
//! `canonical` gives each value a form that is equal exactly when the values
//! are: a float compares by its bits, so -0 differs from +0 and every NaN is
//! one value; a bool differs from the int of the same value; a map compares
//! by its entries in any order; a record compares by its fields in order,
//! and a variant by its name and its payload.

use crate::value::Value;
use std::any::Any;
use std::time::Duration;

/// The form of a value that two values share exactly when they are equal.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum Canonical {
    Null,
    Bool(bool),
    Int(i128),
    Uint(u128),
    Float(u64),
    String(String),
    Bytes(Vec<u8>),
    List(Vec<Canonical>),
    Map(Vec<(Canonical, Canonical)>),
    Record(Vec<(String, Canonical)>),
    Variant(String, Option<Box<Canonical>>),
    Other(String),
}

/// Returns the canonical form of a value.
pub(crate) fn canonical(value: &Value) -> Canonical {
    match value {
        Value::Null => Canonical::Null,
        Value::Bool(held) => Canonical::Bool(*held),
        Value::Int(held) => Canonical::Int(*held),
        Value::Uint(held) => Canonical::Uint(*held),
        Value::Float(held) => Canonical::Float(if held.is_nan() {
            super::choice::NAN_BITS
        } else {
            held.to_bits()
        }),
        Value::String(text) => Canonical::String(text.clone()),
        Value::Bytes(bytes) => Canonical::Bytes(bytes.clone()),
        Value::List(items) => Canonical::List(items.iter().map(canonical).collect()),
        Value::Map(entries) => {
            let mut pairs: Vec<(Canonical, Canonical)> = entries
                .iter()
                .map(|(key, value)| (canonical(key), canonical(value)))
                .collect();
            pairs.sort();
            pairs.dedup();
            Canonical::Map(pairs)
        }
        Value::Record(fields) => Canonical::Record(
            fields
                .iter()
                .map(|(name, value)| (name.clone(), canonical(value)))
                .collect(),
        ),
        Value::Variant { name, payload } => Canonical::Variant(
            name.clone(),
            payload
                .as_deref()
                .map(|payload| Box::new(canonical(payload))),
        ),
        other => Canonical::Other(other.to_string()),
    }
}

/// Returns `true` when two values are equal under [`canonical`].
pub(crate) fn same(a: &Value, b: &Value) -> bool {
    canonical(a) == canonical(b)
}

/// Returns the typed literal of a value of a type that the library knows:
/// [`Value`] itself, the primitives, text, bytes and a duration, as its
/// nanoseconds. A value of any other type has none.
pub(crate) fn literal_any<T: 'static>(value: &T) -> Option<Value> {
    let any: &dyn Any = value;
    if let Some(held) = any.downcast_ref::<Value>() {
        return Some(held.clone());
    }
    macro_rules! known {
        ($($t:ty),*) => {$(
            if let Some(held) = any.downcast_ref::<$t>() {
                return Some(Value::from(*held));
            }
        )*};
    }
    known!(
        bool,
        i8,
        i16,
        i32,
        i64,
        i128,
        isize,
        u8,
        u16,
        u32,
        u64,
        u128,
        usize,
        f32,
        f64,
        char,
        &'static str
    );
    if let Some(held) = any.downcast_ref::<String>() {
        return Some(Value::from(held.as_str()));
    }
    if let Some(held) = any.downcast_ref::<Vec<u8>>() {
        return Some(Value::from(held.as_slice()));
    }
    if let Some(held) = any.downcast_ref::<Duration>() {
        return Some(Value::from(held.as_nanos()));
    }
    None
}
