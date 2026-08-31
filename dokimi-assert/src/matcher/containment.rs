//! What a container holds.

use super::report::{Mode, report};
use crate::seat::Seat;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::fmt::Debug;
use std::hash::{BuildHasher, Hash};

/// A container that can be asked whether it holds a needle.
///
/// What holding means follows the container, which is the rule the standard states: text
/// holds a substring, a sequence holds an element, and a map holds a key. Each is a
/// separate implementation rather than a run-time test of what was passed.
pub trait Holds<N: ?Sized> {
    /// Whether this holds the needle.
    fn holds(&self, needle: &N) -> bool;
}

impl Holds<str> for str {
    fn holds(&self, needle: &str) -> bool {
        self.contains(needle)
    }
}

impl Holds<str> for String {
    fn holds(&self, needle: &str) -> bool {
        self.contains(needle)
    }
}

impl<T: PartialEq> Holds<T> for [T] {
    fn holds(&self, needle: &T) -> bool {
        self.iter().any(|held| held == needle)
    }
}

impl<T: PartialEq> Holds<T> for Vec<T> {
    fn holds(&self, needle: &T) -> bool {
        self.as_slice().holds(needle)
    }
}

impl<T: PartialEq> Holds<T> for VecDeque<T> {
    fn holds(&self, needle: &T) -> bool {
        self.iter().any(|held| held == needle)
    }
}

impl<K: Eq + Hash, V, S: BuildHasher> Holds<K> for HashMap<K, V, S> {
    fn holds(&self, needle: &K) -> bool {
        self.contains_key(needle)
    }
}

impl<T: Eq + Hash, S: BuildHasher> Holds<T> for HashSet<T, S> {
    fn holds(&self, needle: &T) -> bool {
        self.contains(needle)
    }
}

impl<K: Ord, V> Holds<K> for BTreeMap<K, V> {
    fn holds(&self, needle: &K) -> bool {
        self.contains_key(needle)
    }
}

impl<T: Ord> Holds<T> for BTreeSet<T> {
    fn holds(&self, needle: &T) -> bool {
        self.contains(needle)
    }
}

impl<N: ?Sized, T: Holds<N> + ?Sized> Holds<N> for &T {
    fn holds(&self, needle: &N) -> bool {
        (**self).holds(needle)
    }
}

/// Report when haystack does not hold needle.
#[track_caller]
pub fn contains<H, N>(seat: &dyn Seat, mode: Mode, haystack: &H, needle: &N, msg: &str)
where
    H: Holds<N> + Debug + ?Sized,
    N: Debug + ?Sized,
{
    seat.helper();
    if !haystack.holds(needle) {
        report(
            seat,
            mode,
            &format!("{msg}: {haystack:?} does not contain {needle:?}"),
        );
    }
}

/// Report when haystack holds needle.
#[track_caller]
pub fn not_contains<H, N>(seat: &dyn Seat, mode: Mode, haystack: &H, needle: &N, msg: &str)
where
    H: Holds<N> + Debug + ?Sized,
    N: Debug + ?Sized,
{
    seat.helper();
    if haystack.holds(needle) {
        report(
            seat,
            mode,
            &format!("{msg}: {haystack:?} contains {needle:?}"),
        );
    }
}

/// Report when got does not hold every needle, in order.
///
/// Each needle is looked for after the previous one's match ends, so the same text cannot
/// satisfy two needles. Anything may sit between them.
#[track_caller]
pub fn contains_in_order(seat: &dyn Seat, mode: Mode, got: &str, needles: &[&str], msg: &str) {
    seat.helper();
    let mut from = 0;
    for needle in needles {
        let Some(at) = got[from..].find(needle) else {
            report(
                seat,
                mode,
                &format!("{msg}: {got:?} does not contain {needle:?} after the earlier needles"),
            );
            return;
        };
        from += at + needle.len();
    }
}
