//! The spec of `prop::environment`, the variables that set the defaults of
//! every property of a run. Each case runs a property in a child process with
//! the variables of the case.

use crate::helpers::{Scratch, in_child, records};
use dokimi_assert::{Recorder, check, prop};

/// The test of the child process, which runs a property that passes.
const PROPERTY: &str = "prop::environment::child::prints_the_record_of_a_property";

/// The test of the child process, which runs a hermetic property.
const HERMETIC: &str = "prop::environment::child::prints_the_record_of_a_hermetic_property";

/// Runs `test` in a child process with `vars`, and returns the call record
/// and the message that it printed.
fn child(test: &str, vars: &[(&str, &str)]) -> (serde_json::Value, String) {
    let dir = Scratch::new();
    let output = in_child(test, &dir.join(""), vars);
    let printed = String::from_utf8_lossy(&output.stdout).into_owned();
    let after = |label: &str| {
        printed
            .lines()
            .find_map(|line| line.split_once(label).map(|(_, rest)| rest.to_owned()))
    };
    let record = after("record ").map_or_else(
        || panic!("the child prints its record: {printed}"),
        |json| serde_json::from_str(&json).expect("the record is JSON"),
    );
    (record, after("message ").unwrap_or_default())
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
