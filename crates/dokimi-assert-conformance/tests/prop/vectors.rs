// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `prop::vectors`, which runs the definition's property
//! vectors.

use dokimi_assert::Collector;
use dokimi_assert::files::{self, Tree, Workspace};
use dokimi_assert_conformance::definition;
use dokimi_assert_conformance::prop::vectors::{self as engine, Disagreement, Vector};
use serde_json::json;
use std::collections::BTreeSet;

/// Returns an empty directory of a vector's own.
fn directory() -> Workspace {
    files::workspace(&Collector::new(), &Tree::new())
}

/// Returns the vector `id` of the definition.
fn vector(id: &str) -> Vector {
    engine::vectors()
        .expect("the vendored vectors read")
        .into_iter()
        .find(|vector| vector.id == id)
        .expect("the definition states the vector")
}

#[expect(
    clippy::module_inception,
    reason = "the mod of a spec has the name of the export that it tests"
)]
mod vectors {
    use super::*;

    #[test]
    fn reads_every_vector_of_each_file_of_the_engine() {
        let found = engine::vectors().expect("the vendored vectors read");
        let mut kinds = BTreeSet::new();
        let mut stated = 0;
        for file in definition::vector_files("prop").expect("the vectors list") {
            let vectors = definition::read(&file).expect("a vector file reads");
            kinds.insert(
                vectors["kind"]
                    .as_str()
                    .expect("a file names its kind")
                    .to_owned(),
            );
            stated += vectors["cases"]
                .as_array()
                .expect("a file states vectors")
                .iter()
                .filter(|case| case["skip"].get("rust").is_none())
                .count();
        }
        assert_eq!(found.len(), stated);
        assert_eq!(
            found
                .iter()
                .map(|vector| vector.kind.clone())
                .collect::<BTreeSet<_>>(),
            kinds
        );
        assert!(kinds.contains("recording"), "{kinds:?}");
    }

    #[test]
    fn leaves_out_each_vector_that_skips_rust() {
        let found: BTreeSet<String> = engine::vectors()
            .expect("the vendored vectors read")
            .into_iter()
            .map(|vector| vector.id)
            .collect();
        let mut skipped = Vec::new();
        for file in definition::vector_files("prop").expect("the vectors list") {
            let vectors = definition::read(&file).expect("a vector file reads");
            for case in vectors["cases"].as_array().expect("a file states vectors") {
                if case["skip"].get("rust").is_some() {
                    skipped.push(case["id"].as_str().expect("a vector has an id").to_owned());
                }
            }
        }
        assert_ne!(skipped, Vec::<String>::new());
        assert!(skipped.iter().all(|id| !found.contains(id)), "{skipped:?}");
    }
}

mod run {
    use super::*;

    #[test]
    fn agrees_with_every_vector_of_the_definition() {
        let mut disagreements = Vec::new();
        let mut ran = 0;
        for vector in engine::vectors().expect("the vendored vectors read") {
            ran += 1;
            if let Err(disagreement) = engine::run(&vector, directory().path()) {
                disagreements.push(format!("{}: {disagreement}", vector.id));
            }
        }
        assert!(ran > 0);
        assert!(
            disagreements.is_empty(),
            "{} of {ran} vectors disagree:\n{}",
            disagreements.len(),
            disagreements.join("\n")
        );
    }

    #[test]
    fn disagrees_with_an_output_of_another_value() {
        let mut token = vector("token/encodes-no-choices");
        token.raw["token"] = json!("prop1:AA");
        let result = engine::run(&token, directory().path());
        assert!(
            matches!(&result, Err(Disagreement::Output { name, .. }) if name == "token"),
            "{result:?}"
        );
    }

    #[test]
    fn disagrees_with_a_kind_that_the_runner_does_not_compute() {
        let mut made_up = vector("token/encodes-no-choices");
        made_up.kind = "made-up".to_owned();
        let result = engine::run(&made_up, directory().path());
        assert!(
            matches!(&result, Err(Disagreement::Kind(kind)) if kind == "made-up"),
            "{result:?}"
        );
    }

    #[test]
    fn disagrees_with_a_setting_that_the_runner_does_not_take() {
        let mut behaviour = vector("behaviour/passes-a-body-that-never-fails");
        behaviour.raw["settings"]["profile"] = json!("ci");
        let result = engine::run(&behaviour, directory().path());
        assert!(
            matches!(&result, Err(Disagreement::Input(what)) if what.starts_with("settings.profile: ")),
            "{result:?}"
        );
    }

    #[test]
    fn disagrees_with_a_run_that_ends_with_a_fault() {
        let mut behaviour = vector("behaviour/passes-a-body-that-never-fails");
        behaviour.raw["settings"]["replay"] = json!("no token");
        let result = engine::run(&behaviour, directory().path());
        assert!(
            matches!(&result, Err(Disagreement::Fault(text)) if text.contains("is not a replay token")),
            "{result:?}"
        );
    }

    #[test]
    fn disagrees_with_a_recording_of_another_reason() {
        let mut skips = vector("recording/records-the-cases-before-a-case-that-skips");
        skips.raw["reason"] = json!("the disk is full");
        let result = engine::run(&skips, directory().path());
        assert!(
            matches!(&result, Err(Disagreement::Output { name, .. }) if name == "reason"),
            "{result:?}"
        );
    }

    #[test]
    fn disagrees_with_a_recording_of_other_calls() {
        let mut replay = vector("recording/records-the-one-case-of-a-replay-token");
        replay.raw["calls"][0]["phase"] = json!("random");
        let result = engine::run(&replay, directory().path());
        assert!(
            matches!(&result, Err(Disagreement::Output { name, .. }) if name == "calls"),
            "{result:?}"
        );
    }
}

mod disagreement {
    use super::*;

    #[test]
    fn renders_what_disagrees() {
        let output = Disagreement::Output {
            name: "token".to_owned(),
            want: json!("prop1:AA"),
            got: json!("prop1:"),
        };
        assert_eq!(
            output.to_string(),
            r#"the output token is "prop1:", and the vector states "prop1:AA""#
        );
        assert_eq!(
            Disagreement::Fault(
                "prop::for_all: draws: the entries are not a JSON array of objects".to_owned()
            )
            .to_string(),
            r#"the run ends with the fault "prop::for_all: draws: the entries are not a JSON array of objects""#
        );
    }
}
