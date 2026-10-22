// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `prop::directory`: the store of a property of a named test,
//! and the seed files of a fuzz target.
//!
//! A property of a named test keeps its store below the working directory,
//! so each case runs in a child process whose working directory is a scratch
//! directory.

use crate::helpers::{Scratch, in_child, listing, records};
use dokimi_assert::__private::Calls;
use dokimi_assert::{Cancel, Clock, Failure, Recorder, Seat, check, prop};
use std::path::Path;
use std::sync::Arc;

/// A recorder of the named test `test`, whose properties keep a store.
struct Named {
    inner: Recorder,
    test: &'static str,
}

impl Named {
    fn new(test: &'static str) -> Self {
        Self {
            inner: Recorder::new(),
            test,
        }
    }
}

impl Seat for Named {
    fn fail(&self, message: &str) {
        self.inner.fail(message);
    }

    fn record(&self, message: &str) {
        self.inner.record(message);
    }

    fn report(&self, failure: &Failure, aborting: bool) {
        self.inner.report(failure, aborting);
    }

    fn clock(&self) -> &dyn Clock {
        self.inner.clock()
    }

    fn cancellation(&self) -> &Cancel {
        self.inner.cancellation()
    }

    fn calls(&self) -> Option<Arc<Calls>> {
        self.inner.calls()
    }

    fn test(&self) -> Option<String> {
        Some(self.test.to_owned())
    }
}

/// The contract of the property of every case.
const CONTRACT: &str = "n stays below 3";

/// Runs a property that fails for an `n` of 3 or more on a seat of `test`,
/// in a thread of its own, and prints its record and its message.
fn run_failing(test: &'static str) {
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let seat = Named::new(test);
            prop::for_all(&seat, CONTRACT, prop::hermetic().seed(1), |c| {
                let n = c.draw(&prop::integer(0..=9_u8), "n");
                check::is_true(c, n < 3, CONTRACT);
            });
            println!("record {}", records(&seat.inner)[0]);
            println!("message {}", seat.inner.message().replace('\n', "\\n"));
        });
    });
}

/// Runs the child test `test` in the scratch directory `dir`, and returns
/// the records, the messages and the standard error that it printed.
fn child(test: &str, dir: &Path) -> (Vec<serde_json::Value>, Vec<String>, String) {
    let output = in_child(&format!("prop::directory::child::{test}"), dir, &[]);
    let printed = String::from_utf8_lossy(&output.stdout).into_owned();
    let after = |label: &str| -> Vec<String> {
        printed
            .lines()
            .filter_map(|line| line.split_once(label).map(|(_, rest)| rest.to_owned()))
            .collect()
    };
    let records = after("record ")
        .iter()
        .map(|json| serde_json::from_str(json).expect("a record is JSON"))
        .collect();
    (
        records,
        after("message "),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Writes `content` into the file `name` of the store of the test `test`,
/// below the working directory.
fn plant(test: &str, name: &str, content: &str) {
    let mut dir = Path::new("testdata/prop/spec").to_path_buf();
    for part in test.split("::") {
        dir.push(part);
    }
    std::fs::create_dir_all(&dir).expect("the store accepts a directory");
    std::fs::write(dir.join(name), content).expect("the store accepts a file");
}

mod child {
    use super::*;

    #[test]
    #[ignore = "a spec of the store runs it in a child process"]
    fn stores_a_failure_and_replays_it() {
        run_failing("store::a/b:c::d.");
        run_failing("store::a/b:c::d.");
        run_failing("store::a/b:c::d.");
    }

    #[test]
    #[ignore = "a spec of the store runs it in a child process"]
    fn skips_an_entry_of_a_later_format() {
        plant("later", "later.json", r#"{"store": 2}"#);
        run_failing("later");
    }

    #[test]
    #[ignore = "a spec of the store runs it in a child process"]
    fn ends_the_call_at_a_damaged_file() {
        plant("damaged", "damaged.json", "not json");
        plant("damaged", "notes.txt", "not an entry");
        run_failing("damaged");
    }

    #[test]
    #[ignore = "a spec of the store runs it in a child process"]
    fn notes_a_stored_case_that_decodes_to_other_values() {
        let entry = r#"{"store": 1, "definition": "9.0.0", "property": "n stays below 3",
            "identity": {"assertion": "true", "file": "directory.rs", "line": 1},
            "choices": "prop1:AAU", "counterexample": [{"label": "n", "value": {"type": "int", "value": 4}}],
            "found": "2026-01-01"}"#;
        plant("decoded", "decoded.json", entry);
        let other = entry.replace("\"n stays below 3\"", "\"another property\"");
        plant("decoded", "other.json", &other);
        run_failing("decoded");
    }

    #[test]
    #[ignore = "a spec of the store runs it in a child process"]
    fn notes_each_stored_case_whose_entry_records_other_draws() {
        let entry = |counterexample: &str| {
            format!(
                r#"{{"store": 1, "definition": "9.0.0", "property": "n stays below 3",
                "identity": {{"assertion": "true", "file": "directory.rs", "line": 1}},
                "choices": "prop1:AAE", "counterexample": {counterexample}, "found": "2026-01-01"}}"#
            )
        };
        plant("recorded", "a.json", &entry("[]"));
        plant(
            "recorded",
            "b.json",
            &entry(r#"[{"label": "m", "value": {"type": "int", "value": 1}}]"#),
        );
        plant("recorded", "c.json", &entry(r#"[{"label": "n"}]"#));
        plant(
            "recorded",
            "d.json",
            &entry(r#"[{"label": "n", "value": {"type": "tuple"}}]"#),
        );
        run_failing("recorded");
    }

    #[test]
    #[ignore = "a spec of the store runs it in a child process"]
    fn ends_the_call_at_a_store_file_that_is_no_text() {
        std::fs::create_dir_all("testdata/prop/spec/binary")
            .expect("the scratch accepts a directory");
        std::fs::write("testdata/prop/spec/binary/bad.json", [0xFF, 0xFE])
            .expect("the store accepts a file");
        run_failing("binary");
    }

    #[cfg(unix)]
    #[test]
    #[ignore = "a spec of the store runs it in a child process"]
    fn notes_a_store_directory_that_takes_no_file() {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all("testdata/prop/spec/locked")
            .expect("the scratch accepts a directory");
        let read_only = std::fs::Permissions::from_mode(0o555);
        std::fs::set_permissions("testdata/prop/spec/locked", read_only)
            .expect("the directory takes a mode");
        run_failing("locked");
        let writable = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions("testdata/prop/spec/locked", writable)
            .expect("the directory takes a mode");
    }

    #[test]
    #[ignore = "a spec of the store runs it in a child process"]
    fn ends_the_call_at_a_store_that_cannot_be_read() {
        std::fs::create_dir_all("testdata/prop/spec").expect("the scratch accepts a directory");
        std::fs::write(
            "testdata/prop/spec/unreadable",
            "a file in place of a directory",
        )
        .expect("the scratch accepts a file");
        run_failing("unreadable");
    }

    #[cfg(unix)]
    #[test]
    #[ignore = "a spec of the store runs it in a child process"]
    fn notes_a_store_that_does_not_keep_a_case() {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all("testdata/prop/spec").expect("the scratch accepts a directory");
        let read_only = std::fs::Permissions::from_mode(0o555);
        std::fs::set_permissions("testdata/prop/spec", read_only)
            .expect("the directory takes a mode");
        run_failing("unkept");
        let writable = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions("testdata/prop/spec", writable)
            .expect("the directory takes a mode");
    }

    #[test]
    #[ignore = "a spec of the seed files runs it in a child process"]
    fn runs_each_seed_file_of_a_fuzz_target() {
        std::fs::create_dir_all("testdata/fuzz/spec/seeded")
            .expect("the scratch accepts a directory");
        std::fs::write("testdata/fuzz/spec/seeded/a", [1_u8]).expect("the scratch accepts a file");
        std::fs::write("testdata/fuzz/spec/seeded/b", [7_u8]).expect("the scratch accepts a file");
        std::fs::create_dir_all("testdata/fuzz/spec/seeded/nested")
            .expect("the scratch accepts a directory");
        let seat = Named::new("seeded");
        let _target = prop::fuzz(&seat, CONTRACT, prop::hermetic(), |c| {
            let n = c.draw(&prop::integer(0..=9_u8), "n");
            check::is_true(c, n < 3, CONTRACT);
        });
        println!("message {}", seat.inner.message().replace('\n', "\\n"));
        for record in records(&seat.inner) {
            println!("record {record}");
        }
    }

    #[test]
    #[ignore = "a spec of the seed files runs it in a child process"]
    fn runs_a_seed_file_whose_case_ends_with_a_fault() {
        std::fs::create_dir_all("testdata/fuzz/spec/faulted")
            .expect("the scratch accepts a directory");
        std::fs::write("testdata/fuzz/spec/faulted/a", [1_u8]).expect("the scratch accepts a file");
        let seat = Named::new("faulted");
        let options = prop::hermetic().around(|_| panic!("the wrapper fails"));
        let _target = prop::fuzz(&seat, CONTRACT, options, |c| {
            let _ = c.draw(&prop::boolean(), "flag");
        });
        println!("message {}", seat.inner.message());
        for record in records(&seat.inner) {
            println!("record {record}");
        }
    }

    #[test]
    #[ignore = "a spec of the seed files runs it in a child process"]
    fn ends_the_call_at_seed_files_that_cannot_be_read() {
        std::fs::create_dir_all("testdata/fuzz/spec").expect("the scratch accepts a directory");
        std::fs::write(
            "testdata/fuzz/spec/unreadable",
            "a file in place of a directory",
        )
        .expect("the scratch accepts a file");
        let seat = Named::new("unreadable");
        let _target = prop::fuzz(&seat, CONTRACT, prop::hermetic(), |c| {
            let _ = c.draw(&prop::boolean(), "flag");
        });
        println!("message {}", seat.inner.message());
        println!("record {}", records(&seat.inner)[0]);
    }
}

mod store {
    use super::*;

    #[test]
    fn keeps_a_failure_under_the_escaped_path_of_its_test_and_replays_it_first() {
        let dir = Scratch::new();
        let (records, _, _) = child("stores_a_failure_and_replays_it", &dir.join(""));
        let files = listing(&dir.join("testdata/prop/spec"));
        assert!(
            files
                .iter()
                .any(|line| line.starts_with("store/a%2Fb%3Ac/d%2E/") && line.contains(".json")),
            "{files:?}"
        );
        assert_eq!(records[0]["detail"]["outcome"], "counterexample");
        assert_eq!(
            records[1]["detail"]["cases"], 0,
            "the stored case runs first"
        );
        assert_eq!(records[2]["detail"]["cases"], 0);
    }

    #[test]
    fn notes_and_skips_an_entry_of_a_later_format() {
        let dir = Scratch::new();
        let (records, _, errors) = child("skips_an_entry_of_a_later_format", &dir.join(""));
        assert_eq!(records[0]["detail"]["outcome"], "counterexample");
        assert!(
            errors.contains("the entry is of a later format, so the run skips it"),
            "{errors}"
        );
    }

    #[test]
    fn ends_the_call_with_a_fault_at_a_damaged_file() {
        let dir = Scratch::new();
        let (records, messages, _) = child("ends_the_call_at_a_damaged_file", &dir.join(""));
        assert_eq!(records[0]["verdict"], "error");
        assert!(
            messages[0].ends_with("the store has damaged files: damaged.json"),
            "{}",
            messages[0]
        );
    }

    #[test]
    fn notes_a_stored_case_that_decodes_to_other_values_than_it_records() {
        let dir = Scratch::new();
        let (records, _, errors) = child(
            "notes_a_stored_case_that_decodes_to_other_values",
            &dir.join(""),
        );
        assert_eq!(records[0]["detail"]["cases"], 0);
        assert!(
            errors.contains("the stored case decodes to other values than it records"),
            "{errors}"
        );
    }

    #[test]
    fn notes_each_stored_case_whose_entry_records_other_draws() {
        let dir = Scratch::new();
        let (records, _, errors) = child(
            "notes_each_stored_case_whose_entry_records_other_draws",
            &dir.join(""),
        );
        assert_eq!(records[0]["detail"]["outcome"], "counterexample");
        let noted: Vec<&str> = errors
            .lines()
            .filter(|line| line.contains("decodes to other values"))
            .collect();
        assert_eq!(noted.len(), 3, "{errors}");
        for name in ["a.json", "b.json", "d.json"] {
            assert!(
                noted.iter().any(|line| line.contains(name)),
                "{name}: {errors}"
            );
        }
    }

    #[test]
    fn ends_the_call_with_a_fault_at_a_store_file_that_is_no_text() {
        let dir = Scratch::new();
        let (_, messages, _) = child(
            "ends_the_call_at_a_store_file_that_is_no_text",
            &dir.join(""),
        );
        assert!(
            messages[0].ends_with("the store has damaged files: bad.json"),
            "{}",
            messages[0]
        );
    }

    #[cfg(unix)]
    #[test]
    fn notes_a_case_that_a_read_only_store_does_not_keep() {
        let dir = Scratch::new();
        let (records, _, errors) =
            child("notes_a_store_directory_that_takes_no_file", &dir.join(""));
        assert_eq!(records[0]["detail"]["outcome"], "counterexample");
        assert!(
            errors.contains("the store does not keep a case of \"n stays below 3\""),
            "{errors}"
        );
    }

    #[test]
    fn ends_the_call_with_a_fault_at_a_store_that_cannot_be_read() {
        let dir = Scratch::new();
        let (_, messages, _) = child(
            "ends_the_call_at_a_store_that_cannot_be_read",
            &dir.join(""),
        );
        assert!(
            messages[0].contains("the store cannot be read: "),
            "{}",
            messages[0]
        );
    }

    #[cfg(unix)]
    #[test]
    fn notes_a_case_that_the_store_does_not_keep() {
        let dir = Scratch::new();
        let (records, _, errors) = child("notes_a_store_that_does_not_keep_a_case", &dir.join(""));
        assert_eq!(records[0]["detail"]["outcome"], "counterexample");
        assert!(
            errors.contains("the store does not keep a case of \"n stays below 3\""),
            "{errors}"
        );
    }
}

mod seeds {
    use super::*;

    #[test]
    fn runs_each_seed_file_as_one_input() {
        let dir = Scratch::new();
        let (records, _, _) = child("runs_each_seed_file_of_a_fuzz_target", &dir.join(""));
        let verdicts: Vec<&serde_json::Value> = records
            .iter()
            .filter(|record| record["assertion"] == "prop-for-all")
            .map(|record| &record["verdict"])
            .collect();
        assert_eq!(verdicts, ["pass", "pass", "fail"]);
    }

    #[test]
    fn ends_the_call_of_a_seed_file_whose_case_ends_with_a_fault() {
        let dir = Scratch::new();
        let (records, messages, _) = child(
            "runs_a_seed_file_whose_case_ends_with_a_fault",
            &dir.join(""),
        );
        let verdicts: Vec<&serde_json::Value> = records
            .iter()
            .filter(|record| record["assertion"] == "prop-for-all")
            .map(|record| &record["verdict"])
            .collect();
        assert_eq!(verdicts, ["pass", "error"]);
        assert!(
            messages[0].ends_with("around: the wrapper panicked: the wrapper fails"),
            "{}",
            messages[0]
        );
    }

    #[test]
    fn ends_the_call_with_a_fault_at_seed_files_that_cannot_be_read() {
        let dir = Scratch::new();
        let (_, messages, _) = child(
            "ends_the_call_at_seed_files_that_cannot_be_read",
            &dir.join(""),
        );
        assert!(
            messages[0].contains("the seed files cannot be read: "),
            "{}",
            messages[0]
        );
    }
}
