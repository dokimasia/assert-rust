//! The spec of `stateful::vectors`, which runs the definition's machines
//! vectors.

use dokimi_assert_conformance::stateful::vectors;
use dokimi_assert_conformance::vector::{self, Vector};
use serde_json::json;

/// Returns the vector `id` of the definition.
fn vector(id: &str) -> Vector {
    vector::vectors("stateful")
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
        let all = vector::vectors("stateful").expect("the vendored vectors read");
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
        let mut failing = vector("counter-overflows/a-third-increment-without-a-reset-returns-0");
        failing.raw["detail"]["cases"] = json!(2);
        let disagreement = vectors::run(&failing).expect_err("the cases differ");
        assert_eq!(disagreement.at, "detail");
    }

    #[test]
    fn disagrees_with_a_refusal_of_another_entry() {
        let mut refused = vector("store-loses-on-crash/a-flush-of-an-empty-buffer-is-refused");
        refused.raw["error"]["entry"] = json!(0);
        let disagreement = vectors::run(&refused).expect_err("the run refuses entry 1");
        assert_eq!(disagreement.at, "error");
    }

    #[test]
    fn disagrees_with_a_refusal_of_a_run_that_ends_with_a_detail() {
        let mut failing = vector("counter-overflows/a-third-increment-without-a-reset-returns-0");
        failing.raw["error"] = json!({"entry": 0, "name": "increment", "reason": "step"});
        let disagreement = vectors::run(&failing).expect_err("the run reports a counterexample");
        assert_eq!(disagreement.at, "error");
    }

    #[test]
    fn disagrees_with_a_detail_of_a_run_that_ends_with_a_fault() {
        let mut refused = vector("store-loses-on-crash/a-flush-of-an-empty-buffer-is-refused");
        refused.raw["error"] = json!(null);
        let disagreement = vectors::run(&refused).expect_err("the run ends with a fault");
        assert_eq!(disagreement.at, "");
        assert!(
            disagreement
                .reason
                .starts_with("the run ends with \"prop::for_all: draws[1].step: ")
        );
    }

    #[test]
    fn refuses_a_subject_that_the_definition_does_not_state() {
        let mut made_up = vector("correct-queue/passes-with-swarm");
        made_up.raw["subject"] = json!("made-up");
        let disagreement = vectors::run(&made_up).expect_err("no subject has the name");
        assert_eq!(disagreement.at, "subject");
    }

    #[test]
    fn refuses_a_setup_that_states_no_count() {
        let tests = [
            ("mean", json!(0), "setup.mean"),
            ("max", json!("two"), "setup.max"),
            ("swarm", json!(1), "setup.swarm"),
            ("clients", json!(0), "setup.clients"),
            ("concurrent", json!(-1), "setup.concurrent"),
            ("strategy", json!({"pct": 0}), "setup.strategy"),
        ];
        for (give_member, give_value, want_at) in tests {
            let mut stated = vector("correct-queue/passes-with-swarm");
            stated.raw["setup"] = json!({ give_member: give_value });
            let disagreement = vectors::run(&stated).expect_err("the setup states no option");
            assert_eq!(disagreement.at, want_at);
        }
    }

    #[test]
    fn refuses_a_setting_that_the_runner_does_not_take() {
        let tests = [
            (json!({"seed": 1}), "settings.seed"),
            (json!({"cases": 0}), "settings.cases"),
            (json!({"workers": 2}), "settings.workers"),
        ];
        for (give_settings, want_at) in tests {
            let mut stated = vector("correct-queue/passes-with-swarm");
            stated.raw["settings"] = give_settings;
            let disagreement = vectors::run(&stated).expect_err("the runner refuses the setting");
            assert_eq!(disagreement.at, want_at);
        }
    }

    #[test]
    fn runs_the_setup_of_a_concurrent_section_under_pct() {
        let mut stated = vector("correct-counter/passes-with-two-clients");
        stated.raw["setup"] =
            json!({"clients": 3, "concurrent": 4, "mean": 5, "strategy": {"pct": 2}});
        stated.raw["settings"]["cases"] = json!(20);
        stated.raw["detail"]["cases"] = json!(20);
        vectors::run(&stated).expect("a correct counter passes under every schedule");
    }

    #[test]
    fn disagrees_with_a_kind_that_the_runner_does_not_run() {
        let mut made_up = vector("correct-queue/passes-with-swarm");
        made_up.kind = "made-up".to_owned();
        let disagreement = vectors::run(&made_up).expect_err("no runner runs the kind");
        assert_eq!(
            disagreement.to_string(),
            r#"the runner runs no vector of the kind "made-up""#
        );
    }
}
