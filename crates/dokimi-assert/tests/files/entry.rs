// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `files/entry`: the files, directories and links of a tree.

use crate::helpers::panic_of;
use dokimi_assert::files;

mod text {
    use super::*;

    #[test]
    fn prints_as_the_expression_that_states_it() {
        assert_eq!(format!("{:?}", files::text("a\n")), r#"text("a\n")"#);
    }

    #[test]
    fn prints_a_content_longer_than_65536_bytes_by_its_size_and_its_digest() {
        assert_eq!(
            format!("{:?}", files::text("a".repeat(65_537))),
            "text(<65537 bytes, sha256:008ffc88d3c96a9f307524eb361e47c5222a887fc45fa0c1fb8d429c5c23b430>)"
        );
    }
}

mod bytes {
    use super::*;

    #[test]
    fn prints_content_that_is_no_utf8_text_as_bytes() {
        assert_eq!(
            format!("{:?}", files::bytes([0x89, b'P', b'N', b'G'])),
            r#"bytes(b"\x89PNG")"#
        );
    }

    #[test]
    fn equals_the_text_of_the_same_bytes() {
        assert_eq!(files::bytes(*b"a\n"), files::text("a\n"));
    }
}

mod executable {
    use super::*;

    #[test]
    fn prints_as_the_expression_that_states_it() {
        assert_eq!(
            format!("{:?}", files::executable("#!/bin/sh\n")),
            r##"executable("#!/bin/sh\n")"##
        );
    }

    #[test]
    fn differs_from_the_text_of_the_same_content() {
        assert_ne!(files::executable("x"), files::text("x"));
    }
}

mod directory {
    use super::*;

    #[test]
    fn prints_as_the_expression_that_states_it() {
        assert_eq!(format!("{:?}", files::directory()), "directory()");
    }
}

mod link {
    use super::*;

    #[test]
    fn prints_as_the_expression_that_states_it() {
        assert_eq!(format!("{:?}", files::link("a.txt")), r#"link("a.txt")"#);
    }
}

mod with_mode {
    use super::*;

    #[test]
    fn prints_the_mode_after_the_expression() {
        assert_eq!(
            format!("{:?}", files::directory().with_mode(0o700)),
            "directory().with_mode(0o700)"
        );
    }

    #[test]
    fn prints_an_executable_with_a_mode_as_text_with_the_mode() {
        assert_eq!(
            format!("{:?}", files::executable("x").with_mode(0o750)),
            r#"text("x").with_mode(0o750)"#
        );
    }

    #[test]
    fn panics_for_a_bit_beyond_the_nine_permission_bits() {
        let raised = panic_of(|| drop(files::text("x").with_mode(0o1000)));
        assert_eq!(
            raised.as_deref(),
            Some("files::Entry::with_mode(0o1000) states a bit beyond the nine permission bits")
        );
    }

    #[test]
    fn panics_for_a_link() {
        let raised = panic_of(|| drop(files::link("a.txt").with_mode(0o644)));
        assert_eq!(
            raised.as_deref(),
            Some("files::Entry::with_mode(0o644) states the mode of a link, which has no mode")
        );
    }
}
