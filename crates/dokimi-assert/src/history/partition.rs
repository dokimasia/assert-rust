//! The calls of a history as a check of linearizability reads them, and
//! their partitions.
//!
//! The check removes every call that failed. Two calls that share a key are
//! in one partition, and a call that states no key touches every key, so it
//! puts the whole history into one partition. Partitions come in the order
//! of their first invocation.

use super::recording::{Event, Kind};
use super::spec::Operation;
use crate::json::Json;
use crate::prop::engine::canonical::Canonical;
use crate::value::Value;
use std::collections::{HashMap, HashSet};

/// A call of a history from its invocation to its completion.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Span {
    /// The index of the call's invocation event.
    pub(crate) call: usize,
    /// The index of the call's completion event, and `None` for a pending
    /// call.
    pub(crate) completion: Option<usize>,
    pub(crate) process: usize,
    pub(crate) operation: Operation,
}

/// A call that a check reads: its span, its keys, and the identity of each
/// key.
#[derive(Debug, Clone)]
pub(crate) struct Checked {
    pub(crate) span: Span,
    pub(crate) keys: Vec<Value>,
    pub(crate) ids: Vec<Canonical>,
}

/// Calls that share keys, which the check searches on their own.
#[derive(Debug, Clone)]
pub(crate) struct Partition {
    /// The keys of the calls, each once, in the order that the history
    /// first states them, and none for every key.
    pub(crate) keys: Vec<Value>,
    /// The calls, in event order.
    pub(crate) calls: Vec<Checked>,
}

/// Returns the calls of a history in event order, without the calls that
/// failed. A call is known when it completed as ok.
pub(crate) fn calls_of(events: &[Event], ids: &[Vec<Canonical>]) -> Vec<Checked> {
    let completions: HashMap<usize, &Event> = events
        .iter()
        .filter(|event| !matches!(event.kind, Kind::Invoke { .. }))
        .map(|event| (event.call, event))
        .collect();
    let mut calls = Vec::new();
    for event in events {
        let Kind::Invoke {
            operation,
            args,
            keys,
        } = &event.kind
        else {
            continue;
        };
        let completion = completions.get(&event.index);
        let output = match completion.map(|completion| &completion.kind) {
            Some(Kind::Fail { .. }) => continue,
            Some(Kind::Ok { output }) => Some(output.clone()),
            _ => None,
        };
        calls.push(Checked {
            span: Span {
                call: event.index,
                completion: completion.map(|completion| completion.index),
                process: event.process,
                operation: Operation::new(operation.clone(), args.clone(), output),
            },
            keys: keys.clone(),
            ids: ids.get(event.index).cloned().unwrap_or_default(),
        });
    }
    calls
}

/// Returns the representative of the partition of the call at `at`, halving
/// the path to it.
fn root(parent: &mut [usize], mut at: usize) -> usize {
    while parent[at] != at {
        parent[at] = parent[parent[at]];
        at = parent[at];
    }
    at
}

/// Returns the keys that `calls` state, each once, in the order that they
/// first state them.
fn keys_of(calls: &[Checked]) -> Vec<Value> {
    let mut seen: HashSet<&Canonical> = HashSet::new();
    let mut keys = Vec::new();
    for call in calls {
        for (id, key) in call.ids.iter().zip(&call.keys) {
            if seen.insert(id) {
                keys.push(key.clone());
            }
        }
    }
    keys
}

/// Returns the partitions of `calls`, in the order of their first
/// invocation.
pub(crate) fn partitions_of(calls: Vec<Checked>) -> Vec<Partition> {
    if calls.is_empty() {
        return Vec::new();
    }
    if calls.iter().any(|call| call.ids.is_empty()) {
        return vec![Partition {
            keys: Vec::new(),
            calls,
        }];
    }
    let mut parent: Vec<usize> = (0..calls.len()).collect();
    let mut first: HashMap<&Canonical, usize> = HashMap::new();
    for (position, call) in calls.iter().enumerate() {
        for id in &call.ids {
            let other = *first.entry(id).or_insert(position);
            let (a, b) = (root(&mut parent, position), root(&mut parent, other));
            parent[a] = b;
        }
    }
    let mut order: HashMap<usize, usize> = HashMap::new();
    let mut groups: Vec<Vec<Checked>> = Vec::new();
    for (position, call) in calls.into_iter().enumerate() {
        let group = root(&mut parent, position);
        if let Some(&at) = order.get(&group) {
            groups[at].push(call);
        } else {
            order.insert(group, groups.len());
            groups.push(vec![call]);
        }
    }
    groups
        .into_iter()
        .map(|calls| Partition {
            keys: keys_of(&calls),
            calls,
        })
        .collect()
}

/// Returns the most calls that are open at one event. A call is open from
/// its invocation to its completion, and a call that is not known is open to
/// the end of the history.
pub(crate) fn concurrency(calls: &[Checked]) -> usize {
    let mut changes: Vec<(usize, isize)> = Vec::new();
    for Checked { span, .. } in calls {
        changes.push((span.call, 1));
        if let (true, Some(completion)) = (span.operation.known(), span.completion) {
            changes.push((completion, -1));
        }
    }
    changes.sort_unstable();
    let (mut open, mut most) = (0_isize, 0_isize);
    for (_, change) in changes {
        open += change;
        most = most.max(open);
    }
    usize::try_from(most).unwrap_or_default()
}

/// Returns a span in the history's JSON form: the call, the completion, the
/// process, the operation, the args as typed literals, and the output as a
/// typed literal. A pending call states no completion, and a call that is
/// not known states no output.
pub(crate) fn span_json(span: &Span) -> Json {
    let mut members = vec![("call".to_owned(), Json::from(span.call))];
    if let Some(completion) = span.completion {
        members.push(("completion".to_owned(), Json::from(completion)));
    }
    members.push(("process".to_owned(), Json::from(span.process)));
    members.push(("operation".to_owned(), Json::from(span.operation.name())));
    members.push(("args".to_owned(), literals(span.operation.args())));
    if let Some(output) = span.operation.output() {
        members.push(("output".to_owned(), output.to_literal()));
    }
    Json::Object(members)
}

/// Returns values as a JSON array of their typed literals.
pub(crate) fn literals(values: &[Value]) -> Json {
    Json::Array(values.iter().map(Value::to_literal).collect())
}
