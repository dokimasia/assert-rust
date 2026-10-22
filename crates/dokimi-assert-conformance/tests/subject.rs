// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `subject`, the native subjects of the corpus.

use dokimi_assert::Recorder;
use dokimi_assert_conformance::corpus::{Library, Surface};
use dokimi_assert_conformance::subject;

mod run {
    use super::*;

    #[test]
    fn runs_the_subject_of_an_assertion() {
        let seat = Recorder::new();
        assert!(subject::run(
            Library::Core,
            Surface::Check,
            "pure",
            "accumulates",
            &seat,
            "c"
        ));
        assert_eq!(seat.failures()[0].assertion(), Some("pure"));
    }

    #[test]
    fn runs_the_subject_of_a_future_on_the_tokio_crate() {
        let seat = Recorder::new();
        assert!(subject::run(
            Library::Tokio,
            Surface::Soft,
            "eventually",
            "never-settles",
            &seat,
            "c"
        ));
        assert_eq!(seat.failures()[0].assertion(), Some("eventually"));
    }

    #[test]
    fn builds_no_subject_of_another_assertion() {
        let seat = Recorder::new();
        assert!(!subject::run(
            Library::Core,
            Surface::Soft,
            "pure",
            "rotates",
            &seat,
            "c"
        ));
        assert!(!subject::run(
            Library::Core,
            Surface::Soft,
            "made-up",
            "accumulates",
            &seat,
            "c"
        ));
        assert!(!subject::run(
            Library::Tokio,
            Surface::Check,
            "pure",
            "accumulates",
            &seat,
            "c"
        ));
        assert_eq!(seat.records(), [] as [String; 0]);
    }
}
