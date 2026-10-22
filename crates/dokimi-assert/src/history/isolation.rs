// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The checks that the list-append transactions of a history exhibit no
//! anomaly that an isolation level forbids.
//!
//! A check first reads the micro-operations for the six anomalies that need
//! no dependency graph. It then derives the write-write, write-read and
//! read-write dependencies between committed transactions, and searches the
//! graph for the five kinds of cycle. A level forbids a set of the eleven
//! kinds, and the record reports the first of them that the history
//! exhibits.

use super::derivation::{ANOMALIES, Analysis, Anomaly, Edge, Evidence, Instance, Observation};
use super::recording::History;
use super::sentence;
use super::transaction::{Transaction, transaction_json, transactions};
use crate::failure::Failure;
use crate::json::{Json, object};
use crate::seat::Seat;
use crate::value::Value;
use crate::verdict::{Mode, Run};

/// The assertion of a check of serializability.
const SERIALIZABLE: &str = "serializable";

/// The assertion of a check of snapshot isolation.
const SNAPSHOT_ISOLATION: &str = "snapshot-isolation";

/// The five fields of the detail of a failing check, in order.
const FIELDS: [&str; 5] = ["anomaly", "kinds", "transactions", "cycle", "explanation"];

/// The detail of the record of a failing isolation check.
#[derive(Debug, Clone)]
pub(crate) struct IsolationDetail {
    /// The first kind that the level forbids and the history exhibits.
    pub(crate) anomaly: Anomaly,
    /// Every kind that the level forbids and the history exhibits, in report
    /// order.
    pub(crate) kinds: Vec<Anomaly>,
    /// The transactions that the anomaly involves.
    pub(crate) transactions: Vec<Transaction>,
    /// The anomaly's instance: its cycle and its explanation.
    pub(crate) instance: Instance,
}

/// Returns the detail of the check of `history`'s transactions, or `None`
/// for a history that exhibits no anomaly that the level forbids. Snapshot
/// isolation forbids every kind but G2, so it permits a cycle in which two
/// read-write dependencies are adjacent, such as a write skew.
fn check(history: &History, snapshot: bool) -> Result<Option<IsolationDetail>, String> {
    let txns = transactions(&history.events()).map_err(|refused| refused.0)?;
    let analysis = Analysis::new(&txns);
    let found: Vec<(Anomaly, Instance)> = ANOMALIES
        .into_iter()
        .filter(|kind| !(snapshot && *kind == Anomaly::G2))
        .filter_map(|kind| analysis.instance(kind).map(|instance| (kind, instance)))
        .collect();
    let Some((_, first)) = found.first() else {
        return Ok(None);
    };
    Ok(Some(IsolationDetail {
        anomaly: first.anomaly,
        kinds: found.iter().map(|(kind, _)| *kind).collect(),
        transactions: first
            .calls
            .iter()
            .filter_map(|call| txns.iter().find(|txn| txn.call == *call).cloned())
            .collect(),
        instance: first.clone(),
    }))
}

/// Returns an edge of a cycle as the explanation of a record states it.
fn edge_json(edge: &Edge) -> Json {
    let mut members = vec![
        ("from".to_owned(), Json::from(edge.from)),
        ("to".to_owned(), Json::from(edge.to)),
        ("relation".to_owned(), Json::from(edge.relation.name())),
        ("key".to_owned(), edge.key.to_literal()),
        (
            "value".to_owned(),
            edge.value.as_ref().map_or(Json::Null, Value::to_literal),
        ),
    ];
    if let Some(next) = &edge.next {
        members.push(("next".to_owned(), next.to_literal()));
    }
    Json::Object(members)
}

/// Returns an observation as the explanation of a record states it: the
/// fields that its anomaly states.
fn observation_json(observation: &Observation) -> Json {
    match observation {
        Observation::GarbageRead {
            call,
            key,
            read,
            value,
        }
        | Observation::DuplicateAppend {
            call,
            key,
            read,
            value,
        } => object([
            ("call", (*call).into()),
            ("key", key.to_literal()),
            ("read", Value::List(read.clone()).to_literal()),
            ("value", value.to_literal()),
        ]),
        Observation::InternalInconsistency {
            call,
            key,
            read,
            expected,
            whole,
            future,
        } => object([
            ("call", (*call).into()),
            ("key", key.to_literal()),
            ("read", Value::List(read.clone()).to_literal()),
            ("expected", Value::List(expected.clone()).to_literal()),
            ("whole", (*whole).into()),
            (
                "future",
                future.as_ref().map_or(Json::Null, Value::to_literal),
            ),
        ]),
        Observation::IncompatibleOrder { calls, key, reads } => object([
            (
                "calls",
                Json::Array(calls.iter().map(|call| Json::from(*call)).collect()),
            ),
            ("key", key.to_literal()),
            (
                "reads",
                Json::Array(
                    reads
                        .iter()
                        .map(|read| Value::List(read.clone()).to_literal())
                        .collect(),
                ),
            ),
        ]),
        Observation::AbortedRead {
            call,
            key,
            value,
            appender,
        } => object([
            ("call", (*call).into()),
            ("key", key.to_literal()),
            ("value", value.to_literal()),
            ("appender", (*appender).into()),
        ]),
        Observation::IntermediateRead {
            call,
            key,
            value,
            appender,
            next,
        } => object([
            ("call", (*call).into()),
            ("key", key.to_literal()),
            ("value", value.to_literal()),
            ("appender", (*appender).into()),
            ("next", next.to_literal()),
        ]),
    }
}

/// Returns the detail of a failing check as its call record states it.
fn detail_json(detail: &IsolationDetail) -> Json {
    let cycle = detail.instance.cycle.as_ref().map_or(Json::Null, |cycle| {
        Json::Array(
            cycle
                .iter()
                .map(|link| {
                    object([
                        ("call", link.call.into()),
                        (
                            "relations",
                            Json::Array(
                                link.relations
                                    .iter()
                                    .map(|relation| Json::from(relation.name()))
                                    .collect(),
                            ),
                        ),
                    ])
                })
                .collect(),
        )
    });
    object([
        ("anomaly", detail.anomaly.name().into()),
        (
            "kinds",
            Json::Array(
                detail
                    .kinds
                    .iter()
                    .map(|kind| Json::from(kind.name()))
                    .collect(),
            ),
        ),
        (
            "transactions",
            Json::Array(detail.transactions.iter().map(transaction_json).collect()),
        ),
        ("cycle", cycle),
        (
            "explanation",
            Json::Array(
                detail
                    .instance
                    .explanation
                    .iter()
                    .map(|evidence| match evidence {
                        Evidence::Edge(edge) => edge_json(edge),
                        Evidence::Observation(observation) => observation_json(observation),
                    })
                    .collect(),
            ),
        ),
    ])
}

/// Checks `history` at a level and reports the verdict of the level's
/// assertion, whose faults name `op`.
#[track_caller]
fn isolated(seat: &dyn Seat, history: &History, msg: &str, snapshot: bool) {
    seat.helper();
    let run = Run::of(seat);
    let (assertion, op) = if snapshot {
        (SNAPSHOT_ISOLATION, "history::has_snapshot_isolation")
    } else {
        (SERIALIZABLE, "history::is_serializable")
    };
    match check(history, snapshot) {
        Err(refused) => run.fault(Mode::Fatal, assertion, msg, format!("{op}: {refused}")),
        Ok(None) => run.pass(Mode::Fatal, assertion, msg),
        Ok(Some(detail)) => {
            let json = detail_json(&detail);
            let fields = FIELDS
                .iter()
                .map(|name| {
                    (
                        *name,
                        json.get(name).map_or(Value::Null, Value::from_detail),
                    )
                })
                .collect();
            let failure = Failure::new(assertion, msg, fields, Some(run.location()))
                .with_sentence(sentence::isolation(&detail, msg));
            run.fail_run(Mode::Fatal, &failure, json);
        }
    }
}

/// Fails when the list-append transactions of `history` exhibit an anomaly
/// that serializability forbids, among the dependencies that their reads
/// reveal. Serializability forbids every kind of anomaly.
///
/// A transaction is a call of the operation `txn` whose arguments are its
/// micro-operations, each a list of three parts: `"append"`, a key and a
/// value, or `"read"`, a key and null. An ok completion commits the
/// transaction, and its output is a list that repeats the micro-operations
/// with each read's list, or null for the empty list. A fail completion
/// aborts it. A transaction whose outcome is unknown, or that is pending, is
/// committed when a committed read observed one of its appends. Every value
/// is appended to its key once. Keys and values compare by their typed
/// literals.
///
/// The record's detail states the five fields of the definition: anomaly,
/// kinds, transactions, cycle and explanation. A history whose calls are no
/// list-append transactions ends the call with a fault.
#[track_caller]
pub fn is_serializable(seat: &dyn Seat, history: &History, msg: &str) {
    seat.helper();
    isolated(seat, history, msg, false);
}

/// Fails when the list-append transactions of `history` exhibit an anomaly
/// that snapshot isolation forbids. Snapshot isolation forbids every kind of
/// anomaly but G2: it permits a cycle in which two read-write dependencies
/// are adjacent, such as a write skew.
///
/// The workload, the derivation, the record and the faults are those of
/// [`is_serializable`].
#[track_caller]
pub fn has_snapshot_isolation(seat: &dyn Seat, history: &History, msg: &str) {
    seat.helper();
    isolated(seat, history, msg, true);
}
