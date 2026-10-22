//! The record that a failing assertion reports, and the call site it names.

use crate::value::Value;
use std::fmt::{self, Display};
use std::panic::Location;
use std::path::Path;

/// The call site of an assertion: the file and the line that `#[track_caller]`
/// reports for the caller's frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Where {
    file: &'static str,
    line: u32,
}

impl Where {
    /// Returns the path of the file, as the compiler states it.
    #[must_use]
    pub fn file(&self) -> &'static str {
        self.file
    }

    /// Returns the line, from 1.
    #[must_use]
    pub fn line(&self) -> u32 {
        self.line
    }

    /// Returns the call site at `line` of `file`.
    pub(crate) fn at(file: &'static str, line: u32) -> Self {
        Self { file, line }
    }

    /// Returns the base name of the file, as a call record and a property's
    /// store state it, so that both are the same on every machine.
    pub(crate) fn base_name(&self) -> &'static str {
        Path::new(self.file)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(self.file)
    }
}

impl From<&'static Location<'static>> for Where {
    fn from(at: &'static Location<'static>) -> Self {
        Self {
            file: at.file(),
            line: at.line(),
        }
    }
}

impl Display for Where {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.file, self.line)
    }
}

/// The record that a failing assertion reports to its seat.
///
/// The record is the same in every implementation of the definition. Its
/// detail contains exactly the fields that the definition declares for the
/// assertion, in the order the definition lists them. The sentence that
/// [`Display`] renders is this library's own.
///
/// A test reads the records that a [`Recorder`](crate::Recorder) kept,
/// rather than searching the sentence:
///
/// ```
/// use dokimi_assert::{Recorder, Value, check};
///
/// let r = Recorder::new();
/// check::equal(&r, 1 + 1, 3, "the sum is right");
/// let failure = &r.failures()[0];
/// assert_eq!(failure.assertion(), Some("equal"));
/// assert_eq!(failure.contract(), "the sum is right");
/// assert_eq!(failure.want(), Some(&Value::Opaque("3".to_owned())));
/// assert_eq!(failure.to_string(), "the sum is right: want 3, got 2");
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Failure {
    assertion: Option<&'static str>,
    contract: String,
    detail: Vec<(&'static str, Value)>,
    location: Option<Where>,
    case: Option<Box<Failure>>,
    sentence: Option<String>,
}

impl Failure {
    /// Returns the record of a failure of `assertion`.
    pub(crate) fn new(
        assertion: &'static str,
        contract: &str,
        detail: Vec<(&'static str, Value)>,
        location: Option<Where>,
    ) -> Self {
        Self {
            assertion: Some(assertion),
            contract: contract.to_owned(),
            detail,
            location,
            case: None,
            sentence: None,
        }
    }

    /// Returns the record of a message that a body passed to a case's own
    /// `fail` or `record`, which no assertion reported.
    pub(crate) fn message(message: &str, location: Option<Where>) -> Self {
        Self {
            assertion: None,
            contract: message.to_owned(),
            detail: Vec::new(),
            location,
            case: None,
            sentence: None,
        }
    }

    /// Returns the record with `sentence` as the sentence that it renders,
    /// for a record whose detail no single line states.
    pub(crate) fn with_sentence(mut self, sentence: String) -> Self {
        self.sentence = Some(sentence);
        self
    }

    /// Returns the record with `case` as the record of its failing case.
    pub(crate) fn with_case(mut self, case: Failure) -> Self {
        self.case = Some(Box::new(case));
        self
    }

    /// Returns the record with its call site at `location`.
    pub(crate) fn at(mut self, location: Option<Where>) -> Self {
        self.location = location;
        self
    }

    /// Returns the canonical id of the assertion, as the definition spells it,
    /// such as `equal` or `prop-for-all`.
    ///
    /// Returns `None` for the record of a message that a body passed to the
    /// `fail` or `record` of a property's case.
    #[must_use]
    pub fn assertion(&self) -> Option<&'static str> {
        self.assertion
    }

    /// Returns the caller's message, unchanged.
    #[must_use]
    pub fn contract(&self) -> &str {
        &self.contract
    }

    /// Returns the fields of the detail, in the order the definition declares
    /// them for the assertion.
    #[must_use]
    pub fn detail(&self) -> &[(&'static str, Value)] {
        &self.detail
    }

    /// Returns the value of the detail's field `name`, or `None` when the
    /// assertion declares no such field.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<&Value> {
        self.detail
            .iter()
            .find(|(held, _)| *held == name)
            .map(|(_, value)| value)
    }

    /// Returns the value of the field `want`: what the assertion required.
    ///
    /// Returns `None` when the assertion declares no `want`, and
    /// `Some(&Value::Null)` when it declares one without a value.
    #[must_use]
    pub fn want(&self) -> Option<&Value> {
        self.field("want")
    }

    /// Returns the value of the field `got`: what the assertion observed. The
    /// rule of [`Failure::want`] applies.
    #[must_use]
    pub fn got(&self) -> Option<&Value> {
        self.field("got")
    }

    /// Returns the record of a property's failing case: the minimal case of a
    /// counterexample, or the case that the replay of a flaky run
    /// contradicted.
    ///
    /// Returns `None` for any other failure.
    #[must_use]
    pub fn case_failure(&self) -> Option<&Failure> {
        self.case.as_deref()
    }

    /// Returns the call site of the assertion.
    #[must_use]
    pub fn location(&self) -> Option<Where> {
        self.location
    }
}

/// Renders the sentence of the record: the contract, then each field of the
/// detail with its value, such as `the count is right: want 2, got 1`.
impl Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(sentence) = &self.sentence {
            return f.write_str(sentence);
        }
        f.write_str(&self.contract)?;
        for (at, (name, value)) in self.detail.iter().enumerate() {
            let separator = if at == 0 { ": " } else { ", " };
            write!(f, "{separator}{name} {value}")?;
        }
        Ok(())
    }
}
