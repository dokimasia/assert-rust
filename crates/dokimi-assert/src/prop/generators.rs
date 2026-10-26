// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The generators of the definition's vocabulary.
//!
//! Every generator but `map` decodes inside a span labelled with its id. A
//! list and a dict decode their elements as a collection, each in a span that
//! starts at the element's continue flag. The order of the requests, their
//! bounds, which of them decide structure, and the spans around them are the
//! definition, so every language decodes the same value from the same
//! choices.

use super::case::Case;
use super::engine::canonical::{canonical, literal_any, same};
use super::engine::case::{Request, Stop};
use super::engine::choice::{
    Bounds, Choice, FloatBounds, IntegerBounds, SequenceBounds, UINT64_MAX, Width, next_down,
    next_up,
};
use super::engine::collection::{ELEMENT, ENTRY, Sizes, collect};
use super::engine::draw::{Draw, HALF, Ratio};
use super::engine::inverse::{
    CannotInvert, Emitted, Planned, collection, distinct, first_branch, planned, shortlex,
};
use super::generator::{Composite, Generator, Node};
use super::scope;
use crate::alphabet;
use crate::pattern;
use crate::value::Value;
use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;
use std::ops::{Bound, RangeBounds};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;

mod sealed {
    pub trait Sealed {}
}

/// An integer type that [`integer`] generates: `i8` to `i128`, `u8` to `u128`,
/// `isize` and `usize`.
///
/// A type of up to 64 bits generates from one integer choice. `i128` and
/// `u128` generate from two choices, the high 64 bits and then the low 64
/// bits, as the shape of a 128-bit integer decodes.
pub trait Integral: Copy + PartialOrd + Debug + Send + Sync + 'static + sealed::Sealed {
    /// The smallest value of the type.
    #[doc(hidden)]
    const LOWEST: Self;
    /// The largest value of the type.
    #[doc(hidden)]
    const HIGHEST: Self;
    /// Returns the value one above, or `None` at the largest.
    #[doc(hidden)]
    fn up(self) -> Option<Self>;
    /// Returns the value one below, or `None` at the smallest.
    #[doc(hidden)]
    fn down(self) -> Option<Self>;
    /// Returns the generator of the values in `[lo, hi]`.
    #[doc(hidden)]
    fn generator(lo: Self, hi: Self) -> Generator<Self>;
}

/// An integer type of up to 64 bits, which one choice states.
pub(crate) trait Narrow: Copy + Debug + Send + Sync + 'static {
    fn wide(self) -> i128;
    fn narrow(value: i128) -> Self;
}

macro_rules! narrow {
    ($($t:ty),*) => {$(
        impl sealed::Sealed for $t {}

        impl Narrow for $t {
            fn wide(self) -> i128 {
                i128::try_from(self).unwrap_or_default()
            }

            fn narrow(value: i128) -> Self {
                <$t>::try_from(value).unwrap_or_default()
            }
        }

        impl Integral for $t {
            const LOWEST: Self = <$t>::MIN;
            const HIGHEST: Self = <$t>::MAX;

            fn up(self) -> Option<Self> {
                self.checked_add(1)
            }

            fn down(self) -> Option<Self> {
                self.checked_sub(1)
            }

            fn generator(lo: Self, hi: Self) -> Generator<Self> {
                Generator::from_node(Integer::<$t> {
                    id: "integer",
                    bounds: IntegerBounds::of(lo.wide(), hi.wide()),
                    from: <$t as Narrow>::narrow,
                })
            }
        }
    )*};
}

narrow!(i8, i16, i32, i64, isize, u8, u16, u32, u64, usize);

/// A 128-bit integer type, which two choices state: its high and its low 64
/// bits.
pub(crate) trait Wide: Copy + Debug + Send + Sync + 'static {
    fn parts(self) -> (i128, i128);
    fn join(high: i128, low: i128) -> Self;
    fn value(self) -> Value;
}

impl sealed::Sealed for i128 {}
impl sealed::Sealed for u128 {}

impl Wide for i128 {
    fn parts(self) -> (i128, i128) {
        (self >> 64, self & UINT64_MAX)
    }

    fn join(high: i128, low: i128) -> Self {
        (high << 64) | low
    }

    fn value(self) -> Value {
        Value::Int(self)
    }
}

impl Wide for u128 {
    fn parts(self) -> (i128, i128) {
        let high = i128::try_from(self >> 64).unwrap_or_default();
        let low = i128::try_from(self & u128::from(u64::MAX)).unwrap_or_default();
        (high, low)
    }

    fn join(high: i128, low: i128) -> Self {
        (u128::try_from(high).unwrap_or_default() << 64) | u128::try_from(low).unwrap_or_default()
    }

    fn value(self) -> Value {
        Value::from(self)
    }
}

macro_rules! wide {
    ($($t:ty),*) => {$(
        impl Integral for $t {
            const LOWEST: Self = <$t>::MIN;
            const HIGHEST: Self = <$t>::MAX;

            fn up(self) -> Option<Self> {
                self.checked_add(1)
            }

            fn down(self) -> Option<Self> {
                self.checked_sub(1)
            }

            fn generator(lo: Self, hi: Self) -> Generator<Self> {
                Generator::from_node(WideInteger::<$t> { lo, hi })
            }
        }
    )*};
}

wide!(i128, u128);

/// A float type that [`float`] generates: `f32`, of width 32, and `f64`, of
/// width 64.
pub trait Floating: Copy + PartialOrd + Debug + Send + Sync + 'static + sealed::Sealed {
    /// The width of the type.
    #[doc(hidden)]
    const WIDTH: Width;
    /// The largest finite value of the type.
    #[doc(hidden)]
    const HIGHEST: Self;
    /// Returns the value as an `f64`.
    #[doc(hidden)]
    fn to_f64(self) -> f64;
    /// Returns an `f64` that is a value of the type as the type.
    #[doc(hidden)]
    fn from_f64(value: f64) -> Self;
}

impl sealed::Sealed for f32 {}
impl sealed::Sealed for f64 {}

impl Floating for f32 {
    const WIDTH: Width = Width::W32;
    const HIGHEST: Self = f32::MAX;

    fn to_f64(self) -> f64 {
        f64::from(self)
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "the value is a value of width 32, which an f32 states exactly"
    )]
    fn from_f64(value: f64) -> Self {
        value as f32
    }
}

impl Floating for f64 {
    const WIDTH: Width = Width::W64;
    const HIGHEST: Self = f64::MAX;

    fn to_f64(self) -> f64 {
        self
    }

    fn from_f64(value: f64) -> Self {
        value
    }
}

/// Returns the inclusive bounds that a range states over a type with a
/// smallest and a largest value, or `None` for an empty range.
fn inclusive<T: Copy + PartialOrd>(
    range: &impl RangeBounds<T>,
    lowest: T,
    highest: T,
    up: impl Fn(T) -> Option<T>,
    down: impl Fn(T) -> Option<T>,
) -> Option<(T, T)> {
    let lo = match range.start_bound() {
        Bound::Included(start) => *start,
        Bound::Excluded(start) => up(*start)?,
        Bound::Unbounded => lowest,
    };
    let hi = match range.end_bound() {
        Bound::Included(end) => *end,
        Bound::Excluded(end) => down(*end)?,
        Bound::Unbounded => highest,
    };
    (lo <= hi).then_some((lo, hi))
}

/// Returns a generator of the integers in `range`.
///
/// `integer(..)` is the type's whole range. A type of up to 64 bits makes one
/// integer choice, whose target is the value closest to zero.
///
/// # Panics
///
/// Panics for an empty range.
///
/// ```
/// use dokimi_assert::prop;
///
/// let percent = prop::integer(0..=100_u8);
/// let offset = prop::integer(-5..5);
/// let any = prop::integer::<u64>(..);
/// # let _ = (percent, offset, any);
/// ```
#[must_use]
pub fn integer<T: Integral>(range: impl RangeBounds<T>) -> Generator<T> {
    let (lo, hi) = inclusive(&range, T::LOWEST, T::HIGHEST, T::up, T::down)
        .unwrap_or_else(|| panic!("prop::integer: the range states no value"));
    T::generator(lo, hi)
}

/// A generator of integers of up to 64 bits: one integer choice.
pub(crate) struct Integer<T> {
    pub(crate) id: &'static str,
    pub(crate) bounds: IntegerBounds,
    pub(crate) from: fn(i128) -> T,
}

impl<T: Debug + Send + Sync + 'static> Node<T> for Integer<T> {
    fn id(&self) -> &'static str {
        self.id
    }

    /// Returns the integer that the case chooses, which may reuse an earlier
    /// one.
    fn decode(&self, case: &Case<'_>) -> Result<T, Stop> {
        let _span = case.span(self.id, None);
        case.integer(self.bounds, None, true).map(self.from)
    }

    fn emit(&self, value: &Value) -> Result<Emitted, CannotInvert> {
        let Value::Int(held) = value else {
            return Err(CannotInvert::refused("the value is no integer"));
        };
        let step = planned(Bounds::Integer(self.bounds), Choice::Integer(*held))?;
        Ok((vec![step], value.clone()))
    }

    fn integer(&self) -> Option<IntegerBounds> {
        Some(self.bounds)
    }
}

/// Returns the bounds of the second choice of a two-choice value.
///
/// It ranges over `[0, top]`, except where the first choice equals a bound's
/// first part, where it stops at that bound's second part.
pub(crate) fn second_part(
    first: i128,
    lo: (i128, i128),
    hi: (i128, i128),
    top: i128,
) -> IntegerBounds {
    IntegerBounds::of(
        if first == lo.0 { lo.1 } else { 0 },
        if first == hi.0 { hi.1 } else { top },
    )
}

/// A generator of 128-bit integers: the high 64 bits, then the low 64 bits,
/// as one span labelled `int`.
pub(crate) struct WideInteger<T> {
    pub(crate) lo: T,
    pub(crate) hi: T,
}

impl<T: Wide> Node<T> for WideInteger<T> {
    fn id(&self) -> &'static str {
        "int"
    }

    fn decode(&self, case: &Case<'_>) -> Result<T, Stop> {
        let lo = self.lo.parts();
        let hi = self.hi.parts();
        let _span = case.span("int", None);
        let high = case.integer(IntegerBounds::of(lo.0, hi.0), None, true)?;
        let low = case.integer(second_part(high, lo, hi, UINT64_MAX), None, true)?;
        Ok(T::join(high, low))
    }

    fn literal(&self, value: &T) -> Option<Value> {
        Some(value.value())
    }

    fn emit(&self, value: &Value) -> Result<Emitted, CannotInvert> {
        let (high, low) = match value {
            Value::Int(held) => (held >> 64, held & UINT64_MAX),
            Value::Uint(held) => (
                i128::try_from(held >> 64).unwrap_or_default(),
                i128::try_from(held & u128::from(u64::MAX)).unwrap_or_default(),
            ),
            _ => return Err(CannotInvert::refused("the value is no integer")),
        };
        let lo = self.lo.parts();
        let hi = self.hi.parts();
        let first = planned(
            Bounds::Integer(IntegerBounds::of(lo.0, hi.0)),
            Choice::Integer(high),
        )?;
        let second = planned(
            Bounds::Integer(second_part(high, lo, hi, UINT64_MAX)),
            Choice::Integer(low),
        )?;
        Ok((vec![first, second], value.clone()))
    }
}

/// Returns a generator of the floats of `T` in `range`.
///
/// `float(..)` is every finite value of the type. A range with an infinite
/// bound admits that infinity. NaN is no value of the generator unless
/// [`Generator::allow_nan`] admits it.
///
/// # Panics
///
/// Panics for an empty range, and for a bound that is NaN.
///
/// ```
/// use dokimi_assert::prop;
///
/// let unit = prop::float(0.0..=1.0_f64);
/// let any = prop::float::<f32>(..).allow_nan();
/// # let _ = (unit, any);
/// ```
#[must_use]
pub fn float<T: Floating>(range: impl RangeBounds<T>) -> Generator<T> {
    let width = T::WIDTH;
    let start = match range.start_bound() {
        Bound::Included(start) => start.to_f64(),
        Bound::Excluded(start) => next_up(start.to_f64(), width),
        Bound::Unbounded => -T::HIGHEST.to_f64(),
    };
    let end = match range.end_bound() {
        Bound::Included(end) => end.to_f64(),
        Bound::Excluded(end) => next_down(end.to_f64(), width),
        Bound::Unbounded => T::HIGHEST.to_f64(),
    };
    let bounds = FloatBounds::new(start, end, false, width).unwrap_or_else(|_| {
        panic!("prop::float: the range from {start:?} to {end:?} states no value")
    });
    Generator::from_node(Float::<T> {
        bounds,
        from: T::from_f64,
    })
}

/// A generator of floats: one float choice.
pub(crate) struct Float<T> {
    pub(crate) bounds: FloatBounds,
    pub(crate) from: fn(f64) -> T,
}

impl<T: Debug + Send + Sync + 'static> Node<T> for Float<T> {
    fn id(&self) -> &'static str {
        "float"
    }

    fn decode(&self, case: &Case<'_>) -> Result<T, Stop> {
        let _span = case.span("float", None);
        let value = case.choose(Request::of(Bounds::Float(self.bounds)))?;
        Ok((self.from)(value.as_float().unwrap_or_default()))
    }

    fn emit(&self, value: &Value) -> Result<Emitted, CannotInvert> {
        let Value::Float(held) = value else {
            return Err(CannotInvert::refused("the value is no float"));
        };
        let step = planned(Bounds::Float(self.bounds), Choice::Float(*held))?;
        Ok((vec![step], value.clone()))
    }
}

impl<T: Floating> Generator<T> {
    /// Returns the generator with NaN among its values: the canonical quiet
    /// NaN, which sorts after every other value.
    ///
    /// # Panics
    ///
    /// Panics for a generator that [`float`] did not return.
    #[must_use]
    pub fn allow_nan(self) -> Self {
        let node = self.flagged::<Float<T>>("allow_nan");
        let bounds = FloatBounds::new(
            node.bounds.lo(),
            node.bounds.hi(),
            true,
            node.bounds.width(),
        )
        .unwrap_or(node.bounds);
        Self::from_node(Float::<T> {
            bounds,
            from: node.from,
        })
    }
}

/// Returns a generator of booleans, `true` with probability 1/2.
///
/// One integer choice in `[0, 1]`, whose target is `false`.
#[must_use]
pub fn boolean() -> Generator<bool> {
    Generator::from_node(Boolean { p: HALF })
}

/// A generator of booleans: one integer choice in `[0, 1]`, drawn by one
/// coin.
struct Boolean {
    p: Ratio,
}

impl Node<bool> for Boolean {
    fn id(&self) -> &'static str {
        "boolean"
    }

    fn decode(&self, case: &Case<'_>) -> Result<bool, Stop> {
        let request = Request {
            bounds: Bounds::Integer(IntegerBounds::of(0, 1)),
            draw: Draw::Boolean(self.p),
            edge: None,
            reuse: false,
        };
        let _span = case.span("boolean", None);
        Ok(case.choose(request)?.as_integer() == Some(1))
    }

    fn literal(&self, value: &bool) -> Option<Value> {
        Some(Value::Bool(*value))
    }

    fn emit(&self, value: &Value) -> Result<Emitted, CannotInvert> {
        let Value::Bool(held) = value else {
            return Err(CannotInvert::refused("the value is no boolean"));
        };
        Ok((
            vec![Planned::new(
                Bounds::Integer(IntegerBounds::of(0, 1)),
                Choice::Integer(i128::from(*held)),
            )],
            value.clone(),
        ))
    }
}

impl Generator<bool> {
    /// Returns the generator with `true` at the probability `numerator /
    /// denominator`, which a random case draws by one coin.
    ///
    /// # Panics
    ///
    /// Panics for a probability outside `[0, 1]` or a denominator of 0, and
    /// for a generator that [`boolean`] did not return.
    #[must_use]
    pub fn odds(self, numerator: u64, denominator: u64) -> Self {
        self.flagged::<Boolean>("odds");
        let p = Ratio::new(u128::from(numerator), u128::from(denominator))
            .unwrap_or_else(|| panic!("prop: odds({numerator}, {denominator}) is no probability"));
        Self::from_node(Boolean { p })
    }
}

/// Returns a generator of one value, which makes no choice.
#[must_use]
pub fn just<T: Clone + Send + Sync + 'static>(value: T) -> Generator<T> {
    Generator::from_node(Just { value })
}

struct Just<T> {
    value: T,
}

impl<T: Clone + Send + Sync + 'static> Node<T> for Just<T> {
    fn id(&self) -> &'static str {
        "just"
    }

    fn decode(&self, case: &Case<'_>) -> Result<T, Stop> {
        let _span = case.span("just", None);
        Ok(self.value.clone())
    }

    fn emit(&self, value: &Value) -> Result<Emitted, CannotInvert> {
        let held = literal_any(&self.value).ok_or_else(|| CannotInvert::no_inverse("just"))?;
        if !same(value, &held) {
            return Err(CannotInvert::refused(
                "the value is not the generator's value",
            ));
        }
        Ok((Vec::new(), value.clone()))
    }
}

/// Returns the literals of stated values, or `None` when one has none.
fn literals<T: 'static>(values: &[T]) -> Option<Vec<Value>> {
    values.iter().map(literal_any).collect()
}

/// Returns the first index of a value equal to `value`.
fn index_of(values: &[Value], value: &Value) -> Result<usize, CannotInvert> {
    let wanted = canonical(value);
    values
        .iter()
        .position(|held| canonical(held) == wanted)
        .ok_or_else(|| CannotInvert::refused("the value is none of the values"))
}

/// Returns a generator of one of `values`: an integer index that decides
/// structure, whose target is the first value.
///
/// # Panics
///
/// Panics when `values` is empty.
#[must_use]
pub fn sampled_from<T: Clone + Send + Sync + 'static>(
    values: impl IntoIterator<Item = T>,
) -> Generator<T> {
    let values: Vec<T> = values.into_iter().collect();
    assert!(
        !values.is_empty(),
        "prop::sampled_from: no value to sample from"
    );
    Generator::from_node(SampledFrom { values })
}

pub(crate) struct SampledFrom<T> {
    pub(crate) values: Vec<T>,
}

impl<T: Clone + Send + Sync + 'static> SampledFrom<T> {
    fn bounds(&self) -> IntegerBounds {
        IntegerBounds::of(0, index(self.values.len()) - 1)
    }
}

/// Returns a count as an `i128`.
pub(crate) fn index(count: usize) -> i128 {
    i128::try_from(count).unwrap_or(i128::MAX)
}

/// Returns an index that a choice states as a `usize`.
pub(crate) fn position(choice: i128) -> usize {
    usize::try_from(choice).unwrap_or_default()
}

impl<T: Clone + Send + Sync + 'static> Node<T> for SampledFrom<T> {
    fn id(&self) -> &'static str {
        "sampled-from"
    }

    fn decode(&self, case: &Case<'_>) -> Result<T, Stop> {
        let _span = case.span("sampled-from", None);
        let at = case.integer(self.bounds(), Some(0), false)?;
        Ok(self.values[position(at)].clone())
    }

    fn emit(&self, value: &Value) -> Result<Emitted, CannotInvert> {
        let values =
            literals(&self.values).ok_or_else(|| CannotInvert::no_inverse("sampled-from"))?;
        let at = index_of(&values, value)?;
        Ok((
            vec![Planned::new(
                Bounds::Integer(self.bounds()),
                Choice::Integer(index(at)),
            )],
            value.clone(),
        ))
    }
}

/// Returns a generator of a value of one of `generators`: an index that
/// decides structure, then that generator's choices. The target is the first
/// generator.
///
/// # Panics
///
/// Panics when `generators` is empty.
#[must_use]
pub fn one_of<T: 'static>(generators: impl IntoIterator<Item = Generator<T>>) -> Generator<T> {
    let of: Vec<Generator<T>> = generators.into_iter().collect();
    assert!(!of.is_empty(), "prop::one_of: no generator to choose from");
    Generator::from_node(OneOf { of })
}

struct OneOf<T> {
    of: Vec<Generator<T>>,
}

impl<T: 'static> Node<T> for OneOf<T> {
    fn id(&self) -> &'static str {
        "one-of"
    }

    fn decode(&self, case: &Case<'_>) -> Result<T, Stop> {
        let bounds = IntegerBounds::of(0, index(self.of.len()) - 1);
        let _span = case.span("one-of", None);
        let at = case.integer(bounds, Some(0), false)?;
        self.of[position(at)].decode(case)
    }

    fn literal(&self, value: &T) -> Option<Value> {
        self.of
            .iter()
            .find_map(|generator| generator.literal(value))
    }

    fn emit(&self, value: &Value) -> Result<Emitted, CannotInvert> {
        let bounds = IntegerBounds::of(0, index(self.of.len()) - 1);
        first_branch(
            self.of
                .iter()
                .enumerate()
                .map(|(at, generator)| (index(at), generator.emit(value))),
            bounds,
            "no alternative produces the value",
        )
    }
}

/// Returns a generator of a value of `of` or `None`: a presence choice in
/// `[0, 1]` that decides structure, then the value when it is present. The
/// target is absent, and the edge phase makes the value present.
#[must_use]
pub fn optional<T: 'static>(of: Generator<T>) -> Generator<Option<T>> {
    Generator::from_node(Optional { of })
}

struct Optional<T> {
    of: Generator<T>,
}

impl<T: 'static> Node<Option<T>> for Optional<T> {
    fn id(&self) -> &'static str {
        "optional"
    }

    fn decode(&self, case: &Case<'_>) -> Result<Option<T>, Stop> {
        let _span = case.span("optional", None);
        if case.integer(IntegerBounds::of(0, 1), Some(1), false)? == 1 {
            return self.of.decode(case).map(Some);
        }
        Ok(None)
    }

    fn literal(&self, value: &Option<T>) -> Option<Value> {
        match value {
            None => Some(Value::Null),
            Some(held) => self.of.literal(held),
        }
    }

    fn emit(&self, value: &Value) -> Result<Emitted, CannotInvert> {
        emit_optional(&self.of, value)
    }
}

/// Returns the steps of an optional: absent, or present and then the
/// value's steps.
pub(crate) fn emit_optional<T: 'static>(
    of: &Generator<T>,
    value: &Value,
) -> Result<Emitted, CannotInvert> {
    let presence = Bounds::Integer(IntegerBounds::of(0, 1));
    if *value == Value::Null {
        return Ok((
            vec![Planned::new(presence, Choice::Integer(0))],
            Value::Null,
        ));
    }
    let (steps, normal) = of.emit(value)?;
    let mut all = vec![Planned::new(presence, Choice::Integer(1))];
    all.extend(steps);
    Ok((all, normal))
}

/// Returns the sizes that a range states.
fn sizes_of(range: &impl RangeBounds<usize>, call: &str) -> Sizes {
    Sizes::of(range).unwrap_or_else(|| panic!("prop::{call}: the sizes state no length"))
}

/// Returns a generator of lists of `of`'s values, with a length in `sizes`.
///
/// Before each element a continue flag decides whether the list grows. A
/// random case aims for an average length of `min + min(max(min, 5),
/// ceil((max - min) / 2))`.
///
/// # Panics
///
/// Panics for an empty range of sizes.
#[must_use]
pub fn list<T: 'static>(of: Generator<T>, sizes: impl RangeBounds<usize>) -> Generator<Vec<T>> {
    Generator::from_node(List {
        of,
        sizes: sizes_of(&sizes, "list"),
        same: None,
    })
}

/// A function that reports whether two elements are one element.
type Same<T> = Arc<dyn Fn(&T, &T) -> bool + Send + Sync>;

struct List<T> {
    of: Generator<T>,
    sizes: Sizes,
    same: Option<Same<T>>,
}

impl<T: 'static> Node<Vec<T>> for List<T> {
    fn id(&self) -> &'static str {
        "list"
    }

    fn decode(&self, case: &Case<'_>) -> Result<Vec<T>, Stop> {
        let _span = case.span("list", None);
        collect(
            case,
            self.sizes,
            ELEMENT,
            || self.of.decode(case),
            self.same
                .as_deref()
                .map(|same| same as &dyn Fn(&T, &T) -> bool),
            None,
        )
    }

    fn literal(&self, value: &Vec<T>) -> Option<Value> {
        value
            .iter()
            .map(|item| self.of.literal(item))
            .collect::<Option<Vec<_>>>()
            .map(Value::List)
    }

    fn emit(&self, value: &Value) -> Result<Emitted, CannotInvert> {
        let Value::List(items) = value else {
            return Err(CannotInvert::refused("the value is no list"));
        };
        if self.same.is_some() {
            distinct(items)?;
        }
        let elements = items
            .iter()
            .map(|item| self.of.emit(item))
            .collect::<Result<Vec<_>, _>>()?;
        let (steps, normal): (Vec<_>, Vec<_>) = elements.into_iter().unzip();
        Ok((collection(self.sizes, steps)?, Value::List(normal)))
    }
}

impl<T: PartialEq + 'static> Generator<Vec<T>> {
    /// Returns the generator with unique elements: an element equal to an
    /// earlier one is discarded, and the next continue flag is decided for
    /// the same count. Elements that state typed literals compare by them, so
    /// two floats are one element exactly when their bits are equal.
    ///
    /// # Panics
    ///
    /// Panics for a generator that [`list`] did not return.
    #[must_use]
    pub fn unique(self) -> Self {
        let node = self.flagged::<List<T>>("unique");
        let of = node.of.clone();
        let element = of.clone();
        let same: Same<T> = Arc::new(move |a, b| match (element.literal(a), element.literal(b)) {
            (Some(a), Some(b)) => same(&a, &b),
            _ => a == b,
        });
        Self::from_node(List {
            of,
            sizes: node.sizes,
            same: Some(same),
        })
    }
}

/// Returns a generator of maps from `keys`' values to `values`' values, with
/// a number of entries in `sizes`.
///
/// Each entry decodes its key and then its value. An entry whose key equals
/// an earlier key is discarded, and after ten discards in a row the map
/// stops growing.
///
/// # Panics
///
/// Panics for an empty range of sizes.
#[must_use]
pub fn dict<K: Eq + Hash + 'static, V: 'static>(
    keys: Generator<K>,
    values: Generator<V>,
    sizes: impl RangeBounds<usize>,
) -> Generator<HashMap<K, V>> {
    let sizes = sizes_of(&sizes, "dict");
    let entries = Generator::from_node(Pairs {
        keys,
        values,
        sizes,
        same: Arc::new(|a: &K, b: &K| a == b),
    });
    Generator::from_node(Dict { entries })
}

/// The entries of a map, in the order that they were generated.
pub(crate) struct Pairs<K, V> {
    pub(crate) keys: Generator<K>,
    pub(crate) values: Generator<V>,
    pub(crate) sizes: Sizes,
    pub(crate) same: Same<K>,
}

impl<K: 'static, V: 'static> Node<Vec<(K, V)>> for Pairs<K, V> {
    fn id(&self) -> &'static str {
        "dict"
    }

    fn decode(&self, case: &Case<'_>) -> Result<Vec<(K, V)>, Stop> {
        let _span = case.span("dict", None);
        let same = |a: &(K, V), b: &(K, V)| (self.same)(&a.0, &b.0);
        collect(
            case,
            self.sizes,
            ENTRY,
            || {
                let key = self.keys.decode(case)?;
                Ok((key, self.values.decode(case)?))
            },
            Some(&same),
            None,
        )
    }

    fn literal(&self, entries: &Vec<(K, V)>) -> Option<Value> {
        entries
            .iter()
            .map(|(key, value)| Some((self.keys.literal(key)?, self.values.literal(value)?)))
            .collect::<Option<Vec<_>>>()
            .map(Value::Map)
    }

    fn emit(&self, value: &Value) -> Result<Emitted, CannotInvert> {
        emit_entries(&self.keys, &self.values, self.sizes, value)
    }
}

/// Returns a map's flags and entries, each its key's steps then its value's,
/// in the shortlex order of their steps.
pub(crate) fn emit_entries<K: 'static, V: 'static>(
    keys: &Generator<K>,
    values: &Generator<V>,
    sizes: Sizes,
    value: &Value,
) -> Result<Emitted, CannotInvert> {
    let Value::Map(entries) = value else {
        return Err(CannotInvert::refused("the value is no map"));
    };
    let stated: Vec<Value> = entries.iter().map(|(key, _)| key.clone()).collect();
    distinct(&stated)?;
    let mut emitted = Vec::with_capacity(entries.len());
    for (key, held) in entries {
        let (mut steps, key) = keys.emit(key)?;
        let (more, held) = values.emit(held)?;
        steps.extend(more);
        emitted.push((steps, (key, held)));
    }
    emitted.sort_by_cached_key(|(steps, _)| shortlex(steps));
    let (steps, normal): (Vec<_>, Vec<_>) = emitted.into_iter().unzip();
    Ok((collection(sizes, steps)?, Value::Map(normal)))
}

/// A map, decoded as its entries.
struct Dict<K, V> {
    entries: Generator<Vec<(K, V)>>,
}

impl<K: Eq + Hash + 'static, V: 'static> Node<HashMap<K, V>> for Dict<K, V> {
    fn id(&self) -> &'static str {
        "dict"
    }

    fn decode(&self, case: &Case<'_>) -> Result<HashMap<K, V>, Stop> {
        self.entries
            .decode(case)
            .map(|entries| entries.into_iter().collect())
    }

    fn literal(&self, value: &HashMap<K, V>) -> Option<Value> {
        let node = self.entries.flagged::<Pairs<K, V>>("dict");
        value
            .iter()
            .map(|(key, held)| Some((node.keys.literal(key)?, node.values.literal(held)?)))
            .collect::<Option<Vec<_>>>()
            .map(Value::Map)
    }

    fn emit(&self, value: &Value) -> Result<Emitted, CannotInvert> {
        self.entries.emit(value)
    }
}

/// Returns a generator of strings over the default alphabet, with a length
/// in `sizes`.
///
/// A string is one sequence of indices into its alphabet. The default
/// alphabet puts the digits, the letters, space and the punctuation first,
/// so a string shrinks towards `"0"`. [`Generator::alphabet`] states another
/// alphabet.
///
/// # Panics
///
/// Panics for an empty range of sizes.
#[must_use]
pub fn string(sizes: impl RangeBounds<usize>) -> Generator<String> {
    let sizes = sizes_of(&sizes, "string");
    Generator::from_node(Text {
        alphabet: None,
        bounds: sequence_bounds(u64::from(alphabet::SIZE), sizes),
    })
}

/// Returns the bounds of a sequence with `k` element values, 1 or more, and
/// a length in `sizes`, which admit a length.
pub(crate) fn sequence_bounds(k: u64, sizes: Sizes) -> SequenceBounds {
    SequenceBounds::of(k, sizes.min_size(), sizes.max_size())
}

/// A generator of strings: one sequence of indices into an alphabet.
pub(crate) struct Text {
    pub(crate) alphabet: Option<Vec<char>>,
    pub(crate) bounds: SequenceBounds,
}

impl Node<String> for Text {
    fn id(&self) -> &'static str {
        "string"
    }

    fn decode(&self, case: &Case<'_>) -> Result<String, Stop> {
        let indices = {
            let _span = case.span("string", None);
            case.sequence(self.bounds)?
        };
        Ok(indices
            .iter()
            .map(|at| match &self.alphabet {
                Some(characters) => characters[usize::try_from(*at).unwrap_or_default()],
                None => alphabet::character(u32::try_from(*at).unwrap_or_default()).unwrap_or('0'),
            })
            .collect())
    }

    fn literal(&self, value: &String) -> Option<Value> {
        Some(Value::from(value.as_str()))
    }

    fn emit(&self, value: &Value) -> Result<Emitted, CannotInvert> {
        let Value::String(text) = value else {
            return Err(CannotInvert::refused("the value is no string"));
        };
        let indices = text
            .chars()
            .map(|c| match &self.alphabet {
                Some(characters) => characters
                    .iter()
                    .position(|held| *held == c)
                    .map(|at| u64::try_from(at).unwrap_or_default()),
                None => Some(u64::from(alphabet::index(c))),
            })
            .collect::<Option<Vec<u64>>>()
            .ok_or_else(|| {
                CannotInvert::refused("the value has a character outside the alphabet")
            })?;
        let step = planned(Bounds::Sequence(self.bounds), Choice::Sequence(indices))?;
        Ok((vec![step], value.clone()))
    }
}

impl Generator<String> {
    /// Returns the generator over `characters`, in their stated order, so the
    /// first character is the simplest.
    ///
    /// # Panics
    ///
    /// Panics when `characters` is empty or repeats a character, and for a
    /// generator that [`string`] did not return.
    #[must_use]
    pub fn alphabet(self, characters: &str) -> Self {
        let node = self.flagged::<Text>("alphabet");
        let characters: Vec<char> = characters.chars().collect();
        assert!(
            !characters.is_empty(),
            "prop: alphabet(\"\") has no character"
        );
        for (at, c) in characters.iter().enumerate() {
            assert!(
                !characters[..at].contains(c),
                "prop: alphabet repeats the character {c:?}"
            );
        }
        let k = u64::try_from(characters.len()).unwrap_or(u64::MAX);
        let bounds = SequenceBounds::new(k, node.bounds.min_size(), node.bounds.max_size())
            .unwrap_or(node.bounds);
        Self::from_node(Text {
            alphabet: Some(characters),
            bounds,
        })
    }
}

/// Returns a generator of strings that `pattern` matches in full.
///
/// The pattern is a regular expression of the portable subset that
/// [`check::matches`](crate::check::matches) takes. An alternation chooses
/// its branch, a quantifier decodes its repetitions as a collection, and a
/// class chooses a member. The targets give the first branch, the fewest
/// repetitions and the simplest member of each class.
///
/// # Panics
///
/// Panics for a pattern outside the portable subset.
#[must_use]
pub fn string_matching(pattern: &str) -> Generator<String> {
    let node = pattern::parse(pattern)
        .unwrap_or_else(|refused| panic!("prop::string_matching: {refused}"));
    Generator::from_node(Matching { node })
}

/// A generator of the strings that a pattern matches in full.
pub(crate) struct Matching {
    pub(crate) node: pattern::Node,
}

/// The span labels of an alternation and of a quantifier's repetitions.
const ALTERNATION: &str = "alternation";
const REPEAT: &str = "repeat";

/// Asks `case` for the choices of a pattern's node, and appends its
/// characters to `out`.
fn emit_node(node: &pattern::Node, case: &Case<'_>, out: &mut String) -> Result<(), Stop> {
    match node {
        pattern::Node::Literal(c) => out.push(*c),
        pattern::Node::Class(class) => {
            let bounds = IntegerBounds::of(0, i128::from(class.size) - 1);
            let mut offset = u32::try_from(case.integer(bounds, None, false)?).unwrap_or_default();
            for (start, end) in &class.intervals {
                if offset <= end - start {
                    out.push(alphabet::character(start + offset).unwrap_or('0'));
                    break;
                }
                offset -= end - start + 1;
            }
        }
        pattern::Node::Sequence(items) => {
            for item in items {
                emit_node(item, case, out)?;
            }
        }
        pattern::Node::Alternation(branches) => {
            let bounds = IntegerBounds::of(0, index(branches.len()) - 1);
            let _span = case.span(ALTERNATION, None);
            let at = case.integer(bounds, Some(0), false)?;
            emit_node(&branches[position(at)], case, out)?;
        }
        pattern::Node::Repeat(item, sizes) => {
            // The parser refuses a count that runs backwards.
            let sizes = Sizes::new(sizes.min, sizes.max).unwrap_or(Sizes::ANY);
            let _span = case.span(REPEAT, None);
            collect(
                case,
                sizes,
                ELEMENT,
                || emit_node(item, case, out),
                None,
                None,
            )?;
        }
    }
    Ok(())
}

impl Node<String> for Matching {
    fn id(&self) -> &'static str {
        "string-matching"
    }

    fn decode(&self, case: &Case<'_>) -> Result<String, Stop> {
        let mut out = String::new();
        let _span = case.span("string-matching", None);
        emit_node(&self.node, case, &mut out)?;
        Ok(out)
    }

    fn literal(&self, value: &String) -> Option<Value> {
        Some(Value::from(value.as_str()))
    }

    fn emit(&self, value: &Value) -> Result<Emitted, CannotInvert> {
        let Value::String(text) = value else {
            return Err(CannotInvert::refused("the value is no string"));
        };
        let steps = super::engine::inverse::matched(&self.node, text)
            .ok_or_else(|| CannotInvert::refused("the pattern does not match the value in full"))?;
        Ok((steps, value.clone()))
    }
}

/// Returns a generator of byte strings with a length in `sizes`: one
/// sequence of elements in `[0, 255]`, whose target is the zero byte.
///
/// # Panics
///
/// Panics for an empty range of sizes.
#[must_use]
pub fn bytes(sizes: impl RangeBounds<usize>) -> Generator<Vec<u8>> {
    let sizes = sizes_of(&sizes, "bytes");
    Generator::from_node(Bytes {
        bounds: sequence_bounds(256, sizes),
    })
}

/// A generator of byte strings: one sequence of elements in `[0, 255]`.
pub(crate) struct Bytes {
    pub(crate) bounds: SequenceBounds,
}

impl Node<Vec<u8>> for Bytes {
    fn id(&self) -> &'static str {
        "bytes"
    }

    fn decode(&self, case: &Case<'_>) -> Result<Vec<u8>, Stop> {
        let _span = case.span("bytes", None);
        let elements = case.sequence(self.bounds)?;
        Ok(elements
            .into_iter()
            .map(|element| u8::try_from(element).unwrap_or_default())
            .collect())
    }

    fn literal(&self, value: &Vec<u8>) -> Option<Value> {
        Some(Value::Bytes(value.clone()))
    }

    fn emit(&self, value: &Value) -> Result<Emitted, CannotInvert> {
        let Value::Bytes(held) = value else {
            return Err(CannotInvert::refused("the value is no byte string"));
        };
        let elements = held.iter().map(|byte| u64::from(*byte)).collect();
        let step = planned(Bounds::Sequence(self.bounds), Choice::Sequence(elements))?;
        Ok((vec![step], value.clone()))
    }
}

/// The largest duration that [`duration`] generates without an upper bound:
/// the range of a signed 64-bit count of nanoseconds.
const MAX_DURATION_NANOS: u128 = i64::MAX as u128;

/// Returns a generator of durations in `range`, at a resolution of a
/// nanosecond: one integer choice of nanoseconds, whose target is the
/// shortest duration.
///
/// # Panics
///
/// Panics for an empty range, and for a bound above 2^64 - 1 nanoseconds.
#[must_use]
pub fn duration(range: impl RangeBounds<Duration>) -> Generator<Duration> {
    let nanos = |held: &Duration| held.as_nanos();
    let lo = match range.start_bound() {
        Bound::Included(start) => nanos(start),
        Bound::Excluded(start) => nanos(start) + 1,
        Bound::Unbounded => 0,
    };
    let hi = match range.end_bound() {
        Bound::Included(end) => Some(nanos(end)),
        Bound::Excluded(end) => nanos(end).checked_sub(1),
        Bound::Unbounded => Some(MAX_DURATION_NANOS),
    };
    let Some(hi) = hi.filter(|hi| lo <= *hi && *hi <= u128::from(u64::MAX)) else {
        panic!("prop::duration: the range states no duration of 64-bit nanoseconds");
    };
    let bounds = IntegerBounds::of(index_of_nanos(lo), index_of_nanos(hi));
    Generator::from_node(Integer::<Duration> {
        id: "duration",
        bounds,
        from: |nanos| Duration::from_nanos(u64::try_from(nanos).unwrap_or_default()),
    })
}

fn index_of_nanos(nanos: u128) -> i128 {
    i128::try_from(nanos).unwrap_or(i128::MAX)
}

/// Returns a generator of the orderings of `values`: one swap choice per
/// position.
///
/// Position `i`, for `i` from 0 to `n - 2`, swaps with the index that the case
/// chooses in `[i, n - 1]`. The target of that choice is `i`, so the targets
/// leave the values in their stated order.
#[must_use]
pub fn permutation<T: Clone + Send + Sync + 'static>(
    values: impl IntoIterator<Item = T>,
) -> Generator<Vec<T>> {
    Generator::from_node(Permutation {
        values: values.into_iter().collect(),
    })
}

struct Permutation<T> {
    values: Vec<T>,
}

impl<T: Clone + Send + Sync + 'static> Node<Vec<T>> for Permutation<T> {
    fn id(&self) -> &'static str {
        "permutation"
    }

    fn decode(&self, case: &Case<'_>) -> Result<Vec<T>, Stop> {
        let mut ordered = self.values.clone();
        let last = ordered.len().saturating_sub(1);
        let _span = case.span("permutation", None);
        for at in 0..last {
            let other = case.integer(IntegerBounds::of(index(at), index(last)), None, false)?;
            ordered.swap(at, position(other));
        }
        Ok(ordered)
    }

    fn literal(&self, value: &Vec<T>) -> Option<Value> {
        literals(value).map(Value::List)
    }

    fn emit(&self, value: &Value) -> Result<Emitted, CannotInvert> {
        let mut current =
            literals(&self.values).ok_or_else(|| CannotInvert::no_inverse("permutation"))?;
        let Value::List(wanted) = value else {
            return Err(CannotInvert::refused(
                "the value is no permutation of the values",
            ));
        };
        if wanted.len() != current.len() {
            return Err(CannotInvert::refused(
                "the value is no permutation of the values",
            ));
        }
        let last = current.len().saturating_sub(1);
        let mut steps = Vec::with_capacity(last);
        for at in 0..last {
            let other = at + index_of(&current[at..], &wanted[at])?;
            steps.push(Planned::new(
                Bounds::Integer(IntegerBounds::of(index(at), index(last))),
                Choice::Integer(index(other)),
            ));
            current.swap(at, other);
        }
        if !same(&Value::List(current), value) {
            return Err(CannotInvert::refused(
                "the value is no permutation of the values",
            ));
        }
        Ok((steps, value.clone()))
    }
}

/// The values that one recursive value may draw from its base when the
/// generator states no other limit.
const DEFAULT_MAX_LEAVES: usize = 100;

/// The extension of a recursive generator, as a function of the position of
/// the value being decoded.
type Extend<T> = Arc<dyn Fn(Generator<T>) -> Generator<T> + Send + Sync>;

/// Returns a generator of recursive values: a value of `base`, or a value of
/// the extension that `extend` returns, whose positions are recursive values.
///
/// `extend` receives the generator of one position and returns the
/// extension. Each position decides between the base, 0, and the extension,
/// 1, with an integer choice that decides structure. Once one value has drawn
/// 100 values from the base, every further position takes the base;
/// [`Generator::max_leaves`] states another limit.
///
/// Inside the generator of `T` that a run states with
/// [`using`](crate::prop::using), a draw of [`of`](crate::prop::of) in the
/// extension takes each `T` from the generator of the position. That applies
/// to a draw of `T`, to a draw of a type that contains `T`, and to the draws
/// inside the stated generator of another type. The base takes `T` without
/// the stated generator, as `using` states.
///
/// ```
/// use dokimi_assert::prop;
///
/// #[derive(Debug, Clone)]
/// enum Tree {
///     Leaf(u8),
///     Node(Vec<Tree>),
/// }
///
/// let trees = prop::recursive(prop::integer(..).map(Tree::Leaf), |tree| {
///     prop::list(tree, ..=3).map(Tree::Node)
/// });
/// # let _ = trees;
/// ```
#[must_use]
pub fn recursive<T: 'static>(
    base: Generator<T>,
    extend: impl Fn(Generator<T>) -> Generator<T> + Send + Sync + 'static,
) -> Generator<T> {
    build_recursive(base, Arc::new(extend), DEFAULT_MAX_LEAVES)
}

/// The number that tells the recursive values of one case apart.
static NEXT_RECURSIVE: AtomicUsize = AtomicUsize::new(0);

fn build_recursive<T: 'static>(
    base: Generator<T>,
    extend: Extend<T>,
    max_leaves: usize,
) -> Generator<T> {
    let owner = Arc::new_cyclic(|own: &Weak<Recursive<T>>| {
        let position = Generator::from_node(Position { owner: own.clone() });
        Recursive {
            id: NEXT_RECURSIVE.fetch_add(1, Ordering::Relaxed),
            extension: extend(position.clone()),
            position,
            base,
            extend,
            max_leaves,
        }
    });
    Generator::from_arc(owner)
}

/// A recursive value: the base, or the extension whose positions are
/// recursive values.
struct Recursive<T> {
    id: usize,
    base: Generator<T>,
    extend: Extend<T>,
    extension: Generator<T>,
    /// The generator of the positions of the extension.
    position: Generator<T>,
    max_leaves: usize,
}

impl<T: 'static> Recursive<T> {
    /// Decodes one position of the value being decoded.
    fn position(&self, case: &Case<'_>) -> Result<T, Stop> {
        let leaves = case.with_record(|record| {
            record
                .leaves
                .get(&self.id)
                .and_then(|stack| stack.last().copied())
                .unwrap_or_default()
        });
        let bounds = IntegerBounds::of(0, i128::from(leaves < self.max_leaves));
        let _span = case.span("recursive", None);
        if case.integer(bounds, Some(0), false)? == 1 {
            let _run = scope::extending(self.id, &self.position);
            return self.extension.decode(case);
        }
        case.with_record(|record| {
            if let Some(count) = record
                .leaves
                .get_mut(&self.id)
                .and_then(|stack| stack.last_mut())
            {
                *count += 1;
            }
        });
        self.base.decode(case)
    }
}

impl<T: 'static> Node<T> for Recursive<T> {
    fn id(&self) -> &'static str {
        "recursive"
    }

    fn decode(&self, case: &Case<'_>) -> Result<T, Stop> {
        case.with_record(|record| record.leaves.entry(self.id).or_default().push(0));
        let decoded = self.position(case);
        case.with_record(|record| {
            if let Some(stack) = record.leaves.get_mut(&self.id) {
                stack.pop();
            }
        });
        decoded
    }

    fn literal(&self, value: &T) -> Option<Value> {
        self.base.literal(value).or_else(|| {
            let _run = scope::extending(self.id, &self.position);
            self.extension.literal(value)
        })
    }

    fn emit(&self, value: &Value) -> Result<Emitted, CannotInvert> {
        let base = self.base.emit(value);
        let _run = scope::extending(self.id, &self.position);
        first_branch(
            [(0, base), (1, self.extension.emit(value))].into_iter(),
            IntegerBounds::of(0, 1),
            "neither the base nor the extension produces the value",
        )
    }
}

/// A position of the recursive value being decoded, inside its extension.
struct Position<T> {
    owner: Weak<Recursive<T>>,
}

impl<T: 'static> Node<T> for Position<T> {
    fn id(&self) -> &'static str {
        "self"
    }

    fn decode(&self, case: &Case<'_>) -> Result<T, Stop> {
        match self.owner.upgrade() {
            Some(owner) => owner.position(case),
            None => Err(Stop::Rejected),
        }
    }

    fn literal(&self, value: &T) -> Option<Value> {
        self.owner.upgrade().and_then(|owner| owner.literal(value))
    }

    fn emit(&self, value: &Value) -> Result<Emitted, CannotInvert> {
        match self.owner.upgrade() {
            Some(owner) => owner.emit(value),
            None => Err(CannotInvert::refused("the recursive generator is gone")),
        }
    }
}

impl<T: 'static> Generator<T> {
    /// Returns the recursive generator with at most `n` values drawn from its
    /// base in one value.
    ///
    /// # Panics
    ///
    /// Panics for `n` of 0, and for a generator that [`recursive`] did not
    /// return.
    #[must_use]
    pub fn max_leaves(self, n: usize) -> Self {
        let node = self.flagged::<Recursive<T>>("max_leaves");
        assert!(n >= 1, "prop: max_leaves(0) admits no value");
        build_recursive(node.base.clone(), Arc::clone(&node.extend), n)
    }
}

/// Returns a generator whose value `f` returns from the case it receives.
///
/// `f` draws from other generators with [`Case::draw`], in one span labelled
/// `composite` around its draws. Its draws record their values as a body's
/// draws do. A value of a composite generator has no inverse.
#[must_use]
pub fn composite<T: 'static>(f: impl Fn(&Case<'_>) -> T + Send + Sync + 'static) -> Generator<T> {
    Generator::from_node(Composite { f: Arc::new(f) })
}

impl<T: 'static> Generator<T> {
    /// Returns the generator of a node that its owner shares.
    pub(crate) fn from_arc(node: Arc<impl Node<T>>) -> Self {
        Self::from_shared(node)
    }
}
