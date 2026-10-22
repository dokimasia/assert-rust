//! The spec of `files/reader`: the tree in a directory, and the entry at
//! one path.
//!
//! A spec of an entry that cannot be read runs as a user other than root,
//! because root reads an entry whose mode forbids it.

use crate::helpers::{Scratch, written};
use dokimi_assert::Recorder;
use dokimi_assert::files::{self, Tree};

mod read_tree {
    use super::*;

    #[test]
    fn ends_the_call_with_a_fault_for_a_root_that_is_no_directory() {
        let dir = written(&Tree::from([("a.txt", files::text("a\n"))]));
        let seat = Recorder::new();
        files::equal(&seat, dir.path().join("a.txt"), &Tree::new(), "c");
        assert_eq!(
            seat.message(),
            "files::equal: the root of the tree is no directory"
        );
    }

    #[test]
    fn ends_the_call_with_a_fault_for_a_root_that_does_not_exist() {
        let dir = Scratch::new();
        let seat = Recorder::new();
        files::equal(&seat, dir.join("missing"), &Tree::new(), "c");
        assert!(
            seat.message()
                .starts_with("files::equal: the tree cannot be read: "),
            "{}",
            seat.message()
        );
    }

    #[cfg(unix)]
    #[test]
    fn states_the_nine_permission_bits_of_an_entry_where_want_states_a_mode() {
        let dir = written(&Tree::from([(
            "keys/id",
            files::text("x").with_mode(0o640),
        )]));
        let seat = Recorder::new();
        let want = Tree::from([("keys/id", files::text("x").with_mode(0o600))]);
        files::contains(&seat, &dir, &want, "c");
        let got = Tree::from([("keys/id", files::text("x").with_mode(0o640))]);
        assert_eq!(
            seat.failures()[0].got(),
            Some(&dokimi_assert::Value::Tree(got))
        );
    }

    #[cfg(unix)]
    #[test]
    fn reads_a_link_as_a_link_with_its_target() {
        let tree = Tree::from([
            ("a.txt", files::text("a\n")),
            ("current", files::link("a.txt")),
            ("up", files::link("../outside.txt")),
        ]);
        let dir = written(&tree);
        let seat = Recorder::new();
        files::equal(&seat, &dir, &tree, "c");
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[cfg(unix)]
    #[test]
    fn follows_a_link_at_the_root() {
        let tree = Tree::from([("a.txt", files::text("a\n"))]);
        let dir = written(&tree);
        let scratch = Scratch::new();
        std::os::unix::fs::symlink(dir.path(), scratch.join("root"))
            .expect("the scratch directory accepts a link");
        let seat = Recorder::new();
        files::equal(&seat, scratch.join("root"), &tree, "c");
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[cfg(unix)]
    #[test]
    fn ends_the_call_with_a_fault_for_a_directory_that_cannot_be_read() {
        let dir = written(&Tree::from([(
            "locked",
            files::directory().with_mode(0o000),
        )]));
        let seat = Recorder::new();
        files::equal(&seat, &dir, &Tree::new(), "c");
        assert!(
            seat.message()
                .starts_with("files::equal: locked: the directory cannot be read: "),
            "{}",
            seat.message()
        );
    }

    #[cfg(unix)]
    #[test]
    fn ends_the_call_with_a_fault_for_a_file_that_cannot_be_read() {
        let dir = written(&Tree::from([("secret", files::text("x").with_mode(0o000))]));
        let seat = Recorder::new();
        files::equal(&seat, &dir, &Tree::new(), "c");
        assert!(
            seat.message()
                .starts_with("files::equal: secret: the entry cannot be read: "),
            "{}",
            seat.message()
        );
    }

    #[cfg(unix)]
    #[test]
    fn ends_the_call_with_a_fault_for_an_entry_that_is_no_file_directory_or_link() {
        let dir = written(&Tree::new());
        let _socket = std::os::unix::net::UnixListener::bind(dir.path().join("sock"))
            .expect("the workspace accepts a socket");
        let seat = Recorder::new();
        files::equal(&seat, &dir, &Tree::new(), "c");
        assert_eq!(
            seat.message(),
            "files::equal: sock: the entry is no file, directory or link"
        );
    }

    #[cfg(unix)]
    #[test]
    fn ends_the_call_with_a_fault_for_a_name_that_is_no_utf8_text() {
        use std::os::unix::ffi::OsStrExt;
        let dir = written(&Tree::from([("docs", files::directory())]));
        let name = std::ffi::OsStr::from_bytes(b"\xff");
        std::fs::write(dir.path().join("docs").join(name), "")
            .expect("the workspace accepts the name");
        let seat = Recorder::new();
        files::equal(&seat, &dir, &Tree::new(), "c");
        assert_eq!(
            seat.message(),
            r#"files::equal: docs: the name "\xFF" is no UTF-8 text"#
        );
    }

    #[cfg(unix)]
    #[test]
    fn ends_the_call_with_a_fault_for_a_link_whose_target_is_no_utf8_text() {
        use std::os::unix::ffi::OsStrExt;
        let dir = written(&Tree::new());
        let target = std::ffi::OsStr::from_bytes(b"a\xff");
        std::os::unix::fs::symlink(target, dir.path().join("odd"))
            .expect("the workspace accepts the link");
        let seat = Recorder::new();
        files::equal(&seat, &dir, &Tree::new(), "c");
        assert_eq!(
            seat.message(),
            "files::equal: odd: the target a\u{fffd} of the link is no UTF-8 text"
        );
    }
}

mod read_path {
    use super::*;

    #[test]
    fn finds_nothing_below_a_file() {
        let dir = written(&Tree::from([("a.txt", files::text("a\n"))]));
        let seat = Recorder::new();
        files::absent(&seat, dir.path().join("a.txt/b"), "c");
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[cfg(unix)]
    #[test]
    fn follows_a_link_among_the_names_before_the_last_one() {
        let dir = written(&Tree::from([
            ("docs/a.md", files::text("# a\n")),
            ("latest", files::link("docs")),
        ]));
        let seat = Recorder::new();
        files::is_file(&seat, dir.path().join("latest/a.md"), "c");
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[cfg(unix)]
    #[test]
    fn ends_the_call_with_a_fault_for_a_path_whose_directory_cannot_be_searched() {
        let dir = written(&Tree::from([(
            "locked",
            files::directory().with_mode(0o000),
        )]));
        let seat = Recorder::new();
        files::absent(&seat, dir.path().join("locked/a.txt"), "c");
        assert!(
            seat.message()
                .starts_with("files::absent: the entry cannot be read: "),
            "{}",
            seat.message()
        );
    }
}
