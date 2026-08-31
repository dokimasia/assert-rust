//! Comparison against a recorded file.
//!
//! For output too large or too fiddly to write out in a test: record it once, and let the
//! test say only that it has not changed. Set `UPDATE_GOLDEN=1` to rewrite the files.

use crate::matcher::{Mode, report};
use crate::seat::Seat;
use std::path::{Path, PathBuf};

/// The variable that rewrites the files rather than comparing against them.
pub const UPDATE_ENV: &str = "UPDATE_GOLDEN";

/// The directory `matches` resolves a name against.
pub const GOLDEN_DIR: &str = "testdata/golden";

/// A replacement applied to both sides before they are compared.
///
/// For the parts of an output that change every run. A timestamp differs each time and
/// says nothing about whether the code is right, so it is replaced on both sides rather
/// than left to fail.
#[derive(Debug, Clone)]
pub struct Scrubber {
    pattern: regex::Regex,
    replacement: &'static str,
}

impl Scrubber {
    /// Return a scrubber replacing what pattern matches.
    ///
    /// # Panics
    ///
    /// When the pattern does not compile. A scrubber is written once by the test author,
    /// so a pattern that cannot compile is a mistake in the test rather than a failure of
    /// the subject.
    #[must_use]
    pub fn new(pattern: &str, replacement: &'static str) -> Self {
        Self {
            pattern: regex::Regex::new(pattern).expect("a scrubber's pattern has to compile"),
            replacement,
        }
    }

    fn apply(&self, text: &str) -> String {
        self.pattern
            .replace_all(text, self.replacement)
            .into_owned()
    }
}

/// Replace anything shaped like an RFC 3339 timestamp.
#[must_use]
pub fn scrub_timestamps() -> Scrubber {
    Scrubber::new(
        r"\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:?\d{2})?",
        "SCRUBBED_TIMESTAMP",
    )
}

/// Replace anything shaped like a hex digest of 32 characters or more.
#[must_use]
pub fn scrub_hashes() -> Scrubber {
    Scrubber::new(r"\b[0-9a-fA-F]{32,}\b", "SCRUBBED_HASH")
}

/// Replace anything shaped like a UUID.
#[must_use]
pub fn scrub_run_ids() -> Scrubber {
    Scrubber::new(
        r"\b[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\b",
        "SCRUBBED_RUN_ID",
    )
}

/// Whether the run should rewrite the golden files rather than compare against them.
#[must_use]
pub fn should_update() -> bool {
    std::env::var_os(UPDATE_ENV).is_some_and(|set| !set.is_empty() && set != "0")
}

/// Apply every scrubber, in order.
fn scrubbed(text: &str, scrubbers: &[Scrubber]) -> String {
    scrubbers
        .iter()
        .fold(text.to_owned(), |text, one| one.apply(&text))
}

/// Fail when got does not match the golden file at path.
///
/// With `UPDATE_GOLDEN` set the file is written instead, and nothing is reported.
#[track_caller]
pub fn matches_at(seat: &dyn Seat, path: &Path, got: &str, scrubbers: &[Scrubber]) {
    seat.helper();
    let cleaned = scrubbed(got, scrubbers);

    if should_update() {
        if let Some(parent) = path.parent() {
            if let Err(unwritable) = std::fs::create_dir_all(parent) {
                report(
                    seat,
                    Mode::Fatal,
                    &format!("{}: {unwritable}", path.display()),
                );
                return;
            }
        }
        if let Err(unwritable) = std::fs::write(path, &cleaned) {
            report(
                seat,
                Mode::Fatal,
                &format!(
                    "{}: the golden file could not be written: {unwritable}",
                    path.display()
                ),
            );
        }
        return;
    }

    let Ok(recorded) = std::fs::read_to_string(path) else {
        report(
            seat,
            Mode::Fatal,
            &format!(
                "{} does not exist; run the test again with {UPDATE_ENV}=1 to record it",
                path.display()
            ),
        );
        return;
    };

    let want = scrubbed(&recorded, scrubbers);
    if cleaned != want {
        report(
            seat,
            Mode::Fatal,
            &format!(
                "{} does not match; run the test again with {UPDATE_ENV}=1 to accept it\n\
                 --- recorded\n{want}\n--- got\n{cleaned}",
                path.display()
            ),
        );
    }
}

/// Fail when got does not match the golden file of the given name.
///
/// The name is resolved against `testdata/golden`, which is where a Rust project keeps
/// fixtures a test reads.
#[track_caller]
pub fn matches(seat: &dyn Seat, name: &str, got: &str, scrubbers: &[Scrubber]) {
    seat.helper();
    let path = PathBuf::from(GOLDEN_DIR).join(name);
    matches_at(seat, &path, got, scrubbers);
}

/// Fail when got does not match one named field of a golden JSON object.
///
/// For an output where one field is worth pinning and the rest is noise. The field is
/// read by scanning rather than by parsing, so this needs no JSON library: it finds the
/// key at the top level and takes the value whole, counting brackets and ignoring
/// anything inside a string.
#[track_caller]
pub fn matches_json_field(
    seat: &dyn Seat,
    path: &Path,
    field: &str,
    got: &str,
    scrubbers: &[Scrubber],
) {
    seat.helper();

    if should_update() {
        report(
            seat,
            Mode::Fatal,
            &format!(
                "{}: {UPDATE_ENV} cannot rewrite one field of a golden file; \
                 edit it or record the whole file with matches_at",
                path.display()
            ),
        );
        return;
    }

    let Ok(recorded) = std::fs::read_to_string(path) else {
        report(
            seat,
            Mode::Fatal,
            &format!("{} does not exist", path.display()),
        );
        return;
    };

    let Some(raw) = raw_value_of(&recorded, field) else {
        report(
            seat,
            Mode::Fatal,
            &format!(
                "{} carries no field {field:?}; add it, or record the whole file with \
                 {UPDATE_ENV}=1",
                path.display()
            ),
        );
        return;
    };

    let want = scrubbed(raw.trim(), scrubbers);
    let cleaned = scrubbed(got.trim(), scrubbers);
    if cleaned != want {
        report(
            seat,
            Mode::Fatal,
            &format!(
                "{} field {field:?} does not match\n--- recorded\n{want}\n--- got\n{cleaned}",
                path.display()
            ),
        );
    }
}

/// The raw text of one top-level field's value, brackets balanced.
///
/// A regular expression cannot find where a value ends, which is why this is a scan: a
/// nested object would be cut short at its first closing brace.
fn raw_value_of(document: &str, field: &str) -> Option<String> {
    let key = format!("\"{field}\"");
    let mut at = document.find(&key)? + key.len();

    let rest = &document[at..];
    at += rest.find(':')? + 1;

    let bytes = document.as_bytes();
    while at < bytes.len() && bytes[at].is_ascii_whitespace() {
        at += 1;
    }

    let start = at;
    let (mut depth, mut in_string, mut escaped) = (0i32, false, false);

    for (offset, byte) in document[start..].bytes().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        match byte {
            b'\\' if in_string => escaped = true,
            b'"' => in_string = !in_string,
            b'[' | b'{' if !in_string => depth += 1,
            b']' | b'}' if !in_string => {
                if depth == 0 {
                    return Some(document[start..start + offset].to_owned());
                }
                depth -= 1;
            }
            b',' if !in_string && depth == 0 => {
                return Some(document[start..start + offset].to_owned());
            }
            _ => {}
        }
    }
    Some(document[start..].to_owned())
}
