//! The spec of `prop::engine::store`: the format of a store entry, its name,
//! and the verdict that a run gives each file that it reads.

use dokimi_assert::Value;
use dokimi_assert::prop::engine::choice::Choice;
use dokimi_assert::prop::engine::store::{Failure, Verdict, entry, name, read};
use serde_json::json;

/// The contract of the entries of the spec.
const CONTRACT: &str = "n stays below 3";

/// Returns an entry of the format whose member `key` is `value`, or without
/// the member for a `value` of `None`.
fn with(key: &str, value: Option<serde_json::Value>) -> String {
    let mut stated = json!({
        "store": 1,
        "definition": "9.0.0",
        "property": CONTRACT,
        "identity": {"assertion": "true", "file": "store.rs", "line": 1},
        "choices": "prop1:AAU",
        "counterexample": [{"label": "n", "value": {"type": "int", "value": 5}}],
        "found": "2026-01-01"
    });
    let members = stated.as_object_mut().expect("an entry is an object");
    match value {
        Some(value) => members.insert(key.to_owned(), value),
        None => members.remove(key),
    };
    stated.to_string()
}

mod read {
    use super::*;

    #[test]
    fn replays_an_entry_of_the_property() {
        assert_eq!(
            read(&with("found", Some(json!("2024-02-29"))), CONTRACT),
            Verdict::Replay(vec![Choice::Integer(5)])
        );
        assert_eq!(
            read(&with("property", Some(json!("another"))), CONTRACT),
            Verdict::Other
        );
    }

    #[test]
    fn skips_an_entry_of_a_later_format_or_token() {
        assert_eq!(
            read(&with("store", Some(json!(2))), CONTRACT),
            Verdict::Skip
        );
        assert_eq!(
            read(&with("choices", Some(json!("prop2:AA"))), CONTRACT),
            Verdict::Skip
        );
        assert_eq!(
            read(
                &with("choices", Some(json!("prop99999999999999999999:AA"))),
                CONTRACT
            ),
            Verdict::Skip
        );
    }

    #[test]
    fn finds_a_file_that_is_no_entry_damaged() {
        let tests = [
            ("no JSON", "not json".to_owned()),
            ("no format", with("store", None)),
            ("a format of text", with("store", Some(json!("1")))),
            ("an earlier format", with("store", Some(json!(0)))),
            ("another member", with("extra", Some(json!(1)))),
            (
                "a version of two parts",
                with("definition", Some(json!("9.0"))),
            ),
            (
                "an identity of no object",
                with("identity", Some(json!("true"))),
            ),
            (
                "an identity of other keys",
                with("identity", Some(json!({"assertion": "true"}))),
            ),
            (
                "a line of 0",
                with(
                    "identity",
                    Some(json!({"assertion": "true", "file": "store.rs", "line": 0})),
                ),
            ),
            (
                "a file with a path",
                with(
                    "identity",
                    Some(json!({"assertion": "true", "file": "a/store.rs", "line": 1})),
                ),
            ),
            (
                "a draw without a label",
                with("counterexample", Some(json!([{"value": 1}]))),
            ),
            (
                "a date of other length",
                with("found", Some(json!("2026-1-1"))),
            ),
            (
                "a date of letters",
                with("found", Some(json!("2026-ab-01"))),
            ),
            ("a 13th month", with("found", Some(json!("2026-13-01")))),
            (
                "a leap day of a common year",
                with("found", Some(json!("2026-02-29"))),
            ),
            (
                "a token without its version",
                with("choices", Some(json!("prop:AA"))),
            ),
            (
                "a token with a leading zero",
                with("choices", Some(json!("prop01:AA"))),
            ),
            (
                "a token that does not read",
                with("choices", Some(json!("prop1:!"))),
            ),
        ];
        for (name, give) in tests {
            assert_eq!(read(&give, CONTRACT), Verdict::Damaged, "{name}");
        }
    }

    #[test]
    fn names_each_verdict_as_a_vector_states_it() {
        let names: Vec<&str> = [
            Verdict::Replay(Vec::new()),
            Verdict::Other,
            Verdict::Skip,
            Verdict::Damaged,
        ]
        .iter()
        .map(Verdict::name)
        .collect();
        assert_eq!(names, ["replay", "other", "skip", "damaged"]);
    }
}

mod entry {
    use super::*;

    #[test]
    fn writes_an_identity_value_of_another_kind_as_its_text() {
        let failure = Failure {
            contract: CONTRACT.to_owned(),
            choices: vec![Choice::Integer(5)],
            identity: vec![
                ("assertion".to_owned(), Value::Bool(true)),
                ("line".to_owned(), Value::Int(1)),
            ],
            counterexample: vec![("n".to_owned(), None)],
        };
        let written: serde_json::Value =
            serde_json::from_str(&entry(&failure, "9.0.0", "2026-01-01"))
                .expect("an entry is JSON");
        assert_eq!(written["identity"], json!({"assertion": "true", "line": 1}));
        assert_eq!(written["counterexample"], json!([{"label": "n"}]));
    }
}

mod name {
    use super::*;

    #[test]
    fn names_an_entry_after_its_contract_and_its_choices() {
        let named = name(CONTRACT, &[Choice::Integer(5)]);
        assert_eq!(named.len(), 21);
        assert_eq!(&named[16..], ".json", "{named}");
        assert!(
            named[..16].chars().all(|digit| digit.is_ascii_hexdigit()),
            "{named}"
        );
        assert_ne!(named, name(CONTRACT, &[Choice::Integer(6)]));
    }
}
