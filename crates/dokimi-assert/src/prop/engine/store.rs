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
//! A run gives each file one verdict:
//!
//! - `Replay`: an entry of the format for the property, whose token is of
//!   the version that the token module encodes. The run tries its choices
//!   first.
//! - `Other`: an entry of the format for another property of the test.
//! - `Skip`: an entry of a later format, or whose token is of a later version.
//!   The run notes it and goes on.
//! - `Damaged`: any other file, including JSON that repeats a name within an
//!   object, states a non-finite number, or nests past 64 levels. The test
//!   fails, as it fails on a damaged golden file.

use super::choice::Choice;
use super::source::mix;
use super::token;
use crate::json::{self, Json, object};
use crate::value::Value;

/// The format of the entries that this module writes and replays.
const FORMAT: i128 = 1;

/// The version of the tokens that the token module encodes.
const TOKEN_VERSION: u64 = 1;

/// The most levels of objects and arrays that a file nests, the root
/// included.
pub(crate) const MAX_DEPTH: usize = 64;

/// The levels above a draw's value: the root, the counterexample and the
/// draw.
const ABOVE_VALUE: usize = 3;

/// The largest line of an identity, the largest signed 32-bit integer.
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
    /// One draw of the counterexample: its label, and its typed literal where
    /// it has one.
    pub counterexample: Vec<(String, Option<Value>)>,
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
/// written as `YYYY-MM-DD`. A draw whose value would nest the entry past 64
/// levels is recorded by its label alone.
#[must_use]
pub fn entry(failure: &Failure, definition: &str, found: &str) -> String {
    let identity = Json::Object(
        failure
            .identity
            .iter()
            .map(|(key, value)| (key.clone(), plain(value)))
            .collect(),
    );
    let draws = failure
        .counterexample
        .iter()
        .map(|(label, value)| {
            let mut draw = vec![("label".to_owned(), Json::from(label.as_str()))];
            if let Some(value) = value {
                let literal = value.to_literal();
                if literal.depth() <= MAX_DEPTH - ABOVE_VALUE {
                    draw.push(("value".to_owned(), literal));
                }
            }
            Json::Object(draw)
        })
        .collect();
    object([
        ("store", Json::Int(FORMAT)),
        ("definition", definition.into()),
        ("property", failure.contract.as_str().into()),
        ("identity", identity),
        ("choices", token::encode(&failure.choices).into()),
        ("counterexample", Json::Array(draws)),
        ("found", found.into()),
    ])
    .pretty()
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
    if *version < FORMAT {
        return Verdict::Damaged;
    }
    if *version > FORMAT {
        return Verdict::Skip;
    }
    if !well_formed(&document) {
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

/// Returns `true` when every field of an entry of the format has its form.
fn well_formed(document: &Json) -> bool {
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
        && document.get("counterexample").is_some_and(counterexample)
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

/// Returns `true` for a list of draws with a label each.
fn counterexample(draws: &Json) -> bool {
    draws.as_array().is_some_and(|draws| {
        draws.iter().all(|draw| {
            draw.as_object().is_some_and(|members| {
                let keys: Vec<&str> = members.iter().map(|(key, _)| key.as_str()).collect();
                let shaped = matches!(
                    keys.as_slice(),
                    ["label"] | ["label", "value"] | ["value", "label"]
                );
                shaped && draw.get("label").and_then(Json::as_str).is_some()
            })
        })
    })
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
