//! The case tree: the choice sequences that a run generated, as a trie.
//!
//! A node is one choice request, recorded with its bounds, and an edge is the
//! value that the request took. A case that ends marks its last node as a
//! leaf, with how it ended. A walk follows one case down the tree and finds:
//!
//! - A repeated case. A body is a function of its choices, so a choice that
//!   arrives at a leaf would end the body as that leaf's case ended. A walk
//!   stops there, so it never makes a request where an earlier case ended.
//! - A diverging body: a request whose bounds differ from the request that
//!   the tree recorded at the same position, or an end where an earlier case
//!   made a request.
//! - An exhausted domain. A node is exhausted when it is a leaf, or when
//!   every value that its bounds allow leads to an exhausted node. A float
//!   choice never exhausts, and neither does a sequence without a maximum
//!   size.
//!
//! The tree stops growing at `NODE_LIMIT` nodes. A walk that would add a node
//! past the limit stops checking for the rest of its case, and the tree no
//! longer reports an exhausted domain.

use super::case::Request;
use super::choice::{Bounds, Choice, NAN_BITS};
use std::collections::HashMap;

/// The most nodes that a tree holds.
const NODE_LIMIT: usize = 1 << 20;

/// How a case that reached its end ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ending {
    Passed,
    Failed,
    Rejected,
}

/// How a walk stops a case.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Stopped {
    /// The choice arrives at a leaf.
    Repeated,
    /// The body requested other bounds than the tree recorded at `index`, or
    /// ended there. `requested` is `None` for a case that ended.
    Diverged {
        index: usize,
        recorded: Bounds,
        requested: Option<Bounds>,
    },
}

/// The key of an edge: a float by its bits, with one NaN.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Key {
    Integer(i128),
    Float(u64),
    Sequence(Vec<u64>),
}

fn key(value: &Choice) -> Key {
    match value {
        Choice::Integer(held) => Key::Integer(*held),
        Choice::Float(held) => Key::Float(if held.is_nan() {
            NAN_BITS
        } else {
            held.to_bits()
        }),
        Choice::Sequence(held) => Key::Sequence(held.clone()),
    }
}

/// Returns the number of values that the bounds allow, or `None` when it is
/// unbounded. A float's count is never taken, so a float choice never
/// exhausts.
fn size(bounds: &Bounds) -> Option<u128> {
    match bounds {
        Bounds::Integer(bounds) => Some(bounds.size()),
        Bounds::Sequence(bounds) => {
            let max = bounds.max_size()?;
            let k = u128::from(bounds.k());
            let mut total: u128 = 0;
            let mut power: u128 = 1;
            for length in 0..=max {
                if length >= bounds.min_size() {
                    total = total.saturating_add(power);
                }
                power = power.saturating_mul(k);
            }
            Some(total)
        }
        Bounds::Float(_) => None,
    }
}

/// One position of the tree: the request made there, or the case's end.
#[derive(Debug, Default)]
struct Node {
    bounds: Option<Bounds>,
    ending: Option<Ending>,
    exhausted: bool,
    children: HashMap<Key, usize>,
}

/// Every case that the simplest, edge and random phases generated.
#[derive(Debug)]
pub(crate) struct Tree {
    nodes: Vec<Node>,
    limit: usize,
    full: bool,
}

impl Default for Tree {
    fn default() -> Self {
        Self::new(NODE_LIMIT)
    }
}

impl Tree {
    /// Returns a tree with a root and nothing recorded, growing to `limit`
    /// nodes.
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            nodes: vec![Node::default()],
            limit,
            full: false,
        }
    }

    /// Returns `true` when the run has tested every input of its domain.
    pub(crate) fn exhausted(&self) -> bool {
        !self.full && self.nodes[0].exhausted
    }

    /// Recomputes whether a node of a walk's path is exhausted from its
    /// children. Every node of a path has a request or an ending.
    fn settle(&mut self, at: usize) {
        let node = &self.nodes[at];
        let exhausted = node.ending.is_some()
            || node.bounds.as_ref().and_then(size).is_some_and(|count| {
                u128::try_from(node.children.len()).unwrap_or(u128::MAX) == count
                    && node
                        .children
                        .values()
                        .all(|child| self.nodes[*child].exhausted)
            });
        self.nodes[at].exhausted = exhausted;
    }
}

/// One case's path down the tree.
#[derive(Debug)]
pub(crate) struct Walker {
    path: Vec<usize>,
    off: bool,
}

impl Walker {
    /// Returns a walk from the root.
    pub(crate) fn new() -> Self {
        Self {
            path: vec![0],
            off: false,
        }
    }

    /// Follows the choice at `index`.
    pub(crate) fn step(
        &mut self,
        tree: &mut Tree,
        index: usize,
        request: &Request,
        value: &Choice,
    ) -> Result<(), Stopped> {
        if self.off {
            return Ok(());
        }
        let at = *self.path.last().unwrap_or(&0);
        match &tree.nodes[at].bounds {
            None => tree.nodes[at].bounds = Some(request.bounds.clone()),
            Some(recorded) if *recorded != request.bounds => {
                return Err(Stopped::Diverged {
                    index,
                    recorded: recorded.clone(),
                    requested: Some(request.bounds.clone()),
                });
            }
            Some(_) => {}
        }
        let edge = key(value);
        let child = if let Some(child) = tree.nodes[at].children.get(&edge) {
            *child
        } else {
            if tree.nodes.len() >= tree.limit {
                tree.full = true;
                self.off = true;
                return Ok(());
            }
            tree.nodes.push(Node::default());
            let child = tree.nodes.len() - 1;
            tree.nodes[at].children.insert(edge, child);
            child
        };
        if tree.nodes[child].ending.is_some() {
            return Err(Stopped::Repeated);
        }
        self.path.push(child);
        Ok(())
    }

    /// Marks where the case ended as a leaf.
    pub(crate) fn end(&mut self, tree: &mut Tree, ending: Ending) -> Result<(), Stopped> {
        if self.off {
            return Ok(());
        }
        let at = *self.path.last().unwrap_or(&0);
        if let Some(recorded) = &tree.nodes[at].bounds {
            return Err(Stopped::Diverged {
                index: self.path.len() - 1,
                recorded: recorded.clone(),
                requested: None,
            });
        }
        tree.nodes[at].ending = Some(ending);
        for passed in self.path.iter().rev() {
            tree.settle(*passed);
        }
        Ok(())
    }
}
