// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `files/writer`: writing a tree into the directory of a
//! workspace, into a directory that exists, and into a golden directory.
//!
//! A spec of an entry that cannot be read or written runs as a user other
//! than root, because root writes where a mode forbids it.

use crate::helpers::{in_child, records, written};
use dokimi_assert::files::{self, Tree, Workspace};
use dokimi_assert::{Recorder, golden};
use std::fs;

#[cfg(unix)]
use crate::helpers::listing;

mod create {
    use super::*;

    #[test]
    fn writes_each_entry_of_the_tree() {
        let dir = written(&Tree::from([
            ("cache", files::directory()),
            ("go.mod", files::text("module example.com/a\n")),
            ("logo.png", files::bytes([0x89, 0x50, 0x4e, 0x47])),
        ]));
        assert!(dir.path().join("cache").is_dir());
        assert_eq!(
            fs::read_to_string(dir.path().join("go.mod")).expect("the file reads"),
            "module example.com/a\n"
        );
        assert_eq!(
            fs::read(dir.path().join("logo.png")).expect("the file reads"),
            [0x89, 0x50, 0x4e, 0x47]
        );
    }

    #[cfg(unix)]
    #[test]
    fn gives_each_entry_its_stated_mode_or_the_mode_of_its_kind() {
        let dir = written(&Tree::from([
            ("bin/run", files::executable("#!/bin/sh\necho ok\n")),
            ("cache", files::directory()),
            ("current", files::link("bin/run")),
            ("go.mod", files::text("module example.com/a\n")),
            ("keys", files::directory().with_mode(0o700)),
            ("keys/id", files::text("secret\n").with_mode(0o600)),
            ("up", files::link("../outside.txt")),
        ]));
        assert_eq!(
            listing(dir.path()),
            [
                "bin/ 0o755",
                r##"bin/run "#!/bin/sh\necho ok\n" 0o755"##,
                "cache/ 0o755",
                "current -> bin/run",
                r#"go.mod "module example.com/a\n" 0o644"#,
                "keys/ 0o700",
                r#"keys/id "secret\n" 0o600"#,
                "up -> ../outside.txt",
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn gives_a_directory_its_mode_after_every_entry_below_it() {
        let dir = written(&Tree::from([
            ("locked", files::directory().with_mode(0o500)),
            ("locked/a.txt", files::text("a\n")),
        ]));
        assert_eq!(
            listing(dir.path()),
            ["locked/ 0o500", r#"locked/a.txt "a\n" 0o644"#]
        );
    }
}

mod overwrite {
    use super::*;

    #[test]
    fn replaces_the_content_of_a_file_and_keeps_every_other_entry() {
        let dir = written(&Tree::from([
            ("a.txt", files::text("a\n")),
            ("b.txt", files::text("b\n")),
        ]));
        files::write(
            &Recorder::new(),
            &dir,
            &Tree::from([("a.txt", files::text("A\n"))]),
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("a.txt")).expect("the file reads"),
            "A\n"
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("b.txt")).expect("the file reads"),
            "b\n"
        );
    }

    #[test]
    fn ends_the_call_with_a_fault_for_a_dir_that_is_no_directory() {
        let dir = written(&Tree::from([("a.txt", files::text("a\n"))]));
        let seat = Recorder::new();
        files::write(
            &seat,
            dir.path().join("a.txt"),
            &Tree::from([("b.txt", files::text(""))]),
        );
        assert!(
            seat.message()
                .starts_with("files::write: the directory cannot be opened: "),
            "{}",
            seat.message()
        );
    }

    #[test]
    fn ends_the_call_with_a_fault_before_it_writes_for_an_entry_of_another_kind() {
        let dir = written(&Tree::from([("a.txt", files::text("a\n"))]));
        let seat = Recorder::new();
        let tree = Tree::from([("a.txt/b", files::text("")), ("z.txt", files::text(""))]);
        files::write(&seat, &dir, &tree);
        assert_eq!(
            seat.message(),
            "files::write: a.txt: the entry is a file, and the tree states a directory"
        );
        assert!(!dir.path().join("z.txt").exists());
    }

    #[cfg(unix)]
    #[test]
    fn replaces_the_content_of_a_file_that_its_owner_may_not_write() {
        let dir = written(&Tree::from([(
            "a.txt",
            files::text("a\n").with_mode(0o444),
        )]));
        files::write(
            &Recorder::new(),
            &dir,
            &Tree::from([("a.txt", files::text("new\n"))]),
        );
        assert_eq!(listing(dir.path()), [r#"a.txt "new\n" 0o644"#]);
    }

    #[cfg(unix)]
    #[test]
    fn replaces_the_target_of_a_link_and_keeps_the_entry_that_it_pointed_to() {
        let dir = written(&Tree::from([
            ("a.txt", files::text("a\n")),
            ("b.txt", files::text("b\n")),
            ("current", files::link("a.txt")),
        ]));
        files::write(
            &Recorder::new(),
            &dir,
            &Tree::from([("current", files::link("b.txt"))]),
        );
        assert_eq!(
            listing(dir.path()),
            [
                r#"a.txt "a\n" 0o644"#,
                r#"b.txt "b\n" 0o644"#,
                "current -> b.txt"
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn keeps_the_mode_of_a_parent_that_the_tree_implies() {
        let dir = written(&Tree::from([("keys", files::directory().with_mode(0o700))]));
        files::write(
            &Recorder::new(),
            &dir,
            &Tree::from([("keys/id", files::text("x"))]),
        );
        assert_eq!(listing(dir.path()), ["keys/ 0o700", r#"keys/id "x" 0o644"#]);
    }

    #[cfg(unix)]
    #[test]
    fn gives_a_directory_that_the_tree_states_its_mode() {
        let dir = written(&Tree::from([("keys", files::directory().with_mode(0o700))]));
        let tree = Tree::from([("keys", files::directory().with_mode(0o750))]);
        files::write(&Recorder::new(), &dir, &tree);
        assert_eq!(listing(dir.path()), ["keys/ 0o750"]);
    }

    #[cfg(unix)]
    #[test]
    fn creates_a_missing_parent_with_the_mode_of_a_directory() {
        let dir = written(&Tree::new());
        files::write(
            &Recorder::new(),
            &dir,
            &Tree::from([("a/b/c.txt", files::text("c"))]),
        );
        assert_eq!(
            listing(dir.path()),
            ["a/ 0o755", "a/b/ 0o755", r#"a/b/c.txt "c" 0o644"#]
        );
    }

    #[cfg(unix)]
    #[test]
    fn ends_the_call_with_a_fault_for_a_link_where_the_tree_states_a_directory() {
        let dir = written(&Tree::from([
            ("docs/a.md", files::text("# a\n")),
            ("latest", files::link("docs")),
        ]));
        let seat = Recorder::new();
        files::write(&seat, &dir, &Tree::from([("latest/b.md", files::text(""))]));
        assert_eq!(
            seat.message(),
            "files::write: latest: the entry is a link, and the tree states a directory"
        );
    }

    #[cfg(unix)]
    #[test]
    fn ends_the_call_with_a_fault_for_a_second_path_of_one_file() {
        let dir = written(&Tree::from([("a.txt", files::text("a\n"))]));
        fs::hard_link(dir.path().join("a.txt"), dir.path().join("b.txt"))
            .expect("the workspace accepts a second name of the file");
        let seat = Recorder::new();
        let tree = Tree::from([("a.txt", files::text("x")), ("b.txt", files::text("y"))]);
        files::write(&seat, &dir, &tree);
        assert_eq!(
            seat.message(),
            r#"files::write: b.txt: the file system maps the path to the entry at "a.txt""#
        );
    }

    #[cfg(unix)]
    #[test]
    fn ends_the_call_with_a_fault_for_an_entry_that_is_no_file_directory_or_link() {
        let dir = written(&Tree::new());
        let _socket = std::os::unix::net::UnixListener::bind(dir.path().join("sock"))
            .expect("the workspace accepts a socket");
        let seat = Recorder::new();
        files::write(&seat, &dir, &Tree::from([("sock", files::text(""))]));
        assert_eq!(
            seat.message(),
            "files::write: sock: the entry is no file, directory or link"
        );
    }

    #[cfg(unix)]
    #[test]
    fn ends_the_call_with_a_fault_for_an_entry_that_cannot_be_read() {
        let dir = written(&Tree::from([(
            "locked",
            files::directory().with_mode(0o000),
        )]));
        let seat = Recorder::new();
        files::write(
            &seat,
            &dir,
            &Tree::from([("locked/a.txt", files::text(""))]),
        );
        assert!(
            seat.message()
                .starts_with("files::write: locked/a.txt: the entry cannot be read: "),
            "{}",
            seat.message()
        );
    }

    #[cfg(unix)]
    #[test]
    fn ends_the_call_with_a_fault_at_an_entry_that_cannot_be_written() {
        let dir = written(&Tree::from([(
            "locked",
            files::directory().with_mode(0o500),
        )]));
        let seat = Recorder::new();
        files::write(
            &seat,
            &dir,
            &Tree::from([("locked/a.txt", files::text(""))]),
        );
        assert!(
            seat.message()
                .starts_with("files::write: locked/a.txt: the entry cannot be written: "),
            "{}",
            seat.message()
        );
    }
}

mod update {
    use super::*;

    /// The ignored test that updates the golden tree `api` of its working
    /// directory with the tree in its directory `out`.
    const UPDATE: &str = "files::writer::update::updates_the_golden_tree_api_with_the_tree_in_out";

    /// Writes `tree` into a workspace and updates its golden tree `api` with
    /// its tree `out` in a child process. Returns the workspace with the
    /// verdict and the message that the child printed.
    fn updated(tree: &Tree) -> (Workspace, String) {
        let dir = written(tree);
        let output = in_child(UPDATE, dir.path(), &[]);
        let printed = String::from_utf8_lossy(&output.stdout).into_owned();
        assert!(output.status.success(), "{printed}");
        (dir, printed)
    }

    /// Returns the listing of the golden tree `api` of `dir`.
    #[cfg(unix)]
    fn golden_tree(dir: &Workspace) -> Vec<String> {
        listing(&dir.path().join("testdata/golden/api"))
    }

    #[test]
    #[ignore = "a spec of update runs it in a child process"]
    fn updates_the_golden_tree_api_with_the_tree_in_out() {
        let seat = Recorder::new();
        golden::matches_tree(&seat, "api", "out", true, &[]);
        println!("verdict {}", records(&seat)[0]["verdict"]);
        println!("message {}", seat.message());
    }

    #[test]
    fn writes_a_golden_tree_that_is_missing() {
        let (dir, printed) = updated(&Tree::from([
            ("out/api.go", files::text("package api\n")),
            ("out/v1/types.go", files::text("package v1\n")),
        ]));
        assert!(printed.contains(r#"verdict "pass""#), "{printed}");
        let golden = dir.path().join("testdata/golden/api");
        assert_eq!(
            fs::read_to_string(golden.join("v1/types.go")).expect("the update writes the file"),
            "package v1\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn makes_the_golden_tree_equal_the_output() {
        let (dir, printed) = updated(&Tree::from([
            ("out/api.go", files::text("package api\n\nfunc New() {}\n")),
            ("out/new.go", files::text("package api\n")),
            ("out/v1/types.go", files::executable("package v1\n")),
            ("testdata/golden/api/api.go", files::text("package api\n")),
            ("testdata/golden/api/latest", files::link("api.go")),
            (
                "testdata/golden/api/old/deep/stale.go",
                files::text("package old\n"),
            ),
            (
                "testdata/golden/api/v1/types.go",
                files::text("package v1\n"),
            ),
        ]));
        assert!(printed.contains(r#"verdict "pass""#), "{printed}");
        assert_eq!(
            golden_tree(&dir),
            [
                r#"api.go "package api\n\nfunc New() {}\n" 0o644"#,
                r#"new.go "package api\n" 0o644"#,
                "v1/ 0o755",
                r#"v1/types.go "package v1\n" 0o755"#,
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn leaves_an_equal_golden_tree_as_it_is() {
        use std::os::unix::fs::MetadataExt;
        let tree = Tree::from([
            ("out/api.go", files::text("package api\n")),
            (
                "testdata/golden/api/api.go",
                files::text("package api\n").with_mode(0o600),
            ),
        ]);
        let dir = written(&tree);
        let file = dir.path().join("testdata/golden/api/api.go");
        let before = fs::metadata(&file).expect("the golden file reads").ino();
        let output = in_child(UPDATE, dir.path(), &[]);
        assert!(output.status.success());
        assert_eq!(
            fs::metadata(&file).expect("the golden file reads").ino(),
            before
        );
        assert_eq!(golden_tree(&dir), [r#"api.go "package api\n" 0o600"#]);
    }

    #[cfg(unix)]
    #[test]
    fn leaves_a_file_as_it_was_when_its_replacement_cannot_be_written() {
        let (dir, printed) = updated(&Tree::from([
            ("out/a.go", files::text("new\n")),
            ("testdata/golden/api", files::directory().with_mode(0o555)),
            ("testdata/golden/api/a.go", files::text("old\n")),
        ]));
        assert!(printed.contains(r#"verdict "error""#), "{printed}");
        assert!(
            printed.contains("message golden::matches_tree: a.go: the entry cannot be written: "),
            "{printed}"
        );
        assert_eq!(golden_tree(&dir), [r#"a.go "old\n" 0o644"#]);
    }

    #[test]
    fn ends_the_call_with_a_fault_for_a_golden_directory_that_cannot_be_created() {
        let (_dir, printed) = updated(&Tree::from([
            ("out/a.go", files::text("package a\n")),
            ("testdata/golden", files::text("")),
        ]));
        assert!(
            printed.contains("message golden::matches_tree: the directory cannot be created: "),
            "{printed}"
        );
    }
}

#[cfg(unix)]
mod unlock {
    use super::*;

    #[test]
    fn lets_a_workspace_remove_a_directory_whose_mode_forbids_every_access() {
        let dir = written(&Tree::from([
            ("locked", files::directory().with_mode(0o000)),
            ("locked/inner", files::directory().with_mode(0o500)),
            ("locked/inner/a.txt", files::text("a\n")),
        ]));
        let path = dir.path().to_path_buf();
        drop(dir);
        assert!(!path.exists());
    }
}
