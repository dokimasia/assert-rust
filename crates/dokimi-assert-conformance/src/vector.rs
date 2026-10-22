// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The vectors of the definition's history and machine engines, and how a
//! run disagrees with one.

use crate::definition::{self, Error};
use serde_json::Value as Json;
use std::fmt;

/// One vector of an engine.
#[derive(Debug, Clone, PartialEq)]
pub struct Vector {
    /// The id of the vector.
    pub id: String,
    /// The kind of the vector, which its file names.
    pub kind: String,
    /// The vector as its file states it.
    pub raw: Json,
}

/// Returns every vector of the engine `engine`, the files under
/// `corpus/<engine>/`, in the order of its files.
///
/// # Errors
///
/// Returns the error of a vector file that cannot be read, or that states no
/// kind or a vector without an id.
pub fn vectors(engine: &str) -> Result<Vec<Vector>, Error> {
    let mut found = Vec::new();
    for file in definition::vector_files(engine)? {
        let stated = definition::read(&file)?;
        let Some(kind) = stated["kind"].as_str() else {
            return Err(Error::Malformed {
                file,
                what: "the file names no kind",
            });
        };
        for raw in stated["cases"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            let Some(id) = raw["id"].as_str() else {
                return Err(Error::Malformed {
                    file,
                    what: "a vector has no id",
                });
            };
            found.push(Vector {
                id: id.to_owned(),
                kind: kind.to_owned(),
                raw: raw.clone(),
            });
        }
    }
    Ok(found)
}

/// How the run of a vector disagrees with the vector: the member of the
/// vector that the run contradicts or cannot read, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disagreement {
    /// The member of the vector, such as `detail.steps`.
    pub at: String,
    /// What the run observed.
    pub reason: String,
}

impl Disagreement {
    pub(crate) fn new(at: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            at: at.into(),
            reason: reason.into(),
        }
    }
}

impl fmt::Display for Disagreement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.at.is_empty() {
            return f.write_str(&self.reason);
        }
        write!(f, "{}: {}", self.at, self.reason)
    }
}

impl std::error::Error for Disagreement {}
