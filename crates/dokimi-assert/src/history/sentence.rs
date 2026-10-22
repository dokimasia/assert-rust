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

/// Returns the text of the observation of an anomaly that is no cycle: the
/// reads and the appends that show it.
fn observation_text(observation: &Observation) -> String {
    match observation {
        Observation::GarbageRead {
            call,
            key,
            read,
            value,
        } => format!(
            "call {call} read {} from {key}, and no transaction appended {value}",
            list_text(read)
        ),
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
            let mut text = format!("call {call} read {} from {key}", list_text(read));
            if *whole {
                let _ = write!(text, ", and it knew the list was {}", list_text(expected));
            } else if !expected.is_empty() {
                let _ = write!(
                    text,
                    ", and it knew the list ended with {}",
                    list_text(expected)
                );
            }
            if let Some(future) = future {
                let _ = write!(
                    text,
                    ", and {future} is a value that call {call} appends later"
                );
            }
            text
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
        } => {
            format!(
                "call {call} read {value} from {key}, which call {appender} appended and aborted"
            )
        }
        Observation::IntermediateRead {
            call,
            key,
            value,
            appender,
            next,
        } => format!(
            "call {call} read {key} ending in {value}, which call {appender} followed with {next}"
        ),
    }
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
