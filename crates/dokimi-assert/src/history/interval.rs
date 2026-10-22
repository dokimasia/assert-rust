// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! A history built from calls that a log recorded with a start and an end on
//! one clock.

use super::recording::{Call, History, identity};
use crate::value::Value;
use std::collections::HashMap;
use std::fmt;

/// How a call of an [`Interval`] completed.
#[derive(Debug, Clone, PartialEq)]
pub enum Ending {
    /// The call returned the value and took effect.
    Ok(Value),
    /// The call took no effect, with the text of its error.
    Fail(String),
    /// The call ended without an outcome, with the text of its error.
    Unknown(String),
}

/// A call recorded with a start and an end on one clock, as a log of a
/// system's calls states it.
#[derive(Debug, Clone, PartialEq)]
pub struct Interval {
    /// The client that made the call.
    pub client: usize,
    /// The call's operation.
    pub operation: String,
    /// The call's arguments.
    pub args: Vec<Value>,
    /// The keys that the call touches. A call without keys touches every
    /// key.
    pub keys: Vec<Value>,
    /// The time the call started.
    pub start: i128,
    /// The time the call completed and how, or `None` for a pending call.
    pub end: Option<(i128, Ending)>,
}

/// An entry that [`from_intervals`] refuses: its position, and the rule that
/// it breaks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntervalError {
    entry: usize,
    reason: String,
}

impl IntervalError {
    /// Returns the position of the entry that breaks a rule.
    #[must_use]
    pub fn entry(&self) -> usize {
        self.entry
    }
}

impl fmt::Display for IntervalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "history: entry {} {}", self.entry, self.reason)
    }
}

impl std::error::Error for IntervalError {}

/// Returns `true` when two closed intervals share an instant. A pending
/// interval has no end.
fn overlap(a: &Interval, b: &Interval) -> bool {
    a.end.as_ref().is_none_or(|(end, _)| b.start <= *end)
        && b.end.as_ref().is_none_or(|(end, _)| a.start <= *end)
}

/// Returns the rule that `entry` breaks against the earlier entries of its
/// client, or `None`.
fn refusal(entry: &Interval, earlier: &[(usize, &Interval)]) -> Option<String> {
    if let Some((end, _)) = &entry.end {
        if *end < entry.start {
            return Some(format!(
                "ends at {end}, before it starts at {}",
                entry.start
            ));
        }
    }
    if let Some(key) = entry.keys.iter().find(|key| identity(key).is_none()) {
        return Some(format!(
            "states the key {key}, which no typed literal states"
        ));
    }
    earlier
        .iter()
        .find(|(_, before)| overlap(before, entry))
        .map(|(other, _)| format!("overlaps entry {other} of client {}", entry.client))
}

/// Returns the history of calls recorded with a start and an end on one
/// clock.
///
/// The events are in time order. At one time an invocation comes before a
/// completion, and the invocations, or the completions, keep the order of
/// their entries. Each interval is closed, so two entries that share an
/// instant overlap, and a pending entry overlaps every later entry of its
/// client.
///
/// # Errors
///
/// Returns [`IntervalError`] for the first entry, in the given order, that
/// ends before it starts, states a key that no typed literal states, or
/// overlaps an earlier entry of its client.
pub fn from_intervals(entries: &[Interval]) -> Result<History, IntervalError> {
    let mut earlier: HashMap<usize, Vec<(usize, &Interval)>> = HashMap::new();
    for (position, entry) in entries.iter().enumerate() {
        let own = earlier.entry(entry.client).or_default();
        if let Some(reason) = refusal(entry, own) {
            return Err(IntervalError {
                entry: position,
                reason,
            });
        }
        own.push((position, entry));
    }
    let mut moments: Vec<(i128, u8, usize)> = Vec::with_capacity(2 * entries.len());
    for (position, entry) in entries.iter().enumerate() {
        moments.push((entry.start, 0, position));
        if let Some((end, _)) = &entry.end {
            moments.push((*end, 1, position));
        }
    }
    moments.sort_unstable();
    let history = History::new();
    record(&history, entries, &moments);
    Ok(history)
}

/// Records the invocation and the completion of each entry into `history`,
/// at its moments in order. A completion follows the invocation of its
/// entry, because an entry starts no later than it ends.
fn record(history: &History, entries: &[Interval], moments: &[(i128, u8, usize)]) {
    let mut calls: Vec<Option<Call<'_>>> = entries.iter().map(|_| None).collect();
    for &(_, phase, position) in moments {
        let entry = &entries[position];
        if phase == 0 {
            calls[position] = Some(history.invoke(
                entry.client,
                &entry.operation,
                entry.args.clone(),
                entry.keys.clone(),
            ));
        } else {
            // Only an entry with an end has a moment of its completion, which
            // sorts after the moment of its invocation.
            let call = calls[position]
                .take()
                .expect("an entry is invoked before it completes");
            let (_, ending) = entry
                .end
                .as_ref()
                .expect("an entry that completes has an end");
            match ending {
                Ending::Ok(output) => call.ok(output.clone()),
                Ending::Fail(error) => call.fail(error),
                Ending::Unknown(error) => call.unknown(error),
            }
        }
    }
}
