// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `files/workspace`: the workspace of a test, and the helpers
//! that write and read the files of a directory.

use crate::helpers::{listing, written};
use dokimi_assert::Recorder;
use dokimi_assert::files::{self, Tree};
use std::path::Path;

mod workspace {
    use super::*;

    #[test]
    fn writes_the_tree_into_a_new_directory_below_the_temporary_directory() {
        let dir = written(&Tree::from([(
            "go.mod",
            files::text("module example.com/a\n"),
        )]));
        assert!(dir.path().starts_with(std::env::temp_dir()));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("go.mod")).expect("the file reads"),
            "module example.com/a\n"
        );
        assert_ne!(dir.path(), written(&Tree::new()).path());
    }

    #[test]
    fn removes_the_directory_when_it_drops() {
        let dir = written(&Tree::from([("docs/a.md", files::text("# a\n"))]));
        let path = dir.path().to_path_buf();
        assert!(path.is_dir());
        drop(dir);
        assert!(!path.exists());
    }

    #[test]
    fn ends_the_call_with_a_fault_before_it_writes_for_a_tree_that_breaks_a_rule() {
        let seat = Recorder::new();
        let tree = Tree::from([("a.txt", files::text("")), ("a//b", files::text(""))]);
        let dir = files::workspace(&seat, &tree);
        assert_eq!(
            seat.message(),
            r#"files::workspace: a//b: the path "a//b" has the name """#
        );
        let faults: Vec<String> = seat.faults().iter().map(ToString::to_string).collect();
        assert_eq!(faults, [seat.message()]);
        assert_eq!(listing(dir.path()), [] as [String; 0]);
    }

    #[cfg(unix)]
    #[test]
    #[ignore = "a spec of workspace runs it in a child process"]
    fn writes_a_workspace_into_the_temporary_directory_of_the_process() {
        let seat = Recorder::new();
        let dir = files::workspace(&seat, &Tree::new());
        println!("message {}", seat.message());
        println!("path {:?}", dir.path());
    }

    #[cfg(unix)]
    #[test]
    fn ends_the_call_with_a_fault_for_a_directory_that_cannot_be_created() {
        use crate::helpers::{Scratch, in_child};
        let scratch = Scratch::new();
        let missing = scratch.join("missing");
        let output = in_child(
            "files::workspace::workspace::writes_a_workspace_into_the_temporary_directory_of_the_process",
            &scratch.join(""),
            &[("TMPDIR", missing.to_str().expect("the path is UTF-8 text"))],
        );
        let printed = String::from_utf8_lossy(&output.stdout);
        assert!(output.status.success(), "{printed}");
        assert!(
            printed.contains("message files::workspace: the directory cannot be created: "),
            "{printed}"
        );
        assert!(printed.contains(r#"path """#), "{printed}");
    }
}

mod path {
    use super::*;

    #[test]
    fn returns_the_directory_that_as_ref_returns() {
        let dir = written(&Tree::new());
        let path: &Path = dir.as_ref();
        assert_eq!(dir.path(), path);
    }
}

mod write {
    use super::*;

    #[test]
    fn ends_the_test_with_a_fault_for_a_tree_that_breaks_a_rule() {
        let dir = written(&Tree::new());
        let seat = Recorder::new();
        files::write(&seat, &dir, &Tree::from([("./a", files::text(""))]));
        assert_eq!(
            seat.message(),
            r#"files::write: ./a: the path "./a" has the name ".""#
        );
        assert_eq!(seat.messages(), [] as [String; 0]);
    }
}

mod read {
    use super::*;

    #[test]
    fn returns_the_text_of_a_file() {
        let dir = written(&Tree::from([("VERSION", files::text("version 2\n"))]));
        let seat = Recorder::new();
        assert_eq!(
            files::read(&seat, dir.path().join("VERSION")),
            "version 2\n"
        );
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn ends_the_call_with_a_fault_for_a_path_where_no_file_is() {
        let dir = written(&Tree::from([("docs", files::directory())]));
        let seat = Recorder::new();
        let path = dir.path().join("docs");
        assert_eq!(files::read(&seat, &path), "");
        assert_eq!(
            seat.message(),
            format!("files::read: no file is at the path {}", path.display())
        );
    }

    #[test]
    fn ends_the_call_with_a_fault_for_a_file_that_is_no_utf8_text() {
        let dir = written(&Tree::from([("logo.png", files::bytes([0x89, 0x50]))]));
        let seat = Recorder::new();
        let path = dir.path().join("logo.png");
        assert_eq!(files::read(&seat, &path), "");
        assert_eq!(
            seat.message(),
            format!(
                "files::read: the file at {} is no UTF-8 text",
                path.display()
            )
        );
    }

    #[cfg(unix)]
    #[test]
    fn ends_the_call_with_a_fault_for_a_link_to_a_file() {
        let dir = written(&Tree::from([
            ("a.txt", files::text("a\n")),
            ("current", files::link("a.txt")),
        ]));
        let seat = Recorder::new();
        let path = dir.path().join("current");
        assert_eq!(files::read(&seat, &path), "");
        assert!(
            seat.message()
                .starts_with("files::read: no file is at the path "),
            "{}",
            seat.message()
        );
    }
}
