// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The sentences of the records of `linearizable`, `serializable` and
//! `snapshot-isolation`, which a seat without records receives.

use super::cycle::Relation;
use super::derivation::{Edge, Evidence, Observation};
use super::isolation::IsolationDetail;
use super::linearizable::{Detail, state_value};
use super::partition::Span;
use crate::value::Value;
use std::fmt::{Debug, Write as _};

/// Returns the text of the keys of a partition, and `every key` for a
/// partition of every key.
fn keys_text(keys: &[Value]) -> String {
    if keys.is_empty() {
        return "every key".to_owned();
    }
    keys.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Returns the text of a list of values.
fn list_text(values: &[Value]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    )
}

/// Returns a line of the label and the text of each span, or `none` for no
/// span. A span's text states its call, its operation with its arguments,
/// and its output, or that its outcome is unknown.
fn spans_text(label: &str, spans: &[Span]) -> String {
    let texts: Vec<String> = spans
        .iter()
        .map(|span| {
            let operation = &span.operation;
            let args = operation
                .args()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            let outcome = operation.output().map_or_else(
                || ", outcome unknown".to_owned(),
                |output| format!(" → {output}"),
            );
            format!("call {} {}({args}){outcome}", span.call, operation.name())
        })
        .collect();
    let listed = if texts.is_empty() {
        "none".to_owned()
    } else {
        texts.join("; ")
    };
    format!("\n    {label}: {listed}")
}

/// Returns the sentence of a failing check of linearizability: the
/// contract, the outcome with the reported partition and the limit, and then
/// the counts, the frontier's order of calls, its states and the calls that
/// the spec rejected there, each on a line of its own.
pub(crate) fn linearizable<S: Debug + 'static>(detail: &Detail<S>, contract: &str) -> String {
    let limit = detail
        .limit
        .map(|limit| format!(", at the {} limit", limit.name()))
        .unwrap_or_default();
    let states: Vec<String> = detail
        .states
        .iter()
        .map(|state| state_value(state).to_string())
        .collect();
    let mut text = format!(
        "{contract}: {} in the partition of {}{limit}",
        detail.outcome.name(),
        keys_text(&detail.partition)
    );
    let _ = write!(
        text,
        "\n    steps {}, partitions {}, calls {}, concurrency {}",
        detail.steps, detail.partitions, detail.calls, detail.concurrency
    );
    text.push_str(&spans_text("linearized", &detail.linearized));
    let _ = write!(text, "\n    states: {}", states.join(", "));
    text.push_str(&spans_text("rejected", &detail.candidates));
    text
}

/// Returns the text of an edge of a cycle: its two calls and its relation,
/// and the key and the values that prove it.
fn edge_text(edge: &Edge) -> String {
    let head = format!(
        "call {} -{}-> call {}: ",
        edge.from,
        edge.relation.name(),
        edge.to
    );
    let (key, null) = (&edge.key, Value::Null);
    let value = edge.value.as_ref().unwrap_or(&null);
    let next = edge.next.as_ref().unwrap_or(&null);
    if edge.register {
        return head + &register_edge_text(edge, value, next);
    }
    let tail = match (edge.relation, &edge.value) {
        (Relation::Ww, _) => format!("call {} appended {next} to {key} after {value}", edge.to),
        (Relation::Wr, _) => format!("call {} read {key} ending in {value}", edge.to),
        (Relation::Rw, None) => format!(
            "call {} read [] from {key}, and call {} appended {next} to it",
            edge.from, edge.to
        ),
        (Relation::Rw, Some(_)) => format!(
            "call {} read {key} ending in {value}, and call {} appended {next} after it",
            edge.from, edge.to
        ),
    };
    head + &tail
}

/// Returns the values and the calls that prove an edge of a register
/// history.
fn register_edge_text(edge: &Edge, value: &Value, next: &Value) -> String {
    let (from, to, key) = (edge.from, edge.to, &edge.key);
    match (edge.relation, &edge.value) {
        (Relation::Ww, _) => format!("call {to} wrote {next} to {key} after {value}"),
        (Relation::Wr, _) => format!("call {to} read {value} from {key}"),
        (Relation::Rw, None) => {
            format!("call {from} read the initial value of {key}, and call {to} wrote {next} to it")
        }
        (Relation::Rw, Some(_)) => {
            format!("call {from} read {value} from {key}, and call {to} wrote {next} after it")
        }
    }
}

/// Returns the text of a register's value, and `the initial value` for
/// `None`.
fn register_text(value: Option<&Value>) -> String {
    value.map_or_else(|| "the initial value".to_owned(), ToString::to_string)
}

/// Returns the text of the observation of an anomaly that is no cycle: the
/// reads and the appends that show it.
fn observation_text(observation: &Observation) -> String {
    match observation {
        Observation::GarbageRead {
            call,
            key,
            read,
            value,
            register,
        } => {
            if *register {
                format!("call {call} read {value} from {key}, and no transaction wrote it")
            } else {
                format!(
                    "call {call} read {} from {key}, and no transaction appended {value}",
                    list_text(read)
                )
            }
        }
        Observation::DuplicateAppend {
            call,
            key,
            read,
            value,
        } => {
            format!(
                "call {call} read {} from {key}, which contains {value} twice",
                list_text(read)
            )
        }
        Observation::InternalInconsistency {
            call,
            key,
            read,
            expected,
            whole,
            future,
        } => {
            let knew = if *whole {
                Some(format!("the list was {}", list_text(expected)))
            } else {
                (!expected.is_empty())
                    .then(|| format!("the list ended with {}", list_text(expected)))
            };
            let read = format!("call {call} read {} from {key}", list_text(read));
            inconsistency_text(read, knew, future.as_ref(), *call, "appends")
        }
        Observation::RegisterInconsistency {
            call,
            key,
            read,
            known,
            whole,
            future,
        } => {
            let knew = whole.then(|| format!("the value was {}", register_text(known.as_ref())));
            let read = format!(
                "call {call} read {} from {key}",
                register_text(read.as_ref())
            );
            inconsistency_text(read, knew, future.as_ref(), *call, "writes")
        }
        Observation::IncompatibleOrder { calls, key, reads } => format!(
            "calls {} and {} read {} and {} from {key}, and neither is a prefix of the other",
            calls[0],
            calls[1],
            list_text(&reads[0]),
            list_text(&reads[1])
        ),
        Observation::AbortedRead {
            call,
            key,
            value,
            appender,
            register,
        } => {
            let verb = if *register { "wrote" } else { "appended" };
            format!("call {call} read {value} from {key}, which call {appender} {verb} and aborted")
        }
        Observation::IntermediateRead {
            call,
            key,
            value,
            appender,
            next,
            register,
        } => {
            let read = if *register {
                format!("{value} from {key}")
            } else {
                format!("{key} ending in {value}")
            };
            format!("call {call} read {read}, which call {appender} followed with {next}")
        }
    }
}

/// Returns the text of an internal inconsistency: the read, what the
/// transaction knew of the key before the read, and the value of the read
/// that call `call` appends or writes later.
fn inconsistency_text(
    read: String,
    knew: Option<String>,
    future: Option<&Value>,
    call: usize,
    verb: &str,
) -> String {
    let mut text = read;
    if let Some(knew) = knew {
        let _ = write!(text, ", and it knew {knew}");
    }
    if let Some(future) = future {
        let _ = write!(
            text,
            ", and {future} is a value that call {call} {verb} later"
        );
    }
    text
}

/// Returns the sentence of a failing isolation check: the contract and the
/// anomaly, then the kinds, and then a line for each entry of the
/// explanation.
pub(crate) fn isolation(detail: &IsolationDetail, contract: &str) -> String {
    let kinds: Vec<&str> = detail.kinds.iter().map(|kind| kind.name()).collect();
    let mut text = format!(
        "{contract}: {}\n    kinds: {}",
        detail.anomaly.name(),
        kinds.join(", ")
    );
    for evidence in &detail.instance.explanation {
        let line = match evidence {
            Evidence::Edge(edge) => edge_text(edge),
            Evidence::Observation(observation) => observation_text(observation),
        };
        let _ = write!(text, "\n    {line}");
    }
    text
}
