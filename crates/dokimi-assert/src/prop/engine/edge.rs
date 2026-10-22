// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The edge phase: four cases, each with every value at one boundary.
//!
//! A choice that decides structure takes the edge that its request states,
//! so a collection gets one element, a one-of its first alternative and an
//! optional its value. A value choice takes the boundary of its case:
//!
//! - `Min`: `lo`. A sequence's elements are 0.
//! - `Max`: `hi`. A sequence's elements are `k - 1`.
//! - `Above`: one above the target, the next float of the width for a
//!   float, and 1 for a sequence's elements.
//! - `Below`: one below the target, and the next float below it.
//!
//! A value that the bounds do not admit takes the target instead. A sequence
//! has one element, or `min_size` elements when that is more, and none when
//! its `max_size` is 0.

use super::case::{Provider, Request};
use super::choice::{Bounds, Choice, next_down, next_up};

/// The boundary that every value choice of one edge case takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Boundary {
    Min,
    Max,
    Above,
    Below,
}

/// The edge cases, in the order that a run tries them.
pub(crate) const BOUNDARIES: [Boundary; 4] = [
    Boundary::Min,
    Boundary::Max,
    Boundary::Above,
    Boundary::Below,
];

/// A provider that gives every choice its value at one boundary.
#[derive(Debug)]
pub(crate) struct Edge {
    boundary: Boundary,
}

impl Edge {
    pub(crate) fn new(boundary: Boundary) -> Self {
        Self { boundary }
    }
}

impl Provider for Edge {
    fn value(&mut self, request: &Request, _index: usize) -> Choice {
        let bounds = &request.bounds;
        if let Some(edge) = request.edge {
            let value = Choice::Integer(edge);
            return if bounds.admits(&value) {
                value
            } else {
                bounds.target()
            };
        }
        match bounds {
            Bounds::Integer(integer) => {
                let target = integer.target();
                let value = match self.boundary {
                    Boundary::Min => integer.lo(),
                    Boundary::Max => integer.hi(),
                    Boundary::Above => target + 1,
                    Boundary::Below => target - 1,
                };
                Choice::Integer(if integer.admits(value) { value } else { target })
            }
            Bounds::Float(float) => {
                let target = float.target();
                let value = match self.boundary {
                    Boundary::Min => float.lo(),
                    Boundary::Max => float.hi(),
                    Boundary::Above => next_up(target, float.width()),
                    Boundary::Below => next_down(target, float.width()),
                };
                Choice::Float(if float.admits(value) { value } else { target })
            }
            Bounds::Sequence(sequence) => {
                let mut length = sequence.min_size().max(1);
                if let Some(max) = sequence.max_size() {
                    length = length.min(max);
                }
                let element = match self.boundary {
                    Boundary::Min | Boundary::Below => 0,
                    Boundary::Max => sequence.k() - 1,
                    Boundary::Above => u64::from(sequence.k() > 1),
                };
                Choice::Sequence(vec![element; length])
            }
        }
    }
}
