//! The runners of the history vectors.
//!
//! A seam vector records a script through a `History`, or intervals through
//! `history::from_intervals`, and compares the events, or the entry that the
//! seam refuses, with the vector's. A linearizable vector checks a recorded
//! script with `history::is_linearizable` against a named spec on a
//! recorder, and a serializable or a snapshot-isolation vector checks one
//! with the check of its level. Each compares the verdict and the detail of
//! the call record with the vector's.

use super::specs::Named;
use crate::literal;
use crate::prop::choices::{literal as literal_of, same_json};
use crate::vector::{Disagreement, Vector};
use dokimi_assert::__private::quietly;
use dokimi_assert::Recorder;
use dokimi_assert::Value;
use dokimi_assert::history::{self, Call, Ending, Event, History, Interval, Kind, Options};
use serde_json::{Value as Json, json};
use std::collections::HashMap;

/// The contract of the check of a linearizable vector.
const LINEARIZABLE: &str = "the history of the vector is linearizable";

/// The contract of the check of an isolation vector.
const ISOLATION: &str = "the transactions of the vector are isolated";

/// Runs `vector`, and compares what the library reports with the vector.
///
/// # Errors
///
/// Returns how the run disagrees with the vector.
pub fn run(vector: &Vector) -> Result<(), Disagreement> {
    let raw = &vector.raw;
    match vector.kind.as_str() {
        "seam" => seam(raw),
        "linearizable" => linearizable(raw),
        "serializable" => isolation(raw, false),
        "snapshot-isolation" => isolation(raw, true),
        kind => Err(Disagreement::new(
            "",
            format!("the runner runs no vector of the kind {kind:?}"),
        )),
    }
}

/// Returns the values that a list of typed literals states.
fn values(raw: &Json, member: &str) -> Result<Vec<Value>, Disagreement> {
    raw.as_array()
        .ok_or_else(|| Disagreement::new(member, "the member is no list"))?
        .iter()
        .enumerate()
        .map(|(at, stated)| {
            literal::decode(stated).map_err(|_| {
                Disagreement::new(format!("{member}[{at}]"), "the value is no typed literal")
            })
        })
        .collect()
}

/// Returns the value of a typed literal.
fn value(raw: &Json, member: &str) -> Result<Value, Disagreement> {
    literal::decode(raw).map_err(|_| Disagreement::new(member, "the value is no typed literal"))
}

/// Returns the client of an entry.
fn client(raw: &Json, at: &str) -> Result<usize, Disagreement> {
    raw["client"]
        .as_u64()
        .and_then(|client| usize::try_from(client).ok())
        .ok_or_else(|| Disagreement::new(format!("{at}.client"), "the client is no count"))
}

/// Records `script` through `history`, and returns the entry whose call of
/// the seam the history refuses, or `None`. It records no entry after the
/// refused one. A call completes once, so the history refuses an entry that
/// completes a call a second time.
fn record(history: &History, script: &Json, member: &str) -> Result<Option<usize>, Disagreement> {
    let entries = script
        .as_array()
        .ok_or_else(|| Disagreement::new(member, "the script is no list"))?;
    let mut calls: HashMap<u64, Option<Call<'_>>> = HashMap::new();
    for (at, raw) in entries.iter().enumerate() {
        let place = format!("{member}[{at}]");
        if let Some(number) = raw["invoke"].as_u64() {
            let (client, operation) = (
                client(raw, &place)?,
                raw["operation"].as_str().unwrap_or_default(),
            );
            let (args, keys) = (
                values(&raw["args"], &format!("{place}.args"))?,
                values(&raw["keys"], &format!("{place}.keys"))?,
            );
            match quietly(|| history.invoke(client, operation, args, keys)) {
                Ok(call) => {
                    calls.insert(number, Some(call));
                }
                Err(_) => return Ok(Some(at)),
            }
            continue;
        }
        let (number, ending) = ["ok", "fail", "unknown"]
            .into_iter()
            .find_map(|ending| raw[ending].as_u64().map(|number| (number, ending)))
            .ok_or_else(|| Disagreement::new(&place, "the entry states no call of the seam"))?;
        let opened = calls.get_mut(&number).ok_or_else(|| {
            Disagreement::new(
                &place,
                format!("the entry completes call {number}, which the script does not open"),
            )
        })?;
        let Some(call) = opened.take() else {
            return Ok(Some(at));
        };
        let error = raw["error"].as_str().unwrap_or_default();
        match ending {
            "ok" => call.ok(value(&raw["output"], &format!("{place}.output"))?),
            "fail" => call.fail(error),
            _ => call.unknown(error),
        }
    }
    Ok(None)
}

/// Returns an event in the history's JSON form.
fn event_json(event: &Event) -> Json {
    let mut members = serde_json::Map::new();
    members.insert("index".to_owned(), json!(event.index));
    let (kind, extra): (&str, Vec<(&str, Json)>) = match &event.kind {
        Kind::Invoke {
            operation,
            args,
            keys,
        } => (
            "invoke",
            vec![
                ("operation", json!(operation)),
                ("args", Json::Array(args.iter().map(literal_of).collect())),
                ("keys", Json::Array(keys.iter().map(literal_of).collect())),
            ],
        ),
        Kind::Ok { output } => ("ok", vec![("output", literal_of(output))]),
        Kind::Fail { error } => ("fail", vec![("error", json!(error))]),
        Kind::Unknown { error } => ("unknown", vec![("error", json!(error))]),
        _ => ("unknown", Vec::new()),
    };
    members.insert("kind".to_owned(), json!(kind));
    members.insert("call".to_owned(), json!(event.call));
    members.insert("client".to_owned(), json!(event.client));
    members.insert("process".to_owned(), json!(event.process));
    for (name, held) in extra {
        members.insert(name.to_owned(), held);
    }
    Json::Object(members)
}

/// Returns the interval that one entry of a seam vector states.
fn interval_of(raw: &Json, at: &str) -> Result<Interval, Disagreement> {
    let end = match (raw.get("end"), raw["kind"].as_str()) {
        (None, None) => None,
        (Some(end), Some(kind)) => {
            let time = end
                .as_i64()
                .map(i128::from)
                .ok_or_else(|| Disagreement::new(format!("{at}.end"), "the end is no integer"))?;
            let error = raw["error"].as_str().unwrap_or_default().to_owned();
            let ending = match kind {
                "ok" => Ending::Ok(value(&raw["output"], &format!("{at}.output"))?),
                "fail" => Ending::Fail(error),
                "unknown" => Ending::Unknown(error),
                other => {
                    return Err(Disagreement::new(
                        format!("{at}.kind"),
                        format!("{other:?} completes no call"),
                    ));
                }
            };
            Some((time, ending))
        }
        _ => {
            return Err(Disagreement::new(
                at,
                "the entry states a completion kind and an end, or neither",
            ));
        }
    };
    Ok(Interval {
        client: client(raw, at)?,
        operation: raw["operation"].as_str().unwrap_or_default().to_owned(),
        args: values(&raw["args"], &format!("{at}.args"))?,
        keys: values(&raw["keys"], &format!("{at}.keys"))?,
        start: raw["start"]
            .as_i64()
            .map(i128::from)
            .ok_or_else(|| Disagreement::new(format!("{at}.start"), "the start is no integer"))?,
        end,
    })
}

/// Compares the events of a history, or the entry that its seam refused,
/// with the ones that a seam vector states.
fn seam(raw: &Json) -> Result<(), Disagreement> {
    let history = History::new();
    let refused = match (raw.get("script"), raw.get("intervals")) {
        (Some(script), None) => record(&history, script, "script")?,
        (None, Some(intervals)) => {
            let entries = intervals
                .as_array()
                .ok_or_else(|| Disagreement::new("intervals", "the intervals are no list"))?
                .iter()
                .enumerate()
                .map(|(at, entry)| interval_of(entry, &format!("intervals[{at}]")))
                .collect::<Result<Vec<_>, _>>()?;
            match history::from_intervals(&entries) {
                Ok(built) => return compare_events(&built.events(), raw),
                Err(refused) => Some(refused.entry()),
            }
        }
        _ => {
            return Err(Disagreement::new(
                "",
                "the vector states a script or intervals",
            ));
        }
    };
    let want = raw["refused"]
        .as_u64()
        .and_then(|at| usize::try_from(at).ok());
    if refused != want {
        return Err(Disagreement::new(
            "refused",
            format!("the seam refuses {refused:?}, want {want:?}"),
        ));
    }
    if refused.is_some() {
        return Ok(());
    }
    compare_events(&history.events(), raw)
}

/// Compares recorded events with the events of a seam vector, and that the
/// vector states no refusal.
fn compare_events(events: &[Event], raw: &Json) -> Result<(), Disagreement> {
    if !raw["refused"].is_null() {
        return Err(Disagreement::new(
            "refused",
            format!("the seam refuses no entry, want {}", raw["refused"]),
        ));
    }
    let stated = raw["events"]
        .as_array()
        .ok_or_else(|| Disagreement::new("events", "the events are no list"))?;
    for (at, event) in events.iter().map(event_json).enumerate() {
        if let Some(want) = stated.get(at).filter(|want| !same_json(&event, want)) {
            return Err(Disagreement::new(
                format!("events[{at}]"),
                format!("the event is {event}, want {want}"),
            ));
        }
    }
    if events.len() != stated.len() {
        return Err(Disagreement::new(
            "events",
            format!(
                "the number of events is {}, want {}",
                events.len(),
                stated.len()
            ),
        ));
    }
    Ok(())
}

/// Returns the history of a check vector's script.
fn history_of(raw: &Json) -> Result<History, Disagreement> {
    let history = History::new();
    if let Some(refused) = record(&history, &raw["history"], "history")? {
        return Err(Disagreement::new(
            format!("history[{refused}]"),
            "the seam refuses the entry",
        ));
    }
    Ok(history)
}

/// Returns the call record of `check` on a recorder, as JSON.
fn record_of(check: impl FnOnce(&Recorder)) -> Result<Json, Disagreement> {
    let seat = Recorder::new();
    check(&seat);
    let records = seat.records();
    let line = records
        .first()
        .ok_or_else(|| Disagreement::new("", "the recorder kept no call record"))?;
    serde_json::from_str(line)
        .map_err(|error| Disagreement::new("", format!("the call record is no JSON: {error}")))
}

/// Compares the verdict and the detail of a call record with a check
/// vector.
fn compare_verdict(call: &Json, raw: &Json) -> Result<(), Disagreement> {
    let expect = raw["expect"]
        .as_str()
        .filter(|expect| ["pass", "fail"].contains(expect))
        .ok_or_else(|| {
            Disagreement::new(
                "expect",
                format!(
                    "the vector expects {}, neither pass nor fail",
                    raw["expect"]
                ),
            )
        })?;
    if call["verdict"] != expect {
        return Err(Disagreement::new(
            "expect",
            format!(
                "the check ends as {}, want {expect}: {}",
                call["verdict"], call["error"]
            ),
        ));
    }
    if expect == "pass" {
        return Ok(());
    }
    for (name, stated) in raw["detail"].as_object().into_iter().flatten() {
        let got = &call["detail"][name];
        if !same_json(got, stated) {
            return Err(Disagreement::new(
                format!("detail.{name}"),
                format!("the field is {got}, want {stated}"),
            ));
        }
    }
    Ok(())
}

/// Returns the options of a check that a vector states.
fn options_of(raw: &Json) -> Result<Options, Disagreement> {
    let mut options = Options::default();
    for member in ["budget", "memo-limit", "workers"] {
        let Some(stated) = raw.get(member) else {
            continue;
        };
        let n = stated.as_u64().filter(|n| *n >= 1).ok_or_else(|| {
            Disagreement::new(
                member,
                format!("the {member} is {stated}, no integer of 1 or more"),
            )
        })?;
        options = match member {
            "budget" => options.budget(n),
            "memo-limit" => options.memo_limit(n),
            _ => options.workers(usize::try_from(n).unwrap_or(usize::MAX)),
        };
    }
    Ok(options)
}

/// Checks the history of a linearizable vector against its named spec, and
/// compares the call record with the vector's. A passing check states no
/// detail, so the runner observes the steps of a passing vector through two
/// more checks: under a budget of the steps it passes, and under a budget of
/// one step less it ends undecided at the steps limit after that budget.
fn linearizable(raw: &Json) -> Result<(), Disagreement> {
    let spec = Named::of(raw["spec"].as_str().unwrap_or_default())
        .ok_or_else(|| Disagreement::new("spec", format!("{} is no named spec", raw["spec"])))?;
    let history = history_of(raw)?;
    let options = options_of(raw)?;
    let checked = |options: Options| {
        record_of(|seat| history::is_linearizable(seat, &history, &spec, LINEARIZABLE, options))
    };
    compare_verdict(&checked(options)?, raw)?;
    if raw["expect"] == "fail" {
        return Ok(());
    }
    let stated = &raw["detail"];
    let steps = stated["steps"].as_u64().filter(|steps| *steps >= 2 && stated["partitions"] == 1).ok_or_else(|| {
        Disagreement::new(
            "detail",
            format!(
                "the runner observes the steps of a pass over one partition in two steps or more, and the vector states the partitions {} and the steps {}",
                stated["partitions"], stated["steps"]
            ),
        )
    })?;
    let exact = checked(options.budget(steps))?;
    if exact["verdict"] != "pass" {
        return Err(Disagreement::new(
            "detail.steps",
            format!(
                "the check ends as {} within {steps} steps, want pass",
                exact["verdict"]
            ),
        ));
    }
    let under = checked(options.budget(steps - 1))?;
    let detail = &under["detail"];
    let got = json!({
        "outcome": detail["outcome"],
        "partitions": detail["partitions"],
        "steps": detail["steps"],
        "limit": detail["limit"],
    });
    let want =
        json!({"outcome": "undecided", "partitions": 1, "steps": steps - 1, "limit": "steps"});
    if under["verdict"] != "fail" || !same_json(&got, &want) {
        return Err(Disagreement::new(
            "detail.steps",
            format!(
                "the check within {} steps ends as {} with {got}, want {want}",
                steps - 1,
                under["verdict"]
            ),
        ));
    }
    Ok(())
}

/// Checks the history of an isolation vector at its level, and compares the
/// call record with the vector's.
fn isolation(raw: &Json, snapshot: bool) -> Result<(), Disagreement> {
    let history = history_of(raw)?;
    let call = record_of(|seat| {
        if snapshot {
            history::has_snapshot_isolation(seat, &history, ISOLATION);
        } else {
            history::is_serializable(seat, &history, ISOLATION);
        }
    })?;
    compare_verdict(&call, raw)
}
