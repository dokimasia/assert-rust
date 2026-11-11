// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The store: the format of an entry, its name, and what a run does with
//! one.
//!
//! The store keeps the minimal failing case of each identity in a directory
//! of each test, one JSON object per file. This module states the object
//! that a run writes, the file's name, and the verdict that a run gives each
//! file that it reads. The directory and the file system belong to the
//! property, so the module reads and writes no file.
//!
//! A name is the entry's key: [`mix`] applied to the contract's UTF-8 bytes, a
//! zero byte and the replay token's bytes, as 16 lowercase hexadecimal digits
//! followed by `.json`. The same failure, shrunk to the same choices, has the
//! same name in every language.
//!
//! An entry records the counterexample of its case in request order. A run
//! writes format 2 for a counterexample with a step entry, and format 1 for
//! every other counterexample. Format 1 records draw entries alone, so a
//! runner of an earlier definition still replays the entries of a property
//! without a machine.
//!
//! A run gives each file one verdict:
//!
//! - `Replay`: an entry of format 1 or 2 for the property, whose token is of
//!   the version that the token module encodes. The run tries its choices
//!   first.
//! - `Other`: an entry of format 1 or 2 for another property of the test.
//! - `Skip`: an entry of a later format, or whose token is of a later version.
//!   The run notes it and goes on.
//! - `Damaged`: any other file, including JSON that repeats a name within an
//!   object, states a non-finite number, or nests past 64 levels. An entry of
//!   format 1 with a step entry is damaged, and so is an entry of format 2
//!   with a step entry out of its form. The test fails, as it fails on a
//!   damaged golden file.

use super::case::Step;
use super::choice::Choice;
use super::source::mix;
use super::token;
use super::trace::Entry;
use crate::json::{self, Json, object};
use crate::value::Value;

/// The format of an entry whose counterexample has draw entries alone.
const FORMAT_DRAWS: i128 = 1;

/// The format of an entry whose counterexample has a step entry, the latest
/// format.
pub(crate) const FORMAT_STEPS: i128 = 2;

/// The version of the tokens that the token module encodes.
const TOKEN_VERSION: u64 = 1;

/// The most levels of objects and arrays that a file nests, the root
/// included.
pub(crate) const MAX_DEPTH: usize = 64;

/// The levels above a draw's value: the root, the counterexample and the
/// draw.
const ABOVE_VALUE: usize = 3;

/// The largest line of an identity and the largest client of a step entry,
/// the largest signed 32-bit integer.
const LINE_MAX: i128 = 2_147_483_647;

/// The fields of an entry, in the order that a run writes them.
const FIELDS: [&str; 7] = [
    "store",
    "definition",
    "property",
    "identity",
    "choices",
    "counterexample",
    "found",
];

/// The keys of an identity, one set for each way that a case fails.
const IDENTITIES: [&[&str]; 4] = [
    &["assertion", "file", "line"],
    &["assertion", "contract"],
    &["file", "line"],
    &["error", "file", "line"],
];

/// What a run does with one file of the store.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    /// The entry of the property, with its choices.
    Replay(Vec<Choice>),
    /// An entry of another property of the test.
    Other,
    /// An entry of a later format or token version.
    Skip,
    /// A file that is no entry.
    Damaged,
}

impl Verdict {
    /// Returns the name of the verdict, as a vector states it.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Replay(_) => "replay",
            Self::Other => "other",
            Self::Skip => "skip",
            Self::Damaged => "damaged",
        }
    }
}

/// A property's minimal failing case, as an entry records it.
#[derive(Debug, Clone, PartialEq)]
pub struct Failure {
    /// The contract of the property.
    pub contract: String,
    /// The choices of the minimal case.
    pub choices: Vec<Choice>,
    /// The identity of the failure, with the keys of one of the forms.
    pub identity: Vec<(String, Value)>,
    /// The entries of the counterexample, in request order: each draw with
    /// its label and its typed literal where it has one, and each step.
    pub counterexample: Vec<Entry>,
}

/// Returns the file name of the entry of a contract and a minimal case.
#[must_use]
pub fn name(contract: &str, choices: &[Choice]) -> String {
    let mut data = contract.as_bytes().to_vec();
    data.push(0);
    data.extend_from_slice(token::encode(choices).as_bytes());
    format!("{:016x}.json", mix(&data))
}

/// Returns the entry that a run of `definition` writes on the date `found`,
/// written as `YYYY-MM-DD`. The entry is of format 2 for a counterexample
/// with a step entry, and of format 1 otherwise. A draw whose value would
/// nest the entry past 64 levels is recorded by its label alone.
#[must_use]
pub fn entry(failure: &Failure, definition: &str, found: &str) -> String {
    let identity = Json::Object(
        failure
            .identity
            .iter()
            .map(|(key, value)| (key.clone(), plain(value)))
            .collect(),
    );
    let stepped = failure
        .counterexample
        .iter()
        .any(|entry| matches!(entry, Entry::Step(_)));
    let format = if stepped { FORMAT_STEPS } else { FORMAT_DRAWS };
    object([
        ("store", Json::Int(format)),
        ("definition", definition.into()),
        ("property", failure.contract.as_str().into()),
        ("identity", identity),
        ("choices", token::encode(&failure.choices).into()),
        (
            "counterexample",
            Json::Array(failure.counterexample.iter().map(recorded).collect()),
        ),
        ("found", found.into()),
    ])
    .pretty()
}

/// Returns the JSON of an entry of a counterexample: a draw entry with its
/// label and its value, or a step entry with its action, its client and its
/// drain mark.
fn recorded(entry: &Entry) -> Json {
    match entry {
        Entry::Draw { label, value } => {
            let mut draw = vec![("label".to_owned(), Json::from(label.as_str()))];
            let literal = value
                .as_ref()
                .map(Value::to_literal)
                .filter(|literal| literal.depth() <= MAX_DEPTH - ABOVE_VALUE);
            if let Some(literal) = literal {
                draw.push(("value".to_owned(), literal));
            }
            Json::Object(draw)
        }
        Entry::Step(step) => {
            let mut members = vec![("step".to_owned(), Json::from(step.action.as_str()))];
            if let Some(client) = step.client {
                members.push(("client".to_owned(), Json::from(client)));
            }
            if step.drain {
                members.push(("drain".to_owned(), Json::Bool(true)));
            }
            Json::Object(members)
        }
    }
}

/// Returns the step of a step entry of an entry of format 2, which
/// [`read`] finds in its form.
pub(crate) fn step_of(entry: &Json) -> Option<Step> {
    Some(Step {
        action: entry.get("step")?.as_str()?.to_owned(),
        client: entry
            .get("client")
            .and_then(Json::as_int)
            .and_then(|client| usize::try_from(client).ok()),
        drain: entry.get("drain").is_some(),
    })
}

/// Returns the plain JSON of an identity's value: text or a line.
fn plain(value: &Value) -> Json {
    match value {
        Value::Int(line) => Json::Int(*line),
        Value::String(text) => text.as_str().into(),
        other => other.to_string().into(),
    }
}

/// Returns what a run of the property `contract` does with a file's text.
#[must_use]
pub fn read(text: &str, contract: &str) -> Verdict {
    let Ok(document) = json::parse(text, MAX_DEPTH) else {
        return Verdict::Damaged;
    };
    let Some(Json::Int(version)) = document.get("store") else {
        return Verdict::Damaged;
    };
    if *version < FORMAT_DRAWS {
        return Verdict::Damaged;
    }
    if *version > FORMAT_STEPS {
        return Verdict::Skip;
    }
    if !well_formed(&document, *version == FORMAT_STEPS) {
        return Verdict::Damaged;
    }
    let token = document
        .get("choices")
        .and_then(Json::as_str)
        .unwrap_or_default();
    match token_version(token) {
        None => return Verdict::Damaged,
        Some(stated) if stated > TOKEN_VERSION => return Verdict::Skip,
        Some(_) => {}
    }
    let Ok(choices) = token::decode(token) else {
        return Verdict::Damaged;
    };
    if document.get("property").and_then(Json::as_str) != Some(contract) {
        return Verdict::Other;
    }
    Verdict::Replay(choices)
}

/// Returns the version that a token states: the number after `prop` and
/// before the colon.
fn token_version(token: &str) -> Option<u64> {
    let rest = token.strip_prefix("prop")?;
    let (digits, _) = rest.split_once(':')?;
    if digits.is_empty() || digits.starts_with('0') || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok().or(Some(u64::MAX))
}

/// Returns `true` when every field of an entry has its form. A step entry
/// has its form only in an entry of format 2, which `steps` states.
fn well_formed(document: &Json, steps: bool) -> bool {
    let fields = document.as_object().map_or(0, <[(String, Json)]>::len);
    if fields != FIELDS.len() || !FIELDS.iter().all(|field| document.get(field).is_some()) {
        return false;
    }
    document
        .get("definition")
        .and_then(Json::as_str)
        .is_some_and(version)
        && document.get("property").and_then(Json::as_str).is_some()
        && document.get("identity").is_some_and(identity)
        && document.get("choices").and_then(Json::as_str).is_some()
        && document
            .get("counterexample")
            .is_some_and(|entries| counterexample(entries, steps))
        && document
            .get("found")
            .and_then(Json::as_str)
            .is_some_and(date)
}

/// Returns `true` for a definition version, `MAJOR.MINOR.PATCH`.
fn version(text: &str) -> bool {
    let parts: Vec<&str> = text.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.bytes().all(|b| b.is_ascii_digit())
                && (*part == "0" || !part.starts_with('0'))
        })
}

/// Returns `true` for an identity with the keys of one of the forms and
/// their values: `line` in `[1, 2^31 - 1]`, `file` a non-empty base name, and
/// every other key a non-empty string.
fn identity(identity: &Json) -> bool {
    let Some(members) = identity.as_object() else {
        return false;
    };
    let mut keys: Vec<&str> = members.iter().map(|(key, _)| key.as_str()).collect();
    keys.sort_unstable();
    let known = IDENTITIES.iter().any(|form| {
        let mut form = form.to_vec();
        form.sort_unstable();
        form == keys
    });
    known
        && members.iter().all(|(key, value)| match key.as_str() {
            "line" => matches!(value, Json::Int(line) if (1..=LINE_MAX).contains(line)),
            "file" => value
                .as_str()
                .is_some_and(|file| !file.is_empty() && !file.contains(['/', '\\'])),
            _ => value.as_str().is_some_and(|text| !text.is_empty()),
        })
}

/// Returns `true` for a list of entries: draw entries with a label each, and
/// for `steps` step entries too.
fn counterexample(entries: &Json, steps: bool) -> bool {
    entries.as_array().is_some_and(|entries| {
        entries
            .iter()
            .all(|entry| draw_entry(entry) || (steps && step_entry(entry)))
    })
}

/// Returns the keys of an object, sorted.
fn keys(entry: &Json) -> Vec<&str> {
    let mut keys: Vec<&str> = entry
        .as_object()
        .unwrap_or_default()
        .iter()
        .map(|(key, _)| key.as_str())
        .collect();
    keys.sort_unstable();
    keys
}

/// Returns `true` for a draw entry: a label, and a value where it has one.
fn draw_entry(entry: &Json) -> bool {
    matches!(keys(entry).as_slice(), ["label"] | ["label", "value"])
        && entry.get("label").and_then(Json::as_str).is_some()
}

/// Returns `true` for a step entry: an action, with a client from 0 to
/// 2^31 − 1 or a drain mark that is true.
fn step_entry(entry: &Json) -> bool {
    let shaped = matches!(
        keys(entry).as_slice(),
        ["step"] | ["client" | "drain", "step"]
    );
    shaped
        && entry.get("step").and_then(Json::as_str).is_some()
        && entry
            .get("client")
            .is_none_or(|client| matches!(client, Json::Int(at) if (0..=LINE_MAX).contains(at)))
        && entry
            .get("drain")
            .is_none_or(|drain| matches!(drain, Json::Bool(true)))
}

/// Returns `true` for a calendar date written as `YYYY-MM-DD`.
fn date(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let number = |range: std::ops::Range<usize>| -> Option<u32> {
        let part = &text[range];
        part.bytes()
            .all(|b| b.is_ascii_digit())
            .then(|| part.parse().ok())
            .flatten()
    };
    let (Some(year), Some(month), Some(day)) = (number(0..4), number(5..7), number(8..10)) else {
        return false;
    };
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    year >= 1 && (1..=days).contains(&day)
}
