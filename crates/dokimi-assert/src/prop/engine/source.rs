// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The random source behind every generated case.
//!
//! The source is the 64-bit three-rotate variant of Bob Jenkins's small
//! noncryptographic generator. It uses addition, subtraction, exclusive or
//! and rotation modulo 2^64 alone, so every language of the definition
//! computes the same stream from the same seed.
//!
//! Case `i` of a run with seed `s` reads the stream of `s + i`. The draws
//! consume the stream in an order that the definition fixes, so two
//! implementations that consume it differently produce different cases from
//! the same seed.

/// The constant that the initialisation puts in the first word.
const FIRST_WORD: u64 = 0xF1EA_5EED;

/// The rounds that the initialisation discards, so that nearby seeds give
/// unrelated streams.
const WARMUP_ROUNDS: usize = 20;

/// The stream of 64-bit values that one seed produces.
///
/// A source is a value: a copy continues the stream from the same point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Source {
    a: u64,
    b: u64,
    c: u64,
    d: u64,
}

impl Source {
    /// Returns the stream of `seed`.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        let mut source = Self {
            a: FIRST_WORD,
            b: seed,
            c: seed,
            d: seed,
        };
        for _ in 0..WARMUP_ROUNDS {
            source.next_u64();
        }
        source
    }

    /// Returns the next value of the stream.
    pub fn next_u64(&mut self) -> u64 {
        let e = self.a.wrapping_sub(self.b.rotate_left(7));
        self.a = self.b ^ self.c.rotate_left(13);
        self.b = self.c.wrapping_add(self.d.rotate_left(37));
        self.c = self.d.wrapping_add(e);
        self.d = e.wrapping_add(self.a);
        self.d
    }

    /// Returns a value in `[0, n)`, uniform, for `1 <= n <= 2^64`.
    ///
    /// For `n` of 1 the result is 0 and nothing is consumed. Otherwise each
    /// attempt consumes one value and keeps its top `bit_length(n - 1)` bits,
    /// and the draw repeats while the result is `n` or more.
    ///
    /// # Panics
    ///
    /// Panics for `n` outside `[1, 2^64]`, which no draw of the engine asks
    /// for.
    pub fn below(&mut self, n: u128) -> u64 {
        assert!(
            (1..=1 << 64).contains(&n),
            "prop: below({n}) needs 1 <= n <= 2^64"
        );
        if n == 1 {
            return 0;
        }
        let width = 128 - (n - 1).leading_zeros();
        loop {
            let value = self.next_u64() >> (64 - width);
            if u128::from(value) < n {
                return value;
            }
        }
    }

    /// Returns `true` with probability `num / den`, from one `below(den)`
    /// draw.
    ///
    /// # Panics
    ///
    /// Panics for a `den` below 1 or above 2^64, or a `num` above `den`.
    pub fn coin(&mut self, num: u128, den: u128) -> bool {
        assert!(
            den >= 1 && num <= den,
            "prop: coin({num}, {den}) needs 0 <= num <= den"
        );
        u128::from(self.below(den)) < num
    }
}

/// Returns the source of case `index` of a run with `seed`: the stream of
/// `seed + index`, modulo 2^64.
#[must_use]
pub fn case_source(seed: u64, index: u64) -> Source {
    Source::new(seed.wrapping_add(index))
}

/// Folds bytes into one 64-bit value with the random source alone.
///
/// The value starts at zero. For each byte, it becomes the first output of
/// the stream seeded with the value xor the byte. This derives the seed of
/// the `ci` profile from a contract's UTF-8 bytes, and the name of a store
/// entry, without a hash function that a language may lack.
#[must_use]
pub fn mix(data: &[u8]) -> u64 {
    data.iter().fold(0, |value, byte| {
        Source::new(value ^ u64::from(*byte)).next_u64()
    })
}
