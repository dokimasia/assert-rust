// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The runners of the property vectors.
//!
//! Each kind of property vector has a function that takes the inputs of a
//! vector and returns the outputs that the library computes for them, as
//! JSON. [`run`] compares each output with the vector's, and returns the
//! first output that differs.
//!
//! The kinds of the engine run on the engine through
//! `dokimi_assert::prop::engine`: decoding, generation, bridge, inverse,
//! draws, shrinking, coverage, token and store. A behaviour vector and a
//! recording vector run their body with `prop::for_all` on a recorder, so
//! they check the record and the call records of the library.

use super::body::{self, DRAWN, failing};
use super::choices::{choices_literal, literal, parse_choices, same_json, seed};
use super::generator::{SpecError, build};
use super::{fixtures, forms};
use crate::definition::{self, Error};
use crate::literal as typed;
use dokimi_assert::prop::engine::choice::Choice;
use dokimi_assert::prop::engine::coverage;
use dokimi_assert::prop::engine::execution::Fault;
use dokimi_assert::prop::engine::harness;
use dokimi_assert::prop::engine::runner::Settings;
use dokimi_assert::prop::engine::shrink::DEFAULT_BUDGET;
use dokimi_assert::prop::engine::store::{self, Verdict};
use dokimi_assert::prop::engine::token;
use dokimi_assert::prop::engine::trace::Entry;
use dokimi_assert::prop::{self, Case, Generator, Options};
use dokimi_assert::{Recorder, Value, check};
use serde_json::{Value as Json, json};
use std::fmt;
use std::path::Path;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

/// The contract of the property of a behaviour vector and of a recording
/// vector. The stored cases of a vector are entries of this property.
pub const CONTRACT: &str = "the property of the vector";

/// The contract of the call of `is_true` that ends the body of a recording
/// vector.
const END: &str = "the body of the recording vector passes";

/// The settings that a behaviour vector and a recording vector can state.
const SETTINGS: [&str; 9] = [
    "seed",
    "cases",
    "max-choices",
    "requirements",
    "examples",
    "stored",
    "shrink",
    "replay",
    "workers",
];

/// The identity of the entry of each stored case.
const STORED: [(&str, &str); 2] = [
    ("assertion", "stored"),
    ("contract", "a stored case of the vector"),
];

/// One property vector.
#[derive(Debug, Clone, PartialEq)]
pub struct Vector {
    /// The id of the vector.
    pub id: String,
    /// The kind of the vector, which its file names.
    pub kind: String,
    /// The vector as its file states it.
    pub raw: Json,
}

/// Returns every property vector without a skip for Rust, in the order of
/// its files.
///
/// # Errors
///
/// Returns the error of a vector file that cannot be read, or that states
/// no kind or a vector without an id.
pub fn vectors() -> Result<Vec<Vector>, Error> {
    let mut found = Vec::new();
    for file in definition::vector_files("prop")? {
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
            if raw["skip"].get("rust").is_some() {
                continue;
            }
            found.push(Vector {
                id: id.to_owned(),
                kind: kind.to_owned(),
                raw: raw.clone(),
            });
        }
    }
    Ok(found)
}

/// How the run of a property vector disagrees with the vector.
#[derive(Debug)]
#[non_exhaustive]
pub enum Disagreement {
    /// The runner computes no output of the vector's kind.
    Kind(String),
    /// The inputs of the vector do not read.
    Input(String),
    /// The library ends the run with a fault, or writes an output that it
    /// cannot read back.
    Fault(String),
    /// An output differs from the vector's.
    Output {
        /// The name of the output.
        name: String,
        /// The value that the vector states.
        want: Json,
        /// The value that the library computes.
        got: Json,
    },
}

impl fmt::Display for Disagreement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Kind(kind) => write!(f, "the runner computes no output of the kind {kind:?}"),
            Self::Input(what) => write!(f, "the inputs of the vector do not read: {what}"),
            Self::Fault(text) => write!(f, "the run ends with the fault {text:?}"),
            Self::Output { name, want, got } => {
                write!(
                    f,
                    "the output {name} is {got}, and the vector states {want}"
                )
            }
        }
    }
}

impl std::error::Error for Disagreement {}

impl From<SpecError> for Disagreement {
    fn from(error: SpecError) -> Self {
        Self::Input(error.to_string())
    }
}

/// Returns the disagreement of an input that does not read.
fn input(what: impl fmt::Display) -> Disagreement {
    Disagreement::Input(what.to_string())
}

/// Runs `vector` and compares each output that the library computes with the
/// output of the vector. `dir` is an empty directory of the vector's own, in
/// which a behaviour vector and a recording vector keep their store.
///
/// # Errors
///
/// Returns how the run disagrees with the vector.
pub fn run(vector: &Vector, dir: &Path) -> Result<(), Disagreement> {
    let raw = &vector.raw;
    let outputs = match vector.kind.as_str() {
        "decoding" => decoding(raw)?,
        "generation" => generation(raw)?,
        "shapes" => shapes(raw)?,
        "bridge" => bridge(raw)?,
        "inverse" => inverse(raw)?,
        "draws" => draws(raw)?,
        "shrinking" => shrinking(raw)?,
        "coverage" => coverage_of(raw)?,
        "token" => token_of(raw)?,
        "store" => store_of(raw)?,
        "behaviour" => behaviour(raw, dir)?,
        "recording" => recording(raw, dir)?,
        "fixtures" => fixtures_of(raw)?,
        "forms" => {
            forms::run(raw)?;
            json!({})
        }
        kind => return Err(Disagreement::Kind(kind.to_owned())),
    };
    for (name, got) in outputs.as_object().into_iter().flatten() {
        if !same_json(got, &raw[name]) {
            return Err(Disagreement::Output {
                name: name.clone(),
                want: raw[name].clone(),
                got: got.clone(),
            });
        }
    }
    Ok(())
}

/// Returns the seed of a vector or of its settings.
fn seed_of(written: &Json) -> Result<u64, Disagreement> {
    seed(&written["seed"]).ok_or_else(|| input("the seed is no decimal text"))
}

/// Returns a count that a vector states.
fn count(written: &Json, name: &str) -> Result<usize, Disagreement> {
    written
        .as_u64()
        .and_then(|count| usize::try_from(count).ok())
        .ok_or_else(|| input(format!("{name} is no count")))
}

/// Returns the choices that a vector states.
fn choices_of(written: &Json, name: &str) -> Result<Vec<Choice>, Disagreement> {
    parse_choices(written).ok_or_else(|| input(format!("{name} states no choices")))
}

/// Returns the recorded choices and the value of a decode, or that the decode
/// rejected the case, as the members `recorded`, `value` and `rejected`.
fn decoded((recorded, value): (Vec<Choice>, Option<Value>)) -> Json {
    json!({
        "recorded": choices_literal(&recorded),
        "value": value.as_ref().map_or(Json::Null, literal),
        "rejected": value.is_none(),
    })
}

/// Returns a decode in the form of a generated case, whose recorded choices
/// are its member `choices`.
fn case_of(decoded: &Json) -> Json {
    json!({
        "choices": decoded["recorded"],
        "value": decoded["value"],
        "rejected": decoded["rejected"],
    })
}

/// Decodes a generator from stated choices.
fn decoding(raw: &Json) -> Result<Json, Disagreement> {
    let generator = build(&raw["generator"], None)?;
    let choices = choices_of(&raw["choices"], "choices")?;
    Ok(decoded(harness::decode(&generator, choices)))
}

/// Decodes a generator from the first cases of a seed.
fn generation(raw: &Json) -> Result<Json, Disagreement> {
    generated(&build(&raw["generator"], None)?, raw)
}

/// Returns the generator of a vector's shape.
fn shape_of(raw: &Json) -> Result<Generator<Value>, Disagreement> {
    prop::of_shape(&raw["shape"].to_string()).map_err(input)
}

/// Decodes a shape from the first cases of a seed.
fn shapes(raw: &Json) -> Result<Json, Disagreement> {
    generated(&shape_of(raw)?, raw)
}

/// Returns the choices and the value of each of the first cases of a seed.
fn generated(generator: &Generator<Value>, raw: &Json) -> Result<Json, Disagreement> {
    let seed = seed_of(raw)?;
    let count = u64::try_from(count(&raw["count"], "count")?).unwrap_or(u64::MAX);
    let cases: Vec<Json> = (0..count)
        .map(|index| case_of(&decoded(harness::generate(generator, seed, index))))
        .collect();
    Ok(json!({"cases": cases}))
}

/// Decodes a generator from a fuzzer's bytes.
fn bridge(raw: &Json) -> Result<Json, Disagreement> {
    let generator = build(&raw["generator"], None)?;
    let stated = json!({"type": "bytes", "value": raw["bytes"]});
    let Ok(Value::Bytes(data)) = typed::decode(&stated) else {
        return Err(input("the bytes are no hexadecimal text"));
    };
    Ok(case_of(&decoded(harness::bridge(&generator, &data))))
}

/// Runs a shape or a generator backwards from a value.
fn inverse(raw: &Json) -> Result<Json, Disagreement> {
    let generator = match raw.get("shape") {
        Some(_) => shape_of(raw)?,
        None => build(&raw["generator"], None)?,
    };
    let value = typed::decode(&raw["value"]).map_err(input)?;
    Ok(match harness::invert(&generator, &value) {
        Ok(choices) => json!({"choices": choices_literal(&choices), "error": false}),
        Err(_) => json!({"choices": null, "error": true}),
    })
}

/// Runs a body of the vector's draws on a trace of its entries, and returns
/// the choices of the entries and the values of the body's first case, or
/// the entry that the body cannot follow.
fn draws(raw: &Json) -> Result<Json, Disagreement> {
    let draws: Vec<(String, Generator<Value>)> = raw["draws"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|draw| {
            let label = draw["label"].as_str().unwrap_or_default().to_owned();
            Ok((label, build(&draw["generator"], None)?))
        })
        .collect::<Result<_, SpecError>>()?;
    let entries: Vec<(String, Value)> = raw["entries"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|entry| {
            let label = entry["label"].as_str().unwrap_or_default().to_owned();
            Ok((label, typed::decode(&entry["value"]).map_err(input)?))
        })
        .collect::<Result<_, Disagreement>>()?;
    let first: Mutex<Option<Vec<Value>>> = Mutex::new(None);
    let body = |c: &Case<'static>| {
        let values: Vec<Value> = draws
            .iter()
            .map(|(label, generator)| c.draw(generator, label))
            .collect();
        first
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_or_insert(values);
    };
    let mut settings = Settings::new(0);
    settings.cases = 1;
    settings.traces = vec![
        entries
            .iter()
            .map(|(label, value)| Entry::Draw {
                label: label.clone(),
                value: value.clone(),
            })
            .collect(),
    ];
    match harness::run(&body, settings, &mut |_, _| {}) {
        Ok(_) => {}
        Err(Fault::Trace(refused)) => {
            return Ok(json!({
                "choices": null,
                "values": null,
                "error": {"label": refused.name, "reason": refused.refusal.name()},
            }));
        }
        Err(Fault::Run(text)) => return Err(Disagreement::Fault(text)),
        Err(Fault::Case(fault)) => return Err(Disagreement::Fault(fault.0.to_string())),
    }
    let mut choices = Vec::new();
    for ((_, generator), (_, value)) in draws.iter().zip(&entries) {
        choices.extend(harness::invert(generator, value).unwrap_or_default());
    }
    let values = first
        .into_inner()
        .unwrap_or_else(PoisonError::into_inner)
        .unwrap_or_default();
    let values: Vec<Json> = draws
        .iter()
        .zip(&values)
        .map(|((label, _), value)| json!({"label": label, "value": literal(value)}))
        .collect();
    Ok(json!({"choices": choices_literal(&choices), "values": values, "error": null}))
}

/// Runs a one-draw body that fails when a predicate is true, and shrinks
/// its failure.
fn shrinking(raw: &Json) -> Result<Json, Disagreement> {
    let body = body::failing_when(&raw["generator"], &raw["fails-when"])?;
    let mut settings = Settings::new(seed_of(raw)?);
    settings.shrink = match raw.get("budget") {
        Some(budget) => count(budget, "budget")?,
        None => DEFAULT_BUDGET,
    };
    let outcome = harness::run(&|c: &Case<'static>| body(c), settings, &mut |_, _| {})
        .map_err(|fault| Disagreement::Fault(format!("{fault:?}")))?;
    let failing = outcome.failing.as_ref();
    let value = failing
        .and_then(|failing| failing.draws().into_iter().next())
        .and_then(|(_, value)| value)
        .map_or(Json::Null, |value| literal(&value));
    Ok(json!({
        "outcome": outcome.kind.name(),
        "cases": outcome.cases,
        "value": value,
        "choices": failing.map_or(Json::Null, |failing| choices_literal(failing.choices())),
        "token": outcome.token,
        "runs": outcome.runs,
    }))
}

/// Decides one requirement at one check.
fn coverage_of(raw: &Json) -> Result<Json, Disagreement> {
    let verdict = coverage::verdict(
        count(&raw["counted"], "counted")?,
        count(&raw["valid"], "valid")?,
        raw["share"]
            .as_f64()
            .ok_or_else(|| input("share is no number"))?,
        raw["last"] == true,
        raw["exact"] == true,
    );
    Ok(json!({"verdict": verdict.name()}))
}

/// Encodes choices as a token, or decodes a token.
fn token_of(raw: &Json) -> Result<Json, Disagreement> {
    if let Some(stated) = raw.get("choices") {
        let choices = choices_of(stated, "choices")?;
        return Ok(json!({"token": token::encode(&choices)}));
    }
    Ok(
        match token::decode(raw["token"].as_str().unwrap_or_default()) {
            Ok(choices) => json!({"decoded": choices_literal(&choices), "error": false}),
            Err(_) => json!({"decoded": null, "error": true}),
        },
    )
}

/// Reads the text of one file of a store, or writes an entry and names it.
fn store_of(raw: &Json) -> Result<Json, Disagreement> {
    let contract = raw["contract"].as_str().unwrap_or_default();
    if let Some(text) = raw["text"].as_str() {
        let verdict = store::read(text, contract);
        let choices = match &verdict {
            Verdict::Replay(choices) => choices_literal(choices),
            _ => Json::Null,
        };
        return Ok(json!({"verdict": verdict.name(), "choices": choices}));
    }
    let choices = choices_of(&raw["choices"], "choices")?;
    let identity = raw["identity"]
        .as_object()
        .ok_or_else(|| input("the identity is no object"))?
        .iter()
        .map(|(key, value)| {
            let value = match value {
                Json::String(text) => Value::from(text.as_str()),
                Json::Number(line) => Value::Int(line.as_i64().map_or(0, i128::from)),
                other => return Err(input(format!("the identity states {other}"))),
            };
            Ok((key.clone(), value))
        })
        .collect::<Result<_, Disagreement>>()?;
    let counterexample = raw["counterexample"]
        .as_array()
        .ok_or_else(|| input("the counterexample is no list"))?
        .iter()
        .map(|draw| {
            let value = draw
                .get("value")
                .map(typed::decode)
                .transpose()
                .map_err(input)?;
            Ok((draw["label"].as_str().unwrap_or_default().to_owned(), value))
        })
        .collect::<Result<_, Disagreement>>()?;
    let failure = store::Failure {
        contract: contract.to_owned(),
        choices: choices.clone(),
        identity,
        counterexample,
    };
    let written = store::entry(
        &failure,
        raw["definition"].as_str().unwrap_or_default(),
        raw["found"].as_str().unwrap_or_default(),
    );
    if store::read(&written, contract) != Verdict::Replay(choices.clone()) {
        return Err(Disagreement::Fault(
            "the entry does not read back as a replay of its choices".to_owned(),
        ));
    }
    let entry: Json = serde_json::from_str(&written)
        .map_err(|error| Disagreement::Fault(format!("the entry is no JSON: {error}")))?;
    Ok(json!({"name": store::name(contract, &choices), "entry": entry}))
}

/// Runs the body of a behaviour vector with `for_all` on a recorder, and
/// returns the detail of the property's call record.
fn behaviour(raw: &Json, dir: &Path) -> Result<Json, Disagreement> {
    let spec = &raw["body"];
    let body = body::body(spec, failing)?;
    let options = options_of(&raw["settings"], dir, spec)?;
    let seat = Recorder::new();
    prop::for_all(&seat, CONTRACT, options, &*body);
    let records = records_of(&seat)?;
    Ok(json!({"detail": records[0]["detail"]}))
}

/// Ends the case of a recording vector with one call of `is_true`, which
/// fails when the case fails. A failing case first keeps its identity, so
/// the failure of the case has the identity of the vector.
fn asserting(c: &Case<'_>, failure: Option<&str>) {
    if let Some(identity) = failure {
        c.fail_as(identity, false);
    }
    check::is_true(c, failure.is_none(), END);
}

/// Runs the body of a recording vector with `for_all` on a recorder,
/// compares the calls of `is_true` with the calls of the vector, and returns
/// the property's verdict and the reason of a property that skips.
///
/// A call has the run of the body that made it, the phase of the run and its
/// verdict. The call at position `k` from 0 has the number `k + 2`, and its
/// parent is the property, whose number is 1.
fn recording(raw: &Json, dir: &Path) -> Result<Json, Disagreement> {
    let spec = &raw["body"];
    let body = body::body(spec, asserting)?;
    let options = options_of(&raw["settings"], dir, spec)?;
    let seat = Recorder::new();
    prop::for_all(&seat, CONTRACT, options, &*body);
    let records = records_of(&seat)?;
    let got: Vec<Json> = records[1..]
        .iter()
        .map(|call| {
            json!({
                "seq": call["seq"],
                "parent": call["parent"],
                "run": call["run"],
                "phase": call["phase"],
                "verdict": call["verdict"],
            })
        })
        .collect();
    let want: Vec<Json> = raw["calls"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .enumerate()
        .map(|(at, call)| {
            json!({
                "seq": at + 2,
                "parent": 1,
                "run": call["run"],
                "phase": call["phase"],
                "verdict": call["verdict"],
            })
        })
        .collect();
    let (got, want) = (Json::Array(got), Json::Array(want));
    if !same_json(&got, &want) {
        return Err(Disagreement::Output {
            name: "calls".to_owned(),
            want,
            got,
        });
    }
    Ok(json!({"verdict": records[0]["verdict"], "reason": records[0]["reason"]}))
}

/// Reads the shape file of the fixture type that a fixtures vector names,
/// and compares its shape with the vector's. The comparison leaves out the
/// source of the shape file, and names each definition by the order in which
/// the shape first refers to it.
fn fixtures_of(raw: &Json) -> Result<Json, Disagreement> {
    let name = raw["fixture"].as_str().unwrap_or_default();
    let text = fixtures::shape_file(name)
        .ok_or_else(|| input(format!("{name:?} is no fixture type")))?
        .map_err(|refused| Disagreement::Fault(refused.to_string()))?;
    let read: Json = serde_json::from_str(&text)
        .map_err(|error| Disagreement::Fault(format!("the shape file is no JSON: {error}")))?;
    let (got, want) = (
        fixtures::normal(read),
        fixtures::normal(raw["shape"].clone()),
    );
    if !same_json(&got, &want) {
        return Err(Disagreement::Output {
            name: "shape".to_owned(),
            want,
            got,
        });
    }
    Ok(json!({}))
}

/// Returns the call records that a recorder kept, parsed and in the order
/// of their numbers. The first is the property's.
///
/// # Errors
///
/// Returns the fault of a property whose call ended with one, and of a
/// recorder that kept no record or a record that is no JSON.
fn records_of(seat: &Recorder) -> Result<Vec<Json>, Disagreement> {
    let records = seat
        .records()
        .iter()
        .map(|line| serde_json::from_str::<Json>(line))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| Disagreement::Fault(format!("a call record is no JSON: {error}")))?;
    match records.first() {
        None => Err(Disagreement::Fault(
            "the recorder kept no call record".to_owned(),
        )),
        Some(property) if property["verdict"] == "error" => Err(Disagreement::Fault(
            property["error"].as_str().unwrap_or_default().to_owned(),
        )),
        Some(_) => Ok(records),
    }
}

/// Returns the options of the run of a behaviour vector or a recording
/// vector, and writes its stored cases into `dir`, the store of the run.
///
/// The run reads no variable of the environment, and its shrinking has no
/// time limit.
fn options_of(settings: &Json, dir: &Path, spec: &Json) -> Result<Options, Disagreement> {
    let written = settings
        .as_object()
        .ok_or_else(|| input("the settings are no object"))?;
    if let Some(name) = written
        .keys()
        .find(|name| !SETTINGS.contains(&name.as_str()))
    {
        return Err(input(format!(
            "settings.{name}: the runner does not take the setting"
        )));
    }
    let mut options: Options = prop::seed(seed_of(settings)?)
        .hermetic()
        .shrink_time(Duration::ZERO)
        .store(dir);
    if let Some(cases) = written.get("cases") {
        options = options.cases(count(cases, "settings.cases")?);
    }
    if let Some(max) = written.get("max-choices") {
        options = options.max_choices(count(max, "settings.max-choices")?);
    }
    if let Some(budget) = written.get("shrink") {
        options = options.shrink(count(budget, "settings.shrink")?);
    }
    if let Some(workers) = written.get("workers") {
        options = options.workers(count(workers, "settings.workers")?);
    }
    for requirement in settings["requirements"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        let (Some(label), Some(share)) =
            (requirement["label"].as_str(), requirement["share"].as_f64())
        else {
            return Err(input("a requirement states no label and share"));
        };
        options = options.require(label, share);
    }
    if let Some(replayed) = written.get("replay") {
        let replayed = replayed
            .as_str()
            .ok_or_else(|| input("settings.replay is no text"))?;
        options = options.replay(replayed);
    }
    if let Some(examples) = written.get("examples") {
        options = options.draws(&example_of(examples, spec)?);
    }
    write_stored(&settings["stored"], dir, spec)?;
    Ok(options)
}

/// Returns the value that the draw of a body spec decodes from `choices`.
fn drawn(choices: &Json, spec: &Json, at: &str) -> Result<Value, Disagreement> {
    let choices = choices_of(choices, at)?;
    if spec.get("draw").is_none() {
        return Err(input(format!("{at}: the body draws no value")));
    }
    let generator = build(&spec["draw"], None)?;
    harness::decode(&generator, choices)
        .1
        .ok_or_else(|| input(format!("{at}: the choices decode to no value")))
}

/// Returns the entries of the draws option that states the one example of
/// a vector's settings: the value that the example's choices decode to under
/// the body's draw.
fn example_of(examples: &Json, spec: &Json) -> Result<String, Disagreement> {
    let listed = examples
        .as_array()
        .ok_or_else(|| input("settings.examples is no list"))?;
    let [example] = listed.as_slice() else {
        return Err(input(format!(
            "settings.examples: the settings state {} examples, and a run follows one trace",
            listed.len()
        )));
    };
    let value = drawn(example, spec, "settings.examples[0]")?;
    Ok(json!([{"label": DRAWN, "value": literal(&value)}]).to_string())
}

/// Writes each stored case of a vector's settings into the store `dir`, as
/// an entry of the vector's property with the draw that its choices decode
/// to.
///
/// The first case was found on 2026-01-01, and each later case a day later,
/// so the run tries the cases in the order of the vector.
fn write_stored(stored: &Json, dir: &Path, spec: &Json) -> Result<(), Disagreement> {
    let listed = stored.as_array().map(Vec::as_slice).unwrap_or_default();
    if listed.is_empty() {
        return Ok(());
    }
    let version = definition::version().map_err(input)?;
    for (day, written) in listed.iter().enumerate() {
        let at = format!("settings.stored[{day}]");
        if day >= 31 {
            return Err(input(format!(
                "{at}: the runner dates 31 stored cases at most"
            )));
        }
        let choices = choices_of(written, &at)?;
        let value = drawn(written, spec, &at)?;
        let failure = store::Failure {
            contract: CONTRACT.to_owned(),
            choices: choices.clone(),
            identity: STORED
                .iter()
                .map(|(key, value)| ((*key).to_owned(), Value::from(*value)))
                .collect(),
            counterexample: vec![(DRAWN.to_owned(), Some(value))],
        };
        let entry = store::entry(&failure, &version, &format!("2026-01-{:02}", day + 1));
        std::fs::write(dir.join(store::name(CONTRACT, &choices)), entry)
            .map_err(|error| input(format!("{at}: the entry cannot be written: {error}")))?;
    }
    Ok(())
}
