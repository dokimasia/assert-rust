//! The runner of the forms vectors.
//!
//! A forms vector has a form, the kinds of the subjects that the form's
//! assertion takes, the values that the assertion takes beside them, the
//! shape or the generator of the input, the examples, the seed and the
//! detail of the run. The runner calls the form of the library on a
//! recorder, with an input of type [`Value`] whose generator `prop::using`
//! states. A run keeps its subjects across its cases, as a caller's closure
//! keeps them.

use super::choices::{same_json, seed};
use super::generator::{build, function};
use super::predicate::order;
use super::vectors::Disagreement;
use crate::literal::{self, Rendering};
use crate::subject::{Own, reads_handle};
use dokimi_assert::prop::{self, Generator, Options};
use dokimi_assert::{Cancel, Recorder, Value};
use serde_json::Value as Json;
use std::cmp::Ordering as Order;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

/// The contract of the run of the form of a forms vector.
pub const CONTRACT: &str = "the form of a forms vector";

/// One run of the form of a vector: the kinds of its subjects, the values
/// that the assertion takes beside them, and the options of the run.
struct Run<'v> {
    subjects: Vec<&'v str>,
    args: Vec<Value>,
    options: Options<Value>,
}

impl Run<'_> {
    /// Returns the subject kind at `at`.
    fn kind(&self, at: usize) -> &str {
        self.subjects.get(at).copied().unwrap_or_default()
    }

    /// Returns the refusal of the subject kind at `at`, which the form does
    /// not take.
    fn refused(&self, at: usize) -> Disagreement {
        Disagreement::Input(format!(
            "subjects[{at}]: {:?} is no subject that the form takes",
            self.kind(at)
        ))
    }

    /// Returns the function of the input of the subject kind at `at`.
    fn function(&self, at: usize) -> Result<impl Fn(&Value) -> Value + Sync + use<>, Disagreement> {
        let f = function(self.kind(at)).ok_or_else(|| self.refused(at))?;
        Ok(move |input: &Value| f(input.clone()))
    }

    /// Returns the value that the assertion takes at `at` beside the
    /// subjects.
    fn arg(&self, at: usize) -> Result<&Value, Disagreement> {
        self.args
            .get(at)
            .ok_or_else(|| Disagreement::Input(format!("args[{at}]: the vector states no value")))
    }

    /// Returns the text that the assertion takes at `at`.
    fn text(&self, at: usize) -> Result<&str, Disagreement> {
        match self.arg(at)? {
            Value::String(text) => Ok(text),
            other => Err(Disagreement::Input(format!(
                "args[{at}]: {other} is no text"
            ))),
        }
    }

    /// Returns the number that the assertion takes at `at`.
    fn number(&self, at: usize) -> Result<f64, Disagreement> {
        match self.arg(at)? {
            Value::Float(held) => Ok(*held),
            other => Err(Disagreement::Input(format!(
                "args[{at}]: {other} is no float"
            ))),
        }
    }
}

/// Returns the items of a list, and nothing for any other value.
fn items(value: Value) -> Vec<Value> {
    match value {
        Value::List(items) => items,
        _ => Vec::new(),
    }
}

/// Returns the text of a value, and the empty text for any other value.
fn text_of(value: Value) -> String {
    match value {
        Value::String(text) => text,
        _ => String::new(),
    }
}

/// Returns `true` for a negative integer.
fn negative(value: &Value) -> bool {
    matches!(value, Value::Int(held) if *held < 0)
}

/// Returns the integer of a value, and 0 for any other value.
fn int(value: &Value) -> i128 {
    match value {
        Value::Int(held) => *held,
        _ => 0,
    }
}

/// A subject of the error forms.
type Failing = fn(&Value) -> Result<(), Own>;

/// A subject of the forms that hand a handle to their call.
type Handled = fn(Option<&Cancel>, &Value) -> Result<(), dokimi_assert::Stop>;

/// Returns the subject of the error forms: the error that it returns for an
/// input, or success.
fn failing(kind: &str) -> Option<Failing> {
    Some(match kind {
        "returns-ok" => |_| Ok(()),
        "fails-otherwise" => |_| Err(Own),
        "fails-on-negative" => |input| if negative(input) { Err(Own) } else { Ok(()) },
        _ => return None,
    })
}

/// Returns the subject of `panics` and `does_not_panic`.
fn raising(kind: &str) -> Option<fn(&Value)> {
    Some(match kind {
        "returns-ok" => |_| {},
        "raises" => |_| panic!("the subject raises"),
        "raises-on-negative" => |input| assert!(!negative(input), "the subject raises"),
        _ => return None,
    })
}

/// Returns the subject of the forms that hand a handle to their call.
fn handled(kind: &str) -> Option<Handled> {
    Some(match kind {
        "returns-ok" | "ignores-handle" => |_, _| Ok(()),
        "reads-handle" => |cancel, _| reads_handle(cancel),
        "dereferences-handle" => |cancel, _| {
            let _ = cancel
                .expect("the subject dereferences the handle")
                .reason();
            Ok(())
        },
        _ => return None,
    })
}

/// Returns the combination of two inputs of a subject kind.
fn combined(kind: &str) -> Option<fn(&Value, &Value) -> Value> {
    Some(match kind {
        "adds" => |a, b| Value::Int(int(a) + int(b)),
        "subtracts" => |a, b| Value::Int(int(a) - int(b)),
        _ => return None,
    })
}

/// Runs the form of a forms vector on a recorder, and compares the detail of
/// its call record with the vector's.
///
/// # Errors
///
/// Returns how the run disagrees with the vector.
pub fn run(raw: &Json) -> Result<(), Disagreement> {
    let form = raw["form"].as_str().unwrap_or_default();
    let mut options = prop::using(&input_of(raw)?)
        .seed(
            seed(&raw["seed"])
                .ok_or_else(|| Disagreement::Input("the seed is no decimal text".to_owned()))?,
        )
        .hermetic()
        .shrink_time(Duration::ZERO)
        .store("");
    if let Some(examples) = raw.get("examples") {
        let values = examples
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .map(literal::decode)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|malformed| Disagreement::Input(malformed.to_string()))?;
        options = options.examples(values);
    }
    let args = raw["args"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(literal::decode)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|malformed| Disagreement::Input(malformed.to_string()))?;
    let run = Run {
        subjects: raw["subjects"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .map(|kind| kind.as_str().unwrap_or_default())
            .collect(),
        args,
        options,
    };
    let seat = Recorder::new();
    let ran = values(&seat, form, &run)?
        || texts(&seat, form, &run)?
        || errors(&seat, form, &run)?
        || behaviours(&seat, form, &run)?
        || relations(&seat, form, &run)?;
    if !ran {
        return Err(Disagreement::Input(format!(
            "{form:?} is no form that a vector runs"
        )));
    }
    let rendering = if form == "prop-permutation" {
        Rendering::Slices
    } else {
        Rendering::Values
    };
    compare(&detail_of(&seat)?, &raw["detail"], rendering)
}

/// Returns the generator of the input of a vector of a shape or of a
/// generator.
fn input_of(raw: &Json) -> Result<Generator<Value>, Disagreement> {
    match (raw.get("shape"), raw.get("generator")) {
        (Some(shape), None) => prop::of_shape(&shape.to_string())
            .map_err(|refused| Disagreement::Input(refused.to_string())),
        (None, Some(generator)) => Ok(build(generator, None)?),
        _ => Err(Disagreement::Input(
            "the vector states neither a shape nor a generator, or both".to_owned(),
        )),
    }
}

/// Runs a form of the values and the containers. Returns `false` for
/// another form.
fn values(seat: &Recorder, form: &str, run: &Run<'_>) -> Result<bool, Disagreement> {
    let options = run.options.clone();
    match form {
        "prop-equal" => prop::equal(seat, run.function(0)?, run.function(1)?, CONTRACT, options),
        "prop-not-equal" => {
            prop::not_equal(seat, run.function(0)?, run.function(1)?, CONTRACT, options);
        }
        "prop-true" => {
            let f = run.function(0)?;
            prop::is_true(
                seat,
                |input| f(input) == Value::Bool(true),
                CONTRACT,
                options,
            );
        }
        "prop-false" => {
            let f = run.function(0)?;
            prop::is_false(
                seat,
                |input| f(input) == Value::Bool(true),
                CONTRACT,
                options,
            );
        }
        "prop-nil" | "prop-not-nil" => {
            let f = run.function(0)?;
            let got = |input: &Value| Some(f(input)).filter(|value| *value != Value::Null);
            if form == "prop-nil" {
                prop::is_none(seat, got, CONTRACT, options);
            } else {
                prop::is_some(seat, got, CONTRACT, options);
            }
        }
        "prop-length" => {
            let want = match run.arg(0)? {
                Value::Int(want) => usize::try_from(*want)
                    .map_err(|_| Disagreement::Input("args[0] is no count".to_owned()))?,
                other => return Err(Disagreement::Input(format!("args[0]: {other} is no count"))),
            };
            prop::length(seat, run.function(0)?, want, CONTRACT, options);
        }
        "prop-empty" => prop::is_empty(seat, run.function(0)?, CONTRACT, options),
        "prop-not-empty" => prop::is_not_empty(seat, run.function(0)?, CONTRACT, options),
        "prop-contains" => prop::contains(seat, run.function(0)?, run.arg(0)?, CONTRACT, options),
        "prop-not-contains" => {
            prop::not_contains(seat, run.function(0)?, run.arg(0)?, CONTRACT, options);
        }
        "prop-permutation" => {
            let (f, g) = (run.function(0)?, run.function(1)?);
            prop::is_permutation(
                seat,
                |input| items(f(input)),
                |input| items(g(input)),
                CONTRACT,
                options,
            );
        }
        "prop-close-to" => {
            prop::close_to(
                seat,
                run.function(0)?,
                run.number(0)?,
                run.number(1)?,
                CONTRACT,
                options,
            );
        }
        "prop-in-range" => {
            prop::in_range(
                seat,
                run.function(0)?,
                run.number(0)?,
                run.number(1)?,
                CONTRACT,
                options,
            );
        }
        "prop-pairwise" => {
            if run.kind(1) != "ascending" {
                return Err(run.refused(1));
            }
            let f = run.function(0)?;
            let ascending =
                |earlier: &Value, later: &Value| order(earlier, later) != Order::Greater;
            prop::pairwise(seat, |input| items(f(input)), ascending, CONTRACT, options);
        }
        _ => return Ok(false),
    }
    Ok(true)
}

/// Runs a form of text. Returns `false` for another form.
fn texts(seat: &Recorder, form: &str, run: &Run<'_>) -> Result<bool, Disagreement> {
    let options = run.options.clone();
    let f = run.function(0);
    match form {
        "prop-contains-in-order" => {
            let needles: Vec<&str> = match run.arg(0)? {
                Value::List(items) => items
                    .iter()
                    .map(|item| match item {
                        Value::String(text) => Ok(text.as_str()),
                        other => Err(Disagreement::Input(format!("args[0]: {other} is no text"))),
                    })
                    .collect::<Result<_, _>>()?,
                other => {
                    return Err(Disagreement::Input(format!(
                        "args[0]: {other} is no list of text"
                    )));
                }
            };
            let f = f?;
            prop::contains_in_order(seat, |input| text_of(f(input)), &needles, CONTRACT, options);
        }
        "prop-has-prefix" => {
            let f = f?;
            prop::has_prefix(
                seat,
                |input| text_of(f(input)),
                run.text(0)?,
                CONTRACT,
                options,
            );
        }
        "prop-has-suffix" => {
            let f = f?;
            prop::has_suffix(
                seat,
                |input| text_of(f(input)),
                run.text(0)?,
                CONTRACT,
                options,
            );
        }
        "prop-matches" => {
            let f = f?;
            prop::matches(
                seat,
                |input| text_of(f(input)),
                run.text(0)?,
                CONTRACT,
                options,
            );
        }
        _ => return Ok(false),
    }
    Ok(true)
}

/// Runs a form of errors and panics. Returns `false` for another form.
fn errors(seat: &Recorder, form: &str, run: &Run<'_>) -> Result<bool, Disagreement> {
    let options = run.options.clone();
    if let Some(raise) = form
        .strip_prefix("prop-")
        .filter(|name| ["throws", "not-throws"].contains(name))
    {
        let body = raising(run.kind(0)).ok_or_else(|| run.refused(0))?;
        if raise == "throws" {
            prop::panics(seat, body, CONTRACT, options);
        } else {
            prop::does_not_panic(seat, body, CONTRACT, options);
        }
        return Ok(true);
    }
    if ![
        "prop-err-absent",
        "prop-err-present",
        "prop-err-is",
        "prop-err-is-not",
        "prop-err-as",
    ]
    .contains(&form)
    {
        return Ok(false);
    }
    let call = failing(run.kind(0)).ok_or_else(|| run.refused(0))?;
    match form {
        "prop-err-absent" => prop::no_error(seat, call, CONTRACT, options),
        "prop-err-present" => prop::has_error(seat, call, CONTRACT, options),
        "prop-err-is" => prop::error_is(seat, call, &Own, CONTRACT, options),
        "prop-err-is-not" => prop::error_is_not(seat, call, &Own, CONTRACT, options),
        _ => prop::error_as::<Own, _, _, _>(seat, call, CONTRACT, options),
    }
    Ok(true)
}

/// Runs a form of a call's behaviour: its handle and the state that it
/// leaves. Returns `false` for another form.
fn behaviours(seat: &Recorder, form: &str, run: &Run<'_>) -> Result<bool, Disagreement> {
    let options = run.options.clone();
    match form {
        "prop-nil-context-safe" | "prop-honours-cancellation" | "prop-honours-deadline" => {
            let call = handled(run.kind(0)).ok_or_else(|| run.refused(0))?;
            match form {
                "prop-nil-context-safe" => prop::none_handle_safe(seat, call, CONTRACT, options),
                "prop-honours-cancellation" => {
                    prop::honours_cancellation(seat, call, CONTRACT, options);
                }
                _ => prop::honours_deadline(seat, call, CONTRACT, options),
            }
        }
        "prop-pure" | "prop-not-pure" => {
            let counter = AtomicI64::new(0);
            let observe = || counter.load(Ordering::SeqCst);
            let adds = match run.kind(0) {
                "accumulates" => true,
                "leaves-state-alone" => false,
                _ => return Err(run.refused(0)),
            };
            let body = |_: &Value| {
                if adds {
                    counter.fetch_add(1, Ordering::SeqCst);
                }
            };
            if form == "prop-pure" {
                prop::is_pure(seat, observe, body, CONTRACT, options);
            } else {
                prop::is_not_pure(seat, observe, body, CONTRACT, options);
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}

/// Runs a form of a relation between calls. Returns `false` for another
/// form.
fn relations(seat: &Recorder, form: &str, run: &Run<'_>) -> Result<bool, Disagreement> {
    let options = run.options.clone();
    let counter = AtomicI64::new(0);
    let observe = || counter.load(Ordering::SeqCst);
    match (form, run.kind(0)) {
        ("prop-idempotent" | "prop-accumulates", kind) => {
            let call = |input: &Value| {
                match kind {
                    "sets-value" => counter.store(
                        i64::try_from(int(input)).unwrap_or_default(),
                        Ordering::SeqCst,
                    ),
                    "accumulates" => {
                        counter.fetch_add(1, Ordering::SeqCst);
                    }
                    _ => {}
                }
                Ok::<(), Own>(())
            };
            if !["sets-value", "accumulates", "leaves-state-alone"].contains(&kind) {
                return Err(run.refused(0));
            }
            if form == "prop-idempotent" {
                prop::is_idempotent(seat, call, observe, CONTRACT, options);
            } else {
                prop::accumulates(seat, call, observe, CONTRACT, options);
            }
        }
        ("prop-deterministic", "returns-ok") => {
            prop::is_deterministic(
                seat,
                |input: &Value| Ok::<Value, Own>(input.clone()),
                CONTRACT,
                options,
            );
        }
        ("prop-deterministic", "counts-calls") => {
            let call = |_: &Value| Ok::<i64, Own>(counter.fetch_add(1, Ordering::SeqCst) + 1);
            prop::is_deterministic(seat, call, CONTRACT, options);
        }
        ("prop-commutative", kind) => {
            let combine = combined(kind).ok_or_else(|| run.refused(0))?;
            prop::is_commutative(seat, combine, CONTRACT, options);
        }
        ("prop-associative", kind) => {
            let combine = combined(kind).ok_or_else(|| run.refused(0))?;
            prop::is_associative(seat, combine, CONTRACT, options);
        }
        ("prop-round-trip", kind @ ("renders-decimal" | "drops-the-sign")) => {
            let drops = kind == "drops-the-sign";
            let forward = |input: &Value| {
                let held = int(input);
                Ok::<String, Own>(if drops { held.abs() } else { held }.to_string())
            };
            let inverse = |text: &String| text.parse::<i128>().map(Value::Int);
            prop::round_trip(seat, forward, inverse, CONTRACT, options);
        }
        ("prop-deterministic" | "prop-round-trip", _) => return Err(run.refused(0)),
        _ => return Ok(false),
    }
    Ok(true)
}

/// Returns the detail of the call record of the form, the first record of
/// the recorder.
fn detail_of(seat: &Recorder) -> Result<Json, Disagreement> {
    let records = seat.records();
    let record: Json = records
        .first()
        .map(|line| serde_json::from_str(line))
        .transpose()
        .map_err(|error| Disagreement::Fault(format!("a call record is no JSON: {error}")))?
        .ok_or_else(|| Disagreement::Fault("the recorder kept no call record".to_owned()))?;
    if record["verdict"] == "error" {
        return Err(Disagreement::Fault(
            record["error"].as_str().unwrap_or_default().to_owned(),
        ));
    }
    Ok(record["detail"].clone())
}

/// Compares each field of the detail that the vector states with the
/// record's. The failure compares its assertion and the fields of its detail
/// that the vector states, each as the definition compares literals, with
/// the `Debug` text of an opaque value under `rendering`.
fn compare(ours: &Json, theirs: &Json, rendering: Rendering) -> Result<(), Disagreement> {
    for (name, want) in theirs.as_object().into_iter().flatten() {
        let got = &ours[name];
        let differs = || Disagreement::Output {
            name: format!("detail.{name}"),
            want: want.clone(),
            got: got.clone(),
        };
        if name != "failure" {
            if !same_json(got, want) {
                return Err(differs());
            }
            continue;
        }
        if want.is_null() || got.is_null() {
            if want.is_null() != got.is_null() {
                return Err(differs());
            }
            continue;
        }
        if got["assertion"] != want["assertion"] {
            return Err(differs());
        }
        for (field, stated) in want["detail"].as_object().into_iter().flatten() {
            let decoded = literal::decode(&got["detail"][field]).ok();
            let expected = literal::decode(stated)
                .map_err(|malformed| Disagreement::Input(malformed.to_string()))?;
            let agrees = decoded
                .is_some_and(|decoded| literal::differs(&decoded, &expected, rendering).is_none());
            if !agrees {
                return Err(differs());
            }
        }
    }
    Ok(())
}
