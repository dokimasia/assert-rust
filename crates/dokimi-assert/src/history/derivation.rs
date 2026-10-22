//! The derivation over one history's transactions: the committed
//! transactions, the version order of each key, the dependencies between
//! committed transactions, and the first instance of each anomaly.
//!
//! The committed transactions are the ok ones, and the ones whose outcome is
//! unknown, or that are pending, whose append a committed read observed. A
//! key's version order is its longest committed read, and a key whose
//! committed reads are not prefixes of one list has no version order. Every
//! committed read is a prefix of the key's final list, so a committed append
//! that no committed read observed follows the last value of the version
//! order.

use super::cycle::{Graph, RELATIONS, Relation, Walk, components, path, simple};
use super::recording::identity;
use super::transaction::{Completed, Mop, Transaction};
use crate::prop::engine::canonical::Canonical;
use crate::value::Value;
use std::collections::{HashMap, HashSet, VecDeque};

/// A component of a graph: its transactions, and its edges of one relation.
type Component = (HashSet<usize>, Vec<(usize, usize)>);

/// One dependency of a committed read: its source, its target, its relation,
/// and the values that it relates.
type ReadEdge = (
    Option<usize>,
    Option<usize>,
    Relation,
    Option<Value>,
    Option<Value>,
);

/// A kind of anomaly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Anomaly {
    GarbageRead,
    DuplicateAppend,
    InternalInconsistency,
    IncompatibleOrder,
    AbortedRead,
    IntermediateRead,
    G0,
    G1c,
    GSingle,
    GNonadjacent,
    G2,
}

/// The kinds of anomaly, in the order in which a record reports them.
pub(crate) const ANOMALIES: [Anomaly; 11] = [
    Anomaly::GarbageRead,
    Anomaly::DuplicateAppend,
    Anomaly::InternalInconsistency,
    Anomaly::IncompatibleOrder,
    Anomaly::AbortedRead,
    Anomaly::IntermediateRead,
    Anomaly::G0,
    Anomaly::G1c,
    Anomaly::GSingle,
    Anomaly::GNonadjacent,
    Anomaly::G2,
];

impl Anomaly {
    /// Returns the definition's spelling of the anomaly.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::GarbageRead => "garbage-read",
            Self::DuplicateAppend => "duplicate-append",
            Self::InternalInconsistency => "internal-inconsistency",
            Self::IncompatibleOrder => "incompatible-order",
            Self::AbortedRead => "aborted-read",
            Self::IntermediateRead => "intermediate-read",
            Self::G0 => "G0",
            Self::G1c => "G1c",
            Self::GSingle => "G-single",
            Self::GNonadjacent => "G-nonadjacent",
            Self::G2 => "G2",
        }
    }
}

/// The evidence of one dependency of a cycle: the two transactions that it
/// joins, its relation, and the key and the values that prove it. `value`
/// is a value of `from` in the key's version order for ww, the last value of
/// the list that `to` read for wr, and the last value of the list that
/// `from` read for rw, which is `None` for a read of the empty list. `next`
/// is the value of `to` that follows `value` in the version order, for ww
/// and rw.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Edge {
    pub(crate) from: usize,
    pub(crate) to: usize,
    pub(crate) relation: Relation,
    pub(crate) key: Value,
    pub(crate) value: Option<Value>,
    pub(crate) next: Option<Value>,
}

/// The evidence of an anomaly that is no cycle: the reads and the appends
/// that show it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Observation {
    /// A read of `value`, which no transaction appended to `key`.
    GarbageRead {
        call: usize,
        key: Value,
        read: Vec<Value>,
        value: Value,
    },
    /// A read that contains `value` twice.
    DuplicateAppend {
        call: usize,
        key: Value,
        read: Vec<Value>,
        value: Value,
    },
    /// A read that contradicts its own transaction: what the transaction
    /// knew of the list before the read, whether it knew the whole list, and
    /// the first value of the read that it appends to the key later.
    InternalInconsistency {
        call: usize,
        key: Value,
        read: Vec<Value>,
        expected: Vec<Value>,
        whole: bool,
        future: Option<Value>,
    },
    /// Two reads of `key` that are not prefixes of one list.
    IncompatibleOrder {
        calls: [usize; 2],
        key: Value,
        reads: [Vec<Value>; 2],
    },
    /// A read of `value`, which the aborted call `appender` appended.
    AbortedRead {
        call: usize,
        key: Value,
        value: Value,
        appender: usize,
    },
    /// A read that ends in `value`, which `appender` followed with `next`.
    IntermediateRead {
        call: usize,
        key: Value,
        value: Value,
        appender: usize,
        next: Value,
    },
}

/// One entry of an anomaly's explanation.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Evidence {
    Edge(Edge),
    Observation(Observation),
}

/// One transaction of a cycle, with the relations of the edge from it to the
/// next transaction, in the order ww, wr, rw.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Link {
    pub(crate) call: usize,
    pub(crate) relations: Vec<Relation>,
}

/// One instance of an anomaly: the transactions that it involves, in the
/// order the record lists them, the cycle of a cycle anomaly, and the
/// evidence.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Instance {
    pub(crate) anomaly: Anomaly,
    pub(crate) calls: Vec<usize>,
    pub(crate) cycle: Option<Vec<Link>>,
    pub(crate) explanation: Vec<Evidence>,
}

/// How the search of one kind of cycle finds it: the relations of the
/// components that it takes, the relation of the edges that it tries as the
/// closing edge, and the relations of the path from the closing edge's
/// target back to its source.
struct CycleSearch {
    component: &'static [Relation],
    closing: Relation,
    path: &'static [Relation],
}

/// The relations other than a read-write dependency.
const NON_RW: &[Relation] = &[Relation::Ww, Relation::Wr];

/// The read-write dependency alone.
const RW: &[Relation] = &[Relation::Rw];

/// The fewest read-write edges of a G-nonadjacent cycle. A cycle with one is
/// G-single.
const NONADJACENT_RW: usize = 2;

/// Returns the search of a kind of cycle other than G-nonadjacent.
fn cycle_search(anomaly: Anomaly) -> CycleSearch {
    match anomaly {
        Anomaly::G0 => CycleSearch {
            component: &[Relation::Ww],
            closing: Relation::Ww,
            path: &[Relation::Ww],
        },
        Anomaly::G1c => CycleSearch {
            component: NON_RW,
            closing: Relation::Wr,
            path: NON_RW,
        },
        Anomaly::GSingle => CycleSearch {
            component: &RELATIONS,
            closing: Relation::Rw,
            path: NON_RW,
        },
        _ => CycleSearch {
            component: &RELATIONS,
            closing: Relation::Rw,
            path: &RELATIONS,
        },
    }
}

/// A committed read: the reading transaction, its key, and the list it
/// returned.
#[derive(Debug, Clone)]
struct Read {
    call: usize,
    key: Value,
    list: Vec<Value>,
}

/// A key's version order, the committed writer of each value, and the
/// committed appends that no read observed.
struct Versions<'a> {
    key: &'a Value,
    order: &'a [Value],
    writers: Vec<Option<usize>>,
    unobserved: Vec<(usize, Value)>,
}

/// A state of the G-nonadjacent search: a transaction, whether the walk
/// entered it by an rw edge, and whether the walk has taken one.
type State = (usize, bool, bool);

/// Returns the identity of a key or a value that the transactions state,
/// which has one.
fn id(value: &Value) -> Canonical {
    identity(value).unwrap_or(Canonical::Null)
}

/// Returns the instance of an anomaly that is no cycle, with its evidence.
fn single(anomaly: Anomaly, calls: Vec<usize>, observation: Observation) -> Instance {
    Instance {
        anomaly,
        calls,
        cycle: None,
        explanation: vec![Evidence::Observation(observation)],
    }
}

/// Returns the values of the read at `position` that `txn` appends to the
/// key after it.
fn future(txn: &Transaction, position: usize, read: &[Value]) -> Vec<Value> {
    let key = id(txn.mops[position].key());
    let later: HashSet<Canonical> = txn.mops[position + 1..]
        .iter()
        .filter_map(|mop| match mop {
            Mop::Append {
                key: appended,
                value,
            } if id(appended) == key => Some(id(value)),
            _ => None,
        })
        .collect();
    read.iter()
        .filter(|value| later.contains(&id(value)))
        .cloned()
        .collect()
}

/// Returns `true` when a read is the expected list, or ends with it when the
/// transaction knew only the list's end.
fn agrees(read: &[Value], whole: bool, expected: &[Value]) -> bool {
    let tail = if whole {
        read
    } else {
        &read[read.len().saturating_sub(expected.len())..]
    };
    tail.len() == expected.len() && tail.iter().zip(expected).all(|(a, b)| id(a) == id(b))
}

/// Returns the first read of an ok transaction that contradicts the
/// transaction. Before a key's first read, the transaction knows only that
/// the key ends with its own appends. After a read, it knows the whole list,
/// and its later appends extend it. A read contains none of the values that
/// the transaction appends to the key after it.
fn internal(txn: &Transaction) -> Option<Instance> {
    let mut known: HashMap<Canonical, (bool, Vec<Value>)> = HashMap::new();
    for (position, mop) in txn.mops.iter().enumerate() {
        let key = id(mop.key());
        let (whole, expected) = known.get(&key).cloned().unwrap_or((false, Vec::new()));
        let read = match mop {
            Mop::Append { value, .. } => {
                let mut extended = expected;
                extended.push(value.clone());
                known.insert(key, (whole, extended));
                continue;
            }
            Mop::Read { list, .. } => list.clone().unwrap_or_default(),
        };
        let later = future(txn, position, &read);
        if !later.is_empty() || (known.contains_key(&key) && !agrees(&read, whole, &expected)) {
            return Some(single(
                Anomaly::InternalInconsistency,
                vec![txn.call],
                Observation::InternalInconsistency {
                    call: txn.call,
                    key: mop.key().clone(),
                    read,
                    expected,
                    whole,
                    future: later.into_iter().next(),
                },
            ));
        }
        known.insert(key, (true, read));
    }
    None
}

/// Returns the longest of a key's reads, or the first two of its distinct
/// reads, shortest first, that are not prefixes of one list.
fn longest(reads: &[&Read]) -> Result<Vec<Value>, Box<(Read, Read)>> {
    let mut distinct: Vec<(Vec<Canonical>, &Read)> = Vec::new();
    for read in reads {
        let ids: Vec<Canonical> = read.list.iter().map(id).collect();
        if !distinct.iter().any(|(held, _)| *held == ids) {
            distinct.push((ids, read));
        }
    }
    distinct.sort_by_key(|(ids, _)| ids.len());
    for pair in distinct.windows(2) {
        let ((shorter, first), (longer, second)) = (&pair[0], &pair[1]);
        if shorter.iter().zip(longer).any(|(a, b)| a != b) {
            return Err(Box::new(((*first).clone(), (*second).clone())));
        }
    }
    Ok(distinct
        .last()
        .map(|(_, read)| read.list.clone())
        .unwrap_or_default())
}

/// Returns the steps of the walk to `state`: each transaction, and the edge
/// into it.
fn walk_to(
    state: State,
    parent: &HashMap<State, Option<(State, Relation)>>,
) -> Vec<(usize, Relation)> {
    let mut steps = Vec::new();
    let mut current = state;
    while let Some(Some((previous, relation))) = parent.get(&current) {
        steps.push((current.0, *relation));
        current = *previous;
    }
    steps.reverse();
    steps
}

/// The derivation over one history's transactions.
pub(crate) struct Analysis<'t> {
    txns: &'t [Transaction],
    /// The call that appended each value to each key.
    appender: HashMap<(Canonical, Canonical), usize>,
    /// The appends to each key, as their calls and values, in the order of
    /// the transactions.
    appends: HashMap<Canonical, Vec<(usize, Value)>>,
    /// The value that the appender of each append appended to its key after
    /// it.
    later: HashMap<(Canonical, Canonical), Value>,
    /// Each key as the history first states it, in the order the keys first
    /// appear.
    keys: Vec<(Canonical, Value)>,
    aborted: HashSet<usize>,
    reads: Vec<Read>,
    committed: HashSet<usize>,
    incompatible: Option<Instance>,
    orders: Vec<(Canonical, Vec<Value>)>,
    /// The evidence of each dependency, by its source and its target, in the
    /// order derived.
    edges: HashMap<(usize, usize), Vec<Edge>>,
}

impl<'t> Analysis<'t> {
    /// Derives the dependencies between the committed transactions of
    /// `txns`, which are in the order of their invocations, and whose keys
    /// and values each have a typed literal.
    pub(crate) fn new(txns: &'t [Transaction]) -> Self {
        let mut analysis = Self {
            txns,
            appender: HashMap::new(),
            appends: HashMap::new(),
            later: HashMap::new(),
            keys: Vec::new(),
            aborted: txns
                .iter()
                .filter(|txn| txn.kind() == Some(Completed::Fail))
                .map(|txn| txn.call)
                .collect(),
            reads: Vec::new(),
            committed: HashSet::new(),
            incompatible: None,
            orders: Vec::new(),
            edges: HashMap::new(),
        };
        for txn in txns {
            analysis.index(txn);
        }
        analysis.reads = txns
            .iter()
            .filter(|txn| txn.kind() == Some(Completed::Ok))
            .flat_map(|txn| {
                txn.mops.iter().filter_map(|mop| match mop {
                    Mop::Read { key, list } => Some(Read {
                        call: txn.call,
                        key: key.clone(),
                        list: list.clone().unwrap_or_default(),
                    }),
                    Mop::Append { .. } => None,
                })
            })
            .collect();
        let undecided: HashSet<usize> = txns
            .iter()
            .filter(|txn| matches!(txn.kind(), None | Some(Completed::Unknown)))
            .map(|txn| txn.call)
            .collect();
        let mut committed: HashSet<usize> = txns
            .iter()
            .filter(|txn| txn.kind() == Some(Completed::Ok))
            .map(|txn| txn.call)
            .collect();
        for read in &analysis.reads {
            for value in &read.list {
                if let Some(&appender) = analysis.appender.get(&(id(&read.key), id(value))) {
                    if undecided.contains(&appender) {
                        committed.insert(appender);
                    }
                }
            }
        }
        analysis.committed = committed;
        analysis.orders = analysis.version_orders();
        analysis.derive();
        analysis
    }

    /// Indexes the keys that `txn` touches, and the appends that it makes.
    fn index(&mut self, txn: &Transaction) {
        let mut last: HashMap<Canonical, (Canonical, Canonical)> = HashMap::new();
        for mop in &txn.mops {
            let key = id(mop.key());
            if !self.keys.iter().any(|(held, _)| *held == key) {
                self.keys.push((key.clone(), mop.key().clone()));
            }
            let Mop::Append { value, .. } = mop else {
                continue;
            };
            let name = (key.clone(), id(value));
            self.appender.insert(name.clone(), txn.call);
            self.appends
                .entry(key.clone())
                .or_default()
                .push((txn.call, value.clone()));
            if let Some(before) = last.get(&key) {
                self.later.insert(before.clone(), value.clone());
            }
            last.insert(key, name);
        }
    }

    /// Returns the first instance of `anomaly` in the history.
    pub(crate) fn instance(&self, anomaly: Anomaly) -> Option<Instance> {
        match anomaly {
            Anomaly::GarbageRead => self.garbage_read(),
            Anomaly::DuplicateAppend => self.duplicate_append(),
            Anomaly::InternalInconsistency => self
                .txns
                .iter()
                .filter(|txn| txn.kind() == Some(Completed::Ok))
                .find_map(internal),
            Anomaly::IncompatibleOrder => self.incompatible.clone(),
            Anomaly::AbortedRead => self.aborted_read(),
            Anomaly::IntermediateRead => self.intermediate_read(),
            Anomaly::GNonadjacent => self.nonadjacent(),
            cycle => self.cycle(cycle, &cycle_search(cycle)),
        }
    }

    /// Returns the first committed read of a value that no transaction
    /// appended.
    fn garbage_read(&self) -> Option<Instance> {
        self.reads.iter().find_map(
            |Read {
                 call,
                 key,
                 list: read,
             }| {
                read.iter()
                    .find(|value| !self.appender.contains_key(&(id(key), id(value))))
                    .map(|value| {
                        single(
                            Anomaly::GarbageRead,
                            vec![*call],
                            Observation::GarbageRead {
                                call: *call,
                                key: key.clone(),
                                read: read.clone(),
                                value: value.clone(),
                            },
                        )
                    })
            },
        )
    }

    /// Returns the first committed read that returned one value twice.
    fn duplicate_append(&self) -> Option<Instance> {
        self.reads.iter().find_map(
            |Read {
                 call,
                 key,
                 list: read,
             }| {
                let mut seen = HashSet::new();
                read.iter()
                    .find(|value| !seen.insert(id(value)))
                    .map(|value| {
                        single(
                            Anomaly::DuplicateAppend,
                            vec![*call],
                            Observation::DuplicateAppend {
                                call: *call,
                                key: key.clone(),
                                read: read.clone(),
                                value: value.clone(),
                            },
                        )
                    })
            },
        )
    }

    /// Returns the first committed read of a value that an aborted call
    /// appended.
    fn aborted_read(&self) -> Option<Instance> {
        self.reads.iter().find_map(
            |Read {
                 call,
                 key,
                 list: read,
             }| {
                read.iter().find_map(|value| {
                    let appender = *self.appender.get(&(id(key), id(value)))?;
                    self.aborted.contains(&appender).then(|| {
                        single(
                            Anomaly::AbortedRead,
                            vec![*call, appender],
                            Observation::AbortedRead {
                                call: *call,
                                key: key.clone(),
                                value: value.clone(),
                                appender,
                            },
                        )
                    })
                })
            },
        )
    }

    /// Returns the first committed read that ends in an append that its
    /// appender followed with another. A transaction's read of its own
    /// appends is no intermediate read.
    fn intermediate_read(&self) -> Option<Instance> {
        self.reads.iter().find_map(
            |Read {
                 call,
                 key,
                 list: read,
             }| {
                let value = read.last()?;
                let name = (id(key), id(value));
                let appender = *self.appender.get(&name)?;
                let next = self.later.get(&name).filter(|_| appender != *call)?;
                Some(single(
                    Anomaly::IntermediateRead,
                    vec![*call, appender],
                    Observation::IntermediateRead {
                        call: *call,
                        key: key.clone(),
                        value: value.clone(),
                        appender,
                        next: next.clone(),
                    },
                ))
            },
        )
    }

    /// Returns each key's version order, in the order the keys first appear,
    /// and records the first key whose committed reads are not prefixes of
    /// one list as the incompatible order.
    fn version_orders(&mut self) -> Vec<(Canonical, Vec<Value>)> {
        let mut orders = Vec::new();
        for (key, stated) in &self.keys {
            let reads: Vec<&Read> = self
                .reads
                .iter()
                .filter(|read| id(&read.key) == *key)
                .collect();
            match longest(&reads) {
                Ok(order) => orders.push((key.clone(), order)),
                Err(pair) => {
                    let (first, second) = *pair;
                    if self.incompatible.is_none() {
                        let mut calls = vec![first.call];
                        if second.call != first.call {
                            calls.push(second.call);
                        }
                        self.incompatible = Some(single(
                            Anomaly::IncompatibleOrder,
                            calls,
                            Observation::IncompatibleOrder {
                                calls: [first.call, second.call],
                                key: stated.clone(),
                                reads: [first.list, second.list],
                            },
                        ));
                    }
                }
            }
        }
        orders
    }

    /// Returns the committed transaction that appended `value` to the key of
    /// identity `key`.
    fn writer(&self, key: &Canonical, value: &Value) -> Option<usize> {
        self.appender
            .get(&(key.clone(), id(value)))
            .copied()
            .filter(|appender| self.committed.contains(appender))
    }

    /// Derives the dependencies between committed transactions, key by key.
    /// The transaction that appended the last value of a version order, and
    /// every read that returned the whole order, precede each committed
    /// append that no committed read observed.
    fn derive(&mut self) {
        let orders = std::mem::take(&mut self.orders);
        for (key, order) in &orders {
            let stated = self
                .keys
                .iter()
                .find(|(held, _)| held == key)
                .map_or(Value::Null, |(_, stated)| stated.clone());
            let present: HashSet<Canonical> = order.iter().map(id).collect();
            let versions = Versions {
                key: &stated,
                order,
                writers: order.iter().map(|value| self.writer(key, value)).collect(),
                unobserved: self
                    .appends
                    .get(key)
                    .map(Vec::as_slice)
                    .unwrap_or_default()
                    .iter()
                    .filter(|(call, value)| {
                        self.committed.contains(call) && !present.contains(&id(value))
                    })
                    .cloned()
                    .collect(),
            };
            let mut edges = Vec::new();
            for at in 0..order.len().saturating_sub(1) {
                edges.push((
                    versions.writers[at],
                    versions.writers[at + 1],
                    Relation::Ww,
                    Some(order[at].clone()),
                    Some(order[at + 1].clone()),
                ));
            }
            for (call, value) in &versions.unobserved {
                edges.push((
                    versions.writers.last().copied().flatten(),
                    Some(*call),
                    Relation::Ww,
                    order.last().cloned(),
                    Some(value.clone()),
                ));
            }
            let reads: Vec<Read> = self
                .reads
                .iter()
                .filter(|read| id(&read.key) == *key)
                .cloned()
                .collect();
            for read in &reads {
                edges.extend(read_edges(read, &versions));
            }
            for (from, to, relation, value, next) in edges {
                self.edge(from, to, relation, versions.key, value, next);
            }
        }
        self.orders = orders;
    }

    /// Adds the evidence of a dependency of `to` on `from`, when both are
    /// committed and differ.
    fn edge(
        &mut self,
        from: Option<usize>,
        to: Option<usize>,
        relation: Relation,
        key: &Value,
        value: Option<Value>,
        next: Option<Value>,
    ) {
        let (Some(from), Some(to)) = (from, to) else {
            return;
        };
        if from == to {
            return;
        }
        self.edges.entry((from, to)).or_default().push(Edge {
            from,
            to,
            relation,
            key: key.clone(),
            value,
            next,
        });
    }

    /// Returns the evidence of the edge from `a` to `b`.
    fn evidence(&self, a: usize, b: usize) -> &[Edge] {
        self.edges
            .get(&(a, b))
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    /// Returns each transaction's targets over the edges of `relations`, in
    /// ascending order.
    fn graph(&self, relations: &[Relation]) -> Graph {
        let mut out: Graph = HashMap::new();
        for (&(from, to), evidence) in &self.edges {
            if evidence
                .iter()
                .any(|edge| relations.contains(&edge.relation))
            {
                out.entry(from).or_default().push(to);
            }
        }
        for targets in out.values_mut() {
            targets.sort_unstable();
        }
        out
    }

    /// Returns `true` when the edge from `a` to `b` has `relation`.
    fn has(&self, a: usize, b: usize, relation: Relation) -> bool {
        self.evidence(a, b)
            .iter()
            .any(|edge| edge.relation == relation)
    }

    /// Returns the committed transactions in ascending order.
    fn nodes(&self) -> Vec<usize> {
        let mut nodes: Vec<usize> = self.committed.iter().copied().collect();
        nodes.sort_unstable();
        nodes
    }

    /// Returns the components of `around`, each with its transactions and
    /// its edges of `relation`, in the order of the components, then of the
    /// edges' sources, then of their targets.
    fn closing(&self, around: &Graph, relation: Relation) -> Vec<Component> {
        let mut found = Vec::new();
        for component in components(&self.nodes(), around) {
            let members: HashSet<usize> = component.iter().copied().collect();
            let mut edges = Vec::new();
            for &a in &component {
                for &b in around.get(&a).map(Vec::as_slice).unwrap_or_default() {
                    if members.contains(&b) && self.has(a, b, relation) {
                        edges.push((a, b));
                    }
                }
            }
            found.push((members, edges));
        }
        found
    }

    /// Returns the first cycle of a kind, as the kind's search finds it. The
    /// search takes the components of the graph over the component
    /// relations. In each, it tries the edges of the closing relation in the
    /// order of their sources and then their targets. An edge closes a cycle
    /// when a path over the path relations leads from its target back to its
    /// source. The closing edge of a G-single cycle counts as its one
    /// read-write dependency.
    fn cycle(&self, anomaly: Anomaly, search: &CycleSearch) -> Option<Instance> {
        let around = self.graph(search.component);
        let back = self.graph(search.path);
        let closing = [search.closing];
        let first: &[Relation] = if anomaly == Anomaly::GSingle {
            &closing
        } else {
            search.component
        };
        for (members, edges) in self.closing(&around, search.closing) {
            for (a, b) in edges {
                let Some(found) = path(b, a, &back, &members) else {
                    continue;
                };
                let mut used = vec![first];
                used.extend(found[1..].iter().map(|_| search.path));
                let mut nodes = vec![a];
                nodes.extend_from_slice(&found[..found.len() - 1]);
                return Some(self.cycle_instance(anomaly, &nodes, &used));
            }
        }
        None
    }

    /// Returns the first G-nonadjacent cycle. The search tries each
    /// component's rw edges in the order of their sources and then their
    /// targets. From an edge's target it takes the shortest walk back to the
    /// edge's source on which no rw edge follows another, that takes one more
    /// rw edge at least, and whose last edge is no rw edge. It reduces the
    /// closed walk to a simple cycle with no two adjacent rw edges, and
    /// reports that cycle when it has two rw edges or more.
    fn nonadjacent(&self) -> Option<Instance> {
        let around = self.graph(&RELATIONS);
        for (members, edges) in self.closing(&around, Relation::Rw) {
            for (a, b) in edges {
                let Some(steps) = self.alternating(b, a, &around, &members) else {
                    continue;
                };
                let mut walk: Walk = vec![(a, Relation::Rw)];
                let mut nodes = vec![b];
                nodes.extend(steps[..steps.len() - 1].iter().map(|(node, _)| *node));
                walk.extend(
                    nodes
                        .iter()
                        .zip(&steps)
                        .map(|(node, (_, relation))| (*node, *relation)),
                );
                let cycle = simple(walk);
                if cycle
                    .iter()
                    .filter(|(_, relation)| *relation == Relation::Rw)
                    .count()
                    < NONADJACENT_RW
                {
                    continue;
                }
                let nodes: Vec<usize> = cycle.iter().map(|(node, _)| *node).collect();
                let used: Vec<&[Relation]> = cycle
                    .iter()
                    .map(|(_, relation)| {
                        if *relation == Relation::Rw {
                            RW
                        } else {
                            NON_RW
                        }
                    })
                    .collect();
                return Some(self.cycle_instance(Anomaly::GNonadjacent, &nodes, &used));
            }
        }
        None
    }

    /// Returns the shortest alternating walk from `start` to `goal`. An rw
    /// edge leads into `start`. The search tries each transaction's edges in
    /// the order of their targets and of the relations.
    fn alternating(
        &self,
        start: usize,
        goal: usize,
        around: &Graph,
        members: &HashSet<usize>,
    ) -> Option<Vec<(usize, Relation)>> {
        let origin: State = (start, true, false);
        let mut parent: HashMap<State, Option<(State, Relation)>> = HashMap::from([(origin, None)]);
        let mut queue = VecDeque::from([origin]);
        while let Some(state) = queue.pop_front() {
            let (node, after_rw, taken) = state;
            if node == goal && !after_rw && taken {
                return Some(walk_to(state, &parent));
            }
            for (following, relation) in self.successors(state, around, members) {
                if parent.contains_key(&following) {
                    continue;
                }
                parent.insert(following, Some((state, relation)));
                queue.push_back(following);
            }
        }
        None
    }

    /// Returns the states that follow `state` on an alternating walk inside
    /// `members`, each with the relation of its edge, in the order of the
    /// targets and of the relations. An rw edge never follows an rw edge.
    fn successors(
        &self,
        state: State,
        around: &Graph,
        members: &HashSet<usize>,
    ) -> Vec<(State, Relation)> {
        let (node, after_rw, taken) = state;
        let mut found = Vec::new();
        for &target in around.get(&node).map(Vec::as_slice).unwrap_or_default() {
            if !members.contains(&target) {
                continue;
            }
            for relation in self.relations(node, target) {
                if relation == Relation::Rw && after_rw {
                    continue;
                }
                let rw = relation == Relation::Rw;
                found.push(((target, rw, taken || rw), relation));
            }
        }
        found
    }

    /// Returns the relations of the edge from `a` to `b`, in relation order.
    fn relations(&self, a: usize, b: usize) -> Vec<Relation> {
        let present: HashSet<Relation> = self
            .evidence(a, b)
            .iter()
            .map(|edge| edge.relation)
            .collect();
        RELATIONS
            .into_iter()
            .filter(|relation| present.contains(relation))
            .collect()
    }

    /// Returns the instance of a cycle through `nodes` in order, back to the
    /// first. `used` gives, for the edge from each transaction to the next,
    /// the relations that the search followed it by. The cycle lists the
    /// edge's relations among them, and the explanation the first evidence of
    /// one.
    fn cycle_instance(&self, anomaly: Anomaly, nodes: &[usize], used: &[&[Relation]]) -> Instance {
        let mut cycle = Vec::with_capacity(nodes.len());
        let mut explanation = Vec::with_capacity(nodes.len());
        for (position, &node) in nodes.iter().enumerate() {
            let following = nodes[(position + 1) % nodes.len()];
            let evidence: Vec<&Edge> = self
                .evidence(node, following)
                .iter()
                .filter(|edge| used[position].contains(&edge.relation))
                .collect();
            let relations: HashSet<Relation> = evidence.iter().map(|edge| edge.relation).collect();
            cycle.push(Link {
                call: node,
                relations: RELATIONS
                    .into_iter()
                    .filter(|relation| relations.contains(relation))
                    .collect(),
            });
            if let Some(first) = evidence.first() {
                explanation.push(Evidence::Edge((*first).clone()));
            }
        }
        Instance {
            anomaly,
            calls: nodes.to_vec(),
            cycle: Some(cycle),
            explanation,
        }
    }
}

/// Returns the write-read and the read-write dependencies of one committed
/// read, as the source, the target, the relation and the values of each.
fn read_edges(read: &Read, versions: &Versions<'_>) -> Vec<ReadEdge> {
    let seen = read.list.len();
    let last = read.list.last().cloned();
    let mut edges = Vec::new();
    if seen > 0 {
        edges.push((
            versions.writers.get(seen - 1).copied().flatten(),
            Some(read.call),
            Relation::Wr,
            last.clone(),
            None,
        ));
    }
    let following: Vec<(Option<usize>, Value)> = if seen < versions.order.len() {
        vec![(versions.writers[seen], versions.order[seen].clone())]
    } else {
        versions
            .unobserved
            .iter()
            .map(|(call, value)| (Some(*call), value.clone()))
            .collect()
    };
    for (writer, value) in following {
        edges.push((
            Some(read.call),
            writer,
            Relation::Rw,
            last.clone(),
            Some(value),
        ));
    }
    edges
}
