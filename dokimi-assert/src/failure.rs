//! The record a failing assertion reports, and where it came from.
//!
//! The record is the same shape in every implementation of the standard. The sentence a
//! person reads is rendered from it and is not standardised, because each language reads
//! its own conventions.

use std::fmt::Write as _;
use std::panic::Location;

/// The call site a failure came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Where {
    /// The file the assertion was called from.
    pub file: &'static str,
    /// The line within it.
    pub line: u32,
}

impl From<&'static Location<'static>> for Where {
    fn from(at: &'static Location<'static>) -> Self {
        Self {
            file: at.file(),
            line: at.line(),
        }
    }
}

/// One value a failure carries.
///
/// A field the library computes itself is held as the number it is, so a reader
/// comparing it does not have to parse a rendering. A field holding the caller's
/// own value is text: an assertion is generic over what it compares, and
/// requiring a conversion would stop a caller comparing their own types, which
/// is the thing that makes the library useful.
#[derive(Debug, Clone, PartialEq)]
pub enum Detail {
    /// A count the library measured: a length, an index, a number of attempts.
    Count(usize),
    /// A number the caller stated: a tolerance, an end of a range.
    Number(f64),
    /// Anything else, as the assertion would have printed it.
    Said(String),
}

impl std::fmt::Display for Detail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Count(held) => write!(f, "{held}"),
            Self::Number(held) => write!(f, "{held}"),
            Self::Said(held) => write!(f, "{held}"),
        }
    }
}

impl From<usize> for Detail {
    fn from(held: usize) -> Self {
        Self::Count(held)
    }
}

impl From<f64> for Detail {
    fn from(held: f64) -> Self {
        Self::Number(held)
    }
}

impl From<String> for Detail {
    fn from(held: String) -> Self {
        Self::Said(held)
    }
}

impl From<&str> for Detail {
    fn from(held: &str) -> Self {
        Self::Said(held.to_owned())
    }
}

/// What a failing assertion reports.
///
/// `detail` carries exactly the fields the assertion declares, already written out.
/// Rust has no untyped value to put in a map, so a field's value is the text the
/// assertion would have printed rather than the value itself.
#[derive(Debug, Clone, PartialEq)]
pub struct Failure {
    /// The canonical id the definition names.
    pub assertion: &'static str,
    /// The caller's message, unchanged.
    pub contract: String,
    /// The values named by that assertion's declared fields, in the order it declares
    /// them.
    pub detail: Vec<(&'static str, Detail)>,
    /// The call site.
    pub where_at: Where,
}

impl Failure {
    /// Answer one detail field, or `None` when the record does not carry it.
    #[must_use]
    pub fn detail(&self, name: &str) -> Option<&Detail> {
        self.detail
            .iter()
            .find(|(held, _)| *held == name)
            .map(|(_, value)| value)
    }

    /// Turn this record into the sentence a person reads.
    ///
    /// The contract leads, then the detail in the order the assertion states it. The
    /// standard fixes the record, not the sentence.
    #[must_use]
    pub fn render(&self) -> String {
        if self.detail.is_empty() {
            return self.contract.clone();
        }

        let mut said = format!("{}: ", self.contract);
        for (at, (name, value)) in self.detail.iter().enumerate() {
            if at > 0 {
                said.push_str(", ");
            }
            // Writing to a String cannot fail.
            let _ = write!(said, "{name} {value}");
        }
        said
    }
}
