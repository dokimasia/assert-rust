//! The three kinds of choice that a case is made of.
//!
//! A choice is one typed decision within bounds: an integer, a float or a
//! sequence of small integers. Every choice has a target, the simplest value
//! that its bounds allow, and a sort key that orders its values from the
//! simplest. The shrinker moves values towards their targets, and it accepts
//! a candidate only when the case gets shortlex-smaller under these keys.
//!
//! A replay hands a generator the value that a case recorded. When that value
//! is of another kind, or outside the bounds that the generator now requests,
//! the bounds coerce it: a sequence is cut, extended or cleaned, and any
//! other value becomes the target. Every prefix of a recorded case is
//! therefore a valid case.

use std::cmp::Ordering;
use std::fmt;

/// The smallest and the largest bound of an integer choice: both bounds are
/// inside the signed or both inside the unsigned 64-bit range.
pub const INT64_MIN: i128 = i64::MIN as i128;
/// The largest bound of a signed integer choice.
pub const INT64_MAX: i128 = i64::MAX as i128;
/// The largest bound of an unsigned integer choice.
pub const UINT64_MAX: i128 = u64::MAX as i128;

/// Integral floats below this magnitude sort first, by magnitude.
pub const INTEGRAL_LIMIT: f64 = 9_007_199_254_740_992.0;

/// The bits of the canonical quiet NaN. Every NaN of a choice has them.
pub const NAN_BITS: u64 = 0x7FF8_0000_0000_0000;

/// The width of a float choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Width {
    /// A binary32 float, `f32`.
    W32,
    /// A binary64 float, `f64`.
    W64,
}

impl Width {
    /// Returns the width in bits: 32 or 64.
    #[must_use]
    pub fn bits(self) -> u32 {
        match self {
            Self::W32 => 32,
            Self::W64 => 64,
        }
    }
}

/// One recorded decision: its kind and the value that the case used.
#[derive(Debug, Clone, PartialEq)]
pub enum Choice {
    /// An integer, inside the signed or the unsigned 64-bit range.
    Integer(i128),
    /// A float. A float of width 32 is held as the `f64` of its value.
    Float(f64),
    /// A sequence of small integers.
    Sequence(Vec<u64>),
}

impl Choice {
    /// Returns the integer of an integer choice.
    #[must_use]
    pub fn as_integer(&self) -> Option<i128> {
        match self {
            Self::Integer(value) => Some(*value),
            _ => None,
        }
    }

    /// Returns the float of a float choice.
    #[must_use]
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Self::Float(value) => Some(*value),
            _ => None,
        }
    }

    /// Returns the elements of a sequence choice.
    #[must_use]
    pub fn as_sequence(&self) -> Option<&[u64]> {
        match self {
            Self::Sequence(elements) => Some(elements),
            _ => None,
        }
    }

    /// Returns what the choice counts towards a case's cap: one, plus one for
    /// each element of a sequence.
    pub(crate) fn cost(&self) -> usize {
        match self {
            Self::Sequence(elements) => 1 + elements.len(),
            _ => 1,
        }
    }

    /// Returns the choice in the form the case tree and the shrinker compare
    /// it by: a float by its bits, with one NaN.
    pub(crate) fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Float(a), Self::Float(b)) => same_float(*a, *b),
            _ => self == other,
        }
    }
}

/// Why bounds admit no value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundsError(pub(crate) String);

impl fmt::Display for BoundsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for BoundsError {}

/// The bounds of an integer choice, both inclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IntegerBounds {
    lo: i128,
    hi: i128,
}

impl IntegerBounds {
    /// Returns the bounds `[lo, hi]`.
    ///
    /// # Errors
    ///
    /// Returns an error when `lo` exceeds `hi`, or when the bounds lie inside
    /// neither the signed nor the unsigned 64-bit range.
    pub fn new(lo: i128, hi: i128) -> Result<Self, BoundsError> {
        if lo > hi {
            return Err(BoundsError(format!(
                "integer bounds [{lo}, {hi}] are empty"
            )));
        }
        let signed = lo >= INT64_MIN && hi <= INT64_MAX;
        let unsigned = lo >= 0 && hi <= UINT64_MAX;
        if !signed && !unsigned {
            return Err(BoundsError(format!(
                "integer bounds [{lo}, {hi}] are inside neither the signed nor the unsigned 64-bit range"
            )));
        }
        Ok(Self { lo, hi })
    }

    /// Returns the bounds `[lo, hi]` of values that a caller has checked.
    pub(crate) fn of(lo: i128, hi: i128) -> Self {
        Self { lo, hi }
    }

    /// Returns the lower bound.
    #[must_use]
    pub fn lo(&self) -> i128 {
        self.lo
    }

    /// Returns the upper bound.
    #[must_use]
    pub fn hi(&self) -> i128 {
        self.hi
    }

    /// Returns the value closest to zero, which is zero when the bounds
    /// admit it.
    #[must_use]
    pub fn target(&self) -> i128 {
        if self.lo <= 0 && 0 <= self.hi {
            0
        } else if self.lo > 0 {
            self.lo
        } else {
            self.hi
        }
    }

    /// Returns the number of values that the bounds admit.
    pub(crate) fn size(&self) -> u128 {
        self.hi.abs_diff(self.lo) + 1
    }

    /// Returns the sort key of `value`: its distance to the target, then
    /// whether it is below the target.
    pub(crate) fn key(&self, value: i128) -> (u128, bool) {
        let target = self.target();
        (value.abs_diff(target), value < target)
    }

    /// Returns the position of `value` in the key order of the bounds.
    ///
    /// The target is 0. The order runs one above the target, one below, two
    /// above, two below, and so on, and continues on the longer side once
    /// the shorter side reaches its bound.
    pub(crate) fn rank(&self, value: i128) -> u128 {
        let target = self.target();
        let shorter = self.hi.abs_diff(target).min(target.abs_diff(self.lo));
        let distance = value.abs_diff(target);
        if distance > shorter {
            shorter + distance
        } else if value > target {
            2 * distance - 1
        } else {
            2 * distance
        }
    }

    /// Returns the value at a position of the key order, the inverse of
    /// [`IntegerBounds::rank`].
    pub(crate) fn at_rank(&self, rank: u128) -> i128 {
        let target = self.target();
        let above = self.hi.abs_diff(target);
        let below = target.abs_diff(self.lo);
        let shorter = above.min(below);
        if rank > 2 * shorter {
            let distance = signed(rank - shorter);
            return if above > below {
                target + distance
            } else {
                target - distance
            };
        }
        let distance = signed(rank.div_ceil(2));
        if rank % 2 == 1 {
            target + distance
        } else {
            target - distance
        }
    }

    /// Returns `true` when `value` is inside the bounds.
    #[must_use]
    pub fn admits(&self, value: i128) -> bool {
        self.lo <= value && value <= self.hi
    }

    /// Returns the recorded value when it fits, and the target otherwise.
    #[must_use]
    pub fn coerce(&self, recorded: &Choice) -> i128 {
        match recorded {
            Choice::Integer(value) if self.admits(*value) => *value,
            _ => self.target(),
        }
    }
}

/// Returns a distance as an `i128`, and `i128::MAX` for a distance beyond
/// it.
pub(crate) fn signed(distance: u128) -> i128 {
    i128::try_from(distance).unwrap_or(i128::MAX)
}

/// The bounds of a float choice: an inclusive range, NaN and a width.
///
/// An infinite bound admits that infinity. NaN is a value only when
/// `allow_nan` is set, and it is always the canonical quiet NaN.
#[derive(Debug, Clone, Copy)]
pub struct FloatBounds {
    lo: f64,
    hi: f64,
    allow_nan: bool,
    width: Width,
}

impl PartialEq for FloatBounds {
    fn eq(&self, other: &Self) -> bool {
        self.lo == other.lo
            && self.hi == other.hi
            && self.allow_nan == other.allow_nan
            && self.width == other.width
    }
}

impl Eq for FloatBounds {}

impl FloatBounds {
    /// Returns the bounds `[lo, hi]` of a float of `width`.
    ///
    /// # Errors
    ///
    /// Returns an error when a bound is NaN or not a value of the width, or
    /// when `lo` exceeds `hi`.
    pub fn new(lo: f64, hi: f64, allow_nan: bool, width: Width) -> Result<Self, BoundsError> {
        if lo.is_nan() || hi.is_nan() || lo > hi {
            return Err(BoundsError(format!(
                "float bounds [{lo:?}, {hi:?}] are empty"
            )));
        }
        for bound in [lo, hi] {
            if !representable(bound, width) {
                return Err(BoundsError(format!(
                    "float bound {bound:?} is not a value of width {}",
                    width.bits()
                )));
            }
        }
        Ok(Self {
            lo,
            hi,
            allow_nan,
            width,
        })
    }

    /// Returns the lower bound.
    #[must_use]
    pub fn lo(&self) -> f64 {
        self.lo
    }

    /// Returns the upper bound.
    #[must_use]
    pub fn hi(&self) -> f64 {
        self.hi
    }

    /// Returns `true` when NaN is a value of the bounds.
    #[must_use]
    pub fn allow_nan(&self) -> bool {
        self.allow_nan
    }

    /// Returns the width.
    #[must_use]
    pub fn width(&self) -> Width {
        self.width
    }

    /// Returns the value of the bounds with the smallest sort key.
    #[must_use]
    pub fn target(&self) -> f64 {
        simplest_float(self.lo, self.hi)
    }

    /// Returns `true` when `value` is a float of the width inside the
    /// bounds, or NaN where the bounds allow it.
    #[must_use]
    pub fn admits(&self, value: f64) -> bool {
        if value.is_nan() {
            return self.allow_nan;
        }
        self.lo <= value && value <= self.hi && representable(value, self.width)
    }

    /// Returns the recorded value when it fits, and the target otherwise.
    #[must_use]
    pub fn coerce(&self, recorded: &Choice) -> f64 {
        match recorded {
            Choice::Float(value) if self.admits(*value) => {
                if value.is_nan() {
                    f64::from_bits(NAN_BITS)
                } else {
                    *value
                }
            }
            _ => self.target(),
        }
    }
}

/// The bounds of a sequence choice: its element range and its length.
///
/// Every element is an integer in `[0, k)`. A `max_size` of `None` leaves the
/// length unbounded, and the cap on the choices of a case bounds it instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SequenceBounds {
    k: u64,
    min_size: usize,
    max_size: Option<usize>,
}

impl SequenceBounds {
    /// Returns the bounds of a sequence of elements in `[0, k)` with a length
    /// in `[min_size, max_size]`.
    ///
    /// # Errors
    ///
    /// Returns an error when `k` is 0, or when `max_size` is below
    /// `min_size`.
    pub fn new(k: u64, min_size: usize, max_size: Option<usize>) -> Result<Self, BoundsError> {
        if k < 1 {
            return Err(BoundsError(format!(
                "a sequence with k = {k} has no element"
            )));
        }
        if let Some(max) = max_size.filter(|max| *max < min_size) {
            return Err(BoundsError(format!(
                "sequence sizes [{min_size}, {max}] are empty"
            )));
        }
        Ok(Self {
            k,
            min_size,
            max_size,
        })
    }

    /// Returns the bounds of a sequence that a caller has checked: `k` is 1
    /// or more, and `max_size` is not below `min_size`.
    pub(crate) fn of(k: u64, min_size: usize, max_size: Option<usize>) -> Self {
        Self {
            k,
            min_size,
            max_size,
        }
    }

    /// Returns the number of element values.
    #[must_use]
    pub fn k(&self) -> u64 {
        self.k
    }

    /// Returns the fewest elements.
    #[must_use]
    pub fn min_size(&self) -> usize {
        self.min_size
    }

    /// Returns the most elements, or `None` for no maximum.
    #[must_use]
    pub fn max_size(&self) -> Option<usize> {
        self.max_size
    }

    /// Returns `min_size` zeros.
    #[must_use]
    pub fn target(&self) -> Vec<u64> {
        vec![0; self.min_size]
    }

    /// Returns `true` when the bounds admit `elements`.
    #[must_use]
    pub fn admits(&self, elements: &[u64]) -> bool {
        elements.len() >= self.min_size
            && self.max_size.is_none_or(|max| elements.len() <= max)
            && elements.iter().all(|element| *element < self.k)
    }

    /// Fits a recorded sequence to the bounds, or returns the target.
    ///
    /// A longer sequence is cut to `max_size`, a shorter one is extended with
    /// zeros to `min_size`, and an element outside `[0, k)` becomes 0. A value
    /// of another kind becomes the target.
    #[must_use]
    pub fn coerce(&self, recorded: &Choice) -> Vec<u64> {
        let Choice::Sequence(elements) = recorded else {
            return self.target();
        };
        let mut fitted: Vec<u64> = elements
            .iter()
            .map(|element| if *element < self.k { *element } else { 0 })
            .collect();
        if let Some(max) = self.max_size {
            fitted.truncate(max);
        }
        if fitted.len() < self.min_size {
            fitted.resize(self.min_size, 0);
        }
        fitted
    }
}

/// The bounds of one choice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bounds {
    /// The bounds of an integer.
    Integer(IntegerBounds),
    /// The bounds of a float.
    Float(FloatBounds),
    /// The bounds of a sequence.
    Sequence(SequenceBounds),
}

impl Bounds {
    /// Returns the simplest value of the bounds.
    #[must_use]
    pub fn target(&self) -> Choice {
        match self {
            Self::Integer(bounds) => Choice::Integer(bounds.target()),
            Self::Float(bounds) => Choice::Float(bounds.target()),
            Self::Sequence(bounds) => Choice::Sequence(bounds.target()),
        }
    }

    /// Returns `true` when the bounds admit `value`, a value of their kind.
    #[must_use]
    pub fn admits(&self, value: &Choice) -> bool {
        match (self, value) {
            (Self::Integer(bounds), Choice::Integer(held)) => bounds.admits(*held),
            (Self::Float(bounds), Choice::Float(held)) => bounds.admits(*held),
            (Self::Sequence(bounds), Choice::Sequence(held)) => bounds.admits(held),
            _ => false,
        }
    }

    /// Fits a recorded choice to the bounds, as each kind's `coerce` does.
    #[must_use]
    pub fn coerce(&self, recorded: &Choice) -> Choice {
        match self {
            Self::Integer(bounds) => Choice::Integer(bounds.coerce(recorded)),
            Self::Float(bounds) => Choice::Float(bounds.coerce(recorded)),
            Self::Sequence(bounds) => Choice::Sequence(bounds.coerce(recorded)),
        }
    }

    /// Returns the key of `value` under the bounds: the rank of its kind, then
    /// its sort key.
    pub(crate) fn key(&self, value: &Choice) -> ChoiceKey {
        match value {
            Choice::Integer(held) => {
                let unbounded = (held.unsigned_abs(), *held < 0);
                let (distance, below) = self
                    .as_integer()
                    .map_or(unbounded, |bounds| bounds.key(*held));
                ChoiceKey::Integer { distance, below }
            }
            Choice::Float(held) => ChoiceKey::Float(float_key(*held)),
            Choice::Sequence(held) => ChoiceKey::Sequence {
                length: held.len(),
                elements: held.clone(),
            },
        }
    }

    /// Returns the integer bounds of an integer choice.
    #[must_use]
    pub fn as_integer(&self) -> Option<&IntegerBounds> {
        match self {
            Self::Integer(bounds) => Some(bounds),
            _ => None,
        }
    }
}

/// The sort key of a float: a smaller key is a simpler value.
///
/// From the simplest: integral values below 2^53 in magnitude, by magnitude;
/// other finite values, by their number of fractional bits and then their
/// numerator; the infinities; NaN. Within a group a positive value precedes
/// the negative value of the same magnitude, and +0 precedes -0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct FloatKey {
    group: u8,
    fraction_bits: u32,
    /// The bits of the magnitude of the numerator. The bits of a float that
    /// is not negative order as the float does.
    numerator: u64,
    negative: bool,
}

/// The groups of a float's key, simplest first.
const GROUP_INTEGRAL: u8 = 0;
const GROUP_FRACTION: u8 = 1;
const GROUP_INFINITE: u8 = 2;
const GROUP_NAN: u8 = 3;

/// Returns the sort key of a float.
pub(crate) fn float_key(x: f64) -> FloatKey {
    let negative = x.is_sign_negative();
    if x.is_nan() {
        return FloatKey {
            group: GROUP_NAN,
            fraction_bits: 0,
            numerator: 0,
            negative: false,
        };
    }
    if x.is_infinite() {
        return FloatKey {
            group: GROUP_INFINITE,
            fraction_bits: 0,
            numerator: 0,
            negative,
        };
    }
    let magnitude = x.abs();
    let fraction_bits = fraction_bits(magnitude);
    if fraction_bits == 0 && magnitude < INTEGRAL_LIMIT {
        return FloatKey {
            group: GROUP_INTEGRAL,
            fraction_bits: 0,
            numerator: magnitude.to_bits(),
            negative,
        };
    }
    FloatKey {
        group: GROUP_FRACTION,
        fraction_bits,
        numerator: scaled(magnitude, fraction_bits).to_bits(),
        negative,
    }
}

/// Returns the bits after the binary point of a finite, non-negative float:
/// `k` for a value `m / 2^k` in lowest terms, and 0 for an integer.
pub(crate) fn fraction_bits(magnitude: f64) -> u32 {
    if magnitude == 0.0 || magnitude.fract() == 0.0 {
        return 0;
    }
    let bits = magnitude.to_bits();
    let biased = u32::try_from(bits >> 52).unwrap_or_default();
    let mantissa = bits & ((1 << 52) - 1);
    // A value is significand * 2^(exponent - 1075) with a significand that has
    // the implicit bit of a normal value.
    let (significand, exponent) = if biased == 0 {
        (mantissa, 1)
    } else {
        (mantissa | (1 << 52), biased)
    };
    1075 - exponent - significand.trailing_zeros()
}

/// Returns `magnitude * 2^k`, which is the numerator of a value with `k`
/// fractional bits, below 2^53 and exact.
pub(crate) fn scaled(magnitude: f64, k: u32) -> f64 {
    let mut value = magnitude;
    let mut left = k;
    while left > 0 {
        let step = left.min(1000);
        value *= power_of_two(step);
        left -= step;
    }
    value
}

/// Returns 2^n for `n` in `[0, 1023]`.
fn power_of_two(n: u32) -> f64 {
    f64::from_bits(u64::from(1023 + n) << 52)
}

/// Returns 2^-n for `n` in `[0, 1074]`, a normal or a subnormal float.
pub(crate) fn inverse_power_of_two(n: u32) -> f64 {
    if n <= 1022 {
        f64::from_bits(u64::from(1023 - n) << 52)
    } else {
        f64::from_bits(1 << (1074 - n))
    }
}

/// Returns `true` when two floats are one value: NaN is one value, and the
/// two zeros differ.
#[must_use]
pub fn same_float(a: f64, b: f64) -> bool {
    if a.is_nan() || b.is_nan() {
        return a.is_nan() && b.is_nan();
    }
    a.to_bits() == b.to_bits()
}

/// Returns the nearest value of width 32 to `x`, or an infinity.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a float of width 32 is the f32 that x rounds to, as IEEE 754 rounds it"
)]
fn round32(x: f64) -> f64 {
    f64::from(x as f32)
}

/// Returns `true` when `x` is a value of a float of the width.
#[must_use]
#[expect(
    clippy::float_cmp,
    reason = "a value of width 32 rounds to itself exactly, and to no nearby value"
)]
pub fn representable(x: f64, width: Width) -> bool {
    if width == Width::W64 || x.is_nan() || x.is_infinite() {
        return true;
    }
    round32(x) == x
}

/// Returns the smallest value of the width above `x`.
///
/// Both zeros step to the smallest positive subnormal. Positive infinity and
/// NaN return themselves.
#[must_use]
pub fn next_up(x: f64, width: Width) -> f64 {
    if x.is_nan() || x == f64::INFINITY {
        return x;
    }
    match width {
        Width::W64 => {
            if x == 0.0 {
                return f64::from_bits(1);
            }
            let bits = x.to_bits();
            f64::from_bits(if x > 0.0 { bits + 1 } else { bits - 1 })
        }
        Width::W32 => {
            if x == 0.0 {
                return f64::from(f32::from_bits(1));
            }
            #[expect(
                clippy::cast_possible_truncation,
                reason = "x is a value of width 32, which an f32 states exactly"
            )]
            let bits = (x as f32).to_bits();
            f64::from(f32::from_bits(if x > 0.0 { bits + 1 } else { bits - 1 }))
        }
    }
}

/// Returns the largest value of the width below `x`.
#[must_use]
pub fn next_down(x: f64, width: Width) -> f64 {
    -next_up(-x, width)
}

/// Returns the value in `[lo, hi]` with the smallest key.
///
/// The bounds are values of one width with `lo <= hi`, so the range always
/// contains one, and the value is a value of that width.
#[must_use]
pub fn simplest_float(lo: f64, hi: f64) -> f64 {
    if lo <= 0.0 && 0.0 <= hi {
        return 0.0;
    }
    if lo > 0.0 {
        return simplest_positive(lo, hi);
    }
    -simplest_positive(-hi, -lo)
}

/// Returns the simplest value in `[lo, hi]`, for `0 < lo <= hi`.
fn simplest_positive(lo: f64, hi: f64) -> f64 {
    // Every value at 2^53 or above is an integer or an infinity, and the
    // smallest of those values is the simplest.
    if lo >= INTEGRAL_LIMIT {
        return lo;
    }
    // The ceiling of a value of either width below 2^53 is a value of the
    // width.
    let smallest_integral = lo.ceil();
    if smallest_integral <= hi {
        return smallest_integral;
    }
    // lo is a fraction m / 2^k in lowest terms, with an odd m below 2^53. A
    // fraction n / 2^f in lowest terms has an odd n, and at one f the smallest
    // odd n in range has the fewest significant bits. At f = k the fraction is
    // lo itself, so a fraction with fewer bits is simpler than lo.
    let k = fraction_bits(lo);
    let m = scaled(lo, k);
    for f in 1..k {
        // ceil(m / 2^(k - f)). A quotient of 1 or more is a normal float and
        // exact, and a smaller one is above 0, so its ceiling is 1.
        let mut numerator = (m * inverse_power_of_two(k - f)).ceil();
        if numerator % 2.0 == 0.0 {
            numerator += 1.0;
        }
        // An odd numerator below 2^53, or below 2^24 for a lo of width 32,
        // over 2^f with f below k is a value of the width of lo, exactly.
        let candidate = numerator * inverse_power_of_two(f);
        if candidate <= hi {
            return candidate;
        }
    }
    lo
}

/// The key of a choice under its bounds. Choices of different kinds compare
/// by kind: an integer, then a float, then a sequence.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ChoiceKey {
    /// An integer: its distance to the target, then whether it is below.
    Integer { distance: u128, below: bool },
    /// A float, by its sort key.
    Float(FloatKey),
    /// A sequence: its length, then its elements.
    Sequence { length: usize, elements: Vec<u64> },
}

/// The shortlex key of a choice sequence: a shorter sequence is smaller, and
/// of two sequences of one length the one with the smaller key where they
/// first differ is smaller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Shortlex(pub(crate) Vec<ChoiceKey>);

impl PartialOrd for Shortlex {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Shortlex {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0
            .len()
            .cmp(&other.0.len())
            .then_with(|| self.0.cmp(&other.0))
    }
}
