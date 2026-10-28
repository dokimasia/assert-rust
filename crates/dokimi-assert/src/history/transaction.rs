// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The transactions of a history of list appends or of registers, which the
//! isolation checks read.
//!
//! A transaction is one call whose operation is `txn`. Its arguments are its
//! micro-operations in the order it ran them: `["append", key, value]`,
//! `["write", key, value]` and `["read", key, null]`. A history whose
//! micro-operations write is a register history, and every other history is
//! a list-append history. An ok completion commits the transaction. Its
//! output repeats the micro-operations with what each read returned: a list,
//! or null for an empty list, in a list-append history, and a value, or null
//! for the initial value, in a register history. A fail completion aborts
//! it. Every value is appended or written to its key once, so a value
//! identifies the micro-operation that produced it. Keys and values compare
//! by their typed literals.
//!
//! A read of a register is a read of the list of its one value, or of the
//! empty list for the initial value, so the checks read both workloads
//! alike.

use super::recording::{Event, Kind, identity};
use crate::json::Json;
use crate::prop::engine::canonical::Canonical;
use crate::value::Value;
use std::collections::HashMap;

/// The operation of a transaction's call.
const TXN: &str = "txn";

/// The function of a micro-operation that appends a value to a key.
const APPEND: &str = "append";

/// The function of a micro-operation that reads a key's list or value.
const READ: &str = "read";

/// The function of a micro-operation that writes a value to a register.
const WRITE: &str = "write";

/// A history that is no history of list-append or register transactions,
/// with the reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TransactionError(pub(crate) String);

/// One micro-operation of a transaction.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Mop {
    /// An append of `value` to `key`.
    Append { key: Value, value: Value },
    /// A write of `value` to the register `key`.
    Write { key: Value, value: Value },
    /// A read of `key`: the list that it returned, and `None` for a read
    /// whose transaction did not commit. A register read returned the list
    /// of its one value, or the empty list for the initial value.
    Read {
        key: Value,
        list: Option<Vec<Value>>,
    },
}

impl Mop {
    /// Returns the key of the micro-operation.
    pub(crate) fn key(&self) -> &Value {
        match self {
            Self::Append { key, .. } | Self::Write { key, .. } | Self::Read { key, .. } => key,
        }
    }

    /// Returns the function and the value of an append or a write, and
    /// `None` for a read.
    fn produced(&self) -> Option<(&'static str, &Value)> {
        match self {
            Self::Append { value, .. } => Some((APPEND, value)),
            Self::Write { value, .. } => Some((WRITE, value)),
            Self::Read { .. } => None,
        }
    }
}

/// How a transaction's call completed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Completed {
    Ok,
    Fail,
    Unknown,
}

impl Completed {
    /// Returns the definition's spelling of the completion.
    fn name(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Fail => "fail",
            Self::Unknown => "unknown",
        }
    }
}

/// One call of a history, read as a transaction. The micro-operations of an
/// ok transaction come from its output, with each read's list, and from the
/// invocation otherwise.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Transaction {
    pub(crate) call: usize,
    /// The completion's index and kind, and `None` for a pending transaction.
    pub(crate) completion: Option<(usize, Completed)>,
    pub(crate) process: usize,
    pub(crate) args: Vec<Value>,
    pub(crate) output: Option<Value>,
    pub(crate) mops: Vec<Mop>,
}

impl Transaction {
    /// Returns how the transaction completed, and `None` for a pending one.
    pub(crate) fn kind(&self) -> Option<Completed> {
        self.completion.map(|(_, kind)| kind)
    }
}

/// Returns the identity of a key or a value that `call` states.
pub(crate) fn identity_in(call: usize, value: &Value) -> Result<Canonical, TransactionError> {
    identity(value).ok_or_else(|| {
        TransactionError(format!(
            "call {call} states {value}, which no typed literal states"
        ))
    })
}

/// Returns the micro-operation that one argument of an invocation states.
fn invoked(call: usize, position: usize, raw: &Value) -> Result<Mop, TransactionError> {
    let refused = || {
        TransactionError(format!(
            "argument {position} of call {call} is {raw}, not [append, key, value], [write, key, value] or [read, key, null]"
        ))
    };
    let Value::List(parts) = raw else {
        return Err(refused());
    };
    match parts.as_slice() {
        [Value::String(function), _, Value::Null] if function == WRITE => {
            Err(TransactionError(format!(
                "the write at argument {position} of call {call} writes null, the initial value of every key"
            )))
        }
        [Value::String(function), key, value] if function == APPEND || function == WRITE => {
            identity_in(call, key)?;
            identity_in(call, value)?;
            let (key, value) = (key.clone(), value.clone());
            Ok(if function == APPEND {
                Mop::Append { key, value }
            } else {
                Mop::Write { key, value }
            })
        }
        [Value::String(function), key, Value::Null] if function == READ => {
            identity_in(call, key)?;
            Ok(Mop::Read {
                key: key.clone(),
                list: None,
            })
        }
        [Value::String(function), _, _] if function == READ => Err(TransactionError(format!(
            "the read at argument {position} of call {call} states a value before it ran"
        ))),
        _ => Err(refused()),
    }
}

/// Checks that one micro-operation of an ok output repeats `mop`. A read may
/// state what it returned in place of null.
fn repeated(call: usize, position: usize, mop: &Mop, raw: &Value) -> Result<(), TransactionError> {
    let (function, value) = mop.produced().unwrap_or((READ, &Value::Null));
    let refused = || {
        TransactionError(format!(
            "micro-operation {position} of the output of call {call} is {raw}, which does not repeat [{function}, {}, {value}]",
            mop.key()
        ))
    };
    let Value::List(parts) = raw else {
        return Err(refused());
    };
    let [Value::String(stated), key, returned] = parts.as_slice() else {
        return Err(refused());
    };
    let repeats = stated == function
        && identity(key) == identity(mop.key())
        && (function == READ || identity(returned) == identity(value));
    if repeats { Ok(()) } else { Err(refused()) }
}

/// Returns the list of the read at `position` of call `call`, which returned
/// `returned`. A register read returned one value, or null for the initial
/// value, and a list-append read returned a list, or null for the empty
/// list.
fn read_list(
    call: usize,
    position: usize,
    returned: &Value,
    register: bool,
) -> Result<Vec<Value>, TransactionError> {
    let list = match returned {
        Value::Null => Vec::new(),
        value if register => vec![value.clone()],
        Value::List(items) => items.clone(),
        other => {
            return Err(TransactionError(format!(
                "the read at micro-operation {position} of the output of call {call} returned {other}, not a list"
            )));
        }
    };
    for element in &list {
        identity_in(call, element)?;
    }
    Ok(list)
}

/// Gives each read of an ok transaction the list that its output states.
fn read_as(txn: &mut Transaction, register: bool) -> Result<(), TransactionError> {
    let Some(Value::List(output)) = &txn.output else {
        return Ok(());
    };
    for (position, (mop, raw)) in txn.mops.iter_mut().zip(output).enumerate() {
        let (Mop::Read { list, .. }, Value::List(parts)) = (mop, raw) else {
            continue;
        };
        *list = Some(read_list(txn.call, position, &parts[2], register)?);
    }
    Ok(())
}

/// Checks that an ok output repeats the micro-operations of the invocation.
fn returned(call: usize, mops: &[Mop], output: &Value) -> Result<(), TransactionError> {
    match output {
        Value::List(items) if items.len() == mops.len() => mops
            .iter()
            .zip(items)
            .enumerate()
            .try_for_each(|(position, (mop, raw))| repeated(call, position, mop, raw)),
        _ => Err(TransactionError(format!(
            "the output of call {call} does not repeat the micro-operations of its invocation"
        ))),
    }
}

/// Returns the transaction of the call that `invocation` invokes with
/// `operation` and `args`, whose completion is `completion`.
fn transaction_of(
    invocation: &Event,
    operation: &str,
    args: &[Value],
    completion: Option<&Event>,
) -> Result<Transaction, TransactionError> {
    let call = invocation.index;
    if operation != TXN {
        return Err(TransactionError(format!(
            "call {call} is {operation:?}, not {TXN:?}"
        )));
    }
    let mops = args
        .iter()
        .enumerate()
        .map(|(position, raw)| invoked(call, position, raw))
        .collect::<Result<Vec<_>, _>>()?;
    let (completion, output) = match completion {
        None => (None, None),
        Some(event) => match &event.kind {
            Kind::Ok { output } => {
                returned(call, &mops, output)?;
                (Some((event.index, Completed::Ok)), Some(output.clone()))
            }
            Kind::Fail { .. } => (Some((event.index, Completed::Fail)), None),
            _ => (Some((event.index, Completed::Unknown)), None),
        },
    };
    Ok(Transaction {
        call,
        completion,
        process: invocation.process,
        args: args.to_vec(),
        output,
        mops,
    })
}

/// Records each value that `txn` appends or writes, under its key, and
/// refuses a second one of one value to one key.
fn register(
    txn: &Transaction,
    produced: &mut HashMap<(Canonical, Canonical), usize>,
) -> Result<(), TransactionError> {
    for mop in &txn.mops {
        let Some((function, value)) = mop.produced() else {
            continue;
        };
        let key = mop.key();
        let name = (identity_in(txn.call, key)?, identity_in(txn.call, value)?);
        let verb = if function == APPEND {
            "append"
        } else {
            "write"
        };
        match produced.get(&name) {
            Some(&first) if first == txn.call => {
                return Err(TransactionError(format!(
                    "call {} {verb}s {value} to the key {key} twice",
                    txn.call
                )));
            }
            Some(&first) => {
                return Err(TransactionError(format!(
                    "calls {first} and {} both {verb} {value} to the key {key}",
                    txn.call
                )));
            }
            None => {
                produced.insert(name, txn.call);
            }
        }
    }
    Ok(())
}

/// Returns `true` for a register history: a history whose micro-operations
/// write.
///
/// # Errors
///
/// Returns [`TransactionError`] for a history that both appends and writes.
fn registers(txns: &[Transaction]) -> Result<bool, TransactionError> {
    let first = |wanted: &str| {
        txns.iter()
            .find(|txn| {
                txn.mops.iter().any(|mop| {
                    mop.produced()
                        .is_some_and(|(function, _)| function == wanted)
                })
            })
            .map(|txn| txn.call)
    };
    match (first(APPEND), first(WRITE)) {
        (Some(appends), Some(writes)) => Err(TransactionError(format!(
            "call {appends} appends and call {writes} writes, and a history is either list-append or registers"
        ))),
        (_, writes) => Ok(writes.is_some()),
    }
}

/// Returns a history's transactions in the order of their invocations, and
/// `true` for a register history.
///
/// # Errors
///
/// Returns [`TransactionError`] when a call is no list-append or register
/// transaction, an ok output does not repeat the micro-operations of its
/// invocation, a key or a value has no typed literal, a value is appended or
/// written to one key twice, the history both appends and writes, or a
/// list-append read returned no list.
pub(crate) fn transactions(events: &[Event]) -> Result<(Vec<Transaction>, bool), TransactionError> {
    let completions: HashMap<usize, &Event> = events
        .iter()
        .filter(|event| !matches!(event.kind, Kind::Invoke { .. }))
        .map(|event| (event.call, event))
        .collect();
    let mut produced = HashMap::new();
    let mut found = Vec::new();
    for event in events {
        let Kind::Invoke {
            operation, args, ..
        } = &event.kind
        else {
            continue;
        };
        let txn = transaction_of(
            event,
            operation,
            args,
            completions.get(&event.index).copied(),
        )?;
        register(&txn, &mut produced)?;
        found.push(txn);
    }
    let register = registers(&found)?;
    for txn in &mut found {
        read_as(txn, register)?;
    }
    Ok((found, register))
}

/// Returns a transaction in the history's JSON form: the call, the
/// completion and its kind, the process, the args as typed literals, and the
/// output as a typed literal. A pending transaction states no completion and
/// no kind, and only an ok transaction states an output.
pub(crate) fn transaction_json(txn: &Transaction) -> Json {
    let mut members = vec![("call".to_owned(), Json::from(txn.call))];
    if let Some((completion, kind)) = txn.completion {
        members.push(("completion".to_owned(), Json::from(completion)));
        members.push(("kind".to_owned(), Json::from(kind.name())));
    }
    members.push(("process".to_owned(), Json::from(txn.process)));
    members.push((
        "args".to_owned(),
        Json::Array(txn.args.iter().map(Value::to_literal).collect()),
    ));
    if let Some(output) = &txn.output {
        members.push(("output".to_owned(), output.to_literal()));
    }
    Json::Object(members)
}
