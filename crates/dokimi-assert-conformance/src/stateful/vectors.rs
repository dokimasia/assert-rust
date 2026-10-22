// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The runner of the machines vectors.
//!
//! A machines vector names a machine subject. It may state the options of
//! the subject's steps as its setup, the settings of the run, and a trace.
//! The runner runs the subject with `prop::for_all` on a recorder, after the
//! trace, and compares the detail of the property's call record with the
//! vector's detail. A vector whose trace the run refuses states the entry,
//! the name of its step and the reason as its error. The runner compares
//! them with the fault that ends the call.

use super::subjects::{self, Setup};
use crate::prop::choices::same_json;
use crate::vector::{Disagreement, Vector};
use dokimi_assert::Recorder;
use dokimi_assert::prop::{self, Options};
use dokimi_assert::stateful::{self, Strategy};
use serde_json::Value as Json;

/// The contract of the property that runs a machine subject.
const CONTRACT: &str = "the machine subject of the vector";

/// The clients of a vector's setup that states none.
const SETUP_CLIENTS: usize = 2;

/// Runs `vector`, and compares what the library reports with the vector.
///
/// # Errors
///
/// Returns how the run disagrees with the vector, and the member of a vector
/// that names no subject or states a setup or a setting that the runner
/// cannot take.
pub fn run(vector: &Vector) -> Result<(), Disagreement> {
    match vector.kind.as_str() {
        "machines" => machines(&vector.raw),
        kind => Err(Disagreement::new(
            "",
            format!("the runner runs no vector of the kind {kind:?}"),
        )),
    }
}

/// Returns the count of 1 or more that a member states.
fn count(stated: &Json, member: &str) -> Result<usize, Disagreement> {
    stated
        .as_u64()
        .filter(|n| *n >= 1)
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| Disagreement::new(member, format!("{stated} is no count of 1 or more")))
}

/// Returns the strategy that a setup states: `"uniform"`, or the PCT of the
/// depth that `{"pct": depth}` states.
fn strategy_of(stated: &Json) -> Result<Strategy, Disagreement> {
    if stated == "uniform" {
        return Ok(stateful::uniform());
    }
    stated
        .as_object()
        .filter(|object| object.len() == 1)
        .and_then(|object| object["pct"].as_u64())
        .filter(|depth| *depth >= 1)
        .map(stateful::pct)
        .ok_or_else(|| Disagreement::new("setup.strategy", format!("{stated} is no strategy")))
}

/// Returns the options that a vector's setup states, with the clients of
/// the definition's setup for a setup that states none.
fn setup_of(stated: &Json) -> Result<Setup, Disagreement> {
    let mut steps = stateful::Options::default();
    if let Some(mean) = stated.get("mean") {
        steps = steps.mean(count(mean, "setup.mean")?);
    }
    if let Some(max) = stated.get("max") {
        steps = steps.max(count(max, "setup.max")?);
    }
    if let Some(swarm) = stated.get("swarm") {
        let on = swarm
            .as_bool()
            .ok_or_else(|| Disagreement::new("setup.swarm", format!("{swarm} is no bool")))?;
        steps = steps.swarm(on);
    }
    let clients = stated
        .get("clients")
        .map_or(Ok(SETUP_CLIENTS), |clients| count(clients, "setup.clients"))?;
    let concurrent = stated
        .get("concurrent")
        .map(|most| count(most, "setup.concurrent"))
        .transpose()?;
    let strategy = match stated.get("strategy") {
        Some(strategy) => strategy_of(strategy)?,
        None => stateful::uniform(),
    };
    Ok(Setup {
        steps,
        clients,
        concurrent,
        strategy,
    })
}

/// Returns the options of the run that a vector's settings state: its seed
/// and its cases. The run reads no variable of the environment and keeps no
/// store.
fn settings_of(stated: &Json) -> Result<Options, Disagreement> {
    let mut options: Options = prop::hermetic().store("");
    for (name, value) in stated.as_object().into_iter().flatten() {
        options = match name.as_str() {
            "seed" => {
                let seed = value
                    .as_str()
                    .and_then(|seed| seed.parse().ok())
                    .ok_or_else(|| {
                        Disagreement::new("settings.seed", format!("{value} is no decimal seed"))
                    })?;
                options.seed(seed)
            }
            "cases" => options.cases(count(value, "settings.cases")?),
            _ => {
                return Err(Disagreement::new(
                    format!("settings.{name}"),
                    "the runner takes no such setting",
                ));
            }
        };
    }
    Ok(options)
}

/// Returns the detail of a property's call record in the form of the
/// vector's detail, which states the failure by its assertion. No machines
/// vector states another failure, so the runner compares the others of a run
/// as the call record states them.
fn detail_of(record: &Json) -> Json {
    let mut detail = record["detail"].clone();
    detail["failure"] = detail["failure"]["assertion"].clone();
    detail
}

/// Compares the fault that ended a run with the refusal of the trace's entry
/// that the vector states.
fn compare_refusal(record: &Json, error: &Json) -> Result<(), Disagreement> {
    let message = record["error"].as_str().unwrap_or_default();
    let at = format!(
        "prop::for_all: draws[{}].{}: ",
        error["entry"],
        error["reason"].as_str().unwrap_or_default()
    );
    if record["verdict"] == "error"
        && message.starts_with(&at)
        && message.contains(&error["name"].to_string())
    {
        return Ok(());
    }
    Err(Disagreement::new(
        "error",
        format!(
            "the run ends as {} with {message:?}, want a refusal of entry {} of {}",
            record["verdict"], error["entry"], error["name"]
        ),
    ))
}

/// Runs the subject of a machines vector under its setup and its settings,
/// after its trace, and compares the run with the detail or the error that
/// the vector states.
fn machines(raw: &Json) -> Result<(), Disagreement> {
    let subject = raw["subject"]
        .as_str()
        .and_then(subjects::subject)
        .ok_or_else(|| {
            Disagreement::new(
                "subject",
                format!("{} names no machine subject", raw["subject"]),
            )
        })?;
    let setup = setup_of(&raw["setup"])?;
    let mut options = settings_of(&raw["settings"])?;
    if let Some(trace) = raw.get("trace") {
        options = options.draws(&trace.to_string());
    }
    let seat = Recorder::new();
    prop::for_all(&seat, CONTRACT, options, |c| subject(c, &setup));
    let record: Json = seat
        .records()
        .first()
        .and_then(|line| serde_json::from_str(line).ok())
        .ok_or_else(|| Disagreement::new("", "the recorder kept no call record"))?;
    if raw["error"].is_object() {
        return compare_refusal(&record, &raw["error"]);
    }
    if !record["detail"].is_object() {
        return Err(Disagreement::new(
            "",
            format!("the run ends with {}", record["error"]),
        ));
    }
    let got = detail_of(&record);
    if !same_json(&got, &raw["detail"]) {
        return Err(Disagreement::new(
            "detail",
            format!("the run's detail is {got}, want {}", raw["detail"]),
        ));
    }
    Ok(())
}
