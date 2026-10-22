//! How a collection decides its length, one element at a time.
//!
//! Every collection, a list, a dict or a repetition in a pattern, decodes the
//! same way: before each element a continue flag, an integer choice that
//! decides structure, then the element's own choices. The flag's bounds force
//! it while the count is below the minimum or at the maximum, so every replay
//! of recorded choices yields a length inside the sizes.
//!
//! Each element is a span labelled `element`, or `entry` in a dict, that
//! starts at its flag. Deleting that span deletes the element whole.

use super::case::{Request, Stop};
use super::choice::Bounds;
use super::draw::{Draw, average_length, flag_bounds};
use crate::prop::Case;
use std::ops::{Bound, RangeBounds};

/// The span label of one element of a collection.
pub(crate) const ELEMENT: &str = "element";
/// The span label of one entry of a dict.
pub(crate) const ENTRY: &str = "entry";

/// A collection that discards this many duplicates in a row stops. A
/// collection still below its minimum size then rejects the case.
const MAX_DISCARDS: usize = 10;

/// Reports whether an element repeats an earlier one.
pub(crate) type Same<'a, T> = &'a dyn Fn(&T, &T) -> bool;

/// The bounds on a collection's length, both inclusive.
///
/// A `max_size` of `None` leaves the length unbounded, and the cap on the
/// choices of a case bounds it instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sizes {
    min_size: usize,
    max_size: Option<usize>,
}

impl Sizes {
    /// The sizes that admit every length.
    pub(crate) const ANY: Self = Self {
        min_size: 0,
        max_size: None,
    };

    /// Returns the sizes `[0, max_size]`, which admit a length at any maximum.
    pub(crate) const fn up_to(max_size: usize) -> Self {
        Self {
            min_size: 0,
            max_size: Some(max_size),
        }
    }

    /// Returns the sizes `[min_size, max_size]`, or `None` when they admit no
    /// length.
    #[must_use]
    pub fn new(min_size: usize, max_size: Option<usize>) -> Option<Self> {
        if max_size.is_some_and(|max| max < min_size) {
            return None;
        }
        Some(Self { min_size, max_size })
    }

    /// Returns the sizes that a range states, or `None` for an empty range.
    pub(crate) fn of(range: &impl RangeBounds<usize>) -> Option<Self> {
        let min_size = match range.start_bound() {
            Bound::Included(start) => *start,
            Bound::Excluded(start) => start.checked_add(1)?,
            Bound::Unbounded => 0,
        };
        let max_size = match range.end_bound() {
            Bound::Included(end) => Some(*end),
            Bound::Excluded(end) => Some(end.checked_sub(1)?),
            Bound::Unbounded => None,
        };
        Self::new(min_size, max_size)
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

    /// Returns the average length that the random phase aims for.
    pub(crate) fn average(&self) -> usize {
        average_length(self.min_size, self.max_size)
    }
}

/// Returns the case's decision whether a collection of `count` elements
/// grows.
///
/// The decision is an integer choice that decides structure, with the bounds
/// that [`flag_bounds`] states. A stopped collection decides as one at its
/// maximum would: the bounds admit only 0. The edge phase gives a collection
/// one element: its edge is 1 at count 0 and 0 after. `average`, when given,
/// replaces the average length of the sizes, as a machine's mean number of
/// steps does.
pub(crate) fn more(
    case: &Case<'_>,
    count: usize,
    sizes: Sizes,
    stopped: bool,
    average: Option<usize>,
) -> Result<bool, Stop> {
    let min = sizes.min_size;
    let max = if stopped { Some(count) } else { sizes.max_size };
    let request = Request {
        bounds: Bounds::Integer(flag_bounds(count, min, max)),
        draw: Draw::Flag {
            count,
            min,
            max,
            average: average.unwrap_or_else(|| sizes.average()),
        },
        edge: Some(i128::from(count == 0)),
        reuse: false,
    };
    Ok(case.choose(request)?.as_integer() == Some(1))
}

/// Decodes a collection: per element a continue flag, then the element.
///
/// `decode` returns an element, and `same` reports whether an element repeats
/// an earlier one, for a collection whose elements are unique. An element
/// that repeats is discarded, and the next flag is decided for the same
/// count. After `MAX_DISCARDS` discards in a row, the collection stops.
///
/// `stop`, when given, is asked before each flag that the minimum size does
/// not force. When it is true, the flag admits only 0, so the collection
/// takes no further element.
///
/// # Errors
///
/// Returns [`Stop::Rejected`] when the collection stops below its minimum
/// size, and the stop of an element that ends the case.
pub(crate) fn collect<T>(
    case: &Case<'_>,
    sizes: Sizes,
    label: &'static str,
    mut decode: impl FnMut() -> Result<T, Stop>,
    same: Option<Same<'_, T>>,
    stop: Option<&dyn Fn(&Case<'_>) -> bool>,
) -> Result<Vec<T>, Stop> {
    let mut items: Vec<T> = Vec::new();
    let mut discards = 0;
    loop {
        let start = case.len();
        let stopped = items.len() >= sizes.min_size && stop.is_some_and(|stop| stop(case));
        if !more(case, items.len(), sizes, stopped, None)? {
            return Ok(items);
        }
        let item = {
            let _span = case.span(label, Some(start));
            decode()?
        };
        if let Some(same) = same {
            if items.iter().any(|earlier| same(earlier, &item)) {
                discards += 1;
                if discards < MAX_DISCARDS {
                    continue;
                }
                if items.len() < sizes.min_size {
                    return Err(Stop::Rejected);
                }
                return Ok(items);
            }
        }
        discards = 0;
        items.push(item);
    }
}
