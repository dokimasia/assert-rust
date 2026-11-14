// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! Running a generator backwards: the choices that decode to a given value.
//!
//! The inverse walks a generator with a value and emits each choice in the
//! order that the generator's decode asks for it, with the bounds that it
//! asks with. It checks each choice against its bounds as it emits it. It
//! then replays the choices, and returns them only when they decode to the
//! value and the case records them unchanged. A value that the generator
//! cannot produce has no inverse: a value of another type, one outside its
//! bounds, a collection of another size, or a set or a map with a repeated
//! element.
//!
//! `map` and `bind` apply a function that the engine cannot run backwards, so
//! a generator built with one of them has no inverse. Its refusal tells it
//! from a value outside a domain.
//!
//! The inverse emits a placeholder for a composite. A placeholder does not
//! make a choice. In a trace the draws of the composite take their own
//! entries, so `invert_through` returns the choices of the generator around
//! its composites without a replay. `invert` refuses a value through a
//! composite as a value without an inverse, because no trace serves the draws
//! of the composite there. The other generators run backwards around a
//! placeholder, with these exceptions:
//!
//! - filter: the predicate does not test the value of a composite, because
//!   the record of a composite may not state the parts that the predicate
//!   reads.
//! - unique list: the elements through a composite are not compared, for the
//!   same reason.
//! - map: an entry through a composite has no inverse. The draws of the
//!   composites follow the order in which the case drew the entries, and the
//!   value of a map does not state that order.
//!
//! Where more than one sequence of choices decodes to a value, the inverse
//! takes the first in a fixed order:
//!
//! - one-of: the first alternative that produces the value without a
//!   composite, and else the first alternative through a composite.
//! - sampled-from: the first index of an equal value.
//! - permutation: the smallest index at each swap.
//! - string-matching: the match that a backtracking engine finds first.
//! - optional: absent before present.
//! - recursive: the base before the extension, by the rule of a one-of.
//! - a set's elements and a map's entries: in the shortlex order of their own
//!   choices.

use super::canonical::{canonical, same};
use super::case::{MAX_CHOICES, Record, Replaying};
use super::choice::{Bounds, Choice, IntegerBounds, Shortlex};
use super::collection::Sizes;
use super::draw::flag_bounds;
use crate::alphabet;
use crate::pattern;
use crate::prop::generator::COMPOSITE;
use crate::prop::{Case, Generator};
use crate::value::Value;
use std::fmt;

/// A value that a generator cannot produce from any sequence of choices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CannotInvert {
    no_inverse: bool,
    reason: String,
}

impl CannotInvert {
    /// Returns the refusal of a value outside the generator's domain.
    pub(crate) fn refused(reason: &str) -> Self {
        Self {
            no_inverse: false,
            reason: reason.to_owned(),
        }
    }

    /// Returns the refusal of a generator that applies a function the engine
    /// cannot run backwards.
    pub(crate) fn no_inverse(id: &str) -> Self {
        Self {
            no_inverse: true,
            reason: format!("the generator {id} has no inverse"),
        }
    }

    /// Returns the refusal of an entry of a map through a composite. The draws
    /// of the composites follow the order in which the case drew the entries,
    /// and the value of a map does not state that order.
    pub(crate) fn unordered() -> Self {
        Self {
            no_inverse: true,
            reason: "a map does not state the order in which its composites drew".to_owned(),
        }
    }

    /// Returns `true` when a generator on the way applies a function that the
    /// engine cannot run backwards, so whether the generator produces the
    /// value is unknown.
    #[must_use]
    pub fn is_no_inverse(&self) -> bool {
        self.no_inverse
    }

    /// Returns the clause that states why the generator cannot produce the
    /// value.
    pub(crate) fn reason(&self) -> &str {
        &self.reason
    }
}

impl fmt::Display for CannotInvert {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "prop: {}", self.reason)
    }
}

impl std::error::Error for CannotInvert {}

/// Each step that a generator emits is a choice with the bounds that its
/// request states, or the place of a composite. The place of a composite
/// does not make a choice.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Planned {
    /// A choice and the bounds that its request states.
    Choice { bounds: Bounds, value: Choice },
    /// The place of a composite. In a trace the draws of the composite take
    /// their own entries, and the steps after the place serve the generator
    /// around the composite.
    Placeholder,
}

impl Planned {
    /// Returns a step whose value its bounds admit by construction.
    pub(crate) fn new(bounds: Bounds, value: Choice) -> Self {
        Self::Choice { bounds, value }
    }

    /// Returns the bounds and the value of a choice, and `None` for the place
    /// of a composite.
    fn choice(&self) -> Option<(&Bounds, &Choice)> {
        match self {
            Self::Choice { bounds, value } => Some((bounds, value)),
            Self::Placeholder => None,
        }
    }
}

/// The steps that a generator emits for a value, and the value that they
/// decode to, which differs from the given value only in the order of a
/// set's elements and a map's entries.
pub(crate) type Emitted = (Vec<Planned>, Value);

/// The choices of a generator around its composites for a value, and
/// whether the value runs backwards through a composite.
pub(crate) type Inverted = (Vec<Choice>, bool);

/// Returns a step whose value its bounds admit.
pub(crate) fn planned(bounds: Bounds, value: Choice) -> Result<Planned, CannotInvert> {
    if !bounds.admits(&value) {
        return Err(CannotInvert::refused("the value is outside the bounds"));
    }
    Ok(Planned::Choice { bounds, value })
}

/// Returns `true` when steps run backwards through a composite.
pub(crate) fn has_placeholder(steps: &[Planned]) -> bool {
    steps.contains(&Planned::Placeholder)
}

/// Returns the choices of steps, without the places of their composites.
fn choices(steps: &[Planned]) -> Vec<Choice> {
    steps
        .iter()
        .filter_map(Planned::choice)
        .map(|(_, value)| value.clone())
        .collect()
}

/// Returns the shortlex key of the choices of steps, as the shrinker orders
/// choice sequences.
pub(crate) fn shortlex(steps: &[Planned]) -> Shortlex {
    Shortlex(
        steps
            .iter()
            .filter_map(Planned::choice)
            .map(|(bounds, value)| bounds.key(value))
            .collect(),
    )
}

/// Returns a collection's steps: per item a continue flag and its steps,
/// then a stop.
pub(crate) fn collection(
    sizes: Sizes,
    items: Vec<Vec<Planned>>,
) -> Result<Vec<Planned>, CannotInvert> {
    let count = items.len();
    if count < sizes.min_size() || sizes.max_size().is_some_and(|max| count > max) {
        return Err(CannotInvert::refused(
            "the items are outside the collection's sizes",
        ));
    }
    let flag = |at: usize| Bounds::Integer(flag_bounds(at, sizes.min_size(), sizes.max_size()));
    let mut steps = Vec::new();
    for (at, item) in items.into_iter().enumerate() {
        steps.push(Planned::new(flag(at), Choice::Integer(1)));
        steps.extend(item);
    }
    steps.push(Planned::new(flag(count), Choice::Integer(0)));
    Ok(steps)
}

/// Refuses values with two equal ones.
pub(crate) fn distinct(values: &[Value]) -> Result<(), CannotInvert> {
    let mut forms: Vec<_> = values.iter().map(canonical).collect();
    forms.sort();
    forms.dedup();
    if forms.len() != values.len() {
        return Err(CannotInvert::refused("the value repeats an element"));
    }
    Ok(())
}

/// Returns the index of the first branch that produces the value, then its
/// steps. A branch through a composite produces every value, so it counts
/// only when no branch produces the value without one.
///
/// When no branch produces it and one of them has no inverse, the refusal is
/// that there is no inverse.
pub(crate) fn first_branch(
    branches: impl Iterator<Item = (i128, Result<Emitted, CannotInvert>)>,
    bounds: IntegerBounds,
    refusal: &str,
) -> Result<Emitted, CannotInvert> {
    let mut unknown = false;
    let mut through = None;
    for (at, emitted) in branches {
        match emitted {
            Ok((steps, normal)) => {
                let composite = has_placeholder(&steps);
                let mut all = vec![Planned::new(Bounds::Integer(bounds), Choice::Integer(at))];
                all.extend(steps);
                if !composite {
                    return Ok((all, normal));
                }
                if through.is_none() {
                    through = Some((all, normal));
                }
            }
            Err(refused) => unknown |= refused.no_inverse,
        }
    }
    if let Some(emitted) = through {
        return Ok(emitted);
    }
    if unknown {
        return Err(CannotInvert {
            no_inverse: true,
            reason: format!("{refusal}, and one of them has no inverse"),
        });
    }
    Err(CannotInvert::refused(refusal))
}

/// Returns the choices that decode to `value`.
///
/// # Errors
///
/// Returns the refusal when the generator cannot produce `value`. The
/// refusal states no inverse when a generator on the way applies a function
/// that the engine cannot run backwards, and when `value` runs backwards
/// through a composite, whose draws only the entries of a trace serve.
pub(crate) fn invert<T: 'static>(
    generator: &Generator<T>,
    value: &Value,
) -> Result<Vec<Choice>, CannotInvert> {
    let (steps, normal) = generator.emit(value)?;
    if has_placeholder(&steps) {
        return Err(CannotInvert::no_inverse(COMPOSITE));
    }
    verified(generator, choices(&steps), &normal)
}

/// Returns the choices of the generator around its composites for `value`,
/// and whether `value` runs backwards through a composite. The draws of a
/// composite take their own entries of a trace, so the choices of a value
/// through a composite do not replay here. A value without a composite has
/// the choices that `invert` returns.
///
/// # Errors
///
/// Returns the refusals of `invert`, except the refusal of a value through a
/// composite.
pub(crate) fn invert_through<T: 'static>(
    generator: &Generator<T>,
    value: &Value,
) -> Result<Inverted, CannotInvert> {
    let (steps, normal) = generator.emit(value)?;
    let choices = choices(&steps);
    if has_placeholder(&steps) {
        return Ok((choices, true));
    }
    Ok((verified(generator, choices, &normal)?, false))
}

/// Returns `choices` once they decode to `normal` and the case records them
/// unchanged.
fn verified<T: 'static>(
    generator: &Generator<T>,
    choices: Vec<Choice>,
    normal: &Value,
) -> Result<Vec<Choice>, CannotInvert> {
    let case = Case::new(
        Record::new(Box::new(Replaying::new(choices.clone())), MAX_CHOICES),
        None,
    );
    let decoded = generator
        .decode(&case)
        .map_err(|_| CannotInvert::refused("the value decodes to no value"))?;
    let recorded = case.with_record(|record| record.choices.clone());
    let unchanged = recorded.len() == choices.len()
        && recorded
            .iter()
            .zip(&choices)
            .all(|(recorded, stated)| recorded.same(stated));
    let produced = generator
        .literal(&decoded)
        .is_some_and(|decoded| same(&decoded, normal));
    if !unchanged || !produced {
        return Err(CannotInvert::refused(
            "the value is not a value that the generator produces",
        ));
    }
    Ok(choices)
}

/// Returns the steps of the first match of the whole of `text` by a pattern's
/// node: the match that a backtracking engine finds first, which tries
/// alternatives in order and repeats each quantifier as often as the rest of
/// the pattern allows.
pub(crate) fn matched(node: &pattern::Node, text: &str) -> Option<Vec<Planned>> {
    let chars: Vec<char> = text.chars().collect();
    let mut found = None;
    let mut steps = Vec::new();
    matches(node, &chars, 0, &mut steps, &mut |end, steps| {
        if end != chars.len() {
            return false;
        }
        found = Some(steps.clone());
        true
    });
    found
}

/// What runs after a node matched, with where the match ended and the steps
/// so far. It returns `true` to stop the search.
type Then<'k> = &'k mut dyn FnMut(usize, &mut Vec<Planned>) -> bool;

/// Tries each way that `node` matches `text` from `at`, in a backtracking
/// engine's order, and calls `then` after each. Returns `true` when `then`
/// stopped the search.
fn matches(
    node: &pattern::Node,
    text: &[char],
    at: usize,
    steps: &mut Vec<Planned>,
    then: Then<'_>,
) -> bool {
    match node {
        pattern::Node::Literal(c) => text.get(at) == Some(c) && then(at + 1, steps),
        pattern::Node::Class(class) => {
            let Some(offset) = text.get(at).and_then(|c| class_offset(class, *c)) else {
                return false;
            };
            steps.push(Planned::new(
                Bounds::Integer(IntegerBounds::of(0, i128::from(class.size) - 1)),
                Choice::Integer(i128::from(offset)),
            ));
            let stopped = then(at + 1, steps);
            steps.pop();
            stopped
        }
        pattern::Node::Sequence(items) => sequence(items, text, at, steps, then),
        pattern::Node::Alternation(branches) => {
            let bounds = IntegerBounds::of(0, i128::try_from(branches.len()).unwrap_or(1) - 1);
            for (index, branch) in branches.iter().enumerate() {
                steps.push(Planned::new(
                    Bounds::Integer(bounds),
                    Choice::Integer(i128::try_from(index).unwrap_or_default()),
                ));
                if matches(branch, text, at, steps, &mut *then) {
                    return true;
                }
                steps.pop();
            }
            false
        }
        pattern::Node::Repeat(item, sizes) => repeat(item, *sizes, text, at, 0, steps, then),
    }
}

/// Tries each way that `items` match one after another from `at`.
fn sequence(
    items: &[pattern::Node],
    text: &[char],
    at: usize,
    steps: &mut Vec<Planned>,
    then: Then<'_>,
) -> bool {
    let Some((first, rest)) = items.split_first() else {
        return then(at, steps);
    };
    matches(first, text, at, steps, &mut |middle, steps| {
        sequence(rest, text, middle, steps, &mut *then)
    })
}

/// Tries each way that the repetitions of `item` from `count` on match, the
/// most first. A repetition that matches nothing is not repeated beyond the
/// minimum, so the search ends.
fn repeat(
    item: &pattern::Node,
    sizes: pattern::Sizes,
    text: &[char],
    at: usize,
    count: usize,
    steps: &mut Vec<Planned>,
    then: Then<'_>,
) -> bool {
    let flag = flag_bounds(count, sizes.min, sizes.max);
    if flag.hi() == 1 {
        steps.push(Planned::new(Bounds::Integer(flag), Choice::Integer(1)));
        let stopped = matches(item, text, at, steps, &mut |middle, steps| {
            if middle == at && count >= sizes.min {
                return false;
            }
            repeat(item, sizes, text, middle, count + 1, steps, &mut *then)
        });
        if stopped {
            return true;
        }
        steps.pop();
    }
    if flag.lo() == 0 {
        steps.push(Planned::new(Bounds::Integer(flag), Choice::Integer(0)));
        let stopped = then(at, steps);
        steps.pop();
        return stopped;
    }
    false
}

/// Returns a character's offset among a class's members.
fn class_offset(class: &pattern::Class, c: char) -> Option<u32> {
    let position = alphabet::index(c);
    let mut offset = 0;
    for (start, end) in &class.intervals {
        if (*start..=*end).contains(&position) {
            return Some(offset + position - start);
        }
        offset += end - start + 1;
    }
    None
}
