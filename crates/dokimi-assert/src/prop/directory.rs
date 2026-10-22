// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The directory of a property's store, and the files in it.
//!
//! The store of a test is `testdata/prop/<target>/<test>` below the package
//! root, where the test runs. `<target>` is the name of the test binary
//! without the hash that cargo appends, and `<test>` is the path of the test,
//! one directory for each part between `::`. Each part escapes a character
//! that a file name cannot contain on some system as `%` and two hexadecimal
//! digits.
//!
//! Every file of the directory whose name ends in `.json` gets one verdict.
//! An entry of the property replays, and an entry of another property of the
//! test stays in place. An entry of a later format is noted and skipped, and
//! any other file is damaged.

use super::engine::canonical::same;
use super::engine::case::Drawn;
use super::engine::choice::Choice;
use super::engine::store::{self, MAX_DEPTH, Verdict};
use crate::json::{self, Json};
use crate::value::Value;
use std::collections::HashSet;
use std::fmt::Write as _;
use std::fs;
use std::io::{ErrorKind, Write as _};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::thread::ThreadId;

/// The directory of the stores of a package's tests, beside
/// `testdata/golden`.
const STORE_ROOT: [&str; 2] = ["testdata", "prop"];

/// The directory of the seed files of a package's fuzz targets.
const FUZZ_ROOT: [&str; 2] = ["testdata", "fuzz"];

/// A fault of the store: where it is and what is wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoreFault {
    pub(crate) at: String,
    pub(crate) reason: String,
}

/// An entry of a store that a property replays.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Stored {
    pub(crate) name: String,
    pub(crate) choices: Vec<Choice>,
    pub(crate) counterexample: Vec<Json>,
}

/// The entries that a property reads from its store, oldest first, and the
/// files that it skips.
#[derive(Debug, Default)]
pub(crate) struct Loaded {
    pub(crate) entries: Vec<Stored>,
    pub(crate) skipped: Vec<String>,
}

/// Returns a part of a path with each character that some system refuses in
/// a file name escaped: the reserved characters, the control characters, and
/// a dot or a space at the end, which some systems drop.
fn escaped(part: &str) -> String {
    let count = part.chars().count();
    let mut out = String::with_capacity(part.len());
    for (at, c) in part.chars().enumerate() {
        let ending = at + 1 == count && matches!(c, '.' | ' ');
        if "%/\\:*?\"<>|".contains(c) || u32::from(c) < 0x20 || ending {
            // Writing to a String cannot fail.
            let _ = write!(out, "%{:02X}", u32::from(c));
        } else {
            out.push(c);
        }
    }
    out
}

/// Returns the name of the test binary without the hash that cargo appends.
fn target() -> Option<String> {
    let binary = std::env::current_exe().ok()?;
    let stem = binary.file_stem()?.to_str()?;
    let hashed = stem
        .rsplit_once('-')
        .filter(|(_, hash)| hash.len() == 16 && hash.bytes().all(|b| b.is_ascii_hexdigit()));
    Some(hashed.map_or(stem, |(name, _)| name).to_owned())
}

/// Returns the directory of `test` below `root`.
fn below(root: [&str; 2], test: &str) -> Option<PathBuf> {
    let mut path: PathBuf = root.iter().collect();
    path.push(escaped(&target()?));
    for part in test.split("::") {
        path.push(escaped(part));
    }
    Some(path)
}

/// Returns the directory of the store of a property of `test`: the stated
/// directory first, then the store of the test. `None` turns the store off:
/// an empty stated directory, or a seat that runs no named test.
pub(crate) fn store_of(test: Option<&str>, given: Option<&Path>) -> Option<PathBuf> {
    match given {
        Some(given) if given.as_os_str().is_empty() => None,
        Some(given) => Some(given.to_path_buf()),
        None => test.and_then(|test| below(STORE_ROOT, test)),
    }
}

/// Returns the directory of the seed files of a fuzz target of `test`.
pub(crate) fn seeds_of(test: Option<&str>) -> Option<PathBuf> {
    test.and_then(|test| below(FUZZ_ROOT, test))
}

/// The claims of the running properties on the entries of their contracts,
/// by their thread: the thread of a test.
static CLAIMS: Mutex<Option<HashSet<(ThreadId, PathBuf, String)>>> = Mutex::new(None);

/// Claims the entries of `contract` in `directory` for the test of the
/// current thread, and returns `false` when another property of the test has
/// claimed them.
pub(crate) fn claim(directory: &Path, contract: &str) -> bool {
    let mut claims = CLAIMS.lock().unwrap_or_else(PoisonError::into_inner);
    claims.get_or_insert_with(HashSet::new).insert((
        std::thread::current().id(),
        directory.to_path_buf(),
        contract.to_owned(),
    ))
}

/// Returns the names of the `.json` files of a directory, in the order of
/// their names. A directory that does not exist has none.
#[expect(
    clippy::case_sensitive_file_extension_comparisons,
    reason = "every implementation of the definition reads the files whose names end in .json, in that case"
)]
fn files_of(directory: &Path) -> Result<Vec<String>, StoreFault> {
    let listed = match fs::read_dir(directory) {
        Ok(listed) => listed,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(StoreFault {
                at: directory.display().to_string(),
                reason: format!("the store cannot be read: {error}"),
            });
        }
    };
    let mut names: Vec<String> = listed
        .filter_map(Result::ok)
        .filter(|entry| !entry.path().is_dir())
        .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
        .filter(|name| name.ends_with(".json"))
        .collect();
    names.sort();
    Ok(names)
}

/// Returns the entries of `contract` in a store, ordered by the date on
/// which they were found and then by their names, with the files that it
/// skips.
pub(crate) fn load(directory: &Path, contract: &str) -> Result<Loaded, StoreFault> {
    let mut found: Vec<(String, Stored)> = Vec::new();
    let mut loaded = Loaded::default();
    let mut damaged: Vec<String> = Vec::new();
    for name in files_of(directory)? {
        let Ok(text) = fs::read_to_string(directory.join(&name)) else {
            damaged.push(name);
            continue;
        };
        match store::read(&text, contract) {
            Verdict::Replay(choices) => {
                let parsed = json::parse(&text, MAX_DEPTH).unwrap_or(Json::Null);
                let date = parsed
                    .get("found")
                    .and_then(Json::as_str)
                    .unwrap_or_default()
                    .to_owned();
                let counterexample = parsed
                    .get("counterexample")
                    .and_then(Json::as_array)
                    .map(<[Json]>::to_vec)
                    .unwrap_or_default();
                found.push((
                    date,
                    Stored {
                        name,
                        choices,
                        counterexample,
                    },
                ));
            }
            Verdict::Skip => loaded.skipped.push(name),
            Verdict::Damaged => damaged.push(name),
            Verdict::Other => {}
        }
    }
    if !damaged.is_empty() {
        return Err(StoreFault {
            at: directory.display().to_string(),
            reason: format!("the store has damaged files: {}", damaged.join(", ")),
        });
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    loaded.entries = found.into_iter().map(|(_, stored)| stored).collect();
    Ok(loaded)
}

/// Writes an entry into a store under `name`, unless a file of that name
/// exists, and returns `true` when it wrote the entry. The file is the text
/// of the entry with a final newline.
pub(crate) fn save(directory: &Path, name: &str, entry: &str) -> std::io::Result<bool> {
    fs::create_dir_all(directory)?;
    let mut file = match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join(name))
    {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::AlreadyExists => return Ok(false),
        Err(error) => return Err(error),
    };
    file.write_all(entry.as_bytes())?;
    file.write_all(b"\n")?;
    Ok(true)
}

/// Returns the seed files of a fuzz target, in the order of their names. A
/// directory that does not exist has none.
pub(crate) fn seeds(directory: &Path) -> Result<Vec<(String, Vec<u8>)>, StoreFault> {
    let refused = |error: std::io::Error| StoreFault {
        at: directory.display().to_string(),
        reason: format!("the seed files cannot be read: {error}"),
    };
    let listed = match fs::read_dir(directory) {
        Ok(listed) => listed,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(refused(error)),
    };
    let mut names: Vec<(String, PathBuf)> = listed
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_file())
        .filter_map(|entry| Some((entry.file_name().to_str()?.to_owned(), entry.path())))
        .collect();
    names.sort();
    names
        .into_iter()
        .map(|(name, path)| fs::read(path).map(|data| (name, data)).map_err(refused))
        .collect()
}

/// Returns `true` when the draws of a replayed stored case differ from the
/// draws of its entry: other labels, another number of draws, or another
/// value of a draw with a value in the entry.
pub(crate) fn differs(recorded: &[Json], replayed: &[Drawn]) -> bool {
    if recorded.len() != replayed.len() {
        return true;
    }
    recorded.iter().zip(replayed).any(|(entry, drawn)| {
        if entry.get("label").and_then(Json::as_str) != Some(drawn.label.as_str()) {
            return true;
        }
        let Some(stated) = entry.get("value") else {
            return false;
        };
        let stated = Value::from_literal(stated).ok();
        match (stated, &drawn.value.literal) {
            (Some(stated), Some(value)) => !same(&stated, value),
            _ => true,
        }
    })
}
