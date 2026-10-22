//! The spec of `record`, the call records of a run.

use crate::helpers::{Scratch, in_child, records};
use dokimi_assert::{Collector, Controlled, Recorder, Seat, Standard, check, golden};
use serde_json::Value as Json;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

/// Returns a recorder on a controlled clock.
fn controlled() -> Recorder {
    Recorder::new().with_clock(Arc::new(Controlled::new()))
}

/// Returns one member of each record, as its JSON text.
fn members(records: &[Json], name: &str) -> Vec<String> {
    records
        .iter()
        .map(|record| record[name].to_string())
        .collect()
}

mod calls {
    use super::*;

    #[test]
    fn states_no_record_in_its_debug_text() {
        let calls = Recorder::new().calls().expect("a recorder keeps its calls");
        assert_eq!(format!("{calls:?}"), "Calls { .. }");
    }
}

mod recorder {
    use super::*;

    #[test]
    fn numbers_each_record_from_one() {
        let seat = Recorder::new();
        check::is_true(&seat, true, "first");
        check::is_false(&seat, false, "second");
        let records = records(&seat);
        assert_eq!(members(&records, "seq"), ["1", "2"]);
        assert_eq!(members(&records, "assertion"), [r#""true""#, r#""false""#]);
    }

    #[test]
    fn writes_the_members_of_a_call_in_the_order_of_the_definition() {
        let seat = Recorder::new();
        let line = line!() + 1;
        check::length(&seat, "ab", 3, "the code has three characters");
        let record = &seat.records()[0];
        assert_eq!(
            *record,
            format!(
                r#"{{"definition":"9.0.0","seq":1,"assertion":"length","contract":"the code has three characters","verdict":"fail","aborting":true,"where":{{"file":"record.rs","line":{line}}},"detail":{{"want":{{"type":"int","value":3}},"got":{{"type":"int","value":2}}}}}}"#
            )
        );
    }

    #[test]
    fn states_the_error_of_a_fault_without_a_detail() {
        let dir = Scratch::new();
        let seat = Recorder::new();
        golden::matches_at(&seat, dir.join(""), "output", false, &[]);
        let record = &records(&seat)[0];
        assert_eq!(record["verdict"], "error");
        assert_eq!(record["error"].as_str(), Some(seat.message().as_str()));
        assert!(record.get("detail").is_none(), "{record}");
    }

    #[test]
    fn numbers_a_call_that_runs_a_body_before_the_calls_of_its_body() {
        let seat = controlled();
        check::eventually(
            &seat,
            Duration::from_secs(1),
            Duration::from_millis(1),
            |s| check::is_true(s, true, "in the body"),
            "c",
        );
        let records = records(&seat);
        assert_eq!(
            members(&records, "assertion"),
            [r#""eventually""#, r#""true""#]
        );
        assert_eq!(members(&records, "parent"), ["null", "1"]);
        assert_eq!(members(&records, "run"), ["null", "1"]);
    }

    #[test]
    fn keeps_the_parent_of_a_call_in_a_nested_body() {
        let seat = controlled();
        let nested = |outer: &dyn Seat| {
            let _ = check::rejects(outer, "the inner check fails", |inner| {
                check::is_true(inner, false, "innermost");
            });
        };
        check::eventually(
            &seat,
            Duration::from_secs(1),
            Duration::from_millis(1),
            nested,
            "c",
        );
        let records = records(&seat);
        assert_eq!(
            members(&records, "assertion"),
            [r#""eventually""#, r#""rejects""#, r#""true""#]
        );
        assert_eq!(members(&records, "parent"), ["null", "1", "2"]);
    }

    #[test]
    fn writes_the_record_of_a_call_that_runs_a_body_when_the_call_ends() {
        let seat = controlled();
        let during = std::cell::Cell::new(usize::MAX);
        check::eventually(
            &seat,
            Duration::from_secs(1),
            Duration::from_millis(1),
            |_| during.set(seat.records().len()),
            "c",
        );
        assert_eq!(during.get(), 0);
        assert_eq!(seat.records().len(), 1);
    }
}

mod artifact {
    use super::*;
    use std::sync::{Mutex, MutexGuard, PoisonError};

    /// The test that the child process runs by default.
    const CHILD: &str = "record::artifact::makes_two_calls";

    /// The test whose child process calls an assertion on the standard seat.
    const STANDARD: &str = "record::artifact::makes_a_call_on_the_standard_seat";

    /// The lock that each case of the artifact takes, because every child
    /// process of the binary writes to the one artifact of the binary.
    static ARTIFACT: Mutex<()> = Mutex::new(());

    /// Takes the lock of the artifact.
    fn exclusive() -> MutexGuard<'static, ()> {
        ARTIFACT.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Runs the test `test` of this binary in a child process, with
    /// `DOKIMI_ASSERT_RECORD` set to `value`, and returns its output.
    fn child(test: &str, value: &str) -> std::process::Output {
        let binary = std::env::current_exe().expect("the test binary has a path");
        Command::new(binary)
            .args([test, "--exact", "--ignored", "--test-threads", "1"])
            .env("DOKIMI_ASSERT_RECORD", value)
            .output()
            .expect("the test binary runs")
    }

    /// Returns the path of the artifact of this binary.
    fn path() -> PathBuf {
        let binary = std::env::current_exe().expect("the test binary has a path");
        let mut name = binary
            .file_name()
            .expect("the test binary has a name")
            .to_os_string();
        name.push(".dokimi-assert.jsonl");
        binary.with_file_name(name)
    }

    #[test]
    #[ignore = "the test of the artifact runs it in a child process"]
    fn makes_two_calls() {
        let seat = Collector::new();
        check::is_true(&seat, true, "first");
        check::is_true(&seat, true, "second");
    }

    #[test]
    #[ignore = "the test of the artifact runs it in a child process"]
    fn makes_a_call_on_the_standard_seat() {
        check::is_true(&Standard, true, "standard");
    }

    #[test]
    fn writes_each_call_of_a_test_to_the_artifact_of_its_binary() {
        let _artifact = exclusive();
        let _ = std::fs::remove_file(path());
        let output = child(CHILD, "1");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        let text = std::fs::read_to_string(path()).expect("the child writes the artifact");
        let records: Vec<Json> = text
            .lines()
            .map(|line| serde_json::from_str(line).expect("each line of the artifact is JSON"))
            .collect();
        assert_eq!(members(&records, "seq"), ["1", "2"]);
        assert_eq!(
            members(&records, "test"),
            [format!("{CHILD:?}"), format!("{CHILD:?}")]
        );
        assert_eq!(members(&records, "contract"), [r#""first""#, r#""second""#]);

        std::fs::remove_file(path()).expect("the artifact exists");
        for off in ["0", ""] {
            assert!(child(CHILD, off).status.success());
            assert!(!path().exists(), "the switch {off:?} writes no artifact");
        }

        let output = child(CHILD, "yes");
        assert!(
            !output.status.success(),
            "a call ends with the fault of the switch"
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains(r#"DOKIMI_ASSERT_RECORD: "yes" is neither 0 nor 1"#),
            "{stdout}"
        );
    }

    #[test]
    fn names_the_test_of_its_thread_for_a_call_on_the_standard_seat() {
        let _artifact = exclusive();
        let _ = std::fs::remove_file(path());
        let output = child(STANDARD, "1");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        let text = std::fs::read_to_string(path()).expect("the child writes the artifact");
        std::fs::remove_file(path()).expect("the artifact exists");
        let record: Json =
            serde_json::from_str(text.trim_end()).expect("the artifact contains one record");
        assert_eq!(record["test"], STANDARD);
    }

    #[test]
    fn states_an_artifact_that_cannot_be_created_on_the_standard_error() {
        let _artifact = exclusive();
        let _ = std::fs::remove_file(path());
        let _ = std::fs::remove_dir(path());
        std::fs::create_dir(path()).expect("the directory of the binary accepts a directory");
        let output = in_child(
            CHILD,
            &Scratch::new().join(""),
            &[("DOKIMI_ASSERT_RECORD", "1")],
        );
        std::fs::remove_dir(path()).expect("the directory in place of the artifact exists");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("dokimi-assert: the artifact of the call records cannot be created"),
            "{stderr}"
        );
    }
}
