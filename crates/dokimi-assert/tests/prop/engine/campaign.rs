//! The spec of `prop::engine::campaign`, the run of the campaign profile.
//!
//! A campaign runs for its budget on the platform clock, so each case runs a
//! campaign of one second in a child process with the campaign profile.

use crate::helpers::{Scratch, in_child, records};
use dokimi_assert::prop::{self, Case};
use dokimi_assert::{Recorder, check};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// The variables of a campaign of one second.
const CAMPAIGN: [(&str, &str); 2] = [
    ("DOKIMI_ASSERT_PROP_PROFILE", "campaign"),
    ("DOKIMI_ASSERT_PROP_BUDGET", "1"),
];

/// Runs a campaign of `body` under `options` with the seed 1, and prints its
/// record.
fn print_campaign(options: prop::Options, body: impl Fn(&Case<'_>) + Send + Sync) {
    let seat = Recorder::new();
    prop::for_all(&seat, "the campaign holds", options.seed(1).store(""), body);
    println!("record {}", records(&seat)[0]);
}

/// Runs the child test `test` with the campaign profile, and returns the
/// record that it printed.
fn campaign(test: &str) -> serde_json::Value {
    let dir = Scratch::new();
    let output = in_child(
        &format!("prop::engine::campaign::child::{test}"),
        &dir.join(""),
        &CAMPAIGN,
    );
    let printed = String::from_utf8_lossy(&output.stdout).into_owned();
    printed
        .lines()
        .find_map(|line| line.split_once("record ").map(|(_, json)| json.to_owned()))
        .map_or_else(
            || panic!("the child prints its record: {printed}"),
            |json| serde_json::from_str(&json).expect("the record is JSON"),
        )
}

/// Returns a fingerprint of a text: its length and its first character.
fn fingerprint(text: &str) -> u64 {
    let first = text.chars().next().map_or(0, u64::from);
    (text.len() as u64) * 1000 + first
}

mod child {
    use super::*;

    #[test]
    #[ignore = "a spec of the campaign runs it in a child process"]
    fn explores_from_a_pool_of_new_behaviour() {
        print_campaign(prop::Options::default(), |c| {
            let text = prop::one_of([
                prop::integer(0..=1000_i64).map(|n| n.to_string()),
                prop::string(..=4),
            ]);
            let value = c.draw(&text, "value");
            let extra = c.draw(&prop::list(prop::boolean(), ..=3), "extra");
            c.observe(fingerprint(&value));
            c.classify(if extra.is_empty() { "plain" } else { "extra" });
            c.target(
                "length",
                f64::from(u16::try_from(value.len()).unwrap_or(u16::MAX)),
            );
        });
    }

    #[test]
    #[ignore = "a spec of the campaign runs it in a child process"]
    fn explores_cases_without_spans() {
        print_campaign(prop::Options::default(), |c| {
            let value = c.rand().next_u64();
            c.observe(value % 1000);
        });
    }

    #[test]
    #[ignore = "a spec of the campaign runs it in a child process"]
    fn concludes_each_failure_of_a_new_identity() {
        let entries = r#"[{"label": "n", "value": {"type": "int", "value": 90}}]"#;
        print_campaign(prop::draws(entries), |c| {
            let n = c.draw(&prop::integer(0..=100_u8), "n");
            c.observe(u64::from(n));
            check::is_true(c, n != 33, "n is not 33");
            check::is_true(c, n < 80, "n stays below 80");
        });
    }

    #[test]
    #[ignore = "a spec of the campaign runs it in a child process"]
    fn concludes_the_failures_that_the_shrink_of_one_meets() {
        let entries = r#"[{"label": "n", "value": {"type": "int", "value": 5}},
            {"label": "m", "value": {"type": "int", "value": 5}},
            {"label": "k", "value": {"type": "int", "value": 5}}]"#;
        print_campaign(prop::draws(entries), |c| {
            let n = c.draw(&prop::integer(0..=9_u8), "n");
            let m = c.draw(&prop::integer(0..=9_u8), "m");
            let k = c.draw(&prop::integer(0..=9_u8), "k");
            check::is_true(c, n == 0 || m == 0 || k == 0, "one value is 0");
            check::is_true(c, m == 0 || k == 0, "m or k is 0");
            check::is_true(c, k == 0, "k is 0");
        });
    }

    #[test]
    #[ignore = "a spec of the campaign runs it in a child process"]
    fn ends_with_the_fault_of_a_random_case() {
        print_campaign(prop::around(|_| panic!("the wrapper fails")), |c| {
            let _ = c.draw(&prop::boolean(), "flag");
        });
    }

    #[test]
    #[ignore = "a spec of the campaign runs it in a child process"]
    fn ends_as_flaky_at_a_replay_that_differs() {
        let calls = AtomicU64::new(0);
        print_campaign(prop::Options::default(), move |c| {
            let _ = c.draw(&prop::boolean(), "flag");
            let first = calls.fetch_add(1, Ordering::SeqCst) == 0;
            check::is_true(c, !first, "only the first call fails");
        });
    }

    #[test]
    #[ignore = "a spec of the campaign runs it in a child process"]
    fn fails_as_vacuous_when_no_case_requests_an_input() {
        let score = AtomicU32::new(0);
        print_campaign(prop::Options::default(), move |c| {
            c.target("calls", f64::from(score.fetch_add(1, Ordering::SeqCst)));
        });
    }

    #[test]
    #[ignore = "a spec of the campaign runs it in a child process"]
    fn fails_a_requirement_that_the_cases_leave_unmet() {
        print_campaign(prop::require("never", 0.5), |c| {
            let _ = c.draw(&prop::boolean(), "flag");
        });
    }

    #[test]
    #[ignore = "a spec of the campaign runs it in a child process"]
    fn replays_a_stored_case_before_it_explores() {
        let entry = r#"{"store": 1, "definition": "9.0.0", "property": "the campaign holds",
            "identity": {"assertion": "true", "file": "campaign.rs", "line": 1},
            "choices": "prop1:AAE", "counterexample": [{"label": "n", "value": {"type": "int", "value": 1}}],
            "found": "2026-01-01"}"#;
        std::fs::create_dir_all("store").expect("the scratch accepts a directory");
        std::fs::write("store/entry.json", entry).expect("the store accepts a file");
        let seat = Recorder::new();
        prop::for_all(
            &seat,
            "the campaign holds",
            prop::seed(1).store("store"),
            |c| {
                let _ = c.draw(&prop::integer(0..=9_u8), "n");
            },
        );
        println!("record {}", records(&seat)[0]);
    }

    #[test]
    #[ignore = "a spec of the campaign runs it in a child process"]
    fn ends_as_flaky_at_a_known_case_whose_replay_passes() {
        let calls = AtomicU64::new(0);
        let entries = r#"[{"label": "flag", "value": {"type": "bool", "value": true}}]"#;
        print_campaign(prop::draws(entries), move |c| {
            let _ = c.draw(&prop::boolean(), "flag");
            let first = calls.fetch_add(1, Ordering::SeqCst) == 0;
            check::is_true(c, !first, "only the first call fails");
        });
    }

    #[test]
    #[ignore = "a spec of the campaign runs it in a child process"]
    fn concludes_a_failure_of_an_example_of_values() {
        let seat = Recorder::new();
        // A generator that map returns has no inverse, so the example keeps its value.
        let generator = prop::integer(0..=255_u8).map(|n| n);
        let options = prop::examples([200_u8]).using(&generator).seed(1).store("");
        prop::is_true(&seat, |n: &u8| *n < 100, "n stays below 100", options);
        println!("record {}", records(&seat)[0]);
    }
}

mod campaign {
    use super::*;

    #[test]
    fn passes_a_property_that_no_case_fails() {
        let record = campaign("explores_from_a_pool_of_new_behaviour");
        assert_eq!(record["verdict"], "pass", "{record}");
        let record = campaign("explores_cases_without_spans");
        assert_eq!(record["verdict"], "pass", "{record}");
    }

    #[test]
    fn reports_the_first_failure_with_every_later_one_among_its_others() {
        let record = campaign("concludes_each_failure_of_a_new_identity");
        let detail = &record["detail"];
        assert_eq!(detail["outcome"], "counterexample", "{record}");
        assert_eq!(
            detail["counterexample"][0]["value"]["value"], 80,
            "{record}"
        );
        let others = detail["others"]
            .as_array()
            .expect("the detail lists the others");
        assert!(
            others
                .iter()
                .any(|other| other["counterexample"][0]["value"]["value"] == 33),
            "{others:?}"
        );
    }

    #[test]
    fn reports_each_failure_that_the_shrink_of_another_meets_among_its_others() {
        let record = campaign("concludes_the_failures_that_the_shrink_of_one_meets");
        assert_eq!(record["detail"]["outcome"], "counterexample", "{record}");
        assert_eq!(
            record["detail"]["others"].as_array().map(Vec::len),
            Some(2),
            "{record}"
        );
    }

    #[test]
    fn ends_with_the_fault_of_a_case_of_its_exploration() {
        let record = campaign("ends_with_the_fault_of_a_random_case");
        assert_eq!(record["verdict"], "error", "{record}");
        let error = record["error"].as_str().unwrap_or_default();
        assert!(
            error.ends_with("around: the wrapper panicked: the wrapper fails"),
            "{record}"
        );
    }

    #[test]
    fn ends_as_flaky_when_the_replay_of_a_failure_passes() {
        let record = campaign("ends_as_flaky_at_a_replay_that_differs");
        assert_eq!(record["detail"]["outcome"], "flaky", "{record}");
        let record = campaign("ends_as_flaky_at_a_known_case_whose_replay_passes");
        assert_eq!(record["detail"]["outcome"], "flaky", "{record}");
    }

    #[test]
    fn replays_the_stored_cases_among_the_known_cases() {
        let record = campaign("replays_a_stored_case_before_it_explores");
        assert_eq!(record["verdict"], "pass", "{record}");
    }

    #[test]
    fn fails_as_a_run_without_a_counterexample_fails() {
        let record = campaign("fails_as_vacuous_when_no_case_requests_an_input");
        assert_eq!(record["detail"]["outcome"], "vacuous", "{record}");
        let record = campaign("fails_a_requirement_that_the_cases_leave_unmet");
        assert_eq!(record["detail"]["outcome"], "coverage-unmet", "{record}");
    }

    #[test]
    fn reports_a_failing_example_of_values_as_found() {
        let record = campaign("concludes_a_failure_of_an_example_of_values");
        let detail = &record["detail"];
        assert_eq!(detail["outcome"], "counterexample", "{record}");
        assert_eq!(
            detail["counterexample"][0]["value"]["value"], 200,
            "{record}"
        );
        assert_eq!(
            detail["choices"],
            serde_json::Value::Null,
            "an example of values has no token"
        );
    }
}
