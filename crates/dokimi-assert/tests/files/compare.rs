// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `files/compare`: the comparisons of the tree in a directory
//! with a wanted tree.

use crate::helpers::{Scratch, panic_of, records, written};
use dokimi_assert::files::{self, Tree};
use dokimi_assert::{Recorder, Value};
use std::path::Path;

/// Returns the detail of a failing comparison: the two trees and the number
/// of the paths that differ.
fn detail(want: Tree, got: Tree, differences: i128) -> [(&'static str, Value); 3] {
    [
        ("want", Value::Tree(want)),
        ("got", Value::Tree(got)),
        ("differences", Value::Int(differences)),
    ]
}

mod equal {
    use super::*;

    #[test]
    fn passes_the_tree_of_the_workspace() {
        let tree = Tree::from([
            ("cache", files::directory()),
            ("go.mod", files::text("module example.com/a\n")),
            ("logo.png", files::bytes([0x89, 0x50, 0x4e, 0x47])),
        ]);
        let dir = written(&tree);
        let seat = Recorder::new();
        files::equal(&seat, &dir, &tree, "c");
        assert!(!seat.failed(), "{}", seat.message());
        assert_eq!(records(&seat)[0]["assertion"], "tree-equal");
    }

    #[test]
    fn passes_a_stated_directory_that_an_entry_implies() {
        let dir = written(&Tree::from([("docs/a.md", files::text("# a\n"))]));
        let seat = Recorder::new();
        let want = Tree::from([
            ("docs", files::directory()),
            ("docs/a.md", files::text("# a\n")),
        ]);
        files::equal(&seat, &dir, &want, "c");
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn states_a_missing_entry_in_want_alone() {
        let dir = written(&Tree::from([("a.txt", files::text("a\n"))]));
        let seat = Recorder::new();
        let want = Tree::from([("a.txt", files::text("a\n")), ("b.txt", files::text("b\n"))]);
        files::equal(&seat, &dir, &want, "c");
        let failure = &seat.failures()[0];
        assert_eq!(failure.assertion(), Some("tree-equal"));
        assert_eq!(
            failure.detail(),
            detail(Tree::from([("b.txt", files::text("b\n"))]), Tree::new(), 1)
        );
    }

    #[test]
    fn states_an_extra_entry_in_got_alone() {
        let dir = written(&Tree::from([
            ("a.txt", files::text("a\n")),
            ("cache", files::directory()),
        ]));
        let seat = Recorder::new();
        files::equal(
            &seat,
            &dir,
            &Tree::from([("a.txt", files::text("a\n"))]),
            "c",
        );
        assert_eq!(
            seat.failures()[0].detail(),
            detail(Tree::new(), Tree::from([("cache", files::directory())]), 1)
        );
    }

    #[test]
    fn states_a_changed_entry_in_both() {
        let dir = written(&Tree::from([("cache", files::text(""))]));
        let seat = Recorder::new();
        files::equal(
            &seat,
            &dir,
            &Tree::from([("cache", files::directory())]),
            "c",
        );
        assert_eq!(
            seat.failures()[0].detail(),
            detail(
                Tree::from([("cache", files::directory())]),
                Tree::from([("cache", files::text(""))]),
                1
            )
        );
    }

    #[test]
    fn lists_the_first_64_paths_that_differ() {
        let names: Vec<String> = (0..65).map(|at| format!("f{at:02}.txt")).collect();
        let dir = written(
            &names
                .iter()
                .map(|name| (name.as_str(), files::text("a")))
                .collect::<Tree>(),
        );
        let want: Tree = names
            .iter()
            .map(|name| (name.as_str(), files::text("b")))
            .collect();
        let seat = Recorder::new();
        files::equal(&seat, &dir, &want, "c");
        let listed: Tree = names[..64]
            .iter()
            .map(|name| (name.as_str(), files::text("b")))
            .collect();
        let read: Tree = names[..64]
            .iter()
            .map(|name| (name.as_str(), files::text("a")))
            .collect();
        assert_eq!(seat.failures()[0].detail(), detail(listed, read, 65));
    }

    #[test]
    fn names_the_line_of_the_call_and_ends_the_test() {
        let dir = written(&Tree::new());
        let seat = Recorder::new();
        let line = line!() + 1;
        files::equal(&seat, &dir, &Tree::from([("a.txt", files::text(""))]), "c");
        let at = seat.failures()[0]
            .location()
            .expect("a failure states its call site");
        assert_eq!(at.line(), line);
        assert!(Path::new(at.file()).ends_with("files/compare.rs"), "{at}");
        assert_eq!(records(&seat)[0]["aborting"], true);
        assert_eq!(seat.messages(), [] as [String; 0]);
    }

    #[cfg(unix)]
    #[test]
    fn compares_the_execute_bit_of_a_file_that_states_no_mode() {
        let dir = written(&Tree::from([("run.sh", files::executable("#!/bin/sh\n"))]));
        let seat = Recorder::new();
        files::equal(
            &seat,
            &dir,
            &Tree::from([("run.sh", files::text("#!/bin/sh\n"))]),
            "c",
        );
        assert_eq!(
            seat.failures()[0].detail(),
            detail(
                Tree::from([("run.sh", files::text("#!/bin/sh\n"))]),
                Tree::from([("run.sh", files::executable("#!/bin/sh\n"))]),
                1
            )
        );
    }

    #[cfg(unix)]
    #[test]
    fn compares_no_mode_that_want_does_not_state() {
        let dir = written(&Tree::from([
            ("keys", files::directory().with_mode(0o700)),
            ("keys/id", files::text("secret\n").with_mode(0o600)),
        ]));
        let seat = Recorder::new();
        files::equal(
            &seat,
            &dir,
            &Tree::from([("keys/id", files::text("secret\n"))]),
            "c",
        );
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[cfg(unix)]
    #[test]
    fn states_the_mode_of_an_entry_read_where_want_states_none_by_its_execute_bit_alone() {
        let dir = written(&Tree::from([("a.txt", files::text("x").with_mode(0o751))]));
        let seat = Recorder::new();
        files::equal(&seat, &dir, &Tree::from([("a.txt", files::text("y"))]), "c");
        assert_eq!(
            seat.failures()[0].got(),
            Some(&Value::Tree(Tree::from([(
                "a.txt",
                files::executable("x")
            )])))
        );
    }

    #[cfg(unix)]
    #[test]
    fn compares_a_link_by_its_target() {
        let dir = written(&Tree::from([
            ("a.txt", files::text("a\n")),
            ("current", files::link("a.txt")),
        ]));
        let seat = Recorder::new();
        let want = Tree::from([
            ("a.txt", files::text("a\n")),
            ("current", files::link("b.txt")),
        ]);
        files::equal(&seat, &dir, &want, "c");
        assert_eq!(
            seat.failures()[0].detail(),
            detail(
                Tree::from([("current", files::link("b.txt"))]),
                Tree::from([("current", files::link("a.txt"))]),
                1
            )
        );
    }
}

mod contains {
    use super::*;

    #[test]
    fn passes_a_subset_of_the_tree() {
        let dir = written(&Tree::from([
            ("a.txt", files::text("a\n")),
            ("docs/b.md", files::text("# b\n")),
        ]));
        let seat = Recorder::new();
        files::contains(
            &seat,
            &dir,
            &Tree::from([("docs/b.md", files::text("# b\n"))]),
            "c",
        );
        assert!(!seat.failed(), "{}", seat.message());
        assert_eq!(records(&seat)[0]["assertion"], "tree-contains");
    }

    #[test]
    fn passes_an_entry_below_a_stated_directory() {
        let dir = written(&Tree::from([("cache/x.bin", files::bytes([0, 0xff]))]));
        let seat = Recorder::new();
        files::contains(
            &seat,
            &dir,
            &Tree::from([("cache", files::directory())]),
            "c",
        );
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn states_a_missing_entry() {
        let dir = written(&Tree::from([("a.txt", files::text("a\n"))]));
        let seat = Recorder::new();
        files::contains(
            &seat,
            &dir,
            &Tree::from([("b.txt", files::text("b\n"))]),
            "c",
        );
        let failure = &seat.failures()[0];
        assert_eq!(failure.assertion(), Some("tree-contains"));
        assert_eq!(
            failure.detail(),
            detail(Tree::from([("b.txt", files::text("b\n"))]), Tree::new(), 1)
        );
    }

    #[test]
    fn ends_the_call_with_a_fault_for_a_tree_that_cannot_be_read() {
        let dir = Scratch::new();
        let seat = Recorder::new();
        files::contains(&seat, dir.join("missing"), &Tree::new(), "c");
        assert!(
            seat.message()
                .starts_with("files::contains: the tree cannot be read: "),
            "{}",
            seat.message()
        );
        assert_eq!(records(&seat)[0]["verdict"], "error");
    }
}

mod unchanged {
    use super::*;

    #[test]
    fn passes_a_body_that_reads_every_file() {
        let dir = written(&Tree::from([
            ("a.txt", files::text("a\n")),
            ("docs/b.md", files::text("# b\n")),
        ]));
        let seat = Recorder::new();
        let reads = || {
            std::fs::read(dir.path().join("a.txt")).expect("the file reads");
            std::fs::read(dir.path().join("docs/b.md")).expect("the file reads");
        };
        files::unchanged(&seat, &dir, reads, "c");
        assert!(!seat.failed(), "{}", seat.message());
        assert_eq!(records(&seat)[0]["assertion"], "tree-unchanged");
    }

    #[test]
    fn passes_a_body_that_writes_the_bytes_that_a_file_has() {
        let dir = written(&Tree::from([("a.txt", files::text("a\n"))]));
        let seat = Recorder::new();
        let rewrites = || std::fs::write(dir.path().join("a.txt"), "a\n").expect("the file writes");
        files::unchanged(&seat, &dir, rewrites, "c");
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn states_the_tree_before_the_call_as_want() {
        let dir = written(&Tree::from([("a.txt", files::text("a\n"))]));
        let seat = Recorder::new();
        let writes = || std::fs::write(dir.path().join("new.txt"), "new").expect("the file writes");
        files::unchanged(&seat, &dir, writes, "c");
        let failure = &seat.failures()[0];
        assert_eq!(failure.assertion(), Some("tree-unchanged"));
        assert_eq!(
            failure.detail(),
            detail(
                Tree::new(),
                Tree::from([("new.txt", files::text("new"))]),
                1
            )
        );
    }

    #[cfg(unix)]
    #[test]
    fn compares_the_modes_of_the_tree_before_the_call() {
        use std::os::unix::fs::PermissionsExt;
        let dir = written(&Tree::from([("a.txt", files::text("a\n"))]));
        let seat = Recorder::new();
        let opens = || {
            std::fs::set_permissions(
                dir.path().join("a.txt"),
                std::fs::Permissions::from_mode(0o666),
            )
            .expect("the file takes the mode");
        };
        files::unchanged(&seat, &dir, opens, "c");
        assert_eq!(
            seat.failures()[0].detail(),
            detail(
                Tree::from([("a.txt", files::text("a\n").with_mode(0o644))]),
                Tree::from([("a.txt", files::text("a\n").with_mode(0o666))]),
                1
            )
        );
    }

    #[test]
    fn ends_the_call_with_a_fault_without_calling_the_body_for_a_tree_that_cannot_be_read() {
        let dir = Scratch::new();
        let seat = Recorder::new();
        let called = std::cell::Cell::new(false);
        files::unchanged(&seat, dir.join("missing"), || called.set(true), "c");
        assert!(
            seat.message()
                .starts_with("files::unchanged: the tree cannot be read: "),
            "{}",
            seat.message()
        );
        assert!(!called.get());
    }

    #[test]
    fn panics_again_with_the_panic_of_the_body() {
        let dir = written(&Tree::new());
        let seat = Recorder::new();
        let raised = panic_of(|| files::unchanged(&seat, &dir, || panic!("the body broke"), "c"));
        assert_eq!(raised.as_deref(), Some("the body broke"));
        assert!(!seat.failed());
    }
}
