//! The fuzz bridge: a fuzzer's bytes decoded into the choices of one case.
//!
//! The bridge reads each value from the bytes, in request order:
//!
//! - An integer in `[lo, hi]` reads `ceil(bit_length(hi - lo) / 8)` bytes as a
//!   little-endian unsigned `u`, and takes `lo + u mod (hi - lo + 1)`. Bounds
//!   with one value read no byte.
//! - A float reads 8 bytes, or 4 for width 32, little-endian, as the bits of
//!   a float of its width. A value that its bounds do not admit takes the
//!   target, and an allowed NaN becomes the canonical NaN. Bounds that admit
//!   one value read no byte.
//! - A sequence reads its length as an integer in `[min_size, max_size]`, or
//!   in `[min_size, min_size + 65535]` when it has no maximum. Then it reads
//!   each element as an integer in `[0, k - 1]`.
//! - A value that needs more bytes than remain takes its target, and the
//!   bytes that remain are spent, so every later value takes its target too.
//!   An element cut short ends its sequence, which zeros extend to
//!   `min_size`.
//!
//! Every byte string decodes to a valid case, so a failure that a fuzzer
//! finds is a choice sequence that the shrinker can minimise and the store
//! can keep.

use super::case::{Provider, Request};
use super::choice::{Bounds, Choice, FloatBounds, Width};

/// The lengths that a sequence without a maximum may take beyond its
/// minimum, read from two bytes.
const UNBOUNDED_LENGTHS: u128 = 0xFFFF;

/// Returns the bytes that cover an integer range of `span + 1` values.
fn width(span: u128) -> usize {
    let bits = 128 - span.leading_zeros();
    usize::try_from(bits.div_ceil(8)).unwrap_or_default()
}

/// Returns `true` when float bounds admit exactly one value. Both zeros lie
/// inside any range that contains either, so only a nonzero range of one
/// value, without NaN, has one.
#[expect(
    clippy::float_cmp,
    reason = "bounds of one value have equal ends, which only an exact comparison states"
)]
fn single(bounds: &FloatBounds) -> bool {
    bounds.lo() == bounds.hi() && bounds.lo() != 0.0 && !bounds.allow_nan()
}

/// A provider that decodes every value from a fuzzer's bytes.
#[derive(Debug)]
pub(crate) struct Bridging {
    data: Vec<u8>,
    at: usize,
}

impl Bridging {
    pub(crate) fn new(data: &[u8]) -> Self {
        Self {
            data: data.to_vec(),
            at: 0,
        }
    }

    /// Returns the next `count` bytes as a little-endian unsigned integer, or
    /// `None` when fewer remain, which spends the rest.
    fn take(&mut self, count: usize) -> Option<u128> {
        if count > self.data.len() - self.at {
            self.at = self.data.len();
            return None;
        }
        let taken = &self.data[self.at..self.at + count];
        self.at += count;
        Some(
            taken
                .iter()
                .rev()
                .fold(0, |value, byte| (value << 8) | u128::from(*byte)),
        )
    }

    /// Returns an integer in `[0, span]` read from `width(span)` bytes. A span
    /// is below 2^64, so `span + 1` does not overflow.
    fn below(&mut self, span: u128) -> Option<u128> {
        Some(self.take(width(span))? % (span + 1))
    }
}

impl Provider for Bridging {
    fn value(&mut self, request: &Request, _index: usize) -> Choice {
        match &request.bounds {
            Bounds::Integer(bounds) => {
                let span = bounds.hi().abs_diff(bounds.lo());
                Choice::Integer(match self.below(span) {
                    Some(offset) => bounds.lo() + i128::try_from(offset).unwrap_or_default(),
                    None => bounds.target(),
                })
            }
            Bounds::Float(bounds) => {
                if single(bounds) {
                    return Choice::Float(bounds.target());
                }
                let count = if bounds.width() == Width::W64 { 8 } else { 4 };
                let Some(bits) = self.take(count) else {
                    return Choice::Float(bounds.target());
                };
                let value = match bounds.width() {
                    Width::W64 => f64::from_bits(u64::try_from(bits).unwrap_or_default()),
                    Width::W32 => {
                        f64::from(f32::from_bits(u32::try_from(bits).unwrap_or_default()))
                    }
                };
                Choice::Float(bounds.coerce(&Choice::Float(value)))
            }
            Bounds::Sequence(bounds) => {
                let spread = bounds.max_size().map_or(UNBOUNDED_LENGTHS, |max| {
                    u128::try_from(max - bounds.min_size()).unwrap_or(u128::MAX)
                });
                let Some(extra) = self.below(spread) else {
                    return Choice::Sequence(bounds.target());
                };
                let length = bounds.min_size() + usize::try_from(extra).unwrap_or_default();
                let mut elements = Vec::with_capacity(length.min(1 << 16));
                for _ in 0..length {
                    let Some(element) = self.below(u128::from(bounds.k() - 1)) else {
                        break;
                    };
                    elements.push(u64::try_from(element).unwrap_or_default());
                }
                if elements.len() < bounds.min_size() {
                    elements.resize(bounds.min_size(), 0);
                }
                Choice::Sequence(elements)
            }
        }
    }
}
