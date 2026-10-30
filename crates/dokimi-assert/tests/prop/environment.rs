// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `prop::environment`, the variables that set the defaults of
//! every property of a run. Each case runs a property in a child process with
//! the variables of the case.

use crate::helpers::{Scratch, in_child, records};
use dokimi_assert::{Recorder, check, prop};

/// The test of the child process, which runs a property that passes.
const PROPERTY: &str = "prop::environment::child::prints_the_record_of_a_property";

/// The test of the child process, which runs a hermetic property.
const HERMETIC: &str = "prop::environment::child::prints_the_record_of_a_hermetic_property";

/// The test of the child process, which prints two seeds of `run_seed`, the
/// fault of the first call, and the record of a property under the first
/// seed.
const RUN_SEED: &str = "prop::environment::child::prints_two_run_seeds_and_a_property_under_one";

/// Runs `test` in a child process with `vars`, and returns what it printed.
fn printed(test: &str, vars: &[(&str, &str)]) -> String {
    let dir = Scratch::new();
    let output = in_child(test, &dir.join(""), vars);
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Returns the text after `label` on the first line of `printed` that
/// contains it.
fn after(printed: &str, label: &str) -> Option<String> {
    printed
        .lines()
        .find_map(|line| line.split_once(label).map(|(_, rest)| rest.to_owned()))
}

/// Runs `test` in a child process with `vars`, and returns the call record
/// and the message that it printed.
fn child(test: &str, vars: &[(&str, &str)]) -> (serde_json::Value, String) {
    let printed = printed(test, vars);
    let record = after(&printed, "record ").map_or_else(
        || panic!("the child prints its record: {printed}"),
        |json| serde_json::from_str(&json).expect("the record is JSON"),
    );
    (record, after(&printed, "message ").unwrap_or_default())
}

/// Runs a property on a recorder with `options` and prints its record and
/// its message.
fn print_run(options: prop::Options) {
    let seat = Recorder::new();
    prop::for_all(
        &seat,
        "the property of the environment",
        options.store(""),
        |c| {
            let n = c.draw(&prop::integer(0..=9_u8), "n");
            check::is_true(c, n < 100, "n stays below 100");
        },
    );
    println!("record {}", records(&seat)[0]);
    println!("message {}", seat.message());
}

mod child {
    use super::*;

    #[test]
    #[ignore = "a spec of the environment runs it in a child process"]
    fn prints_the_record_of_a_property() {
        print_run(prop::Options::default());
    }

    #[test]
    #[ignore = "a spec of the environment runs it in a child process"]
    fn prints_the_record_of_a_hermetic_property() {
        print_run(prop::hermetic().seed(5));
    }

    #[test]
    #[ignore = "a spec of the environment runs it in a child process"]
    fn prints_two_run_seeds_and_a_property_under_one() {
        let seat = Recorder::new();
        let first = prop::run_seed(&seat, "the property of the environment");
        let second = prop::run_seed(&seat, "the property of the environment");
        println!("seeds {first} {second}");
        println!("fault {}", seat.message());
        print_run(prop::seed(first));
    }
}

mod run_seed {
    use super::*;

    #[test]
    fn returns_the_seed_of_the_variable_under_any_profile() {
        let vars = [
            ("DOKIMI_ASSERT_PROP_SEED", "42"),
            ("DOKIMI_ASSERT_PROP_PROFILE", "ci"),
        ];
        assert_eq!(
            after(&printed(RUN_SEED, &vars), "seeds ").as_deref(),
            Some("42 42")
        );
    }

    #[test]
    fn derives_the_seed_from_the_contract_under_the_ci_profile_and_in_a_mutation_run() {
        let ci = [
            ("DOKIMI_ASSERT_PROP_SEED", ""),
            ("DOKIMI_ASSERT_PROP_PROFILE", "ci"),
        ];
        let mutated = [
            ("DOKIMI_ASSERT_PROP_SEED", ""),
            ("DOKIMI_ASSERT_PROP_PROFILE", ""),
            ("DOKIMI_MUTATE_MUTANT", "1"),
        ];
        let (record, _) = child(PROPERTY, &ci);
        let seed = record["detail"]["seed"]
            .as_str()
            .expect("the record states its seed");
        let want = format!("{seed} {seed}");
        assert_eq!(after(&printed(RUN_SEED, &ci), "seeds "), Some(want.clone()));
        assert_eq!(after(&printed(RUN_SEED, &mutated), "seeds "), Some(want));
    }

    #[test]
    fn draws_a_new_random_seed_for_each_call() {
        let vars = [
            ("DOKIMI_ASSERT_PROP_SEED", ""),
            ("DOKIMI_ASSERT_PROP_PROFILE", ""),
        ];
        let seeds = after(&printed(RUN_SEED, &vars), "seeds ").expect("the child prints the seeds");
        let (first, second) = seeds.split_once(' ').expect("the child prints two seeds");
        assert_ne!(first, second);
    }

    #[test]
    fn ends_the_call_with_a_fault_and_returns_0() {
        let tests = [
            (
                ("x", ""),
                r#"prop::run_seed: DOKIMI_ASSERT_PROP_SEED: "x" is not a decimal number below 2^64"#,
            ),
            (
                ("", "nightly"),
                r#"prop::run_seed: DOKIMI_ASSERT_PROP_PROFILE: "nightly" is not one of the profiles default, ci and campaign"#,
            ),
        ];
        for ((give_seed, give_profile), want) in tests {
            let vars = [
                ("DOKIMI_ASSERT_PROP_SEED", give_seed),
                ("DOKIMI_ASSERT_PROP_PROFILE", give_profile),
            ];
            let printed = printed(RUN_SEED, &vars);
            assert_eq!(after(&printed, "seeds ").as_deref(), Some("0 0"), "{want}");
            assert_eq!(after(&printed, "fault ").as_deref(), Some(want));
        }
    }

    #[test]
    fn gives_a_run_the_seed_of_a_run_without_a_seed_option() {
        let tests = [("42", ""), ("", "ci")];
        for (give_seed, give_profile) in tests {
            let vars = [
                ("DOKIMI_ASSERT_PROP_SEED", give_seed),
                ("DOKIMI_ASSERT_PROP_PROFILE", give_profile),
            ];
            let (without, _) = child(PROPERTY, &vars);
            let (under, _) = child(RUN_SEED, &vars);
            assert_eq!(
                under["detail"]["seed"], without["detail"]["seed"],
                "{give_seed} {give_profile}"
            );
        }
    }
}

mod seed {
    use super::*;

    #[test]
    fn takes_the_seed_of_the_variable() {
        let tests = [
            ("42", "42"),
            ("007", "7"),
            ("0", "0"),
            ("18446744073709551615", "18446744073709551615"),
        ];
        for (give, want) in tests {
            let (record, _) = child(PROPERTY, &[("DOKIMI_ASSERT_PROP_SEED", give)]);
            assert_eq!(record["detail"]["seed"], want, "{give}");
        }
    }

    #[test]
    fn ends_the_call_with_a_fault_at_a_seed_that_is_no_decimal_number_below_2_64() {
        for give in ["x", "-1", "18446744073709551616"] {
            let (record, message) = child(PROPERTY, &[("DOKIMI_ASSERT_PROP_SEED", give)]);
            assert_eq!(record["verdict"], "error");
            assert_eq!(
                message,
                format!(
                    "prop::for_all: DOKIMI_ASSERT_PROP_SEED: {give:?} is not a decimal number below 2^64"
                )
            );
        }
    }

    #[test]
    fn derives_the_seed_of_the_ci_profile_from_the_contract() {
        let (first, _) = child(PROPERTY, &[("DOKIMI_ASSERT_PROP_PROFILE", "ci")]);
        let (second, _) = child(PROPERTY, &[("DOKIMI_ASSERT_PROP_PROFILE", "ci")]);
        assert_eq!(first["detail"]["seed"], second["detail"]["seed"]);
        let (mutated, _) = child(PROPERTY, &[("DOKIMI_MUTATE_MUTANT", "1")]);
        assert_eq!(mutated["detail"]["seed"], first["detail"]["seed"]);
    }

    #[test]
    fn reads_no_variable_for_a_hermetic_run() {
        let (record, _) = child(
            HERMETIC,
            &[
                ("DOKIMI_ASSERT_PROP_SEED", "x"),
                ("DOKIMI_ASSERT_PROP_PROFILE", "x"),
            ],
        );
        assert_eq!(record["detail"]["seed"], "5");
    }
}

mod profile {
    use super::*;

    #[test]
    fn ends_the_call_with_a_fault_at_a_profile_that_it_does_not_name() {
        let (record, message) = child(PROPERTY, &[("DOKIMI_ASSERT_PROP_PROFILE", "nightly")]);
        assert_eq!(record["verdict"], "error");
        assert_eq!(
            message,
            r#"prop::for_all: DOKIMI_ASSERT_PROP_PROFILE: "nightly" is not one of the profiles default, ci and campaign"#
        );
    }

    #[test]
    fn takes_the_default_profile_by_its_name() {
        let (record, _) = child(PROPERTY, &[("DOKIMI_ASSERT_PROP_PROFILE", "default")]);
        assert_eq!(record["verdict"], "pass");
    }
}

mod budget {
    use super::*;

    #[test]
    fn runs_a_campaign_for_the_budget_of_the_campaign_profile() {
        let (record, _) = child(
            PROPERTY,
            &[
                ("DOKIMI_ASSERT_PROP_PROFILE", "campaign"),
                ("DOKIMI_ASSERT_PROP_BUDGET", "1"),
            ],
        );
        assert_eq!(record["verdict"], "pass");
        assert!(
            record["detail"]["cases"]
                .as_u64()
                .is_some_and(|cases| cases > 100),
            "{record}"
        );
    }

    #[test]
    fn ends_the_call_with_a_fault_at_a_budget_that_is_no_whole_number_of_seconds_above_0() {
        for give in ["", "0", "1.5", "8589934592"] {
            let mut vars = vec![("DOKIMI_ASSERT_PROP_PROFILE", "campaign")];
            if !give.is_empty() {
                vars.push(("DOKIMI_ASSERT_PROP_BUDGET", give));
            }
            let (record, message) = child(PROPERTY, &vars);
            assert_eq!(record["verdict"], "error", "{give}");
            assert_eq!(
                message,
                format!(
                    "prop::for_all: DOKIMI_ASSERT_PROP_BUDGET: {give:?} is not a whole number of seconds above 0"
                )
            );
        }
    }

    #[test]
    fn runs_no_campaign_in_a_process_of_a_mutation_run() {
        let vars = [
            ("DOKIMI_ASSERT_PROP_PROFILE", "campaign"),
            ("DOKIMI_ASSERT_PROP_BUDGET", "100"),
            ("DOKIMI_MUTATE_MUTANT", "1"),
        ];
        let (record, _) = child(PROPERTY, &vars);
        assert_eq!(record["detail"]["cases"], 10);
    }
}

mod token {
    use super::*;

    #[test]
    fn replays_the_token_of_the_variable() {
        let (record, _) = child(PROPERTY, &[("DOKIMI_ASSERT_PROP_REPLAY", "prop1:AAU")]);
        assert_eq!(record["verdict"], "pass");
        assert_eq!(record["detail"]["cases"], 1);
    }
}
