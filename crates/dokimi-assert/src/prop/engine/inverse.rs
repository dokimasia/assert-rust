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
//! `map`, `bind` and `composite` apply a function that the engine cannot run
//! backwards, so a generator built with one of them has no inverse. Its
//! refusal tells it from a value outside a domain.
//!
//! Where more than one sequence of choices decodes to a value, the inverse
//! takes the first in a fixed order: the first alternative of a one-of that
//! produces it, the first index of an equal value, the smallest index at each
//! swap of a permutation, the match that a backtracking engine finds first,
//! absent before present, the base before the extension, and the elements of
//! a set and the entries of a map in the shortlex order of their own choices.

use super::canonical::{canonical, same};
use super::case::{MAX_CHOICES, Record, Replaying};
use super::choice::{Bounds, Choice, IntegerBounds, Shortlex};
use super::collection::Sizes;
use super::draw::flag_bounds;
use crate::alphabet;
use crate::pattern;
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

/// One emitted choice: the bounds that its request states, and its value.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Planned {
    pub(crate) bounds: Bounds,
    pub(crate) value: Choice,
}

impl Planned {
    /// Returns a step whose value its bounds admit by construction.
    pub(crate) fn new(bounds: Bounds, value: Choice) -> Self {
        Self { bounds, value }
    }
}

/// The steps that a generator emits for a value, and the value that they
/// decode to, which differs from the given value only in the order of a
/// set's elements and a map's entries.
pub(crate) type Emitted = (Vec<Planned>, Value);

/// Returns a step whose value its bounds admit.
pub(crate) fn planned(bounds: Bounds, value: Choice) -> Result<Planned, CannotInvert> {
    if !bounds.admits(&value) {
        return Err(CannotInvert::refused("the value is outside the bounds"));
    }
    Ok(Planned { bounds, value })
}

/// Returns the shortlex key of steps, as the shrinker orders choice
/// sequences.
pub(crate) fn shortlex(steps: &[Planned]) -> Shortlex {
    Shortlex(
        steps
            .iter()
            .map(|step| step.bounds.key(&step.value))
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
/// steps.
///
/// When no branch produces it and one of them has no inverse, the refusal is
/// that there is no inverse.
pub(crate) fn first_branch(
    branches: impl Iterator<Item = (i128, Result<Emitted, CannotInvert>)>,
    bounds: IntegerBounds,
    refusal: &str,
) -> Result<Emitted, CannotInvert> {
    let mut unknown = false;
    for (at, emitted) in branches {
        match emitted {
            Ok((steps, normal)) => {
                let mut all = vec![Planned::new(Bounds::Integer(bounds), Choice::Integer(at))];
                all.extend(steps);
                return Ok((all, normal));
            }
            Err(refused) => unknown |= refused.no_inverse,
        }
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
/// Returns the refusal when the generator cannot produce `value`, and a
/// refusal that states no inverse when a generator on the way applies a
/// function that the engine cannot run backwards.
pub(crate) fn invert<T: 'static>(
    generator: &Generator<T>,
    value: &Value,
) -> Result<Vec<Choice>, CannotInvert> {
    let (steps, normal) = generator.emit(value)?;
    let choices: Vec<Choice> = steps.into_iter().map(|step| step.value).collect();
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
        .is_some_and(|decoded| same(&decoded, &normal));
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
