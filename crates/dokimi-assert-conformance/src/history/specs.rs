// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The named specs of the definition, which a history vector names in place
//! of a spec of its own.
//!
//! A state is a value: the register's value, the key-value map as a map in
//! the order its keys were stored, and the queue and the set as a list in
//! order. Two values compare by their canonical texts. Every spec accepts a
//! call whose outcome is unknown in every state, checks no output of a
//! write, a put, an append, an enqueue or an add, and panics on an operation
//! that it does not define, which the check reports as a fault. The
//! key-value map and the set compare their states in any order.

use crate::literal::canonical;
use dokimi_assert::Value;
use dokimi_assert::history::{Operation, Spec};
use std::hash::{DefaultHasher, Hash, Hasher};

/// The value of a key that the key-value spec does not store.
const EMPTY: &str = "";

/// One named spec of the definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Named {
    /// A register: a write stores its value, and a read outputs it.
    Register,
    /// A register with compare-and-set.
    CasRegister,
    /// A map from keys to text: get, put and append.
    KeyValue,
    /// A queue: enqueue at the tail, dequeue from the head.
    Queue,
    /// A set: add, remove and contains.
    Set,
    /// A register whose write may be lost.
    LossyRegister,
}

impl Named {
    /// Returns the named spec of `name`, or `None` for a name that the
    /// definition does not define.
    #[must_use]
    pub fn of(name: &str) -> Option<Self> {
        Some(match name {
            "register" => Self::Register,
            "cas-register" => Self::CasRegister,
            "key-value" => Self::KeyValue,
            "queue" => Self::Queue,
            "set" => Self::Set,
            "lossy-register" => Self::LossyRegister,
            _ => return None,
        })
    }

    /// Returns the definition's name of the spec.
    fn name(self) -> &'static str {
        match self {
            Self::Register => "register",
            Self::CasRegister => "cas-register",
            Self::KeyValue => "key-value",
            Self::Queue => "queue",
            Self::Set => "set",
            Self::LossyRegister => "lossy-register",
        }
    }

    /// Panics for an operation that the spec does not define.
    fn undefined(self, operation: &Operation) -> ! {
        panic!(
            "conformance: the {} spec has no operation {:?}",
            self.name(),
            operation.name()
        )
    }

    /// Steps a register's read, which outputs the state and changes nothing,
    /// and panics on any other operation.
    fn read(self, state: &Value, operation: &Operation) -> Vec<Value> {
        if operation.name() != "read" {
            self.undefined(operation);
        }
        accepted(returned(operation, state), state.clone())
    }

    /// Steps the register with compare-and-set. `cas(from, to)` outputs
    /// whether the state equals `from`, and stores `to` when it does. A cas
    /// whose outcome is unknown takes effect when the state equals `from`,
    /// and leaves the state otherwise.
    fn cas(state: &Value, operation: &Operation) -> Vec<Value> {
        let [from, to] = operation.args() else {
            return Vec::new();
        };
        let matches = same(state, from);
        if operation
            .output()
            .is_some_and(|output| *output != Value::Bool(matches))
        {
            return Vec::new();
        }
        vec![if matches { to.clone() } else { state.clone() }]
    }

    /// Steps the map from keys to text.
    fn key_value(self, state: &Value, operation: &Operation) -> Vec<Value> {
        let entries = match state {
            Value::Map(entries) => entries.as_slice(),
            _ => &[],
        };
        let name = operation.args().first().cloned().unwrap_or(Value::Null);
        let current = entries
            .iter()
            .find(|(stored, _)| same(stored, &name))
            .map_or_else(|| Value::from(EMPTY), |(_, value)| value.clone());
        let value = operation.args().get(1).cloned().unwrap_or(Value::Null);
        match operation.name() {
            "get" => accepted(returned(operation, &current), state.clone()),
            "put" => vec![with_value(entries, &name, value)],
            "append" => {
                let text = |value: &Value| match value {
                    Value::String(text) => text.clone(),
                    other => other.to_string(),
                };
                vec![with_value(
                    entries,
                    &name,
                    Value::String(format!("{}{}", text(&current), text(&value))),
                )]
            }
            _ => self.undefined(operation),
        }
    }

    /// Steps the queue: enqueue adds a value at the tail, and dequeue outputs
    /// the head, or null when the queue is empty.
    fn queue(self, state: &Value, operation: &Operation) -> Vec<Value> {
        let items = items(state);
        match (operation.name(), items.split_first()) {
            ("enqueue", _) => {
                let mut added = items.to_vec();
                added.extend(operation.args().first().cloned());
                vec![Value::List(added)]
            }
            ("dequeue", None) => accepted(returned(operation, &Value::Null), state.clone()),
            ("dequeue", Some((head, rest))) => {
                accepted(returned(operation, head), Value::List(rest.to_vec()))
            }
            _ => self.undefined(operation),
        }
    }

    /// Steps the set, which lists its values in the order they were added.
    /// add adds a value when it is absent, and remove and contains output
    /// whether a value is present. A remove whose outcome is unknown removes
    /// the value when it is present.
    fn set(self, state: &Value, operation: &Operation) -> Vec<Value> {
        let items = items(state);
        let value = operation.args().first().cloned().unwrap_or(Value::Null);
        let present = items.iter().any(|item| same(item, &value));
        match operation.name() {
            "add" if present => vec![state.clone()],
            "add" => {
                let mut added = items.to_vec();
                added.push(value);
                vec![Value::List(added)]
            }
            "contains" => accepted(returned(operation, &Value::Bool(present)), state.clone()),
            "remove" => accepted(
                returned(operation, &Value::Bool(present)),
                Value::List(
                    items
                        .iter()
                        .filter(|item| !same(item, &value))
                        .cloned()
                        .collect(),
                ),
            ),
            _ => self.undefined(operation),
        }
    }
}

/// Returns `true` when two values have one canonical text.
fn same(a: &Value, b: &Value) -> bool {
    canonical(a) == canonical(b)
}

/// Returns `true` when `operation` may have returned `value`: a call that is
/// not known may have, and a known call when its output has the canonical
/// text of `value`.
fn returned(operation: &Operation, value: &Value) -> bool {
    operation.output().is_none_or(|output| same(output, value))
}

/// Returns `state` alone when the spec accepts the operation, and no state
/// otherwise.
fn accepted(accepts: bool, state: Value) -> Vec<Value> {
    if accepts { vec![state] } else { Vec::new() }
}

/// Returns the items of a list state.
fn items(state: &Value) -> &[Value] {
    match state {
        Value::List(items) => items,
        _ => &[],
    }
}

/// Returns a key-value state with `value` under `name`: in the place of
/// `name` when the state stores it, and last otherwise. The map stores no
/// empty value, so two maps are equal exactly when every key reads the same
/// value from both.
fn with_value(entries: &[(Value, Value)], name: &Value, value: Value) -> Value {
    let mut entries = entries.to_vec();
    let empty = value == Value::from(EMPTY);
    match entries.iter().position(|(stored, _)| same(stored, name)) {
        None if !empty => entries.push((name.clone(), value)),
        None => {}
        Some(at) if empty => {
            entries.remove(at);
        }
        Some(at) => entries[at] = (name.clone(), value),
    }
    Value::Map(entries)
}

/// Returns the canonical texts of the values of a set state, sorted, which
/// two states of equal values share in any order.
fn set_text(state: &Value) -> String {
    let mut texts: Vec<String> = items(state).iter().map(canonical).collect();
    texts.sort();
    texts.join(",")
}

impl Spec for Named {
    type State = Value;

    fn initial(&self) -> Value {
        match self {
            Self::Register | Self::CasRegister | Self::LossyRegister => Value::Null,
            Self::KeyValue => Value::Map(Vec::new()),
            Self::Queue | Self::Set => Value::List(Vec::new()),
        }
    }

    fn next(&self, state: &Value, operation: &Operation) -> Vec<Value> {
        let write = operation.name() == "write";
        let value = || operation.args().first().cloned().unwrap_or(Value::Null);
        match self {
            Self::Register | Self::CasRegister if write => vec![value()],
            Self::LossyRegister if write => vec![value(), state.clone()],
            Self::CasRegister if operation.name() == "cas" => Self::cas(state, operation),
            Self::Register | Self::CasRegister | Self::LossyRegister => self.read(state, operation),
            Self::KeyValue => self.key_value(state, operation),
            Self::Queue => self.queue(state, operation),
            Self::Set => self.set(state, operation),
        }
    }

    fn equal(&self, a: &Value, b: &Value) -> bool {
        match self {
            Self::Set => set_text(a) == set_text(b),
            _ => same(a, b),
        }
    }

    fn hash(&self, state: &Value) -> u64 {
        let mut hasher = DefaultHasher::new();
        match self {
            Self::Set => set_text(state).hash(&mut hasher),
            _ => canonical(state).hash(&mut hasher),
        }
        hasher.finish()
    }
}
