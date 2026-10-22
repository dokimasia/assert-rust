// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `definition`, the reader of the vendored definition.

use dokimi_assert_conformance::definition::{self, Error};
use std::error::Error as _;

mod dir {
    use super::*;

    #[test]
    fn contains_the_vendored_definition() {
        assert!(
            definition::dir()
                .join("spec")
                .join("assertions.json")
                .is_file()
        );
        assert!(definition::dir().join("manifest.json").is_file());
    }
}

mod version {
    use super::*;

    #[test]
    fn returns_the_version_of_the_vendored_definition() {
        let version = definition::version().expect("the definition states its version");
        let parts: Vec<&str> = version.split('.').collect();
        assert_eq!(parts.len(), 3, "{version}");
        assert!(
            parts
                .iter()
                .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit())),
            "{version}"
        );
    }
}

mod read {
    use super::*;

    #[test]
    fn reads_a_file_of_the_definition_as_json() {
        let version = definition::read("spec/assertions.json").expect("the definition reads");
        assert!(version["assertions"]["equal"].is_object());
    }

    #[test]
    fn refuses_a_file_that_the_definition_lacks() {
        let error = definition::read("missing.json").expect_err("the file does not exist");
        assert!(matches!(&error, Error::Read { .. }), "{error:?}");
        assert!(error.to_string().starts_with("cannot read "), "{error}");
        assert!(error.source().is_some());
    }

    #[test]
    fn refuses_a_file_that_is_no_json() {
        let error = definition::read("Cargo.toml").expect_err("a manifest is no JSON");
        assert!(matches!(&error, Error::Parse { .. }), "{error:?}");
        assert!(error.to_string().ends_with(" is no JSON"), "{error}");
        assert!(error.source().is_some());
    }
}

mod declared {
    use super::*;

    #[test]
    fn returns_the_detail_fields_in_the_order_of_the_definition() {
        assert_eq!(
            definition::declared("in-range").expect("the definition declares it"),
            ["got", "low", "high"]
        );
    }

    #[test]
    fn refuses_an_assertion_that_the_definition_does_not_declare() {
        let error = definition::declared("made-up").expect_err("no such assertion");
        assert!(
            matches!(&error, Error::Undeclared(id) if id == "made-up"),
            "{error:?}"
        );
        assert_eq!(
            error.to_string(),
            "the definition declares no assertion made-up"
        );
        assert!(error.source().is_none());
    }
}

mod declined {
    use super::*;

    #[test]
    fn returns_the_relaxations_that_the_overlay_of_rust_declines() {
        let declined = definition::declined().expect("the overlay reads");
        assert_eq!(declined, ["equate-empty", "equate-nans", "by-identity"]);
    }
}

mod corpus_files {
    use super::*;

    #[test]
    fn lists_the_corpus_files_directly_under_corpus_in_order() {
        let files = definition::corpus_files().expect("the corpus lists");
        assert!(files.contains(&"corpus/equal.json".to_owned()), "{files:?}");
        assert!(
            !files.iter().any(|file| file.starts_with("corpus/prop/")),
            "{files:?}"
        );
        assert!(files.windows(2).all(|pair| pair[0] < pair[1]));
    }
}

mod vector_files {
    use super::*;

    #[test]
    fn lists_the_vector_files_of_an_engine_in_order() {
        let files = definition::vector_files("files").expect("the vectors list");
        assert!(
            files.contains(&"corpus/files/tree-equal.json".to_owned()),
            "{files:?}"
        );
        assert!(files.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn refuses_an_engine_that_the_definition_lacks() {
        let error = definition::vector_files("made-up").expect_err("no such engine");
        assert!(matches!(&error, Error::Read { .. }), "{error:?}");
    }
}

mod error {
    use super::*;

    #[test]
    fn renders_a_malformed_file_with_what_it_lacks() {
        let error = Error::Malformed {
            file: "corpus/x.json".to_owned(),
            what: "the file names no assertion",
        };
        assert_eq!(
            error.to_string(),
            "corpus/x.json: the file names no assertion"
        );
        assert!(error.source().is_none());
    }
}
