// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The generators of the structural shapes that the generator vocabulary
//! does not state: the root and the refs of a recursive shape, an optional,
//! a list and a map that can exit, a record, an enum, and an IP address of
//! either version.
//!
//! A value of a recursive shape has a budget of 100 nodes. A node is the
//! value of a definition that can reach itself through refs. Once a value has
//! used its budget, every container that refers back takes its exit: an
//! optional is absent, a list, a set or a map takes no further element, and an
//! enum takes its first variant that does not refer back. The exit is still a
//! choice, with bounds that admit only the exit, so a replay walks the same
//! positions.

use crate::prop::case::Case;
use crate::prop::engine::canonical::same;
use crate::prop::engine::case::Stop;
use crate::prop::engine::choice::{Bounds, Choice, IntegerBounds};
use crate::prop::engine::collection::{ELEMENT, ENTRY, Sizes, collect};
use crate::prop::engine::inverse::{
    CannotInvert, Emitted, Planned, collection, distinct, shortlex,
};
use crate::prop::generator::{Generator, Node};
use crate::prop::generators::{emit_entries, emit_optional, index, position};
use crate::value::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock, Weak};

/// The nodes that one value of a recursive shape may use.
const BUDGET: usize = 100;

/// The number that tells the budgets of two shape files apart.
static NEXT_BUDGET: AtomicUsize = AtomicUsize::new(0);

/// The budget of the values of one shape file.
///
/// The record of each case keeps the nodes that each value in progress has
/// used, under the budget's number, so the cases that run at once on other
/// workers count their own values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Budget {
    id: usize,
}

impl Budget {
    /// Returns the budget of a new shape file.
    pub(crate) fn new() -> Self {
        Self {
            id: NEXT_BUDGET.fetch_add(1, Ordering::Relaxed),
        }
    }

    /// Starts to count the nodes of one value from 0.
    fn begin(self, case: &Case<'_>) {
        case.with_record(|record| record.budgets.entry(self.id).or_default().push(0));
    }

    /// Stops the count of the value that [`Budget::begin`] started.
    fn end(self, case: &Case<'_>) {
        case.with_record(|record| {
            record.budgets.entry(self.id).or_default().pop();
        });
    }

    /// Counts one node of the value in progress.
    fn spend(self, case: &Case<'_>) {
        case.with_record(|record| {
            if let Some(used) = record.budgets.entry(self.id).or_default().last_mut() {
                *used += 1;
            }
        });
    }

    /// Returns `true` when the value in progress has used its budget.
    pub(crate) fn exhausted(self, case: &Case<'_>) -> bool {
        case.with_record(|record| {
            record
                .budgets
                .get(&self.id)
                .and_then(|used| used.last())
                .is_some_and(|used| *used >= BUDGET)
        })
    }
}

/// The definitions of a shape file that can reach themselves through refs,
/// by name, which the root of the file keeps.
#[derive(Default)]
pub(crate) struct Definitions {
    pub(crate) built: OnceLock<HashMap<String, Generator<Value>>>,
}

/// The root of a shape file with a cyclic definition: it counts the budget
/// of each value from 0.
pub(crate) struct Root {
    pub(crate) node: Generator<Value>,
    pub(crate) budget: Budget,
    /// The cyclic definitions, which the refs inside the root name by a weak
    /// reference, so the root keeps them.
    pub(crate) _definitions: Arc<Definitions>,
}

impl Node<Value> for Root {
    fn id(&self) -> &'static str {
        "shape"
    }

    fn decode(&self, case: &Case<'_>) -> Result<Value, Stop> {
        self.budget.begin(case);
        let decoded = self.node.decode(case);
        self.budget.end(case);
        decoded
    }

    fn emit(&self, value: &Value) -> Result<Emitted, CannotInvert> {
        self.node.emit(value)
    }
}

/// The definition that a ref names: the generator of a definition that
/// cannot reach itself, or the name of one that can, which the root keeps.
pub(crate) enum Target {
    Built(Generator<Value>),
    Cyclic(Weak<Definitions>),
}

/// The value of a definition, by name. A ref to a cyclic definition counts
/// one node of the budget.
pub(crate) struct Ref {
    pub(crate) name: String,
    pub(crate) target: Target,
    pub(crate) budget: Budget,
}

impl Ref {
    /// Returns the generator of the definition.
    fn resolved(&self) -> Option<Generator<Value>> {
        match &self.target {
            Target::Built(generator) => Some(generator.clone()),
            Target::Cyclic(definitions) => {
                definitions.upgrade()?.built.get()?.get(&self.name).cloned()
            }
        }
    }
}

impl Node<Value> for Ref {
    fn id(&self) -> &'static str {
        "ref"
    }

    fn decode(&self, case: &Case<'_>) -> Result<Value, Stop> {
        if matches!(self.target, Target::Cyclic(_)) {
            self.budget.spend(case);
        }
        self.resolved()
            .map_or(Err(Stop::Rejected), |generator| generator.decode(case))
    }

    fn emit(&self, value: &Value) -> Result<Emitted, CannotInvert> {
        self.resolved()
            .map_or(Err(CannotInvert::no_inverse("ref")), |generator| {
                generator.emit(value)
            })
    }
}

/// A value or `Null`: a presence choice, which an exit forces absent.
pub(crate) struct Optional {
    pub(crate) of: Generator<Value>,
    pub(crate) exits: bool,
    pub(crate) budget: Budget,
}

impl Node<Value> for Optional {
    fn id(&self) -> &'static str {
        "optional"
    }

    fn decode(&self, case: &Case<'_>) -> Result<Value, Stop> {
        let forced = self.exits && self.budget.exhausted(case);
        let _span = case.span("optional", None);
        if case.integer(IntegerBounds::of(0, i128::from(!forced)), Some(1), false)? == 1 {
            return self.of.decode(case);
        }
        Ok(Value::Null)
    }

    fn emit(&self, value: &Value) -> Result<Emitted, CannotInvert> {
        emit_optional(&self.of, value)
    }
}

/// A list, a fixed-list or a set: its elements as a collection, which takes
/// no further element at an exit. A set's elements are unique under the
/// definition's equality.
pub(crate) struct List {
    pub(crate) of: Generator<Value>,
    pub(crate) sizes: Sizes,
    pub(crate) unique: bool,
    pub(crate) exits: bool,
    pub(crate) budget: Budget,
}

impl Node<Value> for List {
    fn id(&self) -> &'static str {
        "list"
    }

    fn decode(&self, case: &Case<'_>) -> Result<Value, Stop> {
        let repeats = |a: &Value, b: &Value| same(a, b);
        let exhausted = |case: &Case<'_>| self.budget.exhausted(case);
        let _span = case.span("list", None);
        collect(
            case,
            self.sizes,
            ELEMENT,
            || self.of.decode(case),
            self.unique
                .then_some(&repeats as &dyn Fn(&Value, &Value) -> bool),
            self.exits
                .then_some(&exhausted as &dyn Fn(&Case<'_>) -> bool),
        )
        .map(Value::List)
    }

    /// Returns the flags and the elements, a set's elements in the shortlex
    /// order of their own choices.
    fn emit(&self, value: &Value) -> Result<Emitted, CannotInvert> {
        let Value::List(items) = value else {
            return Err(CannotInvert::refused("the value is no list"));
        };
        if self.unique {
            distinct(items)?;
        }
        let mut elements = items
            .iter()
            .map(|item| self.of.emit(item))
            .collect::<Result<Vec<_>, _>>()?;
        if self.unique {
            elements.sort_by_cached_key(|(steps, _)| shortlex(steps));
        }
        let (steps, normal): (Vec<_>, Vec<_>) = elements.into_iter().unzip();
        Ok((collection(self.sizes, steps)?, Value::List(normal)))
    }
}

/// A map: entries with distinct keys, as a dict decodes them, which takes no
/// further entry at an exit.
pub(crate) struct Map {
    pub(crate) key: Generator<Value>,
    pub(crate) of: Generator<Value>,
    pub(crate) sizes: Sizes,
    pub(crate) exits: bool,
    pub(crate) budget: Budget,
}

impl Node<Value> for Map {
    fn id(&self) -> &'static str {
        "dict"
    }

    fn decode(&self, case: &Case<'_>) -> Result<Value, Stop> {
        let repeats = |a: &(Value, Value), b: &(Value, Value)| same(&a.0, &b.0);
        let exhausted = |case: &Case<'_>| self.budget.exhausted(case);
        let _span = case.span("dict", None);
        collect(
            case,
            self.sizes,
            ENTRY,
            || {
                let key = self.key.decode(case)?;
                Ok((key, self.of.decode(case)?))
            },
            Some(&repeats),
            self.exits
                .then_some(&exhausted as &dyn Fn(&Case<'_>) -> bool),
        )
        .map(Value::Map)
    }

    fn emit(&self, value: &Value) -> Result<Emitted, CannotInvert> {
        emit_entries(&self.key, &self.of, self.sizes, value)
    }
}

/// Named fields, each decoded in declaration order, in one span.
pub(crate) struct Record {
    pub(crate) fields: Vec<(String, Generator<Value>)>,
}

impl Node<Value> for Record {
    fn id(&self) -> &'static str {
        "record"
    }

    fn decode(&self, case: &Case<'_>) -> Result<Value, Stop> {
        let _span = case.span("record", None);
        let mut fields = Vec::with_capacity(self.fields.len());
        for (name, of) in &self.fields {
            fields.push((name.clone(), of.decode(case)?));
        }
        Ok(Value::Record(fields))
    }

    fn emit(&self, value: &Value) -> Result<Emitted, CannotInvert> {
        let Value::Record(stated) = value else {
            return Err(CannotInvert::refused("the value is no record"));
        };
        let names_match = stated.len() == self.fields.len()
            && stated
                .iter()
                .zip(&self.fields)
                .all(|((stated, _), (name, _))| stated == name);
        if !names_match {
            return Err(CannotInvert::refused(
                "the value is no record of the shape's fields",
            ));
        }
        let mut steps = Vec::new();
        let mut normal = Vec::with_capacity(stated.len());
        for ((name, of), (_, field)) in self.fields.iter().zip(stated) {
            let (field_steps, field_normal) = of.emit(field)?;
            steps.extend(field_steps);
            normal.push((name.clone(), field_normal));
        }
        Ok((steps, Value::Record(normal)))
    }
}

/// A variant: an index that decides structure, then the payload of a
/// variant that has one.
///
/// `exit` is the index of the first variant that does not refer back, which
/// the index takes once the budget is used, and `None` for an enum that does
/// not refer back.
pub(crate) struct Enum {
    pub(crate) variants: Vec<(String, Option<Generator<Value>>)>,
    pub(crate) exit: Option<usize>,
    pub(crate) budget: Budget,
}

impl Enum {
    fn bounds(&self) -> IntegerBounds {
        IntegerBounds::of(0, index(self.variants.len()) - 1)
    }
}

impl Node<Value> for Enum {
    fn id(&self) -> &'static str {
        "enum"
    }

    fn decode(&self, case: &Case<'_>) -> Result<Value, Stop> {
        let bounds = match self.exit {
            Some(exit) if self.budget.exhausted(case) => {
                IntegerBounds::of(index(exit), index(exit))
            }
            _ => self.bounds(),
        };
        let _span = case.span("enum", None);
        let (name, payload) = &self.variants[position(case.integer(bounds, Some(0), false)?)];
        let payload = match payload {
            Some(of) => Some(Box::new(of.decode(case)?)),
            None => None,
        };
        Ok(Value::Variant {
            name: name.clone(),
            payload,
        })
    }

    fn emit(&self, value: &Value) -> Result<Emitted, CannotInvert> {
        let Value::Variant { name, payload } = value else {
            return Err(CannotInvert::refused("the value is no variant"));
        };
        let at = self
            .variants
            .iter()
            .position(|(held, _)| held == name)
            .ok_or_else(|| CannotInvert::refused("the value is no variant of the enum"))?;
        let head = Planned::new(Bounds::Integer(self.bounds()), Choice::Integer(index(at)));
        match (&self.variants[at].1, payload) {
            (None, None) => Ok((vec![head], value.clone())),
            (Some(of), Some(payload)) => {
                let (steps, normal) = of.emit(payload)?;
                let mut all = vec![head];
                all.extend(steps);
                Ok((
                    all,
                    Value::Variant {
                        name: name.clone(),
                        payload: Some(Box::new(normal)),
                    },
                ))
            }
            _ => Err(CannotInvert::refused(
                "the payload of the value is not the variant's",
            )),
        }
    }
}

/// An IP address of either version: an index, then 4 or 16 bytes.
pub(crate) struct Address {
    pub(crate) versions: [Generator<Value>; 2],
}

/// The bytes of an address of each version, in the order of the index.
const ADDRESS_BYTES: [usize; 2] = [4, 16];

impl Node<Value> for Address {
    fn id(&self) -> &'static str {
        "ip-address"
    }

    fn decode(&self, case: &Case<'_>) -> Result<Value, Stop> {
        let _span = case.span("ip-address", None);
        let at = case.integer(IntegerBounds::of(0, 1), Some(0), false)?;
        self.versions[position(at)].decode(case)
    }

    fn emit(&self, value: &Value) -> Result<Emitted, CannotInvert> {
        let Value::Bytes(bytes) = value else {
            return Err(CannotInvert::refused("the value is no address"));
        };
        let at = ADDRESS_BYTES
            .iter()
            .position(|count| *count == bytes.len())
            .ok_or_else(|| CannotInvert::refused("the value is neither 4 nor 16 bytes"))?;
        let (steps, normal) = self.versions[at].emit(value)?;
        let mut all = vec![Planned::new(
            Bounds::Integer(IntegerBounds::of(0, 1)),
            Choice::Integer(index(at)),
        )];
        all.extend(steps);
        Ok((all, normal))
    }
}
