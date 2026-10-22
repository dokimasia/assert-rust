// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! What a container contains.

use crate::seat::Seat;
use crate::value::Value;
use crate::verdict::{Mode, Run};
use std::borrow::Borrow;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, LinkedList, VecDeque};
use std::fmt::Debug;
use std::hash::{BuildHasher, Hash};

/// A container that can be asked whether it contains a needle.
///
/// What containing means follows the container, as the definition states:
/// text contains a substring, a sequence an element, and a map or a set a
/// key. An element or a key compares as `==` compares it, so an integer does
/// not match a float.
pub trait Contains<N: ?Sized> {
    /// Returns `true` when the container contains `needle`.
    fn contains(&self, needle: &N) -> bool;
}

impl Contains<str> for str {
    fn contains(&self, needle: &str) -> bool {
        str::contains(self, needle)
    }
}

impl Contains<str> for String {
    fn contains(&self, needle: &str) -> bool {
        str::contains(self, needle)
    }
}

impl Contains<String> for String {
    fn contains(&self, needle: &String) -> bool {
        str::contains(self, needle.as_str())
    }
}

impl Contains<String> for str {
    fn contains(&self, needle: &String) -> bool {
        str::contains(self, needle.as_str())
    }
}

impl<T: PartialEq> Contains<T> for [T] {
    fn contains(&self, needle: &T) -> bool {
        self.iter().any(|held| held == needle)
    }
}

impl<T: PartialEq, const N: usize> Contains<T> for [T; N] {
    fn contains(&self, needle: &T) -> bool {
        self.iter().any(|held| held == needle)
    }
}

impl<T: PartialEq> Contains<T> for Vec<T> {
    fn contains(&self, needle: &T) -> bool {
        self.iter().any(|held| held == needle)
    }
}

impl<T: PartialEq> Contains<T> for VecDeque<T> {
    fn contains(&self, needle: &T) -> bool {
        self.iter().any(|held| held == needle)
    }
}

impl<T: PartialEq> Contains<T> for LinkedList<T> {
    fn contains(&self, needle: &T) -> bool {
        self.iter().any(|held| held == needle)
    }
}

impl<K, V, Q, S> Contains<Q> for HashMap<K, V, S>
where
    K: Eq + Hash + Borrow<Q>,
    Q: Eq + Hash + ?Sized,
    S: BuildHasher,
{
    fn contains(&self, needle: &Q) -> bool {
        self.contains_key(needle)
    }
}

impl<K, V, Q> Contains<Q> for BTreeMap<K, V>
where
    K: Ord + Borrow<Q>,
    Q: Ord + ?Sized,
{
    fn contains(&self, needle: &Q) -> bool {
        self.contains_key(needle)
    }
}

impl<T, Q, S> Contains<Q> for HashSet<T, S>
where
    T: Eq + Hash + Borrow<Q>,
    Q: Eq + Hash + ?Sized,
    S: BuildHasher,
{
    fn contains(&self, needle: &Q) -> bool {
        HashSet::contains(self, needle)
    }
}

impl<T, Q> Contains<Q> for BTreeSet<T>
where
    T: Ord + Borrow<Q>,
    Q: Ord + ?Sized,
{
    fn contains(&self, needle: &Q) -> bool {
        BTreeSet::contains(self, needle)
    }
}

impl<N: ?Sized, C: Contains<N> + ?Sized> Contains<N> for &C {
    fn contains(&self, needle: &N) -> bool {
        (**self).contains(needle)
    }
}

/// A list contains an element, a map a key, text a substring and a byte
/// string a run of bytes. No other value contains anything.
impl Contains<Value> for Value {
    fn contains(&self, needle: &Value) -> bool {
        match (self, needle) {
            (Self::List(items), _) => items.iter().any(|held| held == needle),
            (Self::Map(entries), _) => entries.iter().any(|(key, _)| key == needle),
            (Self::String(text), Self::String(part)) => text.contains(part.as_str()),
            (Self::Bytes(bytes), Self::Bytes(part)) => {
                part.is_empty()
                    || bytes
                        .windows(part.len())
                        .any(|window| window == part.as_slice())
            }
            _ => false,
        }
    }
}

/// Reports whether `haystack` contains `needle`.
#[track_caller]
pub(crate) fn contains<H, N>(seat: &dyn Seat, mode: Mode, haystack: &H, needle: &N, msg: &str)
where
    H: Contains<N> + Debug + ?Sized,
    N: Debug + ?Sized,
{
    seat.helper();
    let run = Run::of(seat);
    if !haystack.contains(needle) {
        run.fail(
            mode,
            "contains",
            msg,
            vec![
                ("haystack", Value::debug(haystack)),
                ("needle", Value::debug(needle)),
            ],
        );
        return;
    }
    run.pass(mode, "contains", msg);
}

/// Reports whether `haystack` does not contain `needle`.
#[track_caller]
pub(crate) fn not_contains<H, N>(seat: &dyn Seat, mode: Mode, haystack: &H, needle: &N, msg: &str)
where
    H: Contains<N> + Debug + ?Sized,
    N: Debug + ?Sized,
{
    seat.helper();
    let run = Run::of(seat);
    if haystack.contains(needle) {
        run.fail(
            mode,
            "not-contains",
            msg,
            vec![
                ("haystack", Value::debug(haystack)),
                ("needle", Value::debug(needle)),
            ],
        );
        return;
    }
    run.pass(mode, "not-contains", msg);
}

/// Reports whether `got` contains every needle, each after the end of the
/// previous needle's match.
#[track_caller]
pub(crate) fn contains_in_order(
    seat: &dyn Seat,
    mode: Mode,
    got: &str,
    needles: &[&str],
    msg: &str,
) {
    seat.helper();
    let run = Run::of(seat);
    let mut from = 0;
    // The index is the needle's place in the list, so a needle listed twice
    // reports the place of the one that failed.
    for (index, needle) in needles.iter().enumerate() {
        let Some(at) = got[from..].find(needle) else {
            run.fail(
                mode,
                "contains-in-order",
                msg,
                vec![
                    ("haystack", Value::from(got)),
                    ("needle", Value::from(*needle)),
                    ("index", Value::from(index)),
                ],
            );
            return;
        };
        from += at + needle.len();
    }
    run.pass(mode, "contains-in-order", msg);
}

/// Reports whether every element of `got` has a partner of its own in
/// `want`: a matching of the bipartite graph whose edges join equal
/// elements, which augmenting paths find when one exists. A type that is
/// only `PartialEq` has no order and no hash, so no sort or count finds it.
#[track_caller]
pub(crate) fn is_permutation<T: PartialEq + Debug>(
    seat: &dyn Seat,
    mode: Mode,
    got: &[T],
    want: &[T],
    msg: &str,
) {
    seat.helper();
    let run = Run::of(seat);
    if !matched(got, want) {
        run.fail(
            mode,
            "permutation",
            msg,
            vec![("want", Value::debug(want)), ("got", Value::debug(got))],
        );
        return;
    }
    run.pass(mode, "permutation", msg);
}

fn matched<T: PartialEq>(got: &[T], want: &[T]) -> bool {
    if got.len() != want.len() {
        return false;
    }
    let mut partner: Vec<Option<usize>> = vec![None; want.len()];
    (0..got.len()).all(|i| augment(got, want, i, &mut partner, &mut vec![false; want.len()]))
}

/// Finds a partner in `want` for the element `i` of `got`, moving earlier
/// partners along an augmenting path where that frees one.
fn augment<T: PartialEq>(
    got: &[T],
    want: &[T],
    i: usize,
    partner: &mut [Option<usize>],
    seen: &mut [bool],
) -> bool {
    for j in 0..want.len() {
        if seen[j] || got[i] != want[j] {
            continue;
        }
        seen[j] = true;
        if partner[j].is_none_or(|held| augment(got, want, held, partner, seen)) {
            partner[j] = Some(i);
            return true;
        }
    }
    false
}
