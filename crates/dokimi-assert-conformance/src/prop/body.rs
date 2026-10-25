// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The bodies of the property vectors, stated as data.
//!
//! A body spec draws one value, labelled `value`, from its `draw` generator.
//! It then classifies the case under each `classify` label whose predicate
//! is true, rejects the case when `rejects-when` is true, and skips the run
//! with the reason of the first `skips` entry whose predicate is true. The
//! identity of its failure is the identity of the first `fails` entry whose
//! predicate is true. An [`Ending`] then ends the case with that identity, or
//! with `None` for a case that does not fail.
//!
//! Four named bodies state what no predicate can:
//!
//! - `draws-nothing` requests no input.
//! - `diverges` draws an integer in [0, 9] on its first call and a boolean on
//!   every later call.
//! - `fails-once` draws an integer in [0, 10^9]. Its case fails with the
//!   identity `once` the first time that it draws a value above 1,000.
//! - `fails-then-skips` draws an integer in [0, 10^9]. Its case fails with
//!   the identity `once` the first time that it draws a value above 1,000.
//!   Every later call draws its value and then skips with the reason `after`.

use super::generator::{SpecError, build};
use super::predicate::{Predicate, predicate};
use dokimi_assert::Value;
use dokimi_assert::prop::Case;
use serde_json::{Value as Json, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// The label of a body's one draw.
pub const DRAWN: &str = "value";

/// A body of a vector.
pub type Body = Arc<dyn Fn(&Case<'_>) + Send + Sync>;

/// Ends the case of a body that ran to its end. It takes the identity of the
/// case's failure, or `None` for a case that does not fail.
pub type Ending = fn(&Case<'_>, Option<&str>);

/// Ends a case that fails with its identity, and lets any other case pass.
pub fn failing(c: &Case<'_>, failure: Option<&str>) {
    if let Some(identity) = failure {
        c.fail_as(identity, true);
    }
}

/// Returns a fresh body for a spec, which ends each case with `end`. A
/// named body keeps its own state.
///
/// # Errors
///
/// Returns [`SpecError`] for a spec that names no body, or misstates an
/// entry, a predicate or the generator of its draw.
pub fn body(spec: &Json, end: Ending) -> Result<Body, SpecError> {
    match spec["kind"].as_str() {
        Some("draws-nothing") => Ok(Arc::new(move |c| end(c, None))),
        Some("diverges") => {
            let drawn = AtomicBool::new(false);
            let digit = build(&json!({"gen": "integer", "min": 0, "max": 9}), None)?;
            let coin = build(&json!({"gen": "boolean"}), None)?;
            Ok(Arc::new(move |c| {
                let generator = if drawn.load(Ordering::SeqCst) {
                    &coin
                } else {
                    &digit
                };
                c.draw(generator, DRAWN);
                drawn.store(true, Ordering::SeqCst);
                end(c, None);
            }))
        }
        Some("fails-once") => {
            let failed = AtomicBool::new(false);
            let wide = build(&wide(), None)?;
            Ok(Arc::new(move |c| {
                let above = matches!(c.draw(&wide, DRAWN), Value::Int(held) if held > 1000);
                let once = above && !failed.swap(true, Ordering::SeqCst);
                end(c, once.then_some("once"));
            }))
        }
        Some("fails-then-skips") => {
            let failed = AtomicBool::new(false);
            let wide = build(&wide(), None)?;
            Ok(Arc::new(move |c| {
                let above = matches!(c.draw(&wide, DRAWN), Value::Int(held) if held > 1000);
                if failed.load(Ordering::SeqCst) {
                    c.skip("after");
                }
                if above {
                    failed.store(true, Ordering::SeqCst);
                }
                end(c, above.then_some("once"));
            }))
        }
        Some(other) => Err(SpecError(format!("prop: {other:?} names no body"))),
        None => drawing(spec, end),
    }
}

/// Returns the generator of the integer that `fails-once` and
/// `fails-then-skips` draw, in [0, 10^9].
fn wide() -> Json {
    json!({"gen": "integer", "min": 0, "max": 1_000_000_000})
}

/// Returns a body that draws once, then classifies, rejects, skips and ends.
fn drawing(spec: &Json, end: Ending) -> Result<Body, SpecError> {
    let generator = build(&spec["draw"], None)?;
    let mut labels: Vec<(String, Predicate)> = Vec::new();
    if let Some(classify) = spec.get("classify") {
        let members = classify
            .as_object()
            .ok_or_else(|| SpecError("prop: classify is not an object".to_owned()))?;
        for (label, when) in members {
            labels.push((label.clone(), predicate(when)?));
        }
    }
    let rejects = spec.get("rejects-when").map(predicate).transpose()?;
    let skips = entries(spec, "skips", "reason")?;
    let fails = entries(spec, "fails", "identity")?;
    Ok(Arc::new(move |c| {
        let value = c.draw(&generator, DRAWN);
        for (label, holds) in &labels {
            if holds(&value) {
                c.classify(label);
            }
        }
        if let Some(rejects) = &rejects {
            c.assume(!rejects(&value));
        }
        if let Some((reason, _)) = skips.iter().find(|(_, holds)| holds(&value)) {
            c.skip(reason);
        }
        let failure = fails
            .iter()
            .find(|(_, holds)| holds(&value))
            .map(|(identity, _)| identity.as_str());
        end(c, failure);
    }))
}

/// Returns the entries of the list `key` of a spec, each with its string
/// under `name` and its predicate under `when`.
fn entries(spec: &Json, key: &str, name: &str) -> Result<Vec<(String, Predicate)>, SpecError> {
    let mut read = Vec::new();
    for entry in spec
        .get(key)
        .and_then(Json::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        let text = entry[name]
            .as_str()
            .ok_or_else(|| SpecError(format!("prop: a {key} entry states no {name}")))?;
        read.push((text.to_owned(), predicate(&entry["when"])?));
    }
    Ok(read)
}

/// Returns a body that draws once from `generator` and fails with the
/// identity `fails-when` when `when` is true, as a shrinking vector states.
///
/// # Errors
///
/// Returns [`SpecError`] for a generator or a predicate that does not read.
pub fn failing_when(generator: &Json, when: &Json) -> Result<Body, SpecError> {
    body(
        &json!({
            "draw": generator,
            "fails": [{"identity": "fails-when", "when": when}],
        }),
        failing,
    )
}
