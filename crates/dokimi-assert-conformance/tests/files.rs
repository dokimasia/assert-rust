//! The spec of `files`, which runs the definition's files vectors.

use dokimi_assert::Collector;
use dokimi_assert::files::{self, Tree, Workspace};
use dokimi_assert_conformance::definition;
use dokimi_assert_conformance::files::{self as engine, Disagreement, Vector};
use serde_json::json;
use std::collections::BTreeSet;
use std::process::Command;

/// The ignored test that runs the vector of its variables in its working
/// directory.
const CHILD: &str = "files::run::runs_the_vector_of_its_variables_in_its_working_directory";

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

/// Runs `vector` in a child process whose working directory is a directory
/// of the vector's own, and returns the outcome that the child printed:
/// `agrees`, or the disagreement.
fn in_child(vector: &Vector) -> String {
    let dir = directory();
    let binary = std::env::current_exe().expect("the test binary has a path");
    let output = Command::new(binary)
        .args([
            CHILD,
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads",
            "1",
        ])
        .current_dir(dir.path())
        .env("DOKIMI_CONFORMANCE_KIND", &vector.kind)
        .env("DOKIMI_CONFORMANCE_VECTOR", vector.raw.to_string())
        .output()
        .expect("the test binary runs");
    let printed = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(output.status.success(), "{printed}");
    // libtest prints the outcome on the line of the test's name.
    printed
        .lines()
        .find_map(|line| line.split_once("outcome: ").map(|(_, outcome)| outcome))
        .expect("the child prints its outcome")
        .to_owned()
}

mod vectors {
    use super::*;

    #[test]
    fn reads_every_vector_of_each_file_of_the_engine() {
        let found = engine::vectors().expect("the vendored vectors read");
        let mut kinds = BTreeSet::new();
        let mut stated = 0;
        for file in definition::vector_files("files").expect("the vectors list") {
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
                .len();
        }
        assert_eq!(found.len(), stated);
        assert_eq!(
            found
                .iter()
                .map(|vector| vector.kind.clone())
                .collect::<BTreeSet<_>>(),
            kinds
        );
        assert!(kinds.contains("golden-match-tree"), "{kinds:?}");
    }
}

mod vector {
    use super::*;

    #[test]
    fn runs_a_vector_of_golden_match_tree_in_its_working_directory() {
        assert!(vector("golden-match-tree/an-equal-golden-tree-passes").in_working_directory());
        assert!(!vector("tree-equal/equal-trees-pass").in_working_directory());
    }

    #[test]
    fn finds_a_vector_that_states_a_mode_recorded_on_unix_alone() {
        let modes = vector("has-mode/the-stated-mode-of-a-file-passes");
        assert_eq!(modes.recorded(), cfg!(unix));
        assert!(vector("is-dir/a-directory-passes").recorded());
    }
}

mod run {
    use super::*;

    #[test]
    #[ignore = "a spec of a golden vector runs it in a child process"]
    fn runs_the_vector_of_its_variables_in_its_working_directory() {
        let kind = std::env::var("DOKIMI_CONFORMANCE_KIND").expect("the parent states the kind");
        let raw = std::env::var("DOKIMI_CONFORMANCE_VECTOR").expect("the parent states the vector");
        let vector = Vector {
            id: String::new(),
            kind,
            raw: serde_json::from_str(&raw).expect("the vector is JSON"),
        };
        let here = std::env::current_dir().expect("the process has a working directory");
        match engine::run(&vector, &here) {
            Ok(()) => println!("outcome: agrees"),
            Err(disagreement) => println!("outcome: {disagreement}"),
        }
    }

    #[test]
    fn agrees_with_every_vector_that_runs_in_a_directory_of_its_own() {
        let mut disagreements = Vec::new();
        let mut ran = 0;
        for vector in engine::vectors().expect("the vendored vectors read") {
            if vector.in_working_directory() || !vector.recorded() {
                continue;
            }
            ran += 1;
            if let Err(disagreement) = engine::run(&vector, directory().path()) {
                disagreements.push(format!("{}: {disagreement}", vector.id));
            }
        }
        assert!(ran > 0);
        assert!(
            disagreements.is_empty(),
            "{} disagreements:\n{}",
            disagreements.len(),
            disagreements.join("\n")
        );
    }

    #[test]
    fn agrees_with_every_vector_of_golden_match_tree() {
        let golden: Vec<Vector> = engine::vectors()
            .expect("the vendored vectors read")
            .into_iter()
            .filter(|vector| vector.in_working_directory() && vector.recorded())
            .collect();
        assert_ne!(golden.len(), 0);
        for vector in golden {
            assert_eq!(in_child(&vector), "agrees", "{}", vector.id);
        }
    }

    #[test]
    fn disagrees_with_a_golden_vector_outside_its_working_directory() {
        let golden = vector("golden-match-tree/an-equal-golden-tree-passes");
        let result = engine::run(&golden, directory().path());
        assert!(
            matches!(result, Err(Disagreement::WorkingDirectory)),
            "{result:?}"
        );
    }

    #[test]
    fn disagrees_with_a_golden_tree_after_an_update_of_another_form() {
        let mut update = vector("golden-match-tree/update-writes-a-missing-golden-tree");
        update.raw["after"] = json!({"type": "tree", "entries": []});
        let outcome = in_child(&update);
        assert!(
            outcome.starts_with("the golden tree after the call is "),
            "{outcome}"
        );
    }

    #[test]
    fn disagrees_with_a_verdict_of_another_kind() {
        let mut pass = vector("tree-equal/equal-trees-pass");
        pass.raw["expect"] = json!("fail");
        let result = engine::run(&pass, directory().path());
        assert!(
            matches!(&result, Err(Disagreement::Verdict { want, .. }) if want == "fail"),
            "{result:?}"
        );
    }

    #[test]
    fn disagrees_with_a_field_of_another_value() {
        let mut missing = vector("tree-equal/a-missing-file-fails");
        missing.raw["detail"]["differences"] = json!({"type": "int", "value": 2});
        let result = engine::run(&missing, directory().path());
        assert!(
            matches!(&result, Err(Disagreement::DetailField { name, .. }) if name == "differences"),
            "{result:?}"
        );
    }

    #[test]
    fn disagrees_with_a_detail_of_other_fields() {
        let mut missing = vector("tree-equal/a-missing-file-fails");
        missing.raw["detail"]
            .as_object_mut()
            .expect("the vector states a detail")
            .remove("differences");
        let result = engine::run(&missing, directory().path());
        assert!(
            matches!(&result, Err(Disagreement::DetailFields { want, got })
                if want.len() == 2 && got.len() == 3),
            "{result:?}"
        );
    }

    #[test]
    fn disagrees_with_a_kind_that_the_runner_does_not_call() {
        let mut made_up = vector("is-dir/a-directory-passes");
        made_up.kind = "made-up".to_owned();
        let result = engine::run(&made_up, directory().path());
        assert!(
            matches!(&result, Err(Disagreement::Kind(kind)) if kind == "made-up"),
            "{result:?}"
        );
    }

    #[test]
    fn disagrees_with_a_subject_that_the_runner_does_not_build() {
        let mut made_up = vector("tree-unchanged/a-subject-that-writes-a-file-fails");
        made_up.raw["subject"]["kind"] = json!("made-up");
        let result = engine::run(&made_up, directory().path());
        assert!(
            matches!(&result, Err(Disagreement::Subject(what)) if what.contains("made-up")),
            "{result:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn disagrees_with_a_subject_that_fails() {
        let mut rewrites = vector("tree-unchanged/a-subject-that-rewrites-every-file-passes");
        rewrites.raw["workspace"] = json!({"type": "tree", "entries": [
            {"path": "locked", "directory": true, "mode": 320},
            {"path": "locked/a.txt", "text": "a\n", "mode": 256},
        ]});
        let result = engine::run(&rewrites, directory().path());
        assert!(
            matches!(&result, Err(Disagreement::Subject(what)) if what.starts_with("the subject fails: ")),
            "{result:?}"
        );
    }

    #[test]
    fn disagrees_with_a_workspace_that_cannot_be_written() {
        let mut broken = vector("is-dir/a-directory-passes");
        broken.raw["workspace"] =
            json!({"type": "tree", "entries": [{"path": "a//b", "text": ""}]});
        let result = engine::run(&broken, directory().path());
        assert!(
            matches!(
                &result,
                Err(Disagreement::Write {
                    member: "workspace",
                    ..
                })
            ),
            "{result:?}"
        );
    }

    #[test]
    fn disagrees_with_a_tree_that_is_no_tree_literal() {
        let mut broken = vector("tree-equal/equal-trees-pass");
        broken.raw["args"][0] = json!({"type": "list", "items": []});
        let result = engine::run(&broken, directory().path());
        assert!(
            matches!(&result, Err(Disagreement::Literal { member: "args", .. })),
            "{result:?}"
        );
    }

    #[test]
    fn disagrees_with_an_argument_of_another_type() {
        let mut broken = vector("links-to/a-file-fails");
        broken.raw["args"][1] = json!({"type": "int", "value": 1});
        let result = engine::run(&broken, directory().path());
        assert!(
            matches!(
                &result,
                Err(Disagreement::Argument {
                    at: 1,
                    expected: "string"
                })
            ),
            "{result:?}"
        );
    }
}

mod disagreement {
    use super::*;

    #[test]
    fn renders_what_disagrees() {
        let verdict = Disagreement::Verdict {
            want: "pass".to_owned(),
            got: json!("error"),
            error: Some("files::equal: the tree cannot be read".to_owned()),
        };
        assert_eq!(
            verdict.to_string(),
            r#"the call ends as "error", and the vector expects "pass": files::equal: the tree cannot be read"#
        );
        assert_eq!(
            Disagreement::Argument {
                at: 0,
                expected: "tree"
            }
            .to_string(),
            "the vector states no tree as argument 0"
        );
    }
}
