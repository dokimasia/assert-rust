// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The transactions of a history of list appends, which the isolation checks
//! read.
//!
//! A transaction is one call whose operation is `txn`. Its arguments are its
//! micro-operations in the order it ran them: `["append", key, value]` and
//! `["read", key, null]`. An ok completion commits the transaction, and its
//! output repeats the micro-operations with each read's list filled in, or
//! null for an empty list. A fail completion aborts it. Every value is
//! appended to its key once, so a value names the append that produced it.
//! Keys and values compare by their typed literals.

use super::recording::{Event, Kind, identity};
use crate::json::Json;
use crate::prop::engine::canonical::Canonical;
use crate::value::Value;
use std::collections::HashMap;

/// The operation of a transaction's call.
const TXN: &str = "txn";

/// The function of a micro-operation that appends a value to a key.
const APPEND: &str = "append";

/// The function of a micro-operation that reads a key's list.
const READ: &str = "read";

/// A history that is no history of list-append transactions, with the
/// reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TransactionError(pub(crate) String);

/// One micro-operation of a transaction.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Mop {
    /// An append of `value` to `key`.
    Append { key: Value, value: Value },
    /// A read of `key`: the list that it returned, and `None` for a read
    /// whose transaction did not commit.
    Read {
        key: Value,
        list: Option<Vec<Value>>,
    },
}

impl Mop {
    /// Returns the key of the micro-operation.
    pub(crate) fn key(&self) -> &Value {
        match self {
            Self::Append { key, .. } | Self::Read { key, .. } => key,
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
            "argument {position} of call {call} is {raw}, not [append, key, value] or [read, key, null]"
        ))
    };
    let Value::List(parts) = raw else {
        return Err(refused());
    };
    match parts.as_slice() {
        [Value::String(function), key, value] if function == APPEND => {
            identity_in(call, key)?;
            identity_in(call, value)?;
            Ok(Mop::Append {
                key: key.clone(),
                value: value.clone(),
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
            "the read at argument {position} of call {call} states a list before it ran"
        ))),
        _ => Err(refused()),
    }
}

/// Returns one micro-operation of an ok output, which repeats `mop` with a
/// read's list.
fn repeated(call: usize, position: usize, mop: &Mop, raw: &Value) -> Result<Mop, TransactionError> {
    let refused = || {
        let (function, key, value) = match mop {
            Mop::Append { key, value } => (APPEND, key, value.clone()),
            Mop::Read { key, .. } => (READ, key, Value::Null),
        };
        TransactionError(format!(
            "micro-operation {position} of the output of call {call} is {raw}, which does not repeat [{function}, {key}, {value}]"
        ))
    };
    let Value::List(parts) = raw else {
        return Err(refused());
    };
    let [Value::String(function), key, value] = parts.as_slice() else {
        return Err(refused());
    };
    let same_key = identity(key) == identity(mop.key());
    match mop {
        Mop::Append {
            value: appended, ..
        } => {
            if function != APPEND || !same_key || identity(value) != identity(appended) {
                return Err(refused());
            }
            Ok(mop.clone())
        }
        Mop::Read { key: read, .. } => {
            if function != READ || !same_key {
                return Err(refused());
            }
            let list = match value {
                Value::Null => Vec::new(),
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
            Ok(Mop::Read {
                key: read.clone(),
                list: Some(list),
            })
        }
    }
}

/// Returns the micro-operations of an ok output, which repeat the ones of
/// the invocation with the lists of the reads.
fn returned(call: usize, mops: &[Mop], output: &Value) -> Result<Vec<Mop>, TransactionError> {
    match output {
        Value::List(items) if items.len() == mops.len() => mops
            .iter()
            .zip(items)
            .enumerate()
            .map(|(position, (mop, raw))| repeated(call, position, mop, raw))
            .collect(),
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
    let (completion, output, mops) = match completion {
        None => (None, None, mops),
        Some(event) => match &event.kind {
            Kind::Ok { output } => (
                Some((event.index, Completed::Ok)),
                Some(output.clone()),
                returned(call, &mops, output)?,
            ),
            Kind::Fail { .. } => (Some((event.index, Completed::Fail)), None, mops),
            _ => (Some((event.index, Completed::Unknown)), None, mops),
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

/// Records each value that `txn` appends, under its key, and refuses a
/// second append of one value to one key.
fn register(
    txn: &Transaction,
    appended: &mut HashMap<(Canonical, Canonical), usize>,
) -> Result<(), TransactionError> {
    for mop in &txn.mops {
        let Mop::Append { key, value } = mop else {
            continue;
        };
        let name = (identity_in(txn.call, key)?, identity_in(txn.call, value)?);
        match appended.get(&name) {
            Some(&first) if first == txn.call => {
                return Err(TransactionError(format!(
                    "call {} appends {value} to the key {key} twice",
                    txn.call
                )));
            }
            Some(&first) => {
                return Err(TransactionError(format!(
                    "calls {first} and {} both append {value} to the key {key}",
                    txn.call
                )));
            }
            None => {
                appended.insert(name, txn.call);
            }
        }
    }
    Ok(())
}

/// Returns a history's transactions in the order of their invocations.
///
/// # Errors
///
/// Returns [`TransactionError`] when a call is no list-append transaction, an
/// ok output does not repeat the micro-operations of its invocation, a key or
/// a value has no typed literal, or a value is appended to one key twice.
pub(crate) fn transactions(events: &[Event]) -> Result<Vec<Transaction>, TransactionError> {
    let completions: HashMap<usize, &Event> = events
        .iter()
        .filter(|event| !matches!(event.kind, Kind::Invoke { .. }))
        .map(|event| (event.call, event))
        .collect();
    let mut appended = HashMap::new();
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
        register(&txn, &mut appended)?;
        found.push(txn);
    }
    Ok(found)
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
