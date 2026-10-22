// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `files/tree`: a tree of files, its rules and its literal.

use crate::helpers::{Scratch, records, written};
use dokimi_assert::Recorder;
use dokimi_assert::files::{self, Tree};
use serde_json::json;

mod tree {
    use super::*;

    #[test]
    fn has_no_entries_when_new() {
        let tree = Tree::new();
        assert!(tree.is_empty());
        assert_eq!(tree.len(), 0);
        assert_eq!(tree, Tree::default());
    }

    #[test]
    fn returns_the_entry_that_was_at_the_path_of_an_insert() {
        let mut tree = Tree::new();
        assert_eq!(tree.insert("a.txt", files::text("a\n")), None);
        assert_eq!(
            tree.insert("a.txt", files::text("b\n")),
            Some(files::text("a\n"))
        );
        assert_eq!(tree.get("a.txt"), Some(&files::text("b\n")));
        assert_eq!(tree.get("b.txt"), None);
        assert_eq!(tree.len(), 1);
    }

    #[test]
    fn keeps_its_entries_in_the_order_of_the_bytes_of_their_paths() {
        let tree = Tree::from([
            ("b", files::text("")),
            ("a/b", files::text("")),
            ("B", files::directory()),
        ]);
        assert_eq!(
            format!("{tree:?}"),
            r#"{"B": directory(), "a/b": text(""), "b": text("")}"#
        );
    }

    #[test]
    fn collects_the_paths_and_the_entries_of_an_iterator() {
        let tree: Tree = [("a.txt".to_owned(), files::text("a\n"))]
            .into_iter()
            .collect();
        assert_eq!(tree, Tree::from([("a.txt", files::text("a\n"))]));
    }
}

mod check {
    use super::*;

    #[test]
    fn ends_the_call_with_a_fault_for_a_tree_that_breaks_a_rule() {
        struct Case {
            name: &'static str,
            give: Tree,
            want_fault: &'static str,
        }
        let tests = [
            Case {
                name: "an empty name",
                give: Tree::from([("a//b", files::text(""))]),
                want_fault: r#"a//b: the path "a//b" has the name """#,
            },
            Case {
                name: "a dot",
                give: Tree::from([("./a", files::text(""))]),
                want_fault: r#"./a: the path "./a" has the name ".""#,
            },
            Case {
                name: "two dots",
                give: Tree::from([("a/..", files::text(""))]),
                want_fault: r#"a/..: the path "a/.." has the name "..""#,
            },
            Case {
                name: "a backslash",
                give: Tree::from([("a\\b", files::text(""))]),
                want_fault: r#"a\b: the path "a\\b" has the name "a\\b""#,
            },
            Case {
                name: "a NUL",
                give: Tree::from([("a\0", files::text(""))]),
                want_fault: "a\0: the path \"a\\0\" has the name \"a\\0\"",
            },
            Case {
                name: "an entry below a file",
                give: Tree::from([("a.txt", files::text("")), ("a.txt/b", files::text(""))]),
                want_fault: r#"a.txt/b: the entry is below "a.txt", which is a file"#,
            },
            Case {
                name: "an entry below a link",
                give: Tree::from([
                    ("current", files::link("a")),
                    ("current/b", files::text("")),
                ]),
                want_fault: r#"current/b: the entry is below "current", which is a link"#,
            },
            Case {
                name: "a link without a target",
                give: Tree::from([("current", files::link(""))]),
                want_fault: r#"current: the link to "" states no target of text without NUL"#,
            },
            Case {
                name: "a target with a NUL",
                give: Tree::from([("current", files::link("a\0"))]),
                want_fault: r#"current: the link to "a\0" states no target of text without NUL"#,
            },
        ];
        let dir = Scratch::new();
        for tt in tests {
            let seat = Recorder::new();
            files::equal(&seat, dir.join(""), &tt.give, "c");
            assert_eq!(
                seat.message(),
                format!("files::equal: {}", tt.want_fault),
                "{}",
                tt.name
            );
            assert_eq!(records(&seat)[0]["verdict"], "error", "{}", tt.name);
        }
    }
}

mod literal {
    use super::*;

    #[test]
    fn states_each_kind_of_entry_with_the_directories_that_the_tree_implies() {
        let dir = Scratch::new();
        let want = Tree::from([
            ("bin/run", files::executable("#!/bin/sh\n")),
            ("cache", files::directory()),
            ("current", files::link("bin/run")),
            ("keys", files::directory().with_mode(0o700)),
            ("keys/id", files::text("secret\n").with_mode(0o600)),
            ("logo.png", files::bytes([0x89, 0x50])),
        ]);
        let seat = Recorder::new();
        files::equal(&seat, dir.join(""), &want, "c");
        assert_eq!(
            records(&seat)[0]["detail"],
            json!({
                "want": {"type": "tree", "entries": [
                    {"path": "bin", "directory": true},
                    {"path": "bin/run", "text": "#!/bin/sh\n", "executable": true},
                    {"path": "cache", "directory": true},
                    {"path": "current", "link": "bin/run"},
                    {"path": "keys", "directory": true, "mode": 448},
                    {"path": "keys/id", "text": "secret\n", "mode": 384},
                    {"path": "logo.png", "bytes": "8950"},
                ]},
                "got": {"type": "tree", "entries": []},
                "differences": {"type": "int", "value": 7},
            })
        );
    }

    #[test]
    fn states_a_content_longer_than_65536_bytes_by_its_digest_and_its_size() {
        let dir = written(&Tree::from([(
            "large.bin",
            files::text("a".repeat(65_537)),
        )]));
        let seat = Recorder::new();
        files::equal(&seat, &dir, &Tree::new(), "c");
        assert_eq!(
            records(&seat)[0]["detail"]["got"],
            json!({"type": "tree", "entries": [{
                "path": "large.bin",
                "digest": "sha256:008ffc88d3c96a9f307524eb361e47c5222a887fc45fa0c1fb8d429c5c23b430",
                "size": 65_537,
            }]})
        );
    }
}
