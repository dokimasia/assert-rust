// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `files/path`: the assertions of the entry at one path.

use crate::helpers::{records, written};
use dokimi_assert::files::{self, Tree, Workspace};
use dokimi_assert::{Recorder, Value};

/// Returns a workspace of a file, a directory with a file, and on Unix a
/// link to the file and a link whose target is missing.
fn workspace() -> Workspace {
    let mut tree = Tree::from([
        ("a.txt", files::text("a\r\n")),
        ("docs/a.md", files::text("# a\n")),
        ("logo.png", files::bytes([0x89, 0x50, 0x4e, 0x47])),
    ]);
    if cfg!(unix) {
        tree.insert("current", files::link("a.txt"));
        tree.insert("dangling", files::link("missing.txt"));
    }
    written(&tree)
}

/// Runs `assertion` on a new recorder, with the path `path` of the
/// workspace, and returns the recorder.
fn checked(path: &str, assertion: impl FnOnce(&Recorder, std::path::PathBuf)) -> Recorder {
    let dir = workspace();
    let seat = Recorder::new();
    assertion(&seat, dir.path().join(path));
    seat
}

/// Returns the one detail of the failure that `seat` kept.
fn detail_of(seat: &Recorder) -> Vec<(&'static str, Value)> {
    let failures = seat.failures();
    assert_eq!(failures.len(), 1, "{failures:?}");
    failures[0].detail().to_vec()
}

mod absent {
    use super::*;

    #[test]
    fn passes_a_path_where_nothing_is() {
        let seat = checked("b.txt", |t, at| files::absent(t, at, "c"));
        assert!(!seat.failed(), "{}", seat.message());
        assert_eq!(records(&seat)[0]["assertion"], "path-absent");
    }

    #[test]
    fn states_the_kind_of_the_entry_at_the_path() {
        let seat = checked("docs", |t, at| files::absent(t, at, "c"));
        assert_eq!(seat.failures()[0].assertion(), Some("path-absent"));
        assert_eq!(detail_of(&seat), [("got", Value::from("directory"))]);
    }

    #[cfg(unix)]
    #[test]
    fn states_a_link_whose_target_is_missing() {
        let seat = checked("dangling", |t, at| files::absent(t, at, "c"));
        assert_eq!(detail_of(&seat), [("got", Value::from("link"))]);
    }
}

mod is_file {
    use super::*;

    #[test]
    fn passes_a_file() {
        let seat = checked("a.txt", |t, at| files::is_file(t, at, "c"));
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn states_the_kind_of_a_directory() {
        let seat = checked("docs", |t, at| files::is_file(t, at, "c"));
        assert_eq!(seat.failures()[0].assertion(), Some("is-file"));
        assert_eq!(detail_of(&seat), [("got", Value::from("directory"))]);
    }

    #[test]
    fn states_null_where_nothing_is() {
        let seat = checked("b.txt", |t, at| files::is_file(t, at, "c"));
        assert_eq!(detail_of(&seat), [("got", Value::Null)]);
    }

    #[cfg(unix)]
    #[test]
    fn states_a_link_to_a_file_as_a_link() {
        let seat = checked("current", |t, at| files::is_file(t, at, "c"));
        assert_eq!(detail_of(&seat), [("got", Value::from("link"))]);
    }
}

mod is_dir {
    use super::*;

    #[test]
    fn passes_a_directory() {
        let seat = checked("docs", |t, at| files::is_dir(t, at, "c"));
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn states_the_kind_of_a_file() {
        let seat = checked("docs/a.md", |t, at| files::is_dir(t, at, "c"));
        assert_eq!(seat.failures()[0].assertion(), Some("is-dir"));
        assert_eq!(detail_of(&seat), [("got", Value::from("file"))]);
    }

    #[test]
    fn states_null_where_nothing_is() {
        let seat = checked("build", |t, at| files::is_dir(t, at, "c"));
        assert_eq!(detail_of(&seat), [("got", Value::Null)]);
    }
}

mod links_to {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn passes_a_link_to_the_target() {
        let seat = checked("current", |t, at| files::links_to(t, at, "a.txt", "c"));
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[cfg(unix)]
    #[test]
    fn states_the_target_of_a_link_to_another_target() {
        let seat = checked("current", |t, at| files::links_to(t, at, "b.txt", "c"));
        assert_eq!(seat.failures()[0].assertion(), Some("links-to"));
        assert_eq!(
            detail_of(&seat),
            [
                ("want", Value::from("b.txt")),
                ("got", Value::from("a.txt")),
                ("kind", Value::from("link"))
            ]
        );
    }

    #[test]
    fn states_null_for_a_file() {
        let seat = checked("a.txt", |t, at| files::links_to(t, at, "a.txt", "c"));
        assert_eq!(
            detail_of(&seat),
            [
                ("want", Value::from("a.txt")),
                ("got", Value::Null),
                ("kind", Value::from("file"))
            ]
        );
    }

    #[test]
    fn states_null_twice_where_nothing_is() {
        let seat = checked("latest", |t, at| files::links_to(t, at, "a.txt", "c"));
        assert_eq!(
            detail_of(&seat),
            [
                ("want", Value::from("a.txt")),
                ("got", Value::Null),
                ("kind", Value::Null)
            ]
        );
    }
}

mod has_content {
    use super::*;

    #[test]
    fn passes_equal_text() {
        let seat = checked("docs/a.md", |t, at| files::has_content(t, at, "# a\n", "c"));
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn passes_equal_bytes() {
        let seat = checked("logo.png", |t, at| {
            files::has_content(t, at, [0x89, 0x50, 0x4e, 0x47], "c");
        });
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn states_text_that_differs_in_its_line_ending() {
        let seat = checked("a.txt", |t, at| files::has_content(t, at, "a\n", "c"));
        assert_eq!(seat.failures()[0].assertion(), Some("has-content"));
        assert_eq!(
            detail_of(&seat),
            [
                ("want", Value::from("a\n")),
                ("got", Value::from("a\r\n")),
                ("kind", Value::from("file"))
            ]
        );
    }

    #[test]
    fn states_content_that_is_no_utf8_text_as_bytes() {
        let seat = checked("logo.png", |t, at| {
            files::has_content(t, at, [0x89, 0x50], "c");
        });
        assert_eq!(
            detail_of(&seat),
            [
                ("want", Value::Bytes(vec![0x89, 0x50])),
                ("got", Value::Bytes(vec![0x89, 0x50, 0x4e, 0x47])),
                ("kind", Value::from("file"))
            ]
        );
    }

    #[test]
    fn states_null_for_a_directory() {
        let seat = checked("docs", |t, at| files::has_content(t, at, "", "c"));
        assert_eq!(
            detail_of(&seat),
            [
                ("want", Value::from("")),
                ("got", Value::Null),
                ("kind", Value::from("directory"))
            ]
        );
    }
}

mod has_mode {
    use super::*;

    #[test]
    fn ends_the_call_with_a_fault_for_a_bit_beyond_the_nine_permission_bits() {
        let seat = checked("a.txt", |t, at| files::has_mode(t, at, 0o1644, "c"));
        assert_eq!(
            seat.message(),
            "files::has_mode: the mode 0o1644 has a bit beyond the nine permission bits"
        );
        assert_eq!(records(&seat)[0]["verdict"], "error");
        assert_eq!(seat.failures(), []);
    }

    #[cfg(unix)]
    #[test]
    fn passes_the_mode_of_a_file_and_of_a_directory() {
        let dir = written(&Tree::from([
            ("keys", files::directory().with_mode(0o700)),
            ("keys/id", files::text("secret\n").with_mode(0o600)),
        ]));
        let seat = Recorder::new();
        files::has_mode(&seat, dir.path().join("keys"), 0o700, "c");
        files::has_mode(&seat, dir.path().join("keys/id"), 0o600, "c");
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[cfg(unix)]
    #[test]
    fn states_another_mode() {
        let seat = checked("a.txt", |t, at| files::has_mode(t, at, 0o600, "c"));
        assert_eq!(seat.failures()[0].assertion(), Some("has-mode"));
        assert_eq!(
            detail_of(&seat),
            [
                ("want", Value::Int(0o600)),
                ("got", Value::Int(0o644)),
                ("kind", Value::from("file"))
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn states_null_for_a_link() {
        let seat = checked("current", |t, at| files::has_mode(t, at, 0o644, "c"));
        assert_eq!(
            detail_of(&seat),
            [
                ("want", Value::Int(0o644)),
                ("got", Value::Null),
                ("kind", Value::from("link"))
            ]
        );
    }

    #[cfg(not(unix))]
    #[test]
    fn ends_the_call_with_a_fault_on_a_platform_that_records_no_permission_bits() {
        let seat = checked("a.txt", |t, at| files::has_mode(t, at, 0o644, "c"));
        assert_eq!(
            seat.message(),
            "files::has_mode: the file system records no permission bits"
        );
    }
}
