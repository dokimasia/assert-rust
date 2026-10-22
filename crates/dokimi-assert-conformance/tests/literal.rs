// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `literal`, the decoder of the definition's typed literals.

use dokimi_assert::Value;
use dokimi_assert::files::{self, Tree};
use dokimi_assert_conformance::literal::{self, Difference, Malformed, Rendering};
use serde_json::json;

mod decode {
    use super::*;

    #[test]
    fn decodes_each_scalar() {
        let tests = [
            (json!({"type": "null"}), Value::Null),
            (json!({"type": "bool", "value": true}), Value::Bool(true)),
            (json!({"type": "int", "value": -7}), Value::Int(-7)),
            (
                json!({"type": "int", "value": "9007199254740993"}),
                Value::Int(9_007_199_254_740_993),
            ),
            (json!({"type": "float", "value": 0.5}), Value::Float(0.5)),
            (
                json!({"type": "float", "value": "-Inf"}),
                Value::Float(f64::NEG_INFINITY),
            ),
            (json!({"type": "string", "value": "a"}), Value::from("a")),
            (
                json!({"type": "bytes", "value": "890a"}),
                Value::Bytes(vec![0x89, 0x0a]),
            ),
            (
                json!({"type": "opaque", "text": "Box<dyn Error>"}),
                Value::Opaque("Box<dyn Error>".to_owned()),
            ),
        ];
        for (give, want) in tests {
            assert_eq!(literal::decode(&give), Ok(want), "{give}");
        }
    }

    #[test]
    fn decodes_nan_by_name() {
        let decoded = literal::decode(&json!({"type": "float", "value": "NaN"}));
        assert!(
            matches!(decoded, Ok(Value::Float(x)) if x.is_nan()),
            "{decoded:?}"
        );
    }

    #[test]
    fn decodes_each_container() {
        let tests = [
            (
                json!({"type": "list", "of": "int", "value": [1, 2]}),
                Value::List(vec![Value::Int(1), Value::Int(2)]),
            ),
            (
                json!({"type": "list", "of": "string", "value": null}),
                Value::Null,
            ),
            (
                json!({"type": "list", "items": [{"type": "null"}]}),
                Value::List(vec![Value::Null]),
            ),
            (
                json!({"type": "map", "entries": [[{"type": "int", "value": 1}, {"type": "bool", "value": false}]]}),
                Value::Map(vec![(Value::Int(1), Value::Bool(false))]),
            ),
            (
                json!({"type": "map", "key": "string", "of": "float", "value": {"a": 1}}),
                Value::Map(vec![(Value::from("a"), Value::Float(1.0))]),
            ),
            (
                json!({"type": "map", "key": "string", "of": "int", "value": null}),
                Value::Null,
            ),
            (
                json!({"type": "record", "fields": [["id", {"type": "int", "value": 1}]]}),
                Value::Record(vec![("id".to_owned(), Value::Int(1))]),
            ),
            (
                json!({"type": "variant", "name": "Paid", "payload": {"type": "null"}}),
                Value::Variant {
                    name: "Paid".to_owned(),
                    payload: Some(Box::new(Value::Null)),
                },
            ),
            (
                json!({"type": "variant", "name": "Pending"}),
                Value::Variant {
                    name: "Pending".to_owned(),
                    payload: None,
                },
            ),
            (
                json!({"type": "reference", "id": "a", "value": {"type": "int", "value": 3}}),
                Value::Int(3),
            ),
        ];
        for (give, want) in tests {
            assert_eq!(literal::decode(&give), Ok(want), "{give}");
        }
    }

    #[test]
    fn refuses_a_malformed_literal() {
        let tests = [
            json!({"value": 1}),
            json!({"type": "date", "value": 1}),
            json!({"type": "bool", "value": 1}),
            json!({"type": "int", "value": 1.5}),
            json!({"type": "int", "value": "x"}),
            json!({"type": "float", "value": "inf"}),
            json!({"type": "bytes", "value": "abc"}),
            json!({"type": "bytes", "value": "zz"}),
            json!({"type": "list", "of": "date", "value": [1]}),
            json!({"type": "list", "of": "int", "value": 1}),
            json!({"type": "list"}),
            json!({"type": "map", "of": "int", "value": [1]}),
            json!({"type": "map", "entries": [[{"type": "null"}]]}),
            json!({"type": "record", "fields": [[1, {"type": "null"}]]}),
            json!({"type": "variant"}),
            json!({"type": "variant", "name": "V", "payload": {"type": "date"}}),
            json!({"type": "opaque"}),
        ];
        for give in tests {
            let decoded = literal::decode(&give);
            assert_eq!(decoded, Err(Malformed(give.clone())), "{give}");
        }
    }

    #[test]
    fn renders_a_malformed_literal() {
        let malformed = Malformed(json!({"type": "date"}));
        assert_eq!(
            malformed.to_string(),
            r#"a malformed typed literal: {"type":"date"}"#
        );
    }

    #[test]
    fn decodes_a_tree() {
        let give = json!({"type": "tree", "entries": [{"path": "a.txt", "text": "a\n"}]});
        let want = Tree::from([("a.txt", files::text("a\n"))]);
        assert_eq!(literal::decode(&give), Ok(Value::Tree(want)));
    }
}

mod tree {
    use super::*;

    #[test]
    fn builds_each_entry_with_the_constructors_of_files() {
        let give = json!({"type": "tree", "entries": [
            {"path": "bin/run", "text": "#!/bin/sh\n", "executable": true},
            {"path": "cache", "directory": true},
            {"path": "current", "link": "bin/run"},
            {"path": "keys", "directory": true, "mode": 448},
            {"path": "keys/id", "text": "secret\n", "mode": 384},
            {"path": "logo.png", "bytes": "8950"},
        ]});
        let want = Tree::from([
            ("bin/run", files::executable("#!/bin/sh\n")),
            ("cache", files::directory()),
            ("current", files::link("bin/run")),
            ("keys", files::directory().with_mode(0o700)),
            ("keys/id", files::text("secret\n").with_mode(0o600)),
            ("logo.png", files::bytes([0x89, 0x50])),
        ]);
        assert_eq!(literal::tree(&give), Ok(want));
    }

    #[test]
    fn refuses_a_literal_that_states_no_tree_of_an_input() {
        let tests = [
            json!({"type": "list", "entries": []}),
            json!({"type": "tree"}),
            json!({"type": "tree", "entries": [{"text": "a"}]}),
            json!({"type": "tree", "entries": [{"path": "a", "text": "a", "bytes": "61"}]}),
            json!({"type": "tree", "entries": [{"path": "a"}]}),
            json!({"type": "tree", "entries": [{"path": "a", "directory": false}]}),
            json!({"type": "tree", "entries": [{"path": "a", "bytes": "61", "executable": true}]}),
            json!({"type": "tree", "entries": [{"path": "a", "text": "a", "mode": 512}]}),
            json!({"type": "tree", "entries": [{"path": "a", "link": "b", "mode": 420}]}),
            json!({"type": "tree", "entries": [{"path": "a", "digest": "sha256:00", "size": 1}]}),
        ];
        for give in tests {
            assert_eq!(literal::tree(&give), Err(Malformed(give.clone())), "{give}");
        }
    }
}

mod entries {
    use super::*;

    #[test]
    fn returns_the_entries_in_the_order_of_the_literal() {
        let give = json!({"type": "tree", "entries": [
            {"path": "b.txt", "text": "b"},
            {"path": "a.txt", "text": "a"},
        ]});
        assert_eq!(
            literal::entries(&give),
            Ok(vec![
                ("b.txt".to_owned(), files::text("b")),
                ("a.txt".to_owned(), files::text("a")),
            ])
        );
        let refused = json!({"type": "tree"});
        assert_eq!(literal::entries(&refused), Err(Malformed(refused.clone())));
    }
}

mod differs {
    use super::*;

    #[test]
    fn agrees_with_an_equal_value() {
        assert_eq!(
            literal::differs(&Value::Int(1), &Value::Int(1), Rendering::Values),
            None
        );
    }

    #[test]
    fn compares_a_float_by_its_bits() {
        let rendering = Rendering::Values;
        assert_eq!(
            literal::differs(&Value::Float(f64::NAN), &Value::Float(f64::NAN), rendering),
            None
        );
        assert!(literal::differs(&Value::Float(0.0), &Value::Float(-0.0), rendering).is_some());
    }

    #[test]
    fn compares_the_entries_of_a_map_in_any_order() {
        let a = Value::Map(vec![
            (Value::Int(1), Value::Int(2)),
            (Value::Int(3), Value::Int(4)),
        ]);
        let b = Value::Map(vec![
            (Value::Int(3), Value::Int(4)),
            (Value::Int(1), Value::Int(2)),
        ]);
        assert_eq!(literal::differs(&a, &b, Rendering::Values), None);
    }

    #[test]
    fn compares_the_elements_of_a_list_and_the_fields_of_a_record() {
        let list = Value::List(vec![Value::Float(f64::NAN)]);
        assert_eq!(
            literal::differs(&list, &list.clone(), Rendering::Values),
            None
        );
        let record = Value::Record(vec![("a".to_owned(), Value::Float(f64::NAN))]);
        assert_eq!(
            literal::differs(&record, &record.clone(), Rendering::Values),
            None
        );
        let renamed = Value::Record(vec![("b".to_owned(), Value::Float(f64::NAN))]);
        assert!(literal::differs(&record, &renamed, Rendering::Values).is_some());
    }

    #[test]
    fn states_both_values_of_a_difference() {
        let difference = literal::differs(&Value::Int(1), &Value::Int(2), Rendering::Values);
        assert_eq!(
            difference,
            Some(Difference {
                stated: "Int(1)".to_owned(),
                expected: "Int(2)".to_owned(),
            })
        );
        assert_eq!(
            difference
                .map(|difference| difference.to_string())
                .as_deref(),
            Some("the record states Int(1), and the case states Int(2)")
        );
    }

    #[test]
    fn reads_an_opaque_value_as_the_debug_text_of_the_rendering() {
        let list = Value::List(vec![Value::Int(1), Value::Int(2)]);
        let tests = [
            (Rendering::Values, "List([Int(1), Int(2)])"),
            (Rendering::Slices, "[Int(1), Int(2)]"),
            (Rendering::Native, "[1, 2]"),
        ];
        for (rendering, text) in tests {
            let got = Value::Opaque(text.to_owned());
            assert_eq!(
                literal::differs(&got, &list, rendering),
                None,
                "{rendering:?}"
            );
        }
    }

    #[test]
    fn renders_each_native_value_as_its_debug_text() {
        let tests = [
            (Value::Int(-3), "-3"),
            (Value::Float(1.0), "1.0"),
            (Value::Bool(true), "true"),
            (Value::from("a"), "\"a\""),
            (Value::Null, "None"),
            (Value::Bytes(vec![1]), "Bytes([1])"),
        ];
        for (give, text) in tests {
            let got = Value::Opaque(text.to_owned());
            assert_eq!(
                literal::differs(&got, &give, Rendering::Native),
                None,
                "{give:?}"
            );
        }
    }
}
