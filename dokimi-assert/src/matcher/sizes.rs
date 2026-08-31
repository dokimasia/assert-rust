//! How much a container holds.

use super::report::{Mode, fail};
use crate::seat::Seat;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::hash::BuildHasher;

/// Something with a number of items.
///
/// A trait rather than a runtime type switch. The other implementations of this standard
/// ask at run time what they were handed and report a value with no length as the
/// failure; here a value with no length does not compile, so that failure cannot happen.
pub trait Container {
    /// How many items this holds.
    ///
    /// Text counts characters rather than bytes, so a value that reads as one character
    /// counts as one however it is encoded.
    fn size(&self) -> usize;
}

impl Container for str {
    fn size(&self) -> usize {
        self.chars().count()
    }
}

impl Container for String {
    fn size(&self) -> usize {
        self.as_str().size()
    }
}

impl<T> Container for [T] {
    fn size(&self) -> usize {
        self.len()
    }
}

impl<T> Container for Vec<T> {
    fn size(&self) -> usize {
        self.len()
    }
}

impl<T> Container for VecDeque<T> {
    fn size(&self) -> usize {
        self.len()
    }
}

impl<K, V, S: BuildHasher> Container for HashMap<K, V, S> {
    fn size(&self) -> usize {
        self.len()
    }
}

impl<T, S: BuildHasher> Container for HashSet<T, S> {
    fn size(&self) -> usize {
        self.len()
    }
}

impl<K, V> Container for BTreeMap<K, V> {
    fn size(&self) -> usize {
        self.len()
    }
}

impl<T> Container for BTreeSet<T> {
    fn size(&self) -> usize {
        self.len()
    }
}

impl<T: Container + ?Sized> Container for &T {
    fn size(&self) -> usize {
        (**self).size()
    }
}

/// Report when got does not hold want items.
#[track_caller]
pub fn length<C: Container + ?Sized>(seat: &dyn Seat, mode: Mode, got: &C, want: usize, msg: &str) {
    seat.helper();
    let size = got.size();
    if size != want {
        fail(
            seat,
            mode,
            "length",
            msg,
            vec![("want", want.into()), ("got", size.into())],
        );
    }
}

/// Report when got holds anything.
///
/// Empty is not absent. An absent container is an `Option` and does not reach here.
#[track_caller]
pub fn is_empty<C: Container + ?Sized>(seat: &dyn Seat, mode: Mode, got: &C, msg: &str) {
    seat.helper();
    let size = got.size();
    if size != 0 {
        fail(seat, mode, "empty", msg, vec![("length", size.into())]);
    }
}

/// Report when got holds nothing.
#[track_caller]
pub fn is_not_empty<C: Container + ?Sized>(seat: &dyn Seat, mode: Mode, got: &C, msg: &str) {
    seat.helper();
    if got.size() == 0 {
        fail(seat, mode, "not-empty", msg, vec![]);
    }
}
