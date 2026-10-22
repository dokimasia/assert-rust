// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `history::vectors`, which runs the definition's history
//! vectors.

use dokimi_assert_conformance::history::vectors;
use dokimi_assert_conformance::vector::{self, Vector};
use serde_json::json;

/// Returns the vector `id` of the definition.
fn vector(id: &str) -> Vector {
    vector::vectors("history")
        .expect("the vendored vectors read")
        .into_iter()
        .find(|vector| vector.id == id)
        .expect("the definition states the vector")
}

mod run {
    use super::*;

    #[test]
    fn agrees_with_every_vector_of_the_definition() {
        let mut disagreements = Vec::new();
        let all = vector::vectors("history").expect("the vendored vectors read");
        for vector in &all {
            if let Err(disagreement) = vectors::run(vector) {
                disagreements.push(format!("{}: {disagreement}", vector.id));
            }
        }
        assert_ne!(all.len(), 0);
        assert!(
            disagreements.is_empty(),
            "{} of {} vectors disagree:\n{}",
            disagreements.len(),
            all.len(),
            disagreements.join("\n")
        );
    }

    #[test]
    fn disagrees_with_a_detail_of_another_value() {
        let mut violated = vector("linearizable/a-read-after-a-completed-write-misses-it");
        violated.raw["detail"]["steps"] = json!(3);
        let disagreement = vectors::run(&violated).expect_err("the steps differ");
        assert_eq!(disagreement.at, "detail.steps");
    }

    #[test]
    fn disagrees_with_the_steps_of_a_pass() {
        let mut passed =
            vector("linearizable/concurrent-writes-take-effect-in-the-order-a-read-needs");
        let steps = passed.raw["detail"]["steps"]
            .as_u64()
            .expect("the vector states its steps");
        passed.raw["detail"]["steps"] = json!(steps + 1);
        let disagreement = vectors::run(&passed).expect_err("the steps differ");
        assert_eq!(disagreement.at, "detail.steps");
    }

    #[test]
    fn disagrees_with_an_event_of_another_value() {
        let mut seam = vector("history/a-client-continues-on-a-new-process-after-unknown");
        seam.raw["events"][2]["process"] = json!(0);
        let disagreement = vectors::run(&seam).expect_err("the process differs");
        assert_eq!(disagreement.at, "events[2]");
    }

    #[test]
    fn disagrees_with_a_refusal_that_the_seam_does_not_make() {
        let mut seam = vector("history/refuses-a-second-completion-of-one-call");
        seam.raw["refused"] = json!(2);
        let disagreement = vectors::run(&seam).expect_err("the seam refuses entry 3");
        assert_eq!(disagreement.at, "refused");
    }

    #[test]
    fn disagrees_with_the_verdict_of_an_isolation_check() {
        let mut isolation =
            vector("serializable/a-read-returns-a-value-that-no-transaction-appended");
        isolation.raw["expect"] = json!("pass");
        let disagreement = vectors::run(&isolation).expect_err("the check fails");
        assert_eq!(disagreement.at, "expect");
    }

    #[test]
    fn disagrees_with_a_kind_that_the_runner_does_not_run() {
        let mut made_up = vector("history/a-client-continues-on-a-new-process-after-unknown");
        made_up.kind = "made-up".to_owned();
        let disagreement = vectors::run(&made_up).expect_err("no runner runs the kind");
        assert_eq!(
            disagreement.to_string(),
            r#"the runner runs no vector of the kind "made-up""#
        );
    }
}
