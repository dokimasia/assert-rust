// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The result of a run of a property, which the function of
//! [`on_result`](crate::prop::on_result) receives.

use super::engine::runner::Outcome;
use crate::json::Json;
use crate::value::Value;

/// The result of one run of a property: the eleven fields of the detail of
/// the run. [`on_result`](crate::prop::on_result) hands it to a function when
/// the run ends.
///
/// A member that the outcome does not use is `None`. Every other member has
/// the value that [`Failure::field`](crate::Failure::field) returns for the
/// field of its name in the record of the run. The seed is the value that
/// [`seed`](crate::prop::seed) takes, and the record states it as decimal
/// text.
///
/// ```
/// use dokimi_assert::{Collector, prop};
/// use std::sync::{Arc, Mutex};
///
/// let counts = Arc::new(Mutex::new(Vec::new()));
/// let kept = Arc::clone(&counts);
/// let options = prop::on_result(move |result| {
///     kept.lock().expect("no function panicked").push((result.cases, result.seed));
/// });
/// let t = Collector::new();
/// prop::for_all(&t, "a byte has 8 bits", options.cases(20).seed(7), |c| {
///     let _ = c.draw(&prop::integer(0..=255_u8), "byte");
/// });
/// assert_eq!(*counts.lock().expect("no function panicked"), [(20, 7)]);
/// ```
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct RunResult {
    /// How the run ended, such as `passed` or `counterexample`.
    pub outcome: &'static str,
    /// The number of valid cases.
    pub cases: usize,
    /// The number of rejected cases.
    pub rejected: usize,
    /// The seed of the run.
    pub seed: u64,
    /// The draws and the steps of the failing case of a counterexample or a
    /// flaky run, in request order.
    pub counterexample: Option<Value>,
    /// The failure record of the failing case.
    pub failure: Option<Value>,
    /// The replay token of a counterexample, which
    /// [`replay`](crate::prop::replay) takes. A counterexample of values has
    /// none.
    pub choices: Option<String>,
    /// The other failures of a counterexample.
    pub others: Option<Value>,
    /// What differed first in a flaky run.
    pub divergence: Option<Value>,
    /// The coverage requirement that the run refuted or left unmet.
    pub coverage: Option<Value>,
    /// The reason of a skipped run.
    pub reason: Option<String>,
}

/// Returns the value of the field `name` of a detail, or `None` for a field
/// that the outcome does not use.
fn member(detail: &Json, name: &str) -> Option<Value> {
    detail
        .get(name)
        .filter(|value| **value != Json::Null)
        .map(Value::from_detail)
}

/// Returns the result of a run from its outcome and the detail that
/// [`detail_json`](super::detail::detail_json) returns for the outcome.
pub(crate) fn result_of(outcome: &Outcome, detail: &Json) -> RunResult {
    let text = |name| detail.get(name).and_then(Json::as_str).map(str::to_owned);
    RunResult {
        outcome: outcome.kind.name(),
        cases: outcome.cases,
        rejected: outcome.rejected,
        seed: outcome.seed,
        counterexample: member(detail, "counterexample"),
        failure: member(detail, "failure"),
        choices: text("choices"),
        others: member(detail, "others"),
        divergence: member(detail, "divergence"),
        coverage: member(detail, "coverage"),
        reason: text("reason"),
    }
}
