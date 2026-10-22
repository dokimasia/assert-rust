//! The replay token: the choice values of one case as one printable string.
//!
//! A token is `prop1:` followed by the unpadded base64url encoding of the
//! choice values in order. Each value is a tag byte and then its payload:
//!
//! - Tag 0: a non-negative integer, in unsigned LEB128.
//! - Tag 1: a negative integer, its magnitude in unsigned LEB128.
//! - Tag 2: a float, its binary64 bits in eight bytes, little-endian. Every
//!   NaN has the canonical bits.
//! - Tag 3: a sequence, its length and then each element, each in unsigned
//!   LEB128.
//!
//! The token records no bounds, because the generators state them again when
//! the case replays. A choice sequence has exactly one token, so the name of
//! a store entry, which derives from the token, is the same in every
//! language.

use super::choice::{Choice, NAN_BITS};
use super::draw::wide;
use std::fmt;

/// The text that every token of this version starts with.
pub const PREFIX: &str = "prop1:";

const TAG_NON_NEGATIVE: u8 = 0;
const TAG_NEGATIVE: u8 = 1;
const TAG_FLOAT: u8 = 2;
const TAG_SEQUENCE: u8 = 3;

/// The base64url alphabet.
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Why a text is no replay token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenError(String);

impl fmt::Display for TokenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for TokenError {}

/// Returns the token of a choice sequence.
#[must_use]
pub fn encode(choices: &[Choice]) -> String {
    let mut data = Vec::new();
    for choice in choices {
        match choice {
            Choice::Integer(value) => {
                data.push(if *value < 0 {
                    TAG_NEGATIVE
                } else {
                    TAG_NON_NEGATIVE
                });
                append_number(&mut data, value.unsigned_abs());
            }
            Choice::Float(value) => {
                let bits = if value.is_nan() {
                    NAN_BITS
                } else {
                    value.to_bits()
                };
                data.push(TAG_FLOAT);
                data.extend_from_slice(&bits.to_le_bytes());
            }
            Choice::Sequence(elements) => {
                data.push(TAG_SEQUENCE);
                append_number(&mut data, wide(elements.len()));
                for element in elements {
                    append_number(&mut data, u128::from(*element));
                }
            }
        }
    }
    let mut token = String::with_capacity(PREFIX.len() + data.len() * 4 / 3 + 3);
    token.push_str(PREFIX);
    base64(&data, &mut token);
    token
}

/// Returns the choice sequence that a token records.
///
/// A token is valid only in the form that [`encode`] returns. The decoder
/// reads the text and then encodes the choices that it read, so padding,
/// characters outside base64url, trailing bits, a negative zero and a
/// number with superfluous bytes each fail that comparison.
///
/// # Errors
///
/// Returns an error for a text that is not the token of any choice sequence:
/// one without the prefix, outside base64url, with an unknown tag, with a
/// payload cut short, with a number of 2^64 or more, or with a negative
/// integer below -2^63.
pub fn decode(token: &str) -> Result<Vec<Choice>, TokenError> {
    let refused = |reason: &str| TokenError(format!("prop: token {token:?} {reason}"));
    let text = token
        .strip_prefix(PREFIX)
        .ok_or_else(|| refused(&format!("does not start with {PREFIX}")))?;
    let data = unbase64(text).ok_or_else(|| refused("is not base64url"))?;
    let mut reader = Reader { data: &data, at: 0 };
    let mut choices = Vec::new();
    while reader.at < data.len() {
        choices.push(reader.choice().map_err(refused)?);
    }
    if encode(&choices) != token {
        return Err(refused("is not in canonical form"));
    }
    Ok(choices)
}

/// Appends a number in unsigned LEB128.
fn append_number(data: &mut Vec<u8>, mut number: u128) {
    while number > 0x7F {
        data.push(u8::try_from(number & 0x7F).unwrap_or_default() | 0x80);
        number >>= 7;
    }
    data.push(u8::try_from(number).unwrap_or_default());
}

/// Appends the unpadded base64url text of `data`.
fn base64(data: &[u8], out: &mut String) {
    for chunk in data.chunks(3) {
        let bytes = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let group = (u32::from(bytes[0]) << 16) | (u32::from(bytes[1]) << 8) | u32::from(bytes[2]);
        let characters = chunk.len() + 1;
        for at in 0..characters {
            let index = usize::try_from((group >> (18 - 6 * at)) & 0x3F).unwrap_or_default();
            out.push(char::from(ALPHABET[index]));
        }
    }
}

/// Returns the bytes of unpadded base64url text, ignoring the bits that a
/// last character holds beyond the data, or `None` for other text.
fn unbase64(text: &str) -> Option<Vec<u8>> {
    if text.len() % 4 == 1 {
        return None;
    }
    let values: Vec<u32> = text
        .bytes()
        .map(|byte| {
            ALPHABET
                .iter()
                .position(|held| *held == byte)
                .and_then(|at| u32::try_from(at).ok())
        })
        .collect::<Option<_>>()?;
    let mut data = Vec::with_capacity(values.len() * 3 / 4);
    for chunk in values.chunks(4) {
        let group = chunk
            .iter()
            .enumerate()
            .fold(0, |group, (at, value)| group | (value << (18 - 6 * at)));
        let bytes = group.to_be_bytes();
        data.extend_from_slice(&bytes[1..chunk.len()]);
    }
    Some(data)
}

/// The bytes of a token, read once from the start.
struct Reader<'d> {
    data: &'d [u8],
    at: usize,
}

impl Reader<'_> {
    fn byte(&mut self) -> Result<u8, &'static str> {
        let byte = *self.data.get(self.at).ok_or("ends inside a choice")?;
        self.at += 1;
        Ok(byte)
    }

    /// Returns the next unsigned LEB128 number.
    fn number(&mut self) -> Result<u128, &'static str> {
        let mut number: u128 = 0;
        let mut shift = 0;
        loop {
            let byte = self.byte()?;
            if shift >= 64 {
                return Err("states a number of 2^64 or more");
            }
            number |= u128::from(byte & 0x7F) << shift;
            if number >= 1 << 64 {
                return Err("states a number of 2^64 or more");
            }
            if byte < 0x80 {
                return Ok(number);
            }
            shift += 7;
        }
    }

    fn choice(&mut self) -> Result<Choice, &'static str> {
        match self.byte()? {
            TAG_NON_NEGATIVE => Ok(Choice::Integer(signed(self.number()?))),
            TAG_NEGATIVE => {
                let magnitude = self.number()?;
                if magnitude > 1 << 63 {
                    return Err("states a negative integer below -2^63");
                }
                Ok(Choice::Integer(-signed(magnitude)))
            }
            TAG_FLOAT => {
                let mut bytes = [0; 8];
                for byte in &mut bytes {
                    *byte = self.byte()?;
                }
                Ok(Choice::Float(f64::from_bits(u64::from_le_bytes(bytes))))
            }
            TAG_SEQUENCE => {
                let length = self.number()?;
                let mut elements = Vec::new();
                let mut left = length;
                while left > 0 {
                    elements.push(u64::try_from(self.number()?).unwrap_or_default());
                    left -= 1;
                }
                Ok(Choice::Sequence(elements))
            }
            _ => Err("states an unknown tag"),
        }
    }
}

/// Returns a number below 2^64 as an `i128`.
fn signed(number: u128) -> i128 {
    i128::try_from(number).unwrap_or_default()
}
