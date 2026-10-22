// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The default alphabet of the string generator, ordered from simplest.
//!
//! A string is a sequence of indices into an alphabet, so the order of the
//! alphabet decides what a string shrinks towards. The default alphabet puts
//! the 95 printable ASCII characters first: the digits, the lowercase and the
//! uppercase letters, then space and the punctuation. A string then shrinks
//! towards `"0"`, then `"00"`. The other code points of the Basic
//! Multilingual Plane follow in ascending order without the surrogates, and
//! the code points above the plane come last.

/// The first 95 characters, in order.
const PRINTABLE: &[u8; 95] =
    b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ !\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~";

/// The code point ranges after the printable characters, inclusive and
/// ascending: the controls below space, everything from DEL to the
/// surrogates, everything after the surrogates in the plane, and the
/// supplementary planes.
const REST: [(u32, u32); 4] = [
    (0x00, 0x1F),
    (0x7F, 0xD7FF),
    (0xE000, 0xFFFF),
    (0x1_0000, 0x10_FFFF),
];

/// The number of characters in the default alphabet: every Unicode scalar
/// value.
pub(crate) const SIZE: u32 = 0x11_0000 - 0x800;

/// Returns the index of the printable character `byte`.
fn printable_index(byte: u8) -> Option<u32> {
    PRINTABLE
        .iter()
        .position(|held| *held == byte)
        .and_then(|at| u32::try_from(at).ok())
}

/// Returns the character at `index` of the default alphabet, or `None` for an
/// index outside `[0, SIZE)`.
pub(crate) fn character(index: u32) -> Option<char> {
    if let Some(byte) = PRINTABLE.get(usize::try_from(index).ok()?) {
        return Some(char::from(*byte));
    }
    let mut offset = index - 95;
    REST.iter().find_map(|(low, high)| {
        if offset <= high - low {
            return char::from_u32(low + offset);
        }
        offset -= high - low + 1;
        None
    })
}

/// Returns the index of `c` in the default alphabet.
pub(crate) fn index(c: char) -> u32 {
    let point = u32::from(c);
    if (0x20..=0x7E).contains(&point) {
        // A point in this range is a printable byte, which PRINTABLE lists.
        return printable_index(u8::try_from(point).unwrap_or_default()).unwrap_or_default();
    }
    // A char is a scalar value, and every scalar value is printable or in REST.
    let mut offset = 95;
    REST.iter()
        .find_map(|(low, high)| {
            if (*low..=*high).contains(&point) {
                return Some(offset + point - low);
            }
            offset += high - low + 1;
            None
        })
        .unwrap_or_default()
}

/// Returns the indices of the code points in `[low, high]`, as sorted
/// inclusive intervals that neither overlap nor touch. The surrogates have no
/// index, and an empty range has none.
pub(crate) fn indices(low: u32, high: u32) -> Vec<(u32, u32)> {
    let mut found = Vec::new();
    for point in low.max(0x20)..=high.min(0x7E) {
        if let Some(at) = u8::try_from(point).ok().and_then(printable_index) {
            found.push((at, at));
        }
    }
    let mut offset = 95;
    for (first, last) in REST {
        let (start, end) = (low.max(first), high.min(last));
        if start <= end {
            found.push((offset + start - first, offset + end - first));
        }
        offset += last - first + 1;
    }
    merge(found)
}

/// Returns the union of inclusive intervals, sorted, with touching intervals
/// joined.
pub(crate) fn merge(mut intervals: Vec<(u32, u32)>) -> Vec<(u32, u32)> {
    intervals.sort_unstable();
    let mut merged: Vec<(u32, u32)> = Vec::with_capacity(intervals.len());
    for (start, end) in intervals {
        match merged.last_mut() {
            Some(last) if start <= last.1.saturating_add(1) => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    merged
}
