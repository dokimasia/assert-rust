// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `pattern`, the parser of the portable pattern subset.
//!
//! `check::matches` reads its pattern with the parser, so the spec drives the
//! parser through it. A pattern outside the subset fails the call, and the
//! field `reason` states the parser's refusal.

use dokimi_assert::{Recorder, Value, check};

/// Returns the field `reason` of a call of `matches` with `pattern` on the
/// empty text, or `None` when the call passes.
fn reason(pattern: &str) -> Option<Value> {
    let seat = Recorder::new();
    check::matches(&seat, "", pattern, "c");
    seat.failures()
        .first()
        .and_then(|failure| failure.field("reason").cloned())
}

/// Returns `true` when `pattern` matches `text`.
fn matches(pattern: &str, text: &str) -> bool {
    let seat = Recorder::new();
    check::matches(&seat, text, pattern, "c");
    !seat.failed()
}

mod parse {
    use super::*;

    /// Each entry of `testdata/pattern/verdicts.json` is the verdict of the
    /// definition's reference parser on the entry's pattern, with the
    /// position at which it refuses the pattern. A Rust string contains
    /// Unicode scalar values alone, so the file has no pattern with a lone
    /// surrogate.
    #[test]
    fn gives_each_pattern_the_verdict_of_the_reference_parser() {
        let text = include_str!("../testdata/pattern/verdicts.json");
        let verdicts: serde_json::Value =
            serde_json::from_str(text).expect("the verdicts are JSON");
        let verdicts = verdicts.as_array().expect("the verdicts are a list");
        assert!(
            verdicts.len() > 100,
            "the file states {} verdicts",
            verdicts.len()
        );
        for verdict in verdicts {
            let pattern = verdict["pattern"]
                .as_str()
                .expect("a verdict states a pattern");
            match (verdict["accepted"].as_bool(), reason(pattern)) {
                (Some(true), None | Some(Value::Null)) => {}
                (Some(false), Some(Value::String(refusal))) => {
                    let at = verdict["at"]
                        .as_u64()
                        .expect("a refusal states its position");
                    assert!(
                        refusal.contains(&format!(" at {at}: ")),
                        "{pattern:?} is refused at {at}: {refusal}"
                    );
                }
                (accepted, stated) => panic!(
                    "{pattern:?}: the verdict accepts {accepted:?}, and the reason is {stated:?}"
                ),
            }
        }
    }

    #[test]
    fn refuses_groups_nested_past_100() {
        let deep = format!("{}a{}", "(".repeat(101), ")".repeat(101));
        let refusal = reason(&deep);
        assert!(
            matches!(&refusal, Some(Value::String(text)) if text.contains("deeper than 100")),
            "{refusal:?}"
        );
        let shallow = format!("{}a{}", "(".repeat(100), ")".repeat(100));
        assert_eq!(reason(&shallow), Some(Value::Null));
    }

    #[test]
    fn refuses_a_class_without_a_member() {
        let refusal = reason("[^\u{0}-\u{10FFFF}]");
        assert!(
            matches!(&refusal, Some(Value::String(text)) if text.ends_with("a class in the pattern has no member")),
            "{refusal:?}"
        );
    }

    #[test]
    fn states_the_pattern_with_the_position_of_the_construct_that_it_refuses() {
        assert_eq!(
            reason("(a)\\1"),
            Some(Value::from(
                "pattern \"(a)\\\\1\" at 5: \\1 is not in the portable subset"
            ))
        );
    }
}

mod compile {
    use super::*;

    #[test]
    fn matches_anywhere_in_the_text() {
        assert!(matches("[0-9a-f]{8}", "id=deadbeef;"));
    }

    #[test]
    fn matches_dollar_at_the_end_of_the_text_alone() {
        assert!(matches("^abc$", "abc"));
        assert!(!matches("^abc$", "abc\n"));
    }

    #[test]
    fn reads_the_shorthands_as_ascii_classes() {
        assert!(!matches("^\\d$", "\u{663}"));
        assert!(!matches("^\\w$", "\u{e9}"));
        assert!(!matches("^\\s$", "\u{A0}"));
        assert!(matches("^[\\s]$", "\u{C}"));
    }

    #[test]
    fn matches_no_line_terminator_with_a_dot() {
        for terminator in ["\n", "\r", "\u{85}", "\u{2028}", "\u{2029}"] {
            assert!(
                !matches("a.b", &format!("a{terminator}b")),
                "{terminator:?}"
            );
        }
        assert!(matches("a.b", "a-b"));
    }

    #[test]
    fn matches_an_escaped_metacharacter_literally() {
        assert!(matches("^a\\.b\\$$", "a.b$"));
        assert!(!matches("^a\\.b$", "axb"));
        assert!(matches("^[\\-\\]]+$", "-]"));
    }

    #[test]
    fn states_the_refusal_of_the_engine_without_a_position() {
        let pattern = format!("(?:{})", [".{1000}"; 40].join("|"));
        let refusal = reason(&pattern);
        let prefix = format!("pattern {pattern:?}: ");
        assert!(
            matches!(&refusal, Some(Value::String(text)) if text.starts_with(&prefix) && text.contains("size limit")),
            "{refusal:?}"
        );
    }
}
