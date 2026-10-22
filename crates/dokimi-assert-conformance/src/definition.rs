//! The files of the vendored definition.

use crate::literal::Malformed;
use serde_json::Value as Json;
use std::fmt;
use std::path::PathBuf;

/// Why a file of the vendored definition cannot be read.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// The file cannot be read.
    Read {
        /// The path of the file.
        path: PathBuf,
        /// The error of the read.
        source: std::io::Error,
    },
    /// The file is no JSON.
    Parse {
        /// The path of the file.
        path: PathBuf,
        /// The error of the parser.
        source: serde_json::Error,
    },
    /// The definition declares no assertion of the id.
    Undeclared(String),
    /// A file of the definition does not have the structure that the
    /// definition states.
    Malformed {
        /// The path of the file, relative to the definition.
        file: String,
        /// What the file lacks.
        what: &'static str,
    },
    /// A typed literal of a file is malformed.
    Literal {
        /// The path of the file, relative to the definition.
        file: String,
        /// The malformed literal.
        source: Malformed,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, .. } => write!(f, "cannot read {}", path.display()),
            Self::Parse { path, .. } => write!(f, "{} is no JSON", path.display()),
            Self::Undeclared(assertion) => {
                write!(f, "the definition declares no assertion {assertion}")
            }
            Self::Malformed { file, what } => write!(f, "{file}: {what}"),
            Self::Literal { file, source } => write!(f, "{file}: {source}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
            Self::Literal { source, .. } => Some(source),
            Self::Undeclared(_) | Self::Malformed { .. } => None,
        }
    }
}

/// Returns the directory of the vendored definition, which is the directory
/// of this crate. Each file of the definition is at its path in assert-spec,
/// such as `spec/assertions.json`.
#[must_use]
pub fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Returns the version of the vendored definition, which its file `VERSION`
/// states.
///
/// # Errors
///
/// Returns [`Error::Read`] when the file cannot be read.
pub fn version() -> Result<String, Error> {
    let path = dir().join("VERSION");
    match std::fs::read_to_string(&path) {
        Ok(text) => Ok(text.trim().to_owned()),
        Err(source) => Err(Error::Read { path, source }),
    }
}

/// Returns the file `relative` of the definition as JSON.
///
/// # Errors
///
/// Returns [`Error::Read`] when the file cannot be read, and
/// [`Error::Parse`] when it is no JSON.
pub fn read(relative: &str) -> Result<Json, Error> {
    let path = dir().join(relative);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(source) => return Err(Error::Read { path, source }),
    };
    serde_json::from_str(&text).map_err(|source| Error::Parse { path, source })
}

/// Returns the detail fields that the definition declares for `assertion`,
/// in the order of the definition.
///
/// # Errors
///
/// Returns [`Error::Undeclared`] when the definition declares no such
/// assertion, and the error of [`read`] when `spec/assertions.json` cannot
/// be read.
pub fn declared(assertion: &str) -> Result<Vec<String>, Error> {
    let assertions = read("spec/assertions.json")?;
    let Some(fields) = assertions["assertions"][assertion]["detail_fields"].as_array() else {
        return Err(Error::Undeclared(assertion.to_owned()));
    };
    Ok(fields
        .iter()
        .filter_map(Json::as_str)
        .map(str::to_owned)
        .collect())
}

/// Returns the ids of the relaxations that the overlay of Rust declines.
///
/// # Errors
///
/// Returns the error of [`read`] when `overlays/rust.json` cannot be read.
pub fn declined() -> Result<Vec<String>, Error> {
    let overlay = read("overlays/rust.json")?;
    let relaxations = overlay["relaxations"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    Ok(relaxations
        .iter()
        .filter_map(|relaxation| relaxation["id"].as_str())
        .map(str::to_owned)
        .collect())
}

/// Returns the paths of the corpus files directly under `corpus/`, relative
/// to the definition and sorted. The vectors of the engines are in the
/// subdirectories of `corpus/`.
///
/// # Errors
///
/// Returns [`Error::Read`] when the directory cannot be listed.
pub fn corpus_files() -> Result<Vec<String>, Error> {
    json_files("corpus")
}

/// Returns the paths of the vector files of the engine `engine`, the files
/// directly under `corpus/<engine>/`, relative to the definition and sorted.
///
/// # Errors
///
/// Returns [`Error::Read`] when the directory cannot be listed.
pub fn vector_files(engine: &str) -> Result<Vec<String>, Error> {
    json_files(&format!("corpus/{engine}"))
}

/// Returns the paths of the JSON files directly under `under`, relative to
/// the definition and sorted.
fn json_files(under: &str) -> Result<Vec<String>, Error> {
    let path = dir().join(under);
    let entries = match std::fs::read_dir(&path) {
        Ok(entries) => entries,
        Err(source) => return Err(Error::Read { path, source }),
    };
    let mut files: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .filter_map(|path| {
            path.file_name()?
                .to_str()
                .map(|name| format!("{under}/{name}"))
        })
        .collect();
    files.sort();
    Ok(files)
}
