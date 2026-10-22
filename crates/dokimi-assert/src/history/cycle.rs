//! The searches of the dependency graph of committed transactions: its
//! strongly connected components, the shortest path within one, and the
//! reduction of a closed walk to a simple cycle.
//!
//! A graph is the targets of each transaction, in ascending order, so every
//! search visits the edges of a transaction in the order of their targets.

use std::collections::{HashMap, HashSet, VecDeque};

/// A dependency of one committed transaction on another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Relation {
    /// A write-write dependency.
    Ww,
    /// A write-read dependency.
    Wr,
    /// A read-write dependency.
    Rw,
}

/// The relations in the order in which a cycle's entry lists them.
pub(crate) const RELATIONS: [Relation; 3] = [Relation::Ww, Relation::Wr, Relation::Rw];

impl Relation {
    /// Returns the definition's spelling of the relation.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Ww => "ww",
            Self::Wr => "wr",
            Self::Rw => "rw",
        }
    }
}

/// The targets of each transaction, in ascending order.
pub(crate) type Graph = HashMap<usize, Vec<usize>>;

/// A closed walk: each transaction, and the relation of the edge that leaves
/// it.
pub(crate) type Walk = Vec<(usize, Relation)>;

/// The state of Tarjan's search over one graph.
struct Tarjan<'g> {
    graph: &'g Graph,
    order: HashMap<usize, usize>,
    low: HashMap<usize, usize>,
    stack: Vec<usize>,
    on_stack: HashSet<usize>,
    /// The transactions whose edges the search is visiting, each with the
    /// position of its next edge.
    work: Vec<(usize, usize)>,
    found: Vec<Vec<usize>>,
}

impl Tarjan<'_> {
    fn enter(&mut self, node: usize) {
        let at = self.order.len();
        self.order.insert(node, at);
        self.low.insert(node, at);
        self.stack.push(node);
        self.on_stack.insert(node);
        self.work.push((node, 0));
    }

    fn lower(&mut self, node: usize, value: usize) {
        if let Some(low) = self.low.get_mut(&node) {
            *low = (*low).min(value);
        }
    }

    fn leave(&mut self, node: usize) {
        self.work.pop();
        let low = self.low[&node];
        if let Some(&(parent, _)) = self.work.last() {
            self.lower(parent, low);
        }
        if low != self.order[&node] {
            return;
        }
        let from = self
            .stack
            .iter()
            .rposition(|held| *held == node)
            .unwrap_or_default();
        let mut component = self.stack.split_off(from);
        for member in &component {
            self.on_stack.remove(member);
        }
        if component.len() > 1 {
            component.sort_unstable();
            self.found.push(component);
        }
    }

    /// Visits the edges of the transactions on the work stack until it
    /// empties.
    fn visit(&mut self) {
        while let Some(&(node, next)) = self.work.last() {
            let target = self
                .graph
                .get(&node)
                .and_then(|targets| targets.get(next))
                .copied();
            if let Some(top) = self.work.last_mut() {
                top.1 = next + 1;
            }
            match target {
                None => self.leave(node),
                Some(target) if !self.order.contains_key(&target) => self.enter(target),
                Some(target) => {
                    if self.on_stack.contains(&target) {
                        let order = self.order[&target];
                        self.lower(node, order);
                    }
                }
            }
        }
    }
}

/// Returns the strongly connected components of two or more transactions,
/// by Tarjan's algorithm without recursion. Each component lists its
/// transactions in ascending order, and the components come in the order of
/// their lowest transactions.
pub(crate) fn components(nodes: &[usize], graph: &Graph) -> Vec<Vec<usize>> {
    let mut tarjan = Tarjan {
        graph,
        order: HashMap::new(),
        low: HashMap::new(),
        stack: Vec::new(),
        on_stack: HashSet::new(),
        work: Vec::new(),
        found: Vec::new(),
    };
    for &root in nodes {
        if !tarjan.order.contains_key(&root) {
            tarjan.enter(root);
            tarjan.visit();
        }
    }
    let mut found = tarjan.found;
    found.sort_unstable_by_key(|component| component[0]);
    found
}

/// Returns the shortest path from `start` to `goal` within `members`, or
/// `None`. Of two shortest paths it returns the one that comes first in the
/// order of the targets.
pub(crate) fn path(
    start: usize,
    goal: usize,
    graph: &Graph,
    members: &HashSet<usize>,
) -> Option<Vec<usize>> {
    let mut parent: HashMap<usize, usize> = HashMap::from([(start, start)]);
    let mut queue = VecDeque::from([start]);
    while let Some(node) = queue.pop_front() {
        if node == goal {
            let mut found = vec![node];
            let mut at = node;
            while at != start {
                at = parent[&at];
                found.push(at);
            }
            found.reverse();
            return Some(found);
        }
        for &target in graph.get(&node).map(Vec::as_slice).unwrap_or_default() {
            if members.contains(&target) && !parent.contains_key(&target) {
                parent.insert(target, node);
                queue.push_back(target);
            }
        }
    }
    None
}

/// Returns the two positions of the first transaction that the walk visits
/// twice.
fn first_repeat(walk: &[(usize, Relation)]) -> Option<(usize, usize)> {
    let mut seen: HashMap<usize, usize> = HashMap::new();
    for (position, (node, _)) in walk.iter().enumerate() {
        if let Some(&first) = seen.get(node) {
            return Some((first, position));
        }
        seen.insert(*node, position);
    }
    None
}

/// Returns `true` when no read-write edge of a closed walk follows another.
pub(crate) fn alternates(walk: &[(usize, Relation)]) -> bool {
    !walk.iter().enumerate().any(|(at, (_, relation))| {
        *relation == Relation::Rw && walk[(at + 1) % walk.len()].1 == Relation::Rw
    })
}

/// Returns a simple cycle of a closed walk with no two adjacent read-write
/// edges. While a transaction occurs twice, the walk splits at the first
/// repeat into the part between the two occurrences and the rest. It keeps
/// the part between them when that part alternates, and the rest otherwise.
/// The cycle starts at its first read-write edge.
pub(crate) fn simple(walk: Walk) -> Walk {
    let mut current = walk;
    while let Some((first, second)) = first_repeat(&current) {
        let inner = current[first..second].to_vec();
        current = if alternates(&inner) {
            inner
        } else {
            let mut rest = current[..first].to_vec();
            rest.extend_from_slice(&current[second..]);
            rest
        };
    }
    let start = current
        .iter()
        .position(|(_, relation)| *relation == Relation::Rw)
        .unwrap_or(0);
    current.rotate_left(start);
    current
}
