//! How a generated case draws each value from its random source.
//!
//! Every draw consumes the source in a fixed order, and the order is part of
//! the definition. The same seed gives the same values in every language only
//! when every implementation consumes the stream exactly as these functions
//! do, including the draws that they skip: equal bounds, a forced collection
//! flag and a one-element range consume nothing.

use super::choice::{
    Bounds, Choice, FloatBounds, IntegerBounds, NAN_BITS, SequenceBounds, Width, next_down,
    next_up, same_float,
};
use super::source::Source;
use std::sync::Arc;

/// One integer draw in `EDGE_ODDS` takes an edge value.
const EDGE_ODDS: u128 = 8;

/// A reusable integer draw takes an earlier value of its case with the same
/// bounds with odds 1 in `REUSE_ODDS`.
pub(crate) const REUSE_ODDS: u128 = 4;

/// The caps below which an integer draw chooses its offset, with even odds,
/// so that small offsets from the target are common and every value of the
/// bounds stays reachable.
const OFFSET_CAPS: [u128; 4] = [1 << 4, 1 << 8, 1 << 16, 1 << 64];

/// The average number of elements beyond its minimum that a collection
/// without a closer maximum gets.
const DEFAULT_EXTRA: usize = 5;

/// A float draw picks one of `FLOAT_BRANCHES`: branch 0 takes an edge value,
/// branches 1 to 3 an integral value, and the rest assemble the bits.
const FLOAT_BRANCHES: u128 = 8;
const FLOAT_LAST_INTEGRAL_BRANCH: u64 = 3;

/// A probability, as a fraction in lowest terms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ratio {
    num: u128,
    den: u128,
}

impl Ratio {
    /// Returns `num / den` in lowest terms.
    ///
    /// # Errors
    ///
    /// Returns `None` unless `0 <= num <= den`, `1 <= den <= 2^64`.
    #[must_use]
    pub fn new(num: u128, den: u128) -> Option<Self> {
        if !(1..=1 << 64).contains(&den) || num > den {
            return None;
        }
        let divisor = gcd(num, den);
        Some(Self {
            num: num / divisor,
            den: den / divisor,
        })
    }
}

/// The probability of 1/2.
pub(crate) const HALF: Ratio = Ratio { num: 1, den: 2 };

/// The probability of 1/4.
pub(crate) const QUARTER: Ratio = Ratio { num: 1, den: 4 };

fn gcd(a: u128, b: u128) -> u128 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// How a request draws its value from a source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Draw {
    /// An integer, a float or a sequence, as the draw of the kind of the
    /// bounds draws it.
    Bounds,
    /// 1 with a probability, and 0 otherwise, from one coin.
    Boolean(Ratio),
    /// The continue flag of a collection with `count` elements.
    Flag {
        /// The elements so far.
        count: usize,
        /// The fewest elements.
        min: usize,
        /// The most elements, or `None` for no maximum.
        max: Option<usize>,
        /// The average length.
        average: usize,
    },
    /// An index with a probability proportional to its weight.
    Weighted(Arc<[u64]>),
    /// An index below a count, uniform: `below(count)`.
    Below(usize),
    /// Whether swarm keeps an action.
    Keep {
        /// The probability of each coin.
        probability: Ratio,
        /// The actions from this one on.
        remaining: usize,
        /// Whether an earlier action is kept.
        kept: bool,
    },
}

impl Draw {
    /// Draws a value for `bounds` from `source`.
    pub(crate) fn run(&self, source: &mut Source, bounds: &Bounds) -> Choice {
        match (self, bounds) {
            (Self::Bounds, Bounds::Integer(bounds)) => Choice::Integer(integer(source, bounds)),
            (Self::Bounds, Bounds::Float(bounds)) => Choice::Float(float_value(source, bounds)),
            (Self::Bounds, Bounds::Sequence(bounds)) => Choice::Sequence(sequence(source, bounds)),
            (Self::Boolean(probability), _) => Choice::Integer(boolean(source, *probability)),
            (
                Self::Flag {
                    count,
                    min,
                    max,
                    average,
                },
                _,
            ) => Choice::Integer(flag(source, *count, *min, *max, *average)),
            (Self::Weighted(weights), _) => {
                Choice::Integer(i128::try_from(weighted(source, weights)).unwrap_or_default())
            }
            (Self::Below(count), _) => {
                Choice::Integer(i128::try_from(below(source, *count)).unwrap_or_default())
            }
            (
                Self::Keep {
                    probability,
                    remaining,
                    kept,
                },
                _,
            ) => Choice::Integer(keep(source, *probability, *remaining, *kept)),
        }
    }
}

/// Returns the edge values of integer bounds: the target, `lo`, `hi`, one
/// above the target and one below it, in that order, each once, and only
/// those inside the bounds.
pub(crate) fn integer_edges(bounds: &IntegerBounds) -> Vec<i128> {
    let target = bounds.target();
    let mut values = Vec::with_capacity(5);
    for value in [
        target,
        bounds.lo(),
        bounds.hi(),
        target.saturating_add(1),
        target.saturating_sub(1),
    ] {
        if bounds.admits(value) && !values.contains(&value) {
            values.push(value);
        }
    }
    values
}

/// Returns `below(n)` for a count of values.
pub(crate) fn below(source: &mut Source, n: usize) -> usize {
    usize::try_from(source.below(wide(n))).unwrap_or_default()
}

/// Returns a count as a `u128`, which holds every `usize`.
pub(crate) fn wide(n: usize) -> u128 {
    u128::try_from(n).unwrap_or(u128::MAX)
}

/// Draws an integer inside the bounds.
///
/// Equal bounds return their value and consume nothing. Otherwise a coin of 1
/// in 8 decides whether the draw takes an edge value, chosen with
/// `below(number of edges)`. The rest move away from the target: upward when
/// the target is `lo`, downward when it is `hi`, and by a coin of 1 in 2 when
/// it lies strictly between them. The offset is `below(min(span, cap - 1) +
/// 1)`, where span is the distance to the bound in that direction and cap is
/// one of `OFFSET_CAPS`, chosen with `below(4)`.
pub(crate) fn integer(source: &mut Source, bounds: &IntegerBounds) -> i128 {
    if bounds.lo() == bounds.hi() {
        return bounds.lo();
    }
    if source.coin(1, EDGE_ODDS) {
        let values = integer_edges(bounds);
        return values[below(source, values.len())];
    }
    let target = bounds.target();
    let up = if target == bounds.lo() {
        true
    } else if target == bounds.hi() {
        false
    } else {
        source.coin(1, 2)
    };
    let span = if up {
        bounds.hi().abs_diff(target)
    } else {
        target.abs_diff(bounds.lo())
    };
    let cap = OFFSET_CAPS[below(source, OFFSET_CAPS.len())];
    let offset = i128::from(source.below(span.min(cap - 1) + 1));
    if up { target + offset } else { target - offset }
}

/// Draws 1 with the probability and 0 otherwise, from one coin.
pub(crate) fn boolean(source: &mut Source, probability: Ratio) -> i128 {
    i128::from(source.coin(probability.num, probability.den))
}

/// Draws an index with a probability proportional to its weight.
///
/// One weight returns 0 and consumes nothing. Otherwise the draw is
/// `below(sum of the weights)`, and the index is the first whose running sum
/// exceeds it.
pub(crate) fn weighted(source: &mut Source, weights: &[u64]) -> usize {
    if weights.len() == 1 {
        return 0;
    }
    let total: u128 = weights.iter().map(|weight| u128::from(*weight)).sum();
    let point = u128::from(source.below(total));
    // The point is below the total, so a running sum exceeds it.
    let mut running = 0;
    weights
        .iter()
        .position(|weight| {
            running += u128::from(*weight);
            point < running
        })
        .unwrap_or(weights.len() - 1)
}

/// Decides whether swarm keeps an action: 1 keeps it, and 0 disables it.
///
/// `remaining` is the number of actions from this one on, and `kept` is true
/// when an earlier action is kept. Once an earlier action is kept, one coin
/// with the probability decides. While none is, the last action is kept and
/// consumes nothing. Before it, the draw tosses one coin for this action and
/// one for each later action, again until a coin comes up, and keeps the
/// action when its own coin came up.
pub(crate) fn keep(source: &mut Source, probability: Ratio, remaining: usize, kept: bool) -> i128 {
    if kept {
        return boolean(source, probability);
    }
    if remaining == 1 {
        return 1;
    }
    loop {
        let coins: Vec<i128> = (0..remaining)
            .map(|_| boolean(source, probability))
            .collect();
        if coins.contains(&1) {
            return coins[0];
        }
    }
}

/// Returns the average length of a collection.
///
/// The average is `min + min(max(min, 5), ceil((max - min) / 2))`, and an
/// unbounded maximum leaves `min + max(min, 5)`. It is an integer, so the
/// continue coin has integer odds.
pub(crate) fn average_length(min_size: usize, max_size: Option<usize>) -> usize {
    let mut extra = min_size.max(DEFAULT_EXTRA);
    if let Some(max) = max_size {
        extra = extra.min((max - min_size).div_ceil(2));
    }
    min_size + extra
}

/// Returns the bounds of the decision whether a collection gets another
/// element: `[1, 1]` while `count` is below the minimum, `[0, 0]` once it is
/// at the maximum, and `[0, 1]` otherwise.
pub(crate) fn flag_bounds(count: usize, min_size: usize, max_size: Option<usize>) -> IntegerBounds {
    if count < min_size {
        return IntegerBounds::of(1, 1);
    }
    if max_size.is_some_and(|max| count >= max) {
        return IntegerBounds::of(0, 0);
    }
    IntegerBounds::of(0, 1)
}

/// Decides whether a collection with `count` elements gets another.
///
/// A forced decision returns the one value that its bounds allow and
/// consumes nothing. A free one continues by a coin of `(average - min)` in
/// `(average - min + 1)`.
pub(crate) fn flag(
    source: &mut Source,
    count: usize,
    min_size: usize,
    max_size: Option<usize>,
    average: usize,
) -> i128 {
    let bounds = flag_bounds(count, min_size, max_size);
    if bounds.lo() == bounds.hi() {
        return bounds.lo();
    }
    let extra = wide(average - min_size);
    i128::from(source.coin(extra, extra + 1))
}

/// Returns the edge values of float bounds: the target, `lo`, `hi`, the next
/// value above the target and the next below it, in that order, each once,
/// and only those that the bounds admit, followed by NaN when the bounds
/// allow it.
pub(crate) fn float_edges(bounds: &FloatBounds) -> Vec<f64> {
    let target = bounds.target();
    let mut values: Vec<f64> = Vec::with_capacity(6);
    for value in [
        target,
        bounds.lo(),
        bounds.hi(),
        next_up(target, bounds.width()),
        next_down(target, bounds.width()),
    ] {
        if bounds.admits(value) && !values.iter().any(|held| same_float(*held, value)) {
            values.push(value);
        }
    }
    if bounds.allow_nan() {
        values.push(f64::from_bits(NAN_BITS));
    }
    values
}

/// Returns the magnitude below which every integer is a value of the width.
fn exact_integers(width: Width) -> i128 {
    match width {
        Width::W32 => 1 << 24,
        Width::W64 => 1 << 53,
    }
}

/// Returns the integers that the bounds admit and that are exact values of
/// the width, or `None` when there are none.
fn integral_bounds(bounds: &FloatBounds) -> Option<IntegerBounds> {
    if bounds.lo() == f64::INFINITY || bounds.hi() == f64::NEG_INFINITY {
        return None;
    }
    let limit = exact_integers(bounds.width()) - 1;
    let lo = if bounds.lo() == f64::NEG_INFINITY {
        -limit
    } else {
        integral(bounds.lo().ceil()).max(-limit)
    };
    let hi = if bounds.hi() == f64::INFINITY {
        limit
    } else {
        integral(bounds.hi().floor()).min(limit)
    };
    (lo <= hi).then(|| IntegerBounds::of(lo, hi))
}

/// Returns an integral float as an integer, saturated at the range of
/// `i128`, which the integral bounds then clamp to 2^53.
#[expect(
    clippy::cast_possible_truncation,
    reason = "an integral float converts to its integer, saturated, and the bounds clamp it to 2^53"
)]
fn integral(x: f64) -> i128 {
    x as i128
}

/// Assembles a float from a sign, an exponent and a mantissa.
///
/// The draws are `below(2)`, `below(2^exponent bits)` and `below(2^mantissa
/// bits)`, in that order. A NaN becomes the canonical NaN when the bounds
/// allow it and the target otherwise. A value below `lo` becomes `lo` and one
/// above `hi` becomes `hi`, compared numerically, so -0 stays -0 when `lo` is
/// +0.
fn from_parts(source: &mut Source, bounds: &FloatBounds) -> f64 {
    let (exponent_bits, mantissa_bits) = match bounds.width() {
        Width::W32 => (8, 23),
        Width::W64 => (11, 52),
    };
    let sign = source.below(2);
    let exponent = source.below(1 << exponent_bits);
    let mantissa = source.below(1 << mantissa_bits);
    let bits = (sign << (exponent_bits + mantissa_bits)) | (exponent << mantissa_bits) | mantissa;
    let value = match bounds.width() {
        Width::W64 => f64::from_bits(bits),
        Width::W32 => f64::from(f32::from_bits(u32::try_from(bits).unwrap_or_default())),
    };
    if value.is_nan() {
        return if bounds.allow_nan() {
            f64::from_bits(NAN_BITS)
        } else {
            bounds.target()
        };
    }
    if value < bounds.lo() {
        return bounds.lo();
    }
    if value > bounds.hi() {
        return bounds.hi();
    }
    value
}

/// Draws a float inside the bounds.
///
/// `below(8)` picks the branch. Branch 0 takes an edge value, chosen with
/// `below(number of edges)`. Branches 1 to 3 draw an integer, as [`integer`]
/// does, over the integers that the bounds admit and that are exact values
/// of the width. The other branches, and an integral branch whose bounds
/// admit no such integer, assemble the value from its bits.
#[expect(
    clippy::cast_precision_loss,
    reason = "the integer is below 2^53 in magnitude, which an f64 states exactly"
)]
pub(crate) fn float_value(source: &mut Source, bounds: &FloatBounds) -> f64 {
    let branch = source.below(FLOAT_BRANCHES);
    if branch == 0 {
        let values = float_edges(bounds);
        return values[below(source, values.len())];
    }
    if branch <= FLOAT_LAST_INTEGRAL_BRANCH {
        if let Some(integral) = integral_bounds(bounds) {
            return integer(source, &integral) as f64;
        }
    }
    from_parts(source, bounds)
}

/// Draws a sequence inside the bounds.
///
/// Each position first takes the decision that [`flag`] makes for a
/// collection, and when it continues, an element drawn as an integer in
/// `[0, k - 1]`. A byte string therefore consumes the stream exactly as a list
/// of integers in `[0, 255]` with the same sizes does, and gets the same
/// values.
pub(crate) fn sequence(source: &mut Source, bounds: &SequenceBounds) -> Vec<u64> {
    let average = average_length(bounds.min_size(), bounds.max_size());
    let element = IntegerBounds::of(0, i128::from(bounds.k()) - 1);
    let mut elements = Vec::new();
    while flag(
        source,
        elements.len(),
        bounds.min_size(),
        bounds.max_size(),
        average,
    ) == 1
    {
        elements.push(u64::try_from(integer(source, &element)).unwrap_or_default());
    }
    elements
}
