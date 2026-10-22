// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `vector`, which reads the vectors of an engine.

use dokimi_assert_conformance::definition::{self, Error};
use dokimi_assert_conformance::vector::{self, Disagreement};

mod vectors {
    use super::*;

    #[test]
    fn reads_every_vector_of_each_file() {
        for engine in ["history", "stateful"] {
            let found = vector::vectors(engine).expect("the vendored vectors read");
            let stated: usize = definition::vector_files(engine)
                .expect("the vectors list")
                .iter()
                .map(|file| {
                    definition::read(file).expect("a vector file reads")["cases"]
                        .as_array()
                        .expect("a file states vectors")
                        .len()
                })
                .sum();
            assert_eq!(found.len(), stated);
        }
    }

    #[test]
    fn names_each_vector_by_the_kind_of_its_file() {
        let found = vector::vectors("stateful").expect("the vendored vectors read");
        assert!(found.iter().all(|vector| vector.kind == "machines"));
        assert_eq!(found[0].id, found[0].raw["id"]);
    }

    #[test]
    fn returns_the_read_error_of_an_engine_without_vectors() {
        let refused = vector::vectors("made-up").expect_err("no directory states the engine");
        assert!(matches!(refused, Error::Read { .. }));
    }
}

mod disagreement {
    use super::*;

    #[test]
    fn states_the_member_before_the_reason() {
        let disagreement = Disagreement {
            at: "detail.steps".to_owned(),
            reason: "the field is 3, want 4".to_owned(),
        };
        assert_eq!(
            disagreement.to_string(),
            "detail.steps: the field is 3, want 4"
        );
    }

    #[test]
    fn states_the_reason_alone_without_a_member() {
        let disagreement = Disagreement {
            at: String::new(),
            reason: "the recorder kept no call record".to_owned(),
        };
        assert_eq!(disagreement.to_string(), "the recorder kept no call record");
    }
}
