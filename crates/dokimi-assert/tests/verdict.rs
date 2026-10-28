// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `verdict`, the pass, the failure, the skip or the fault of
//! each call.

use crate::helpers::{Scratch, in_child, records};
use dokimi_assert::{Failure, Recorder, check, golden, prop, soft};
use serde_json::json;

/// The line that the child prints for each call when the switch of
/// recording states no value.
const SWITCHED: &str = r#"DOKIMI_ASSERT_RECORD: "yes" is neither 0 nor 1, with 0 records"#;

/// One call of the child, with the name that the child prints for it.
type Call = (&'static str, fn(&Recorder));

/// Runs the child test with `DOKIMI_ASSERT_RECORD` set to a value that is
/// neither 0 nor 1, and returns the line that it printed for each call.
fn switched_calls() -> Vec<String> {
    let dir = Scratch::new();
    let output = in_child(
        "verdict::child::calls_each_verdict_on_a_recorder",
        &dir.join(""),
        &[("DOKIMI_ASSERT_RECORD", "yes")],
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_once("verdict ").map(|(_, rest)| rest.to_owned()))
        .collect()
}

/// The text of the fault of a switch that states `yes`, as the child prints
/// it in a list.
const SWITCHED_FAULT: &str = r#""DOKIMI_ASSERT_RECORD: \"yes\" is neither 0 nor 1""#;

/// Runs the child test that prints the faults of a recorder of a call, with
/// `DOKIMI_ASSERT_RECORD` set to a value that is neither 0 nor 1.
fn switched_faults() -> Vec<String> {
    let dir = Scratch::new();
    let output = in_child(
        "verdict::child::prints_the_faults_of_a_call",
        &dir.join(""),
        &[("DOKIMI_ASSERT_RECORD", "yes")],
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_once("faults: ").map(|(_, rest)| rest.to_owned()))
        .collect()
}

mod child {
    use super::*;

    #[test]
    #[ignore = "a spec of the switch runs it in a child process"]
    fn calls_each_verdict_on_a_recorder() {
        let calls: [Call; 6] = [
            ("pass", |seat| check::is_true(seat, true, "c")),
            ("fail", |seat| check::is_true(seat, false, "c")),
            ("fault", |seat| {
                golden::matches_at(seat, Scratch::new().join(""), "output", false, &[]);
            }),
            ("property pass", |seat| {
                prop::for_all(seat, "c", prop::hermetic(), |c| {
                    _ = c.draw(&prop::boolean(), "flag");
                });
            }),
            ("property fail", |seat| {
                prop::for_all(seat, "c", prop::hermetic(), |_| panic!("the case fails"));
            }),
            ("property skip", |seat| {
                prop::for_all(seat, "c", prop::hermetic(), |c| {
                    c.skip("the case skips");
                });
            }),
        ];
        for (name, call) in calls {
            let seat = Recorder::new();
            call(&seat);
            println!(
                "verdict {name}: {}, with {} records",
                seat.message(),
                seat.records().len()
            );
        }
    }

    #[test]
    #[ignore = "a spec of the switch runs it in a child process"]
    fn prints_the_faults_of_a_call() {
        let seat = Recorder::new();
        check::is_true(&seat, true, "c");
        let texts: Vec<String> = seat.faults().iter().map(ToString::to_string).collect();
        println!("faults: {texts:?}");
    }
}

mod run {
    use super::*;

    #[test]
    fn states_its_call_site_in_its_debug_text() {
        let seat = Recorder::new();
        let line = line!() + 1;
        let run = dokimi_assert::__private::Run::of(&seat);
        assert_eq!(
            format!("{run:?}"),
            format!(
                "Run {{ location: Where {{ file: {:?}, line: {line} }}, .. }}",
                file!()
            )
        );
    }

    #[test]
    fn takes_no_body_for_a_call_that_runs_no_body() {
        let seat = Recorder::new();
        let run = dokimi_assert::__private::Run::of(&seat);
        let body = run.body_calls();
        run.take(body.as_ref());
        assert!(body.is_none());
        assert_eq!(records(&seat), [] as [serde_json::Value; 0]);
    }
}

mod pass {
    use super::*;

    #[test]
    fn ends_the_call_with_the_fault_of_a_switch_that_states_no_value() {
        let printed = switched_calls();
        assert!(
            printed.contains(&format!("pass: {SWITCHED}")),
            "{printed:?}"
        );
        assert!(
            printed.contains(&format!("property pass: {SWITCHED}")),
            "{printed:?}"
        );
    }

    #[test]
    fn writes_a_record_without_detail() {
        let seat = Recorder::new();
        check::is_true(&seat, true, "the flag is set");
        let record = &records(&seat)[0];
        assert_eq!(record["verdict"], "pass");
        assert_eq!(record["aborting"], true);
        assert_eq!(record["contract"], "the flag is set");
        assert!(record.get("detail").is_none(), "{record}");
        assert!(!seat.failed());
    }

    #[test]
    fn states_the_recording_surface_as_not_aborting() {
        let seat = Recorder::new();
        soft::is_true(&seat, true, "c");
        assert_eq!(records(&seat)[0]["aborting"], false);
    }

    #[test]
    fn states_the_line_of_the_call() {
        let seat = Recorder::new();
        let line = line!() + 1;
        check::is_true(&seat, true, "c");
        let record = &records(&seat)[0];
        assert_eq!(record["where"], json!({"file": "verdict.rs", "line": line}));
    }
}

mod fail {
    use super::*;

    #[test]
    fn ends_the_call_with_the_fault_of_a_switch_that_states_no_value() {
        let printed = switched_calls();
        assert!(
            printed.contains(&format!("fail: {SWITCHED}")),
            "{printed:?}"
        );
        assert!(
            printed.contains(&format!("property fail: {SWITCHED}")),
            "{printed:?}"
        );
    }

    #[test]
    fn writes_a_record_with_the_literal_of_each_field() {
        let seat = Recorder::new();
        soft::length(&seat, &[1], 3, "c");
        let record = &records(&seat)[0];
        assert_eq!(record["verdict"], "fail");
        assert_eq!(
            record["detail"],
            json!({"want": {"type": "int", "value": 3}, "got": {"type": "int", "value": 1}})
        );
    }

    #[test]
    fn reports_the_failure_to_the_seat() {
        let seat = Recorder::new();
        check::is_true(&seat, false, "the flag is set");
        assert_eq!(seat.message(), "the flag is set");
        assert_eq!(seat.failures()[0].assertion(), Some("true"));
    }

    #[test]
    fn records_a_failure_of_the_recording_surface() {
        let seat = Recorder::new();
        soft::is_true(&seat, false, "the flag is set");
        assert_eq!(seat.messages(), ["the flag is set"]);
    }
}

mod skip {
    use super::*;

    #[test]
    fn ends_the_call_with_the_fault_of_a_switch_that_states_no_value() {
        let printed = switched_calls();
        assert!(
            printed.contains(&format!("property skip: {SWITCHED}")),
            "{printed:?}"
        );
    }
}

mod fault {
    use super::*;

    #[test]
    fn ends_the_call_with_the_fault_of_a_switch_that_states_no_value() {
        let printed = switched_calls();
        assert!(
            printed.contains(&format!("fault: {SWITCHED}")),
            "{printed:?}"
        );
    }

    #[test]
    fn writes_the_text_of_the_fault() {
        let dir = Scratch::new();
        let seat = Recorder::new();
        golden::matches_at(&seat, dir.join(""), "output", false, &[]);
        let record = &records(&seat)[0];
        assert_eq!(record["verdict"], "error");
        assert!(
            record["error"]
                .as_str()
                .is_some_and(|text| text.starts_with("golden: ")),
            "{record}"
        );
    }

    #[test]
    fn hands_the_fault_to_the_seat_without_a_failure_record() {
        let dir = Scratch::new();
        let seat = Recorder::new();
        golden::matches_at(&seat, dir.join(""), "output", false, &[]);
        let faults: Vec<String> = seat.faults().iter().map(ToString::to_string).collect();
        assert_eq!(faults, [seat.message()]);
        assert!(seat.message().starts_with("golden: "), "{}", seat.message());
        assert_eq!(seat.messages(), [] as [String; 0]);
        assert_eq!(seat.failures(), [] as [Failure; 0]);
    }

    #[test]
    fn hands_the_fault_of_a_switch_that_states_no_value_to_the_seat() {
        let printed = switched_faults();
        assert_eq!(printed, [format!("[{SWITCHED_FAULT}]")]);
    }
}
