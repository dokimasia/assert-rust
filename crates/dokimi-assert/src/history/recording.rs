//! The recording of a history: the invocations and the completions of the
//! calls that the clients of an object make, in one order.

use crate::prop::engine::canonical::{Canonical, canonical};
use crate::value::Value;
use std::collections::HashMap;
use std::fmt::Display;
use std::sync::{Mutex, MutexGuard, PoisonError};

/// What one event of a history records: the invocation of a call, or how
/// the call completed.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Kind {
    /// The invocation of a call, which starts it.
    Invoke {
        /// The call's operation.
        operation: String,
        /// The call's arguments.
        args: Vec<Value>,
        /// The keys that the call touches. A call without keys touches every
        /// key.
        keys: Vec<Value>,
    },
    /// The call returned `output` and took effect.
    Ok {
        /// What the call returned.
        output: Value,
    },
    /// The call took no effect.
    Fail {
        /// The text of the call's error.
        error: String,
    },
    /// The call ended without an outcome, so it may still take effect.
    Unknown {
        /// The text of the call's error.
        error: String,
    },
}

/// One recorded event of a [`History`].
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Event {
    /// The event's position in recording order, from 0.
    pub index: usize,
    /// The index of the invocation of the event's call.
    pub call: usize,
    /// The client that made the call.
    pub client: usize,
    /// The process that made the call.
    pub process: usize,
    /// The invocation, or the completion and its outcome.
    pub kind: Kind,
}

/// The events of one history and the identity of each key of each event.
#[derive(Debug, Default)]
struct State {
    events: Vec<Event>,
    /// The identities of each event's keys, and none for a completion.
    ids: Vec<Vec<Canonical>>,
    /// The index of the invocation of the open call of each client that has
    /// one.
    open: HashMap<usize, usize>,
    /// The process of each client.
    process: HashMap<usize, usize>,
    processes: usize,
}

/// The events of one history, in one recording order that every client
/// shares.
///
/// A test records each call through [`History::invoke`] before the call to
/// the subject starts, and through [`Call::ok`], [`Call::fail`] or
/// [`Call::unknown`] after it returns. Each client starts on a process of its
/// own, and processes are numbered in the order of their first invocation. A
/// call that completes as unknown may still be in progress inside the
/// subject, so its client continues on a new process.
///
/// # Concurrency
///
/// A history is safe for concurrent use. One mutex orders every invocation
/// and completion, so call `a` precedes call `b` only when `a` completed
/// before `b` was invoked.
#[derive(Debug, Default)]
pub struct History {
    state: Mutex<State>,
}

/// Returns the identity of a key: the canonical form of its typed literal,
/// so two maps of equal entries are one key whatever their order. A value
/// that contains an opaque value has no typed literal, and no identity.
pub(crate) fn identity(value: &Value) -> Option<Canonical> {
    fn literal(value: &Value) -> bool {
        match value {
            Value::Opaque(_) => false,
            Value::List(items) => items.iter().all(literal),
            Value::Map(entries) => entries
                .iter()
                .all(|(key, held)| literal(key) && literal(held)),
            Value::Record(fields) => fields.iter().all(|(_, held)| literal(held)),
            Value::Variant { payload, .. } => payload.as_deref().is_none_or(literal),
            _ => true,
        }
    }
    literal(value).then(|| canonical(value))
}

impl History {
    /// Returns an empty history.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn locked(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Records an invocation of `operation` with `args` by `client`, and
    /// returns the call through which the client records the completion.
    /// `keys` are the keys that the call touches, and a call without keys
    /// touches every key. Two keys are one key when their typed literals are
    /// equal.
    ///
    /// ```
    /// use dokimi_assert::Value;
    /// use dokimi_assert::history::History;
    ///
    /// let h = History::new();
    /// let write = h.invoke(0, "write", [1.into()], ["x".into()]);
    /// write.ok(Value::Null);
    /// let read = h.invoke(0, "read", [], ["x".into()]);
    /// read.ok(1);
    /// assert_eq!(h.events().len(), 4);
    /// ```
    ///
    /// # Panics
    ///
    /// Panics when `client` has a call open, and for a key that contains an
    /// opaque value, which no typed literal states.
    pub fn invoke(
        &self,
        client: usize,
        operation: &str,
        args: impl IntoIterator<Item = Value>,
        keys: impl IntoIterator<Item = Value>,
    ) -> Call<'_> {
        let keys: Vec<Value> = keys.into_iter().collect();
        let mut ids = Vec::with_capacity(keys.len());
        for key in &keys {
            let Some(id) = identity(key) else {
                panic!(
                    "history: invoke({client}, {operation:?}) states the key {key}, which no typed literal states"
                );
            };
            ids.push(id);
        }
        let mut state = self.locked();
        if let Some(open) = state.open.get(&client).copied() {
            drop(state);
            panic!(
                "history: invoke({client}, {operation:?}) while call {open} of client {client} is open"
            );
        }
        let process = if let Some(process) = state.process.get(&client) {
            *process
        } else {
            let process = state.processes;
            state.processes += 1;
            state.process.insert(client, process);
            process
        };
        let index = state.events.len();
        state.events.push(Event {
            index,
            call: index,
            client,
            process,
            kind: Kind::Invoke {
                operation: operation.to_owned(),
                args: args.into_iter().collect(),
                keys,
            },
        });
        state.ids.push(ids);
        state.open.insert(client, index);
        Call {
            history: self,
            call: index,
        }
    }

    /// Returns the recorded events in recording order: the events with the
    /// indices 0 to n − 1 for every event recorded before the call.
    #[must_use]
    pub fn events(&self) -> Vec<Event> {
        self.locked().events.clone()
    }

    /// Returns the events and the identities of the keys of each event.
    pub(crate) fn recorded(&self) -> (Vec<Event>, Vec<Vec<Canonical>>) {
        let state = self.locked();
        (state.events.clone(), state.ids.clone())
    }

    /// Records the completion `kind` of the call whose invocation is at index
    /// `call`.
    fn complete(&self, call: usize, kind: Kind) {
        let mut state = self.locked();
        let (client, process) = (state.events[call].client, state.events[call].process);
        state.open.remove(&client);
        if matches!(kind, Kind::Unknown { .. }) {
            state.process.remove(&client);
        }
        let index = state.events.len();
        state.events.push(Event {
            index,
            call,
            client,
            process,
            kind,
        });
        state.ids.push(Vec::new());
    }
}

/// An open call of a [`History`], through which its client records the
/// completion. Each method takes the call, so a call completes once. A call
/// that the client drops without a completion is pending.
#[derive(Debug)]
#[must_use = "a call records its completion through ok, fail or unknown"]
pub struct Call<'h> {
    history: &'h History,
    call: usize,
}

impl Call<'_> {
    /// Records that the call returned `output` and took effect.
    pub fn ok(self, output: impl Into<Value>) {
        self.history.complete(
            self.call,
            Kind::Ok {
                output: output.into(),
            },
        );
    }

    /// Records that the call took no effect, such as a refused connection.
    /// An error that states what the subject observed is an output for
    /// [`Call::ok`]: a compare-and-set that refuses because the value
    /// differs returns `false`.
    pub fn fail(self, error: impl Display) {
        self.history.complete(
            self.call,
            Kind::Fail {
                error: error.to_string(),
            },
        );
    }

    /// Records that the call ended without an outcome, such as a timeout, a
    /// lost reply or a crash. The call may still take effect, so its client's
    /// next invocation starts on a new process.
    pub fn unknown(self, error: impl Display) {
        self.history.complete(
            self.call,
            Kind::Unknown {
                error: error.to_string(),
            },
        );
    }
}
