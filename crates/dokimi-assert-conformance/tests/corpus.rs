// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `corpus`, which runs the definition's corpus on both
//! surfaces of each library.

use dokimi_assert::{Recorder, Seat, Value, check};
use dokimi_assert_conformance::corpus::{self, Case, Disagreement, Library, Observed, Surface};
use dokimi_assert_conformance::definition;
use dokimi_assert_conformance::literal::Rendering;
use serde_json::{Value as Json, json};

/// Returns a case of `assertion` that states `args`, and expects a failure
/// when `fails` is `true`.
fn case(assertion: &str, args: Vec<Value>, fails: bool) -> Case {
    Case {
        id: format!("{assertion}/made-up"),
        assertion: assertion.to_owned(),
        args,
        fails,
        detail: Vec::new(),
        subject: None,
    }
}

/// Returns what a failing call of `equal` reports, with the id of `case` as
/// its contract. The call is made from this file, outside the driver.
fn equal_failure(case: &Case) -> Observed {
    let seat = Recorder::new();
    check::equal(&seat, 1, 2, &case.id);
    Observed::of(&seat)
}

/// Returns an observation with the call record `record` alone.
fn recorded(record: &Json, failed: bool) -> Observed {
    Observed {
        failed,
        message: String::new(),
        failures: Vec::new(),
        records: vec![record.to_string()],
    }
}

/// Returns the call record of `case` on `check`, with the verdict that the
/// case expects and without a detail.
fn record_of(case: &Case) -> Json {
    json!({
        "definition": definition::version().expect("the definition states its version"),
        "seq": 1,
        "assertion": case.assertion,
        "contract": case.id,
        "verdict": if case.fails { "fail" } else { "pass" },
        "aborting": true,
    })
}

mod cases {
    use super::*;

    #[test]
    fn reads_each_case_without_a_skip_for_rust_or_a_declined_option() {
        let read: Vec<String> = corpus::cases()
            .expect("the vendored corpus reads")
            .into_iter()
            .map(|case| case.id)
            .collect();
        let declined = definition::declined().expect("the overlay reads");
        let mut stated = 0;
        for file in definition::corpus_files().expect("the corpus lists") {
            let corpus = definition::read(&file).expect("a corpus file reads");
            for case in corpus["cases"]
                .as_array()
                .expect("a corpus file states cases")
            {
                let id = case["id"].as_str().expect("a case has an id");
                let options = case["options"].as_array().cloned().unwrap_or_default();
                let skipped = case["skip"].get("rust").is_some()
                    || options
                        .iter()
                        .any(|option| declined.iter().any(|id| option == id.as_str()));
                assert_eq!(read.contains(&id.to_owned()), !skipped, "{id}");
                stated += 1;
            }
        }
        assert!(
            read.len() > 100 && read.len() < stated,
            "{} of {stated} cases",
            read.len()
        );
    }

    #[test]
    fn leaves_out_a_case_with_a_skip_for_rust() {
        let cases = corpus::cases().expect("the vendored corpus reads");
        let skipped = "permutation/an-absent-list-against-an-empty-one-fails";
        assert!(!cases.iter().any(|case| case.id == skipped));
    }
}

mod library {
    use super::*;

    #[test]
    fn states_the_assertions_of_a_future_on_both_libraries() {
        assert_eq!(Library::ALL, [Library::Core, Library::Tokio]);
        for assertion in corpus::FUTURES {
            assert!(Library::Tokio.states(assertion), "{assertion}");
        }
        assert!(Library::Core.states("eventually"));
        assert!(Library::Core.states("equal"));
        assert!(!Library::Tokio.states("equal"));
    }

    #[test]
    fn states_no_task_leaks_on_the_tokio_crate_alone() {
        assert!(Library::Tokio.states("no-task-leaks"));
        assert!(!Library::Core.states("no-task-leaks"));
    }
}

mod surface {
    use super::*;

    #[test]
    fn aborts_on_check_alone() {
        assert!(Surface::Check.aborting());
        assert!(!Surface::Soft.aborting());
        assert_eq!(Surface::ALL, [Surface::Check, Surface::Soft]);
    }
}

mod run {
    use super::*;

    #[test]
    fn agrees_with_every_case_that_each_library_states_on_both_surfaces() {
        let cases = corpus::cases().expect("the vendored corpus reads");
        assert!(
            cases
                .iter()
                .any(|case| Library::Tokio.states(&case.assertion))
        );
        let mut disagreements = Vec::new();
        for library in Library::ALL {
            for surface in Surface::ALL {
                for case in cases.iter().filter(|case| library.states(&case.assertion)) {
                    if let Err(disagreement) = corpus::run(library, surface, case) {
                        disagreements.push(format!(
                            "{library:?} {surface:?} {}: {disagreement}",
                            case.id
                        ));
                    }
                }
            }
        }
        assert!(
            disagreements.is_empty(),
            "{} disagreements:\n{}",
            disagreements.len(),
            disagreements.join("\n")
        );
    }

    #[test]
    fn disagrees_with_an_assertion_that_the_driver_does_not_call() {
        let result = corpus::run(
            Library::Core,
            Surface::Check,
            &case("made-up", vec![], false),
        );
        assert!(matches!(result, Err(Disagreement::NoDriver)), "{result:?}");
    }

    #[test]
    fn disagrees_with_a_case_of_values_on_the_tokio_crate() {
        let args = vec![Value::Int(1), Value::Int(1)];
        let result = corpus::run(Library::Tokio, Surface::Soft, &case("equal", args, false));
        assert!(matches!(result, Err(Disagreement::NoDriver)), "{result:?}");
    }

    #[test]
    fn disagrees_with_a_subject_that_the_driver_does_not_build() {
        let mut made_up = case("pure", vec![], false);
        made_up.subject = Some("made-up".to_owned());
        let result = corpus::run(Library::Core, Surface::Soft, &made_up);
        assert!(
            matches!(&result, Err(Disagreement::NoSubject(kind)) if kind == "made-up"),
            "{result:?}"
        );
    }

    #[test]
    fn disagrees_with_a_case_without_an_argument() {
        let result = corpus::run(Library::Core, Surface::Check, &case("equal", vec![], false));
        assert!(
            matches!(
                result,
                Err(Disagreement::Argument {
                    at: 0,
                    stated: None,
                    ..
                })
            ),
            "{result:?}"
        );
    }

    #[test]
    fn disagrees_with_an_argument_of_another_type() {
        let args = vec![Value::Int(1), Value::from("a")];
        let result = corpus::run(
            Library::Core,
            Surface::Check,
            &case("has-prefix", args, false),
        );
        assert!(
            matches!(
                &result,
                Err(Disagreement::Argument {
                    at: 0,
                    expected: "text",
                    stated: Some(Value::Int(1))
                })
            ),
            "{result:?}"
        );
    }

    #[test]
    fn disagrees_with_a_pass_where_the_case_expects_a_failure() {
        let result = corpus::run(
            Library::Core,
            Surface::Check,
            &case("equal", vec![Value::Int(1), Value::Int(1)], true),
        );
        assert!(
            matches!(result, Err(Disagreement::Verdict { fails: true, .. })),
            "{result:?}"
        );
    }

    #[test]
    fn disagrees_with_a_field_that_states_another_value() {
        let mut made_up = case("equal", vec![Value::Int(1), Value::Int(2)], true);
        made_up.detail = vec![("want".to_owned(), Value::Int(9))];
        let result = corpus::run(Library::Core, Surface::Soft, &made_up);
        assert!(
            matches!(&result, Err(Disagreement::Field { name, .. }) if name == "want"),
            "{result:?}"
        );
    }
}

mod compare_failure {
    use super::*;

    #[test]
    fn disagrees_with_a_failure_without_a_record() {
        let seat = Recorder::new();
        seat.fail("a message without a record");
        let result = corpus::compare_failure(
            &Observed::of(&seat),
            &case("true", vec![], true),
            Rendering::Values,
        );
        assert!(matches!(result, Err(Disagreement::NoRecord)), "{result:?}");
    }

    #[test]
    fn disagrees_with_a_record_of_a_passing_case() {
        let made_up = case("equal", vec![], false);
        let result = corpus::compare_failure(&equal_failure(&made_up), &made_up, Rendering::Values);
        assert!(
            matches!(result, Err(Disagreement::Unexpected(_))),
            "{result:?}"
        );
    }

    #[test]
    fn disagrees_with_a_record_of_another_assertion() {
        let made_up = case("not-equal", vec![], true);
        let result = corpus::compare_failure(&equal_failure(&made_up), &made_up, Rendering::Values);
        assert!(
            matches!(result, Err(Disagreement::Assertion(Some("equal")))),
            "{result:?}"
        );
    }

    #[test]
    fn disagrees_with_a_record_of_another_contract() {
        let made_up = case("equal", vec![], true);
        let mut other = made_up.clone();
        other.id = "equal/another".to_owned();
        let result = corpus::compare_failure(&equal_failure(&other), &made_up, Rendering::Values);
        assert!(
            matches!(&result, Err(Disagreement::Contract(contract)) if contract == "equal/another"),
            "{result:?}"
        );
    }

    #[test]
    fn disagrees_with_a_call_site_outside_the_driver() {
        let made_up = case("equal", vec![], true);
        let result = corpus::compare_failure(&equal_failure(&made_up), &made_up, Rendering::Values);
        assert!(
            matches!(&result, Err(Disagreement::Location(at)) if at.contains("corpus.rs")),
            "{result:?}"
        );
    }
}

mod compare_record {
    use super::*;

    #[test]
    fn agrees_with_the_call_record_of_a_pass() {
        let made_up = case("true", vec![], false);
        let observed = recorded(&record_of(&made_up), false);
        assert!(
            corpus::compare_record(&observed, Surface::Check, &made_up, Rendering::Values).is_ok()
        );
    }

    #[test]
    fn disagrees_without_a_call_record() {
        let made_up = case("true", vec![], false);
        let mut observed = recorded(&record_of(&made_up), false);
        observed.records.clear();
        let result = corpus::compare_record(&observed, Surface::Check, &made_up, Rendering::Values);
        assert!(
            matches!(result, Err(Disagreement::NoCallRecord)),
            "{result:?}"
        );
    }

    #[test]
    fn disagrees_with_a_call_record_that_is_no_json() {
        let made_up = case("true", vec![], false);
        let mut observed = recorded(&record_of(&made_up), false);
        observed.records = vec!["{".to_owned()];
        let result = corpus::compare_record(&observed, Surface::Check, &made_up, Rendering::Values);
        assert!(
            matches!(result, Err(Disagreement::CallRecordJson(_))),
            "{result:?}"
        );
    }

    #[test]
    fn disagrees_with_a_member_of_another_value() {
        let made_up = case("true", vec![], false);
        let observed = recorded(&record_of(&made_up), false);
        let result = corpus::compare_record(&observed, Surface::Soft, &made_up, Rendering::Values);
        assert!(
            matches!(
                result,
                Err(Disagreement::Member {
                    name: "aborting",
                    ..
                })
            ),
            "{result:?}"
        );
    }

    #[test]
    fn disagrees_with_a_call_record_that_states_a_parent() {
        let made_up = case("true", vec![], false);
        let mut stated = record_of(&made_up);
        stated["parent"] = json!(1);
        let result = corpus::compare_record(
            &recorded(&stated, false),
            Surface::Check,
            &made_up,
            Rendering::Values,
        );
        assert!(matches!(result, Err(Disagreement::Parent)), "{result:?}");
    }

    #[test]
    fn disagrees_with_the_detail_of_a_pass() {
        let made_up = case("true", vec![], false);
        let mut stated = record_of(&made_up);
        stated["detail"] = json!({});
        let result = corpus::compare_record(
            &recorded(&stated, false),
            Surface::Check,
            &made_up,
            Rendering::Values,
        );
        assert!(
            matches!(result, Err(Disagreement::Detail { fails: false })),
            "{result:?}"
        );
    }

    #[test]
    fn disagrees_with_a_failure_without_a_detail() {
        let made_up = case("true", vec![], true);
        let result = corpus::compare_record(
            &recorded(&record_of(&made_up), true),
            Surface::Check,
            &made_up,
            Rendering::Values,
        );
        assert!(
            matches!(result, Err(Disagreement::Detail { fails: true })),
            "{result:?}"
        );
    }

    #[test]
    fn disagrees_with_a_detail_of_other_fields() {
        let made_up = case("equal", vec![], true);
        let mut stated = record_of(&made_up);
        stated["detail"] = json!({ "want": { "type": "null" } });
        let result = corpus::compare_record(
            &recorded(&stated, true),
            Surface::Check,
            &made_up,
            Rendering::Values,
        );
        assert!(
            matches!(&result, Err(Disagreement::DetailFields(names)) if names == &["want"]),
            "{result:?}"
        );
    }

    #[test]
    fn disagrees_with_a_detail_field_of_another_value() {
        let mut made_up = case("equal", vec![], true);
        made_up.detail = vec![("want".to_owned(), Value::Int(2))];
        let mut stated = record_of(&made_up);
        stated["detail"] = json!({
            "want": { "type": "opaque", "text": "3" },
            "got": { "type": "opaque", "text": "1" },
        });
        let result = corpus::compare_record(
            &recorded(&stated, true),
            Surface::Check,
            &made_up,
            Rendering::Native,
        );
        assert!(
            matches!(&result, Err(Disagreement::DetailField { name, .. }) if name == "want"),
            "{result:?}"
        );
    }

    #[test]
    fn disagrees_with_a_detail_field_that_is_no_typed_literal() {
        let mut made_up = case("equal", vec![], true);
        made_up.detail = vec![("want".to_owned(), Value::Int(2))];
        let mut stated = record_of(&made_up);
        stated["detail"] = json!({ "want": 2, "got": { "type": "null" } });
        let result = corpus::compare_record(
            &recorded(&stated, true),
            Surface::Check,
            &made_up,
            Rendering::Values,
        );
        assert!(
            matches!(&result, Err(Disagreement::DetailLiteral { name, .. }) if name == "want"),
            "{result:?}"
        );
    }
}

mod disagreement {
    use super::*;

    #[test]
    fn renders_what_disagrees() {
        let result = corpus::run(
            Library::Core,
            Surface::Check,
            &case("equal", vec![Value::Int(1), Value::Int(1)], true),
        );
        let message = result.expect_err("the case disagrees").to_string();
        assert_eq!(
            message,
            "the case expects a failure, and the call reported \"\""
        );
    }
}

mod observed {
    use super::*;

    #[test]
    fn reads_what_a_recorder_was_told() {
        let seat = Recorder::new();
        seat.record("a message");
        check::is_true(&seat, true, "c");
        let observed = Observed::of(&seat);
        assert!(observed.failed);
        assert_eq!(observed.message, "a message");
        assert_eq!(observed.failures.len(), 0);
        assert_eq!(observed.records.len(), 1);
    }
}
