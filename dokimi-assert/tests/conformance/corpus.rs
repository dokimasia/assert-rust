//! Reading the corpus, and saying whether an outcome matched a case.

use super::value::{Value, decode};
use dokimi_assert::seat::Recorder;
use serde_json::Value as Json;
use std::path::Path;

/// What the corpus says one call should do.
#[derive(Debug, Clone)]
pub struct Case {
    /// The case's own name, used as the message the assertion is given.
    pub id: String,
    /// Which assertion the case drives.
    pub assertion: String,
    /// The decoded arguments.
    pub args: Vec<Value>,
    /// Whether the call should pass or fail.
    pub expect: String,
    /// Substrings the failure has to carry.
    pub message_contains: Vec<String>,
    /// Why this language skips the case, when it does.
    pub skip: Option<String>,
}

impl Case {
    /// Say how an outcome differs from what this case states.
    ///
    /// A value rather than a panic, so the rule itself can be driven against cases it
    /// must reject.
    pub fn verdict(&self, seat: &Recorder) -> Result<(), String> {
        match self.expect.as_str() {
            "pass" if seat.failed() => Err(format!(
                "{}: expects pass, got failure: {}",
                self.id,
                seat.message()
            )),
            "pass" => Ok(()),
            "fail" if !seat.failed() => Err(format!("{}: expects fail, got pass", self.id)),
            "fail" => {
                let got = seat.message();
                for wanted in &self.message_contains {
                    if !got.contains(wanted.as_str()) {
                        return Err(format!(
                            "{}: failure {got:?} does not carry {wanted:?}",
                            self.id
                        ));
                    }
                }
                Ok(())
            }
            other => Err(format!(
                "{}: states an unknown expectation {other:?}",
                self.id
            )),
        }
    }
}

/// Read every case the vendored corpus states.
///
/// # Panics
///
/// When a corpus file cannot be read or a literal cannot be decoded. Both are a broken
/// vendored copy rather than a failing subject, and neither is worth a test's time.
pub fn cases() -> Vec<Case> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/spec/corpus");
    let mut found = Vec::new();

    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|why| {
            panic!(
                "the vendored corpus is readable at {}: {why}",
                dir.display()
            )
        })
        .map(|entry| entry.expect("a directory entry is readable").path())
        .collect();
    files.sort();

    for path in files {
        let text = std::fs::read_to_string(&path).expect("a corpus file is readable");
        let doc: Json = serde_json::from_str(&text).expect("a corpus file is JSON");
        let assertion = doc["assertion"]
            .as_str()
            .unwrap_or_else(|| path.file_stem().unwrap().to_str().unwrap())
            .to_owned();

        for raw in doc["cases"].as_array().expect("a corpus file states cases") {
            let args = raw["args"]
                .as_array()
                .map(|held| {
                    held.iter()
                        .map(|one| decode(one).expect("a corpus literal decodes"))
                        .collect()
                })
                .unwrap_or_default();

            found.push(Case {
                id: raw["id"].as_str().expect("a case is named").to_owned(),
                assertion: assertion.clone(),
                args,
                expect: raw["expect"]
                    .as_str()
                    .expect("a case states an outcome")
                    .to_owned(),
                message_contains: raw["message_contains"]
                    .as_array()
                    .map(|held| {
                        held.iter()
                            .filter_map(|one| one.as_str().map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default(),
                skip: raw["skip"]["rust"].as_str().map(str::to_owned),
            });
        }
    }
    found
}
