// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! How many items a container has.

use crate::seat::Seat;
use crate::value::Value;
use crate::verdict::{Mode, Run};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, LinkedList, VecDeque};
use std::fmt::Debug;
use std::hash::BuildHasher;

/// A value with a number of items: the elements of a sequence, the entries of
/// a map, the bytes of a byte string or the Unicode scalar values of text.
///
/// [`check::length`](crate::check::length), [`check::is_empty`](crate::check::is_empty)
/// and [`check::is_not_empty`](crate::check::is_not_empty) take any
/// container. A value with no length, such as `None`, fails those
/// assertions, as the definition states for a null value.
pub trait Container: Debug {
    /// Returns the number of items, or `None` for a value that has no length.
    fn length(&self) -> Option<usize>;
}

impl Container for str {
    /// Counts the Unicode scalar values, so `"é"` has one item however it is
    /// encoded.
    fn length(&self) -> Option<usize> {
        Some(self.chars().count())
    }
}

impl Container for String {
    fn length(&self) -> Option<usize> {
        self.as_str().length()
    }
}

impl<T: Debug> Container for [T] {
    fn length(&self) -> Option<usize> {
        Some(self.len())
    }
}

impl<T: Debug, const N: usize> Container for [T; N] {
    fn length(&self) -> Option<usize> {
        Some(N)
    }
}

impl<T: Debug> Container for Vec<T> {
    fn length(&self) -> Option<usize> {
        Some(self.len())
    }
}

impl<T: Debug> Container for VecDeque<T> {
    fn length(&self) -> Option<usize> {
        Some(self.len())
    }
}

impl<T: Debug> Container for LinkedList<T> {
    fn length(&self) -> Option<usize> {
        Some(self.len())
    }
}

impl<K: Debug, V: Debug, S: BuildHasher> Container for HashMap<K, V, S> {
    fn length(&self) -> Option<usize> {
        Some(self.len())
    }
}

impl<K: Debug, V: Debug> Container for BTreeMap<K, V> {
    fn length(&self) -> Option<usize> {
        Some(self.len())
    }
}

impl<T: Debug, S: BuildHasher> Container for HashSet<T, S> {
    fn length(&self) -> Option<usize> {
        Some(self.len())
    }
}

impl<T: Debug> Container for BTreeSet<T> {
    fn length(&self) -> Option<usize> {
        Some(self.len())
    }
}

impl<C: Container + ?Sized> Container for &C {
    fn length(&self) -> Option<usize> {
        (**self).length()
    }
}

impl<C: Container + ?Sized> Container for Box<C> {
    fn length(&self) -> Option<usize> {
        (**self).length()
    }
}

/// `None` has no length.
impl<C: Container> Container for Option<C> {
    fn length(&self) -> Option<usize> {
        self.as_ref().and_then(Container::length)
    }
}

/// A list, a map, text and a byte string have a length. Every other value
/// has none.
impl Container for Value {
    fn length(&self) -> Option<usize> {
        match self {
            Self::List(items) => Some(items.len()),
            Self::Map(entries) => Some(entries.len()),
            Self::String(text) => text.length(),
            Self::Bytes(bytes) => Some(bytes.len()),
            _ => None,
        }
    }
}

/// Returns the value of a container for a record: null for a value without a
/// length, and its `Debug` text otherwise.
fn stated<C: Container + ?Sized>(got: &C) -> Value {
    match got.length() {
        None => Value::Null,
        Some(_) => Value::debug(got),
    }
}

fn count(length: Option<usize>) -> Value {
    length.map_or(Value::Null, Value::from)
}

/// Reports whether `got` has `want` items.
#[track_caller]
pub(crate) fn length<C: Container + ?Sized>(
    seat: &dyn Seat,
    mode: Mode,
    got: &C,
    want: usize,
    msg: &str,
) {
    seat.helper();
    let run = Run::of(seat);
    let found = got.length();
    if found != Some(want) {
        run.fail(
            mode,
            "length",
            msg,
            vec![("want", Value::from(want)), ("got", count(found))],
        );
        return;
    }
    run.pass(mode, "length", msg);
}

/// Reports whether `got` has no items.
#[track_caller]
pub(crate) fn is_empty<C: Container + ?Sized>(seat: &dyn Seat, mode: Mode, got: &C, msg: &str) {
    seat.helper();
    let run = Run::of(seat);
    let found = got.length();
    if found != Some(0) {
        run.fail(
            mode,
            "empty",
            msg,
            vec![("got", stated(got)), ("length", count(found))],
        );
        return;
    }
    run.pass(mode, "empty", msg);
}

/// Reports whether `got` has at least one item.
#[track_caller]
pub(crate) fn is_not_empty<C: Container + ?Sized>(seat: &dyn Seat, mode: Mode, got: &C, msg: &str) {
    seat.helper();
    let run = Run::of(seat);
    if got.length().is_none_or(|found| found == 0) {
        run.fail(mode, "not-empty", msg, vec![("got", stated(got))]);
        return;
    }
    run.pass(mode, "not-empty", msg);
}
