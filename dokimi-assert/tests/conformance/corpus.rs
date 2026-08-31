//! Reading the corpus, and saying whether an outcome matched a case.

use super::value::{Value, decode};
use dokimi_assert::failure::{Detail, Failure};
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
    /// What the failure's record has to hold, keyed by the names the assertion
    /// declares. A field the case leaves out is not checked.
    pub detail: Vec<(String, Value)>,
    /// The behaviour this case hands the assertion in place of arguments, or None
    /// for a case that states values.
    pub subject: Option<String>,
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
                let Some(held) = seat.failures().into_iter().next() else {
                    return Err(format!(
                        "{}: reported no record; the assertion did not report one",
                        self.id
                    ));
                };
                self.check_detail(&held)
            }
            other => Err(format!(
                "{}: states an unknown expectation {other:?}",
                self.id
            )),
        }
    }

    /// Say how a record's detail differs from what this case states.
    ///
    /// Rust's records hold text rather than values, because there is no untyped value to
    /// put in a field. A case states a typed literal, so the comparison is against the
    /// two ways an assertion writes one: the value's own `Debug`, and the plain scalar a
    /// count or a measurement is written as.
    fn check_detail(&self, held: &Failure) -> Result<(), String> {
        for (name, want) in &self.detail {
            let Some(found) = held.detail(name) else {
                return Err(format!(
                    "{}: the record holds no detail {name:?}, want {want:?}",
                    self.id
                ));
            };
            if !written_as(want, found) {
                return Err(format!(
                    "{}: detail {name:?} is {found:?}, want {want:?}",
                    self.id
                ));
            }
        }
        Ok(())
    }
}

/// Whether a reported field is how an assertion writes this value.
///
/// A NaN is unequal to itself under the standard's own rules, but here the question is
/// whether the assertion reported the value the case named, so the text settles it.
fn written_as(want: &Value, found: &Detail) -> bool {
    // A field the library computed is compared as the number it is. One
    // holding the caller's own value is text, because an assertion is
    // generic over what it compares.
    match (want, found) {
        (Value::Int(held), Detail::Count(at)) => usize::try_from(*held) == Ok(*at),
        // A whole number stated where the library reported a real one
        // is compared by rendering, which loses nothing either way.
        (Value::Int(held), Detail::Number(at)) => at.to_string() == held.to_string(),
        (Value::Float(held), Detail::Number(at)) => same_number(*at, *held),
        (_, Detail::Said(said)) => said_as(want, said),
        _ => false,
    }
}

/// Whether two readings are the same number.
///
/// A NaN is unequal to itself under the standard's own rules, but here the
/// question is whether the assertion reported the value the case named.
fn same_number(at: f64, held: f64) -> bool {
    // A NaN is unequal to itself and an infinity minus itself is a NaN,
    // so neither survives a tolerance. Both are settled by rendering.
    if !at.is_finite() || !held.is_finite() {
        return at.to_string() == held.to_string();
    }
    (at - held).abs() <= f64::EPSILON * at.abs().max(held.abs()).max(1.0)
}

/// Whether a rendered field is how an assertion writes this value.
fn said_as(want: &Value, found: &str) -> bool {
    if found == format!("{want:?}") {
        return true;
    }
    match want {
        Value::Int(held) => found == held.to_string(),
        Value::Float(held) => found == held.to_string(),
        Value::Str(held) => found == *held || found == format!("{held:?}"),
        Value::Bool(held) => found == held.to_string(),
        Value::List(items) => found == format!("{items:?}"),
        Value::Null | Value::Map(_) => false,
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
                detail: raw["detail"]
                    .as_object()
                    .map(|held| {
                        held.iter()
                            .filter_map(|(name, one)| {
                                decode(one).ok().map(|value| (name.clone(), value))
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                subject: raw["subject"]["kind"].as_str().map(str::to_owned),
                skip: raw["skip"]["rust"].as_str().map(str::to_owned),
            });
        }
    }
    found
}
