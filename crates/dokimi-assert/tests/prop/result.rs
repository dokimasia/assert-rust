// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `prop::result`, the result of a run of a property.

use crate::helpers::Results;
use dokimi_assert::prop::{self, Case, RunResult};
use dokimi_assert::{Recorder, Value, check};

/// The contract of every property of the spec.
const CONTRACT: &str = "the property holds";

/// Returns the one result of a run of `body` on `seat` with the seed 1 under
/// `options`.
fn result_of(
    seat: &Recorder,
    options: prop::Options,
    body: impl Fn(&Case<'_>) + Send + Sync,
) -> RunResult {
    let results = Results::default();
    let options = options
        .hermetic()
        .seed(1)
        .store("")
        .on_result(results.keep());
    prop::for_all(seat, CONTRACT, options, body);
    let mut kept = results.kept();
    assert_eq!(kept.len(), 1, "the run hands one result");
    kept.remove(0)
}

/// Returns the members of `result` as the fields of a detail. A member that
/// the outcome does not use is null, and the seed is its decimal text.
fn fields(result: &RunResult) -> Vec<(&'static str, Value)> {
    let value = |member: &Option<Value>| member.clone().unwrap_or(Value::Null);
    let text = |member: &Option<String>| member.clone().map_or(Value::Null, Value::String);
    vec![
        ("outcome", result.outcome.into()),
        ("cases", result.cases.into()),
        ("rejected", result.rejected.into()),
        ("seed", result.seed.to_string().into()),
        ("counterexample", value(&result.counterexample)),
        ("failure", value(&result.failure)),
        ("choices", text(&result.choices)),
        ("others", value(&result.others)),
        ("divergence", value(&result.divergence)),
        ("coverage", value(&result.coverage)),
        ("reason", text(&result.reason)),
    ]
}

mod run_result {
    use super::*;

    #[test]
    fn states_the_counts_and_the_seed_of_a_passing_run_alone() {
        let result = result_of(&Recorder::new(), prop::cases(10), |c| {
            _ = c.draw(&prop::integer(0..=99_u8), "n");
        });
        assert_eq!(
            (result.outcome, result.cases, result.rejected, result.seed),
            ("passed", 10, 0, 1)
        );
        assert_eq!(
            [
                &result.counterexample,
                &result.failure,
                &result.others,
                &result.divergence,
                &result.coverage,
            ],
            [&None; 5]
        );
        assert_eq!((&result.choices, &result.reason), (&None, &None));
    }

    #[test]
    fn states_the_fields_of_the_detail_of_a_failing_run() {
        let seat = Recorder::new();
        let result = result_of(&seat, prop::Options::default(), |c| {
            let n = c.draw(&prop::integer(0..=9_u8), "n");
            check::is_true(c, n < 3, "n stays below 3");
        });
        assert_eq!(result.outcome, "counterexample");
        assert_eq!(fields(&result), seat.failures()[0].detail());
    }

    #[test]
    fn states_the_requirement_that_a_run_leaves_unmet() {
        let seat = Recorder::new();
        let result = result_of(&seat, prop::require("never", 0.5), |c| {
            _ = c.draw(&prop::integer(0..=9_u8), "n");
        });
        assert_eq!(result.outcome, "coverage-unmet");
        assert_eq!(fields(&result), seat.failures()[0].detail());
    }

    #[test]
    fn states_the_reason_of_a_skipped_run() {
        let result = result_of(&Recorder::new(), prop::cases(10), |c| {
            _ = c.draw(&prop::integer(0..=9_u8), "n");
            c.skip("the case needs a capability");
        });
        assert_eq!(
            (result.outcome, result.reason.as_deref()),
            ("skipped", Some("the case needs a capability"))
        );
    }
}
