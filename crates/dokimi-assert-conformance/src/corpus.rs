//! The corpus of the definition, run on each surface through a recorder.
//!
//! A case runs on `check` and on `soft` of each library that states its
//! assertion. The driver compares the verdict of the call, its failure record
//! and its call record with the case.

use crate::definition::{self, Error};
use crate::literal::{self, Difference, Rendering};
use crate::subject;
use dokimi_assert::{Controlled, Failure, Recorder, Value, check, soft};
use serde_json::Value as Json;
use std::fmt;
use std::sync::Arc;

/// The assertions of `dokimi-assert-tokio`, whose subject is a future.
pub const FUTURES: [&str; 6] = [
    "honours-cancellation",
    "honours-deadline",
    "completes-within",
    "eventually",
    "eventually-true",
    "no-task-leaks",
];

/// A library that the corpus runs against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Library {
    /// `dokimi-assert`.
    Core,
    /// `dokimi-assert-tokio`.
    Tokio,
}

impl Library {
    /// Every library, in the order that the corpus runs them.
    pub const ALL: [Self; 2] = [Self::Core, Self::Tokio];

    /// Returns `true` when the library states `assertion`.
    /// `dokimi-assert` states every assertion but `no-task-leaks`.
    #[must_use]
    pub fn states(self, assertion: &str) -> bool {
        match self {
            Self::Core => assertion != "no-task-leaks",
            Self::Tokio => FUTURES.contains(&assertion),
        }
    }
}

/// A surface of the library that the corpus runs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Surface {
    /// The surface whose failures end the test.
    Check,
    /// The surface whose failures let the test continue.
    Soft,
}

impl Surface {
    /// Every surface, in the order that the corpus runs them.
    pub const ALL: [Self; 2] = [Self::Check, Self::Soft];

    /// Returns `true` for the surface whose failures end the test, as the
    /// member `aborting` of a call record states it.
    #[must_use]
    pub fn aborting(self) -> bool {
        self == Self::Check
    }
}

/// One case of the corpus.
#[derive(Debug, Clone, PartialEq)]
pub struct Case {
    /// The id of the case, which the driver passes as the contract.
    pub id: String,
    /// The id of the assertion.
    pub assertion: String,
    /// The arguments of a case that calls the assertion with values.
    pub args: Vec<Value>,
    /// `true` when the case expects a failure.
    pub fails: bool,
    /// The fields of the failure's detail that the case states.
    pub detail: Vec<(String, Value)>,
    /// The kind of the subject of a case that runs a subject.
    pub subject: Option<String>,
}

/// Returns the cases of the corpus that apply to Rust: every case without a
/// skip for Rust, and without an option that the overlay of Rust declines.
///
/// # Errors
///
/// Returns the error of a corpus file that cannot be read, or whose cases do
/// not have the structure that the definition states.
pub fn cases() -> Result<Vec<Case>, Error> {
    let declined = definition::declined()?;
    let mut found = Vec::new();
    for file in definition::corpus_files()? {
        let corpus = definition::read(&file)?;
        let Some(assertion) = corpus["assertion"].as_str() else {
            return Err(Error::Malformed {
                file,
                what: "the file names no assertion",
            });
        };
        for case in corpus["cases"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            if applies(case, &declined) {
                found.push(parse(&file, assertion, case)?);
            }
        }
    }
    Ok(found)
}

/// Returns `true` for a case without a skip for Rust, and without an option
/// that the overlay declines.
fn applies(case: &Json, declined: &[String]) -> bool {
    let options = case["options"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    case["skip"].get("rust").is_none()
        && !options
            .iter()
            .filter_map(Json::as_str)
            .any(|option| declined.iter().any(|id| id == option))
}

fn parse(file: &str, assertion: &str, case: &Json) -> Result<Case, Error> {
    let malformed = |what| Error::Malformed {
        file: file.to_owned(),
        what,
    };
    let literal = |source| Error::Literal {
        file: file.to_owned(),
        source,
    };
    let id = case["id"]
        .as_str()
        .ok_or_else(|| malformed("a case has no id"))?;
    let fails = match case["expect"].as_str() {
        Some("pass") => false,
        Some("fail") => true,
        _ => return Err(malformed("a case expects neither pass nor fail")),
    };
    let args = case["args"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let detail = case["detail"].as_object().into_iter().flatten();
    Ok(Case {
        id: id.to_owned(),
        assertion: assertion.to_owned(),
        args: args
            .iter()
            .map(literal::decode)
            .collect::<Result<_, _>>()
            .map_err(literal)?,
        fails,
        detail: detail
            .map(|(name, value)| Ok((name.clone(), literal::decode(value).map_err(literal)?)))
            .collect::<Result<_, Error>>()?,
        subject: case["subject"]["kind"].as_str().map(str::to_owned),
    })
}

/// How the library's run of a case disagrees with the case.
#[derive(Debug)]
#[non_exhaustive]
pub enum Disagreement {
    /// The driver calls no assertion of the case's id.
    NoDriver,
    /// The driver builds no subject of the kind for the case's assertion.
    NoSubject(String),
    /// The case states no argument at a position, or a value of a type that
    /// the assertion does not take there.
    Argument {
        /// The position of the argument.
        at: usize,
        /// The kind of value that the assertion takes there.
        expected: &'static str,
        /// The value that the case states, or `None` for no argument.
        stated: Option<Value>,
    },
    /// The call passed where the case expects a failure, or failed where the
    /// case expects a pass.
    Verdict {
        /// `true` when the case expects a failure.
        fails: bool,
        /// The message of the failure that the call reported, or the empty
        /// string.
        message: String,
    },
    /// A failing call reported no failure record.
    NoRecord,
    /// A passing call reported a failure record.
    Unexpected(Failure),
    /// The failure record names another assertion.
    Assertion(Option<&'static str>),
    /// The failure record states another contract.
    Contract(String),
    /// The failure record states other fields than the definition declares.
    Fields(Vec<String>),
    /// The failure record names a call site outside the driver.
    Location(String),
    /// A field of the failure record differs from the case.
    Field {
        /// The name of the field.
        name: String,
        /// How the field differs.
        difference: Difference,
    },
    /// The recorder kept no call record.
    NoCallRecord,
    /// The call record is no JSON.
    CallRecordJson(String),
    /// A member of the call record differs from the case.
    Member {
        /// The name of the member.
        name: &'static str,
        /// The value of the member that the call record states.
        stated: Json,
    },
    /// The call record of a case states a parent.
    Parent,
    /// The call record of a failure states no detail, or the call record of
    /// a pass states one.
    Detail {
        /// `true` when the case expects a failure.
        fails: bool,
    },
    /// The detail of the call record states other fields than the
    /// definition declares.
    DetailFields(Vec<String>),
    /// A field of the detail of the call record differs from the case.
    DetailField {
        /// The name of the field.
        name: String,
        /// How the field differs.
        difference: Difference,
    },
    /// A field of the detail of the call record is no typed literal.
    DetailLiteral {
        /// The name of the field.
        name: String,
        /// The value of the field.
        stated: Json,
    },
    /// The definition cannot be read.
    Definition(Error),
}

impl fmt::Display for Disagreement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let verdict = |fails: bool| if fails { "a failure" } else { "a pass" };
        match self {
            Self::NoDriver => f.write_str("the driver calls no assertion of the case's id"),
            Self::NoSubject(kind) => {
                write!(f, "the driver builds no subject {kind} for the assertion")
            }
            Self::Argument {
                at,
                expected,
                stated: None,
            } => write!(
                f,
                "the case states no argument {at}, where {expected} belongs"
            ),
            Self::Argument {
                at,
                expected,
                stated: Some(stated),
            } => write!(
                f,
                "the case states {stated:?} as argument {at}, where {expected} belongs"
            ),
            Self::Verdict { fails, message } => {
                write!(
                    f,
                    "the case expects {}, and the call reported {message:?}",
                    verdict(*fails)
                )
            }
            Self::NoRecord => f.write_str("the failure reported no record"),
            Self::Unexpected(failure) => write!(f, "a passing case reported the failure {failure}"),
            Self::Assertion(assertion) => write!(f, "the record names the assertion {assertion:?}"),
            Self::Contract(contract) => write!(f, "the record states the contract {contract:?}"),
            Self::Fields(names) => write!(f, "the record states the fields {names:?}"),
            Self::Location(at) => {
                write!(f, "the record names the call site {at}, outside the driver")
            }
            Self::Field { name, difference } => write!(f, "the field {name}: {difference}"),
            Self::NoCallRecord => f.write_str("the recorder kept no call record"),
            Self::CallRecordJson(line) => write!(f, "the call record is no JSON: {line}"),
            Self::Member { name, stated } => write!(f, "the call record states {name} {stated}"),
            Self::Parent => f.write_str("the call record of a case states a parent"),
            Self::Detail { fails: true } => {
                f.write_str("the call record of a failure states no detail")
            }
            Self::Detail { fails: false } => {
                f.write_str("the call record of a pass states a detail")
            }
            Self::DetailFields(names) => write!(f, "the call record states the fields {names:?}"),
            Self::DetailField { name, difference } => {
                write!(f, "the call record's field {name}: {difference}")
            }
            Self::DetailLiteral { name, stated } => {
                write!(
                    f,
                    "the call record's field {name} is no typed literal: {stated}"
                )
            }
            Self::Definition(error) => write!(f, "the definition cannot be read: {error}"),
        }
    }
}

impl std::error::Error for Disagreement {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Definition(error) => Some(error),
            _ => None,
        }
    }
}

impl From<Error> for Disagreement {
    fn from(error: Error) -> Self {
        Self::Definition(error)
    }
}

fn arg<'a>(
    args: &'a [Value],
    at: usize,
    expected: &'static str,
) -> Result<&'a Value, Disagreement> {
    args.get(at).ok_or(Disagreement::Argument {
        at,
        expected,
        stated: None,
    })
}

fn unfit(at: usize, expected: &'static str, stated: &Value) -> Disagreement {
    Disagreement::Argument {
        at,
        expected,
        stated: Some(stated.clone()),
    }
}

fn value(args: &[Value], at: usize) -> Result<&Value, Disagreement> {
    arg(args, at, "a value")
}

fn text(args: &[Value], at: usize) -> Result<&str, Disagreement> {
    match arg(args, at, "text")? {
        Value::String(text) => Ok(text),
        other => Err(unfit(at, "text", other)),
    }
}

fn number(args: &[Value], at: usize) -> Result<f64, Disagreement> {
    match *arg(args, at, "a number")? {
        Value::Float(held) => Ok(held),
        #[expect(
            clippy::cast_precision_loss,
            reason = "every bound of the corpus is a small integer"
        )]
        Value::Int(held) => Ok(held as f64),
        ref other => Err(unfit(at, "a number", other)),
    }
}

fn count(args: &[Value], at: usize) -> Result<usize, Disagreement> {
    let stated = arg(args, at, "a count")?;
    match stated {
        Value::Int(held) => usize::try_from(*held).map_err(|_| unfit(at, "a count", stated)),
        other => Err(unfit(at, "a count", other)),
    }
}

fn items(args: &[Value], at: usize) -> Result<&[Value], Disagreement> {
    match arg(args, at, "a list")? {
        Value::List(items) => Ok(items),
        other => Err(unfit(at, "a list", other)),
    }
}

fn texts(args: &[Value], at: usize) -> Result<Vec<&str>, Disagreement> {
    let stated = items(args, at)?;
    stated
        .iter()
        .map(|item| match item {
            Value::String(text) => Ok(text.as_str()),
            _ => Err(unfit(at, "a list of text", &Value::List(stated.to_vec()))),
        })
        .collect()
}

/// Returns `None` for null, the value that an absent option states.
fn option(value: &Value) -> Option<&Value> {
    match value {
        Value::Null => None,
        held => Some(held),
    }
}

/// Defines the function `$name`, which calls the assertion of a case that
/// states values on the surface `$surface`, and returns how it handed the
/// values to the assertion.
macro_rules! data_case {
    ($name:ident, $surface:ident) => {
        fn $name(seat: &Recorder, case: &Case) -> Result<Rendering, Disagreement> {
            let (a, m) = (case.args.as_slice(), case.id.as_str());
            match case.assertion.as_str() {
                "equal" => $surface::equal(seat, value(a, 0)?.clone(), value(a, 1)?.clone(), m),
                "not-equal" => {
                    $surface::not_equal(seat, value(a, 0)?.clone(), value(a, 1)?.clone(), m)
                }
                "true" => $surface::is_true(seat, *value(a, 0)? == Value::Bool(true), m),
                "false" => $surface::is_false(seat, *value(a, 0)? == Value::Bool(true), m),
                "nil" => $surface::is_none(seat, option(value(a, 0)?), m),
                "not-nil" => $surface::is_some(seat, option(value(a, 0)?), m),
                "length" => $surface::length(seat, value(a, 0)?, count(a, 1)?, m),
                "empty" => $surface::is_empty(seat, value(a, 0)?, m),
                "not-empty" => $surface::is_not_empty(seat, value(a, 0)?, m),
                "contains" => $surface::contains(seat, value(a, 0)?, value(a, 1)?, m),
                "not-contains" => $surface::not_contains(seat, value(a, 0)?, value(a, 1)?, m),
                "contains-in-order" => {
                    $surface::contains_in_order(seat, text(a, 0)?, &texts(a, 1)?, m)
                }
                "permutation" => {
                    $surface::is_permutation(seat, items(a, 0)?, items(a, 1)?, m);
                    return Ok(Rendering::Slices);
                }
                "has-prefix" => $surface::has_prefix(seat, text(a, 0)?, text(a, 1)?, m),
                "has-suffix" => $surface::has_suffix(seat, text(a, 0)?, text(a, 1)?, m),
                "matches" => $surface::matches(seat, text(a, 0)?, text(a, 1)?, m),
                "close-to" => {
                    $surface::close_to(seat, value(a, 0)?.clone(), number(a, 1)?, number(a, 2)?, m)
                }
                "in-range" => {
                    $surface::in_range(seat, value(a, 0)?.clone(), number(a, 1)?, number(a, 2)?, m)
                }
                _ => return Err(Disagreement::NoDriver),
            }
            Ok(Rendering::Values)
        }
    };
}

data_case!(check_data_case, check);
data_case!(soft_data_case, soft);

/// What a call reported to its recorder.
#[derive(Debug, Clone, PartialEq)]
pub struct Observed {
    /// `true` when the call reported a failure.
    pub failed: bool,
    /// The message of the first failure, or the empty string.
    pub message: String,
    /// The failure records, in the order of the calls.
    pub failures: Vec<Failure>,
    /// The call records, one line of JSON each.
    pub records: Vec<String>,
}

impl Observed {
    /// Returns what `seat` was told.
    #[must_use]
    pub fn of(seat: &Recorder) -> Self {
        Self {
            failed: seat.failed(),
            message: seat.message(),
            failures: seat.failures(),
            records: seat.records(),
        }
    }
}

/// Runs `case` on `surface` of `library` and compares what the call reported
/// with the case.
///
/// # Errors
///
/// Returns how the run disagrees with the case.
pub fn run(library: Library, surface: Surface, case: &Case) -> Result<(), Disagreement> {
    let seat = Recorder::new().with_clock(Arc::new(Controlled::new()));
    let rendering = if let Some(kind) = &case.subject {
        if !subject::run(library, surface, &case.assertion, kind, &seat, &case.id) {
            return Err(Disagreement::NoSubject(kind.clone()));
        }
        Rendering::Native
    } else {
        match (library, surface) {
            (Library::Core, Surface::Check) => check_data_case(&seat, case)?,
            (Library::Core, Surface::Soft) => soft_data_case(&seat, case)?,
            (Library::Tokio, _) => return Err(Disagreement::NoDriver),
        }
    };
    compare(&Observed::of(&seat), surface, case, rendering)
}

/// Compares what a call of `case` on `surface` reported with the case. The
/// call handed the case's values to the assertion as `rendering` states.
///
/// # Errors
///
/// Returns how the observation disagrees with the case.
pub fn compare(
    observed: &Observed,
    surface: Surface,
    case: &Case,
    rendering: Rendering,
) -> Result<(), Disagreement> {
    if observed.failed != case.fails {
        return Err(Disagreement::Verdict {
            fails: case.fails,
            message: observed.message.clone(),
        });
    }
    compare_failure(observed, case, rendering)?;
    compare_record(observed, surface, case, rendering)
}

/// Compares the failure record of the case's call with the case.
///
/// # Errors
///
/// Returns how the failure record disagrees with the case.
pub fn compare_failure(
    observed: &Observed,
    case: &Case,
    rendering: Rendering,
) -> Result<(), Disagreement> {
    let failure = match (observed.failures.first(), case.fails) {
        (None, true) => return Err(Disagreement::NoRecord),
        (None, false) => return Ok(()),
        (Some(failure), false) => return Err(Disagreement::Unexpected(failure.clone())),
        (Some(failure), true) => failure,
    };
    if failure.assertion() != Some(case.assertion.as_str()) {
        return Err(Disagreement::Assertion(failure.assertion()));
    }
    if failure.contract() != case.id {
        return Err(Disagreement::Contract(failure.contract().to_owned()));
    }
    let names: Vec<String> = failure
        .detail()
        .iter()
        .map(|(name, _)| (*name).to_owned())
        .collect();
    if names != definition::declared(&case.assertion)? {
        return Err(Disagreement::Fields(names));
    }
    if let Some(at) = failure.location().filter(|at| !in_driver(at.file())) {
        return Err(Disagreement::Location(at.to_string()));
    }
    for (name, want) in &case.detail {
        let got = failure.field(name).unwrap_or(&Value::Null);
        if let Some(difference) = literal::differs(got, want, rendering) {
            return Err(Disagreement::Field {
                name: name.clone(),
                difference,
            });
        }
    }
    Ok(())
}

/// Returns `true` for a file of the driver, which makes every call of the
/// corpus: `src/corpus.rs` or `src/subject.rs` of this crate.
fn in_driver(file: &str) -> bool {
    let path = std::path::Path::new(file);
    ["corpus.rs", "subject.rs"]
        .iter()
        .any(|name| path.ends_with(name))
        && path.parent().is_some_and(|parent| parent.ends_with("src"))
        && path
            .components()
            .any(|part| part.as_os_str() == "dokimi-assert-conformance")
}

/// Compares the one call record of the case's call on `surface` with the
/// case.
///
/// # Errors
///
/// Returns how the call record disagrees with the case.
pub fn compare_record(
    observed: &Observed,
    surface: Surface,
    case: &Case,
    rendering: Rendering,
) -> Result<(), Disagreement> {
    let line = observed.records.first().ok_or(Disagreement::NoCallRecord)?;
    let record: Json =
        serde_json::from_str(line).map_err(|_| Disagreement::CallRecordJson(line.clone()))?;
    let members = [
        ("seq", Json::from(1)),
        ("assertion", Json::from(case.assertion.as_str())),
        ("contract", Json::from(case.id.as_str())),
        (
            "verdict",
            Json::from(if case.fails { "fail" } else { "pass" }),
        ),
        ("aborting", Json::from(surface.aborting())),
        ("definition", Json::from("10.0.0")),
    ];
    if let Some((name, _)) = members
        .into_iter()
        .find(|(name, want)| record[name] != *want)
    {
        return Err(Disagreement::Member {
            name,
            stated: record[name].clone(),
        });
    }
    if record.get("parent").is_some() {
        return Err(Disagreement::Parent);
    }
    match (record.get("detail"), case.fails) {
        (None, false) => Ok(()),
        (Some(detail), true) => compare_detail(detail, case, rendering),
        (_, fails) => Err(Disagreement::Detail { fails }),
    }
}

/// Compares the detail of the call record of a failing case with the case.
fn compare_detail(detail: &Json, case: &Case, rendering: Rendering) -> Result<(), Disagreement> {
    let mut names: Vec<String> = detail
        .as_object()
        .into_iter()
        .flatten()
        .map(|(name, _)| name.clone())
        .collect();
    let mut declared = definition::declared(&case.assertion)?;
    names.sort();
    declared.sort();
    if names != declared {
        return Err(Disagreement::DetailFields(names));
    }
    for (name, want) in &case.detail {
        let stated = &detail[name];
        let Ok(got) = literal::decode(stated) else {
            return Err(Disagreement::DetailLiteral {
                name: name.clone(),
                stated: stated.clone(),
            });
        };
        if let Some(difference) = literal::differs(&got, want, rendering) {
            return Err(Disagreement::DetailField {
                name: name.clone(),
                difference,
            });
        }
    }
    Ok(())
}
