// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The record of a property's run, its detail and its sentence.
//!
//! The detail has the eleven fields of `prop-for-all`: outcome, cases,
//! rejected, seed, counterexample, failure, choices, others, divergence,
//! coverage and reason. A field that the outcome does not use is null. The
//! call record of a property states the same detail as JSON, with each drawn
//! value a typed literal of its generator's type, or an opaque literal where
//! the type has none.

use super::engine::case::{Drawn, Rendered, Step};
use super::engine::choice::Bounds;
use super::engine::execution::{Divergence, Execution, Version};
use super::engine::runner::{Kind, Outcome};
use super::engine::shrink::Explained;
use super::engine::token;
use super::engine::trace::{Traced, entries};
use super::environment::REPLAY_VARIABLE;
use crate::failure::Failure;
use crate::json::{self, Json, object};
use crate::value::Value;
use std::fmt::Write as _;

/// The id of the record of `for_all`.
pub(crate) const FOR_ALL: &str = "prop-for-all";

/// The largest integer that a JSON reader in JavaScript keeps exact.
const SAFE: i128 = (1 << 53) - 1;

/// Returns an integer as plain JSON: a number within ±(2^53 − 1), and its
/// decimal text beyond.
fn plain(held: i128) -> Json {
    if (-SAFE..=SAFE).contains(&held) {
        Json::Int(held)
    } else {
        Json::String(held.to_string())
    }
}

/// Returns a float bound as plain JSON, with the infinities by name. A bound
/// is never NaN.
fn plain_float(held: f64) -> Json {
    if held.is_infinite() {
        if held > 0.0 { "Inf" } else { "-Inf" }.into()
    } else {
        Json::Float(held)
    }
}

/// Returns the bounds of a request in the form of the definition's vectors.
pub(crate) fn bounds_json(bounds: &Bounds) -> Json {
    match bounds {
        Bounds::Integer(integer) => object([
            ("kind", "integer".into()),
            ("min", plain(integer.lo())),
            ("max", plain(integer.hi())),
        ]),
        Bounds::Float(float) => object([
            ("kind", "float".into()),
            ("min", plain_float(float.lo())),
            ("max", plain_float(float.hi())),
            ("allow_nan", float.allow_nan().into()),
            ("width", Json::Int(i128::from(float.width().bits()))),
        ]),
        Bounds::Sequence(sequence) => object([
            ("kind", "sequence".into()),
            ("k", Json::Int(i128::from(sequence.k()))),
            ("min_size", sequence.min_size().into()),
            (
                "max_size",
                sequence.max_size().map_or(Json::Null, Json::from),
            ),
        ]),
    }
}

/// Returns the literal of a drawn value: its typed literal, or its opaque
/// literal.
fn literal(value: &Rendered) -> Json {
    match &value.literal {
        Some(literal) => literal.to_literal(),
        None => object([
            ("type", "opaque".into()),
            ("text", value.text.as_str().into()),
        ]),
    }
}

/// Returns the JSON of a step of a machine, with its client and its drain
/// mark.
pub(crate) fn step_json(step: &Step) -> Json {
    let mut members = vec![("step".to_owned(), Json::from(step.action.as_str()))];
    if let Some(client) = step.client {
        members.push(("client".to_owned(), client.into()));
    }
    if step.drain {
        members.push(("drain".to_owned(), true.into()));
    }
    Json::Object(members)
}

/// Returns the entries of a case: its steps and its draws, each draw with
/// its explanation where the explain phase tested it, or with its label and
/// value alone when `brief`.
fn entries_json(execution: &Execution, explanation: &[Explained], brief: bool) -> Json {
    let mut listed = Vec::new();
    for entry in entries(&execution.record) {
        let number = match entry {
            Traced::Step(step) => {
                listed.push(step_json(&step));
                continue;
            }
            Traced::Draw(number) => number,
        };
        let drawn: &Drawn = &execution.record.draws[number];
        if brief {
            listed.push(object([
                ("label", drawn.label.as_str().into()),
                ("value", literal(&drawn.value)),
            ]));
            continue;
        }
        let explained = explanation.get(number);
        listed.push(object([
            ("label", drawn.label.as_str().into()),
            ("value", literal(&drawn.value)),
            (
                "any-value-fails",
                explained
                    .and_then(|explained| explained.any_value_fails)
                    .map_or(Json::Null, Json::from),
            ),
            (
                "nearest-passing",
                explained
                    .and_then(|explained| explained.nearest_passing.as_ref())
                    .map_or(Json::Null, literal),
            ),
        ]));
    }
    Json::Array(listed)
}

/// Returns the JSON of a failure record, each value of its detail a typed
/// literal.
pub(crate) fn failure_json(failure: &Failure) -> Json {
    object([
        (
            "assertion",
            failure.assertion().map_or(Json::Null, Json::from),
        ),
        ("contract", failure.contract().into()),
        (
            "detail",
            Json::Object(
                failure
                    .detail()
                    .iter()
                    .map(|(name, value)| ((*name).to_owned(), value.to_literal()))
                    .collect(),
            ),
        ),
        (
            "where",
            failure.location().map_or(Json::Null, |at| {
                object([("file", at.base_name().into()), ("line", at.line().into())])
            }),
        ),
    ])
}

/// Returns how the detail states the failure of a failed execution: its
/// record, or the identity of a failure without a record.
fn failed_json(execution: &Execution) -> Json {
    let failed = execution
        .failure
        .as_ref()
        .expect("a failed case has its failure");
    match &failed.record {
        Some(record) => failure_json(record),
        None => failed.identity.as_str().into(),
    }
}

/// Returns the text of an identity in a sentence: the assertion and its
/// call site, a message and its call site, or a panic and its location.
pub(crate) fn identity_text(identity: &str) -> String {
    let Ok(Json::Array(parts)) = json::parse(identity, 2) else {
        return identity.to_owned();
    };
    let text = |at: usize| parts.get(at).and_then(Json::as_str);
    let at = match (text(3), parts.get(4).and_then(Json::as_int)) {
        (Some(file), Some(line)) => format!(" at {file}:{line}"),
        _ => String::new(),
    };
    if let Some(error) = text(2) {
        return format!("a {error}{at}");
    }
    match (text(0), text(1)) {
        (None, _) => format!("a message{at}"),
        (Some(assertion), Some(contract)) => format!("{assertion} of {contract:?}{at}"),
        (Some(assertion), None) => format!("{assertion}{at}"),
    }
}

/// Returns one side of a divergence in the detail.
fn side_json(side: &Version) -> Json {
    match side {
        Version::Bounds(bounds) => bounds_json(bounds),
        Version::Fingerprint(fingerprint) => plain(i128::from(*fingerprint)),
        Version::Identity(identity) => identity_text(identity).into(),
        Version::Nothing => Json::Null,
    }
}

/// Returns a divergence in the form of the detail.
fn divergence_json(divergence: &Divergence) -> Json {
    object([
        ("what", divergence.what.into()),
        ("index", divergence.index.into()),
        ("recorded", side_json(&divergence.recorded)),
        ("replayed", side_json(&divergence.replayed)),
        (
            "label",
            divergence.label.as_deref().map_or(Json::Null, Json::from),
        ),
        (
            "step",
            divergence.step.as_ref().map_or(Json::Null, |step| {
                object([
                    ("part", step.part.into()),
                    ("position", step.position.map_or(Json::Null, Json::from)),
                    (
                        "action",
                        step.action.as_deref().map_or(Json::Null, Json::from),
                    ),
                ])
            }),
        ),
    ])
}

/// Returns the detail of a run's outcome as JSON.
pub(crate) fn detail_json(outcome: &Outcome) -> Json {
    let concluded = outcome
        .failing
        .as_ref()
        .filter(|_| matches!(outcome.kind, Kind::Counterexample | Kind::Flaky));
    let counterexample = outcome.kind == Kind::Counterexample;
    object([
        ("outcome", outcome.kind.name().into()),
        ("cases", outcome.cases.into()),
        ("rejected", outcome.rejected.into()),
        ("seed", outcome.seed.to_string().into()),
        (
            "counterexample",
            concluded.map_or(Json::Null, |failing| {
                entries_json(failing, &outcome.explanation, false)
            }),
        ),
        ("failure", concluded.map_or(Json::Null, failed_json)),
        (
            "choices",
            outcome
                .token
                .as_deref()
                .filter(|_| counterexample)
                .map_or(Json::Null, Json::from),
        ),
        (
            "others",
            if counterexample {
                Json::Array(
                    outcome
                        .others
                        .iter()
                        .map(|other| {
                            object([
                                ("failure", failed_json(other)),
                                ("counterexample", entries_json(other, &[], true)),
                                ("choices", token::encode(&other.record.choices).into()),
                            ])
                        })
                        .collect(),
                )
            } else {
                Json::Null
            },
        ),
        (
            "divergence",
            outcome
                .divergence
                .as_ref()
                .map_or(Json::Null, divergence_json),
        ),
        (
            "coverage",
            outcome.shortfall.as_ref().map_or(Json::Null, |shortfall| {
                object([
                    ("label", shortfall.requirement.label.as_str().into()),
                    ("share", Json::Float(shortfall.requirement.share)),
                    ("counted", shortfall.counted.into()),
                    ("valid", shortfall.valid.into()),
                    ("verdict", shortfall.verdict.name().into()),
                ])
            }),
        ),
        (
            "reason",
            outcome.reason.as_deref().map_or(Json::Null, Json::from),
        ),
    ])
}

/// The names of the eleven fields of the detail, in order.
const FIELDS: [&str; 11] = [
    "outcome",
    "cases",
    "rejected",
    "seed",
    "counterexample",
    "failure",
    "choices",
    "others",
    "divergence",
    "coverage",
    "reason",
];

/// Returns the record of a run that did not pass, and its detail in the form
/// of a call record.
pub(crate) fn record_of(
    outcome: &Outcome,
    assertion: &'static str,
    contract: &str,
    at: Option<crate::Where>,
) -> (Failure, Json) {
    let json = detail_json(outcome);
    let detail = FIELDS
        .iter()
        .map(|name| {
            (
                *name,
                json.get(name).map_or(Value::Null, Value::from_detail),
            )
        })
        .collect();
    let mut failure = Failure::new(assertion, contract, detail, at);
    if let Some(record) = outcome
        .failing
        .as_ref()
        .and_then(|failing| failing.failure.as_ref())
        .and_then(|failed| failed.record.as_deref())
    {
        failure = failure.with_case(record.clone());
    }
    let sentence = sentence(outcome, contract);
    (failure.with_sentence(sentence), json)
}

/// Returns the line of the record of a failed case.
fn failure_line(what: &str, execution: &Execution) -> String {
    let failed = execution
        .failure
        .as_ref()
        .expect("a failed case has its failure");
    match &failed.record {
        Some(record) => {
            let of = record
                .assertion()
                .map(|assertion| format!(" of {assertion}"))
                .unwrap_or_default();
            let at = record
                .location()
                .map(|at| format!(" at {}:{}", at.base_name(), at.line()))
                .unwrap_or_default();
            format!("\n{what}{of}{at}: {record}")
        }
        None => format!("\n{what}: {}", identity_text(&failed.identity)),
    }
}

/// Returns the line of one entry of a counterexample.
fn entry_lines(execution: &Execution, explanation: &[Explained], out: &mut String) {
    for entry in entries(&execution.record) {
        match entry {
            Traced::Step(step) => {
                let client = step
                    .client
                    .map(|client| format!(" on client {client}"))
                    .unwrap_or_default();
                let drain = if step.drain { " in the drain" } else { "" };
                let _ = write!(out, "\n  step {}{client}{drain}", step.action);
            }
            Traced::Draw(number) => {
                let drawn = &execution.record.draws[number];
                let explained = explanation.get(number);
                let fails =
                    if explained.and_then(|explained| explained.any_value_fails) == Some(true) {
                        ", any value fails"
                    } else {
                        ""
                    };
                let passes = explained
                    .and_then(|explained| explained.nearest_passing.as_ref())
                    .map(|nearest| format!(", {} passes", nearest.text))
                    .unwrap_or_default();
                let _ = write!(
                    out,
                    "\n  {}: {}{fails}{passes}",
                    drawn.label, drawn.value.text
                );
            }
        }
    }
}

/// Returns the line that shows how to replay the case of a token.
fn replay_line(token: &str) -> String {
    format!("\nreplay: prop::replay({token:?}) or {REPLAY_VARIABLE}={token}")
}

/// Returns the sentence of the record of a failed run: the contract, the
/// outcome with the counts and the seed, a line for each part of the detail
/// that the outcome uses, and the notes of the failing case.
fn sentence(outcome: &Outcome, contract: &str) -> String {
    let mut text = format!(
        "{contract}: {} after {} valid and {} rejected cases, seed {}",
        outcome.kind.name(),
        outcome.cases,
        outcome.rejected,
        outcome.seed
    );
    if let Some(failing) = &outcome.failing {
        entry_lines(failing, &outcome.explanation, &mut text);
        text.push_str(&failure_line("failure", failing));
    }
    if outcome.kind == Kind::Counterexample {
        if let Some(token) = &outcome.token {
            text.push_str(&replay_line(token));
        }
        for other in &outcome.others {
            text.push_str(&failure_line("other failure", other));
            entry_lines(other, &[], &mut text);
            text.push_str(&replay_line(&token::encode(&other.record.choices)));
        }
    }
    if let Some(divergence) = &outcome.divergence {
        let label = divergence
            .label
            .as_ref()
            .map(|label| format!(" (in the draw {label:?})"))
            .unwrap_or_default();
        let side = |side: &Version| match side {
            Version::Bounds(bounds) => bounds_json(bounds).to_string(),
            Version::Fingerprint(fingerprint) => fingerprint.to_string(),
            Version::Identity(identity) => identity_text(identity),
            Version::Nothing => match divergence.what {
                "request" => "no request".to_owned(),
                "fingerprint" => "no fingerprint".to_owned(),
                _ => "no failure".to_owned(),
            },
        };
        let _ = write!(
            text,
            "\ndivergence: the {} at {}{label}, recorded {}, replayed {}",
            divergence.what,
            divergence.index,
            side(&divergence.recorded),
            side(&divergence.replayed)
        );
    }
    if let Some(shortfall) = &outcome.shortfall {
        let _ = write!(
            text,
            "\ncoverage: {}, {:?} counted {} of {} valid cases against a required share of {}",
            shortfall.verdict.name(),
            shortfall.requirement.label,
            shortfall.counted,
            shortfall.valid,
            shortfall.requirement.share
        );
    }
    if let Some(failing) = &outcome.failing {
        for note in &failing.record.notes {
            let _ = write!(text, "\nnote: {note}");
        }
    }
    text
}
