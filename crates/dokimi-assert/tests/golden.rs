// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `golden`, the comparisons against recorded output.

use crate::helpers::{Scratch, in_child, records};
use dokimi_assert::golden::{self, Scrubber};
use dokimi_assert::{Recorder, Value};

mod matches {
    use super::*;

    #[test]
    fn passes_output_that_equals_the_golden_file() {
        let seat = Recorder::new();
        golden::matches(&seat, "greeting.txt", "hello, world\n", false, &[]);
        assert!(!seat.failed(), "{}", seat.message());
        assert_eq!(records(&seat)[0]["assertion"], "golden-match");
    }

    #[test]
    fn states_null_for_a_golden_file_that_does_not_exist() {
        let seat = Recorder::new();
        golden::matches(&seat, "missing.txt", "anything", false, &[]);
        let failure = &seat.failures()[0];
        assert_eq!(failure.assertion(), Some("golden-match"));
        assert_eq!(
            failure.detail(),
            [("want", Value::Null), ("got", Value::from("anything"))]
        );
    }

    #[test]
    fn names_the_golden_file_in_the_contract() {
        let seat = Recorder::new();
        golden::matches(&seat, "missing.txt", "anything", false, &[]);
        let contract = seat.failures()[0].contract().to_owned();
        assert!(
            contract.starts_with("the golden file testdata"),
            "{contract}"
        );
        assert!(
            contract.ends_with("matches the output, and DOKIMI_ASSERT_UPDATE_GOLDEN=1 writes it"),
            "{contract}"
        );
    }
}

mod matches_at {
    use super::*;

    #[test]
    fn states_the_golden_content_with_the_output() {
        let dir = Scratch::new();
        let path = dir.join("report.txt");
        std::fs::write(&path, "total: 3\n").expect("the scratch directory accepts a file");
        let seat = Recorder::new();
        golden::matches_at(&seat, &path, "total: 4\n", false, &[]);
        let failure = &seat.failures()[0];
        assert_eq!(failure.assertion(), Some("golden-match-at"));
        assert_eq!(
            failure.detail(),
            [
                ("want", Value::from("total: 3\n")),
                ("got", Value::from("total: 4\n"))
            ]
        );
    }

    #[test]
    fn writes_the_scrubbed_output_on_an_update() {
        let dir = Scratch::new();
        let path = dir.join("nested/report.txt");
        let seat = Recorder::new();
        let stamp = [golden::scrub_timestamps()];
        golden::matches_at(&seat, &path, "at 2026-10-21T08:00:00Z\n", true, &stamp);
        assert!(!seat.failed(), "{}", seat.message());
        let written = std::fs::read_to_string(&path).expect("the update writes the golden file");
        assert_eq!(written, "at SCRUBBED_TIMESTAMP\n");
    }

    #[test]
    fn scrubs_both_sides_before_it_compares_them() {
        let dir = Scratch::new();
        let path = dir.join("run.txt");
        std::fs::write(&path, "run_0123456789abcdef done\n")
            .expect("the scratch directory accepts a file");
        let seat = Recorder::new();
        golden::matches_at(
            &seat,
            &path,
            "run_fedcba9876543210 done\n",
            false,
            &[golden::scrub_run_ids()],
        );
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn ends_the_call_with_a_fault_for_a_golden_file_that_cannot_be_read() {
        let dir = Scratch::new();
        let seat = Recorder::new();
        golden::matches_at(&seat, dir.join(""), "anything", false, &[]);
        assert!(seat.message().starts_with("golden: "), "{}", seat.message());
        assert_eq!(seat.failures(), []);
        assert_eq!(records(&seat)[0]["verdict"], "error");
    }

    #[cfg(unix)]
    #[test]
    fn ends_the_call_with_a_fault_for_a_golden_file_that_cannot_be_written() {
        use dokimi_assert::files::{self, Tree};
        let dir = crate::helpers::written(&Tree::from([(
            "locked",
            files::directory().with_mode(0o500),
        )]));
        let path = dir.path().join("locked/new/report.txt");
        let seat = Recorder::new();
        golden::matches_at(&seat, &path, "anything", true, &[]);
        let wanted = format!(
            "golden: {}: the golden file cannot be written: ",
            path.display()
        );
        assert!(seat.message().starts_with(&wanted), "{}", seat.message());
        assert_eq!(records(&seat)[0]["verdict"], "error");
    }
}

mod matches_tree {
    use super::*;
    use crate::helpers::written;
    use dokimi_assert::files::{self, Tree};

    /// Returns the tree of the golden tree `tree` under `testdata/golden`.
    fn golden_tree() -> Tree {
        Tree::from([
            ("api.go", files::text("package api\n")),
            ("v1/types.go", files::text("package v1\n")),
        ])
    }

    #[test]
    fn passes_an_output_that_equals_the_golden_tree() {
        let out = written(&golden_tree());
        let seat = Recorder::new();
        golden::matches_tree(&seat, "tree", &out, false, &[]);
        assert!(!seat.failed(), "{}", seat.message());
        assert_eq!(records(&seat)[0]["assertion"], "golden-match-tree");
    }

    #[test]
    fn states_the_entries_at_the_paths_that_differ() {
        let out = written(&Tree::from([(
            "api.go",
            files::text("package api\n\nfunc New() {}\n"),
        )]));
        let seat = Recorder::new();
        golden::matches_tree(&seat, "tree", &out, false, &[]);
        let failure = &seat.failures()[0];
        assert_eq!(failure.assertion(), Some("golden-match-tree"));
        let want = Tree::from([
            ("api.go", files::text("package api\n")),
            ("v1", files::directory()),
            ("v1/types.go", files::text("package v1\n")),
        ]);
        let got = Tree::from([("api.go", files::text("package api\n\nfunc New() {}\n"))]);
        assert_eq!(
            failure.detail(),
            [
                ("want", Value::Tree(want)),
                ("got", Value::Tree(got)),
                ("differences", Value::Int(3))
            ]
        );
    }

    #[test]
    fn states_null_for_a_golden_tree_that_does_not_exist() {
        let out = written(&golden_tree());
        let seat = Recorder::new();
        golden::matches_tree(&seat, "absent", &out, false, &[]);
        let got = Tree::from([
            ("api.go", files::text("package api\n")),
            ("v1", files::directory()),
            ("v1/types.go", files::text("package v1\n")),
        ]);
        assert_eq!(
            seat.failures()[0].detail(),
            [
                ("want", Value::Null),
                ("got", Value::Tree(got)),
                ("differences", Value::Int(3))
            ]
        );
    }

    #[test]
    fn scrubs_the_text_of_both_trees_before_it_compares_them() {
        let out = written(&Tree::from([(
            "report.txt",
            files::text("built at 2026-10-21T08:00:00Z\n"),
        )]));
        let seat = Recorder::new();
        golden::matches_tree(&seat, "stamped", &out, false, &[golden::scrub_timestamps()]);
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn names_the_golden_tree_in_the_contract() {
        let out = written(&Tree::new());
        let seat = Recorder::new();
        golden::matches_tree(&seat, "tree", &out, false, &[]);
        let contract = seat.failures()[0].contract().to_owned();
        assert!(
            contract.starts_with("the golden tree testdata"),
            "{contract}"
        );
        assert!(
            contract
                .ends_with("tree matches the output, and DOKIMI_ASSERT_UPDATE_GOLDEN=1 writes it"),
            "{contract}"
        );
    }

    #[test]
    fn ends_the_call_with_a_fault_for_a_name_that_is_no_path() {
        let out = written(&Tree::new());
        let seat = Recorder::new();
        golden::matches_tree(&seat, "../tree", &out, false, &[]);
        assert_eq!(
            seat.message(),
            r#"golden::matches_tree: the path "../tree" has the name "..""#
        );
        assert_eq!(records(&seat)[0]["verdict"], "error");
    }

    #[test]
    fn ends_the_call_with_a_fault_for_an_output_that_cannot_be_read() {
        let dir = Scratch::new();
        let seat = Recorder::new();
        golden::matches_tree(&seat, "tree", dir.join("missing"), false, &[]);
        assert!(
            seat.message()
                .starts_with("golden::matches_tree: the tree cannot be read: "),
            "{}",
            seat.message()
        );
    }

    #[test]
    fn ends_the_call_with_a_fault_for_a_golden_tree_that_is_no_directory() {
        let out = written(&Tree::new());
        let seat = Recorder::new();
        golden::matches_tree(&seat, "greeting.txt", &out, false, &[]);
        assert_eq!(
            seat.message(),
            "golden::matches_tree: the root of the tree is no directory"
        );
    }
}

mod matches_json_field {
    use super::*;

    #[test]
    fn passes_a_value_that_differs_in_layout_alone() {
        let dir = Scratch::new();
        let path = dir.join("fields.json");
        std::fs::write(&path, "{\"user\": {\"id\": 7, \"tags\": [\"a\"]}}\n")
            .expect("the scratch directory accepts a file");
        let seat = Recorder::new();
        golden::matches_json_field(&seat, &path, "user", r#"{"id":7,"tags":["a"]}"#, false, &[]);
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn states_the_field_with_both_values() {
        let dir = Scratch::new();
        let path = dir.join("fields.json");
        std::fs::write(&path, "{\"count\": 1}\n").expect("the scratch directory accepts a file");
        let seat = Recorder::new();
        golden::matches_json_field(&seat, &path, "count", "2", false, &[]);
        let failure = &seat.failures()[0];
        assert_eq!(failure.assertion(), Some("golden-match-json-field"));
        assert_eq!(
            failure.detail(),
            [
                ("want", Value::from("1")),
                ("got", Value::from("2")),
                ("field", Value::from("count"))
            ]
        );
    }

    #[test]
    fn writes_the_field_and_keeps_every_other_field_on_an_update() {
        let dir = Scratch::new();
        let path = dir.join("fields.json");
        std::fs::write(&path, "{\"kept\": true}\n").expect("the scratch directory accepts a file");
        let seat = Recorder::new();
        golden::matches_json_field(&seat, &path, "added", "[1,2]", true, &[]);
        assert!(!seat.failed(), "{}", seat.message());
        let written = std::fs::read_to_string(&path).expect("the update writes the golden file");
        assert_eq!(
            written,
            "{\n  \"kept\": true,\n  \"added\": [\n    1,\n    2\n  ]\n}\n"
        );
    }

    #[test]
    fn replaces_the_value_of_the_field_on_an_update() {
        let dir = Scratch::new();
        let path = dir.join("fields.json");
        std::fs::write(&path, "{\"count\": 1, \"kept\": true}\n")
            .expect("the scratch directory accepts a file");
        let seat = Recorder::new();
        golden::matches_json_field(&seat, &path, "count", "2", true, &[]);
        assert!(!seat.failed(), "{}", seat.message());
        let written = std::fs::read_to_string(&path).expect("the update writes the golden file");
        assert_eq!(written, "{\n  \"count\": 2,\n  \"kept\": true\n}\n");
    }

    #[test]
    fn ends_the_call_with_a_fault_for_a_value_that_is_no_json() {
        let dir = Scratch::new();
        let seat = Recorder::new();
        golden::matches_json_field(&seat, dir.join("fields.json"), "count", "{", false, &[]);
        assert!(seat.message().contains("is no JSON"), "{}", seat.message());
        assert_eq!(records(&seat)[0]["verdict"], "error");
    }

    #[test]
    fn ends_the_call_with_a_fault_for_a_golden_file_that_is_no_object() {
        let dir = Scratch::new();
        let path = dir.join("fields.json");
        std::fs::write(&path, "[1]\n").expect("the scratch directory accepts a file");
        let seat = Recorder::new();
        golden::matches_json_field(&seat, &path, "count", "1", false, &[]);
        assert!(
            seat.message().contains("cannot be read as a JSON object"),
            "{}",
            seat.message()
        );
    }
}

mod scrubber {
    use super::*;

    #[test]
    fn applies_the_replacement_of_a_function() {
        let ports = Scrubber::new(|text| text.replace(":8080", ":PORT"));
        assert_eq!(ports.apply("localhost:8080"), "localhost:PORT");
        assert_eq!(format!("{ports:?}"), "Scrubber { .. }");
    }
}

mod scrub_timestamps {
    use super::*;

    #[test]
    fn replaces_each_form_of_a_timestamp() {
        let scrubber = golden::scrub_timestamps();
        for stamp in [
            "2026-10-21T08:00:00Z",
            "2026-10-21 08:00:00.123+02:00",
            "2026-10-21t08:00:00-0500",
        ] {
            assert_eq!(scrubber.apply(stamp), "SCRUBBED_TIMESTAMP", "{stamp}");
        }
    }
}

mod scrub_hashes {
    use super::*;

    #[test]
    fn replaces_a_digest_of_32_to_128_digits() {
        let scrubber = golden::scrub_hashes();
        assert_eq!(scrubber.apply(&"a".repeat(32)), "SCRUBBED_HASH");
        assert_eq!(scrubber.apply(&"F".repeat(128)), "SCRUBBED_HASH");
        assert_eq!(scrubber.apply(&"a".repeat(31)), "a".repeat(31));
    }
}

mod scrub_run_ids {
    use super::*;

    #[test]
    fn replaces_a_run_id_of_sixteen_characters() {
        let scrubber = golden::scrub_run_ids();
        assert_eq!(
            scrubber.apply("id run_0123456789abcdef."),
            "id SCRUBBED_RUN_ID."
        );
        assert_eq!(scrubber.apply("run_0123"), "run_0123");
    }
}

mod scrub_json_fields {
    use super::*;

    #[test]
    fn replaces_each_scalar_value_of_a_named_field() {
        let scrubber = golden::scrub_json_fields(&["id", "at"]);
        let text = r#"{"id": 12, "at": "2026", "ok": true, "nested": {"id": null}}"#;
        assert_eq!(
            scrubber.apply(text),
            r#"{"id": "SCRUBBED", "at": "SCRUBBED", "ok": true, "nested": {"id": "SCRUBBED"}}"#
        );
    }

    #[test]
    fn leaves_text_alone_without_a_field() {
        assert_eq!(
            golden::scrub_json_fields(&[]).apply(r#"{"id": 1}"#),
            r#"{"id": 1}"#
        );
    }
}

mod should_update {
    use super::*;

    #[test]
    #[ignore = "a spec of the update variable runs it in a child process"]
    fn prints_whether_to_update() {
        println!("update {}", golden::should_update());
    }

    #[test]
    fn updates_for_a_variable_other_than_the_empty_string_and_0() {
        let dir = Scratch::new();
        let tests = [("1", true), ("yes", true), ("0", false), ("", false)];
        for (give, want) in tests {
            let output = in_child(
                "golden::should_update::prints_whether_to_update",
                &dir.join(""),
                &[(golden::UPDATE_ENV, give)],
            );
            let printed = String::from_utf8_lossy(&output.stdout);
            assert!(
                printed.contains(&format!("update {want}")),
                "{give:?}: {printed}"
            );
        }
    }
}
