//! The sequential specification of an object, against which a check reads a
//! history, and a call as the specification sees it.

use crate::prop::engine::canonical::canonical;
use crate::value::Value;
use std::fmt::Debug;
use std::hash::{DefaultHasher, Hash, Hasher};

/// A call as a [`Spec`] sees it: its operation and its arguments, and its
/// output when the call completed as ok.
#[derive(Debug, Clone, PartialEq)]
pub struct Operation {
    name: String,
    args: Vec<Value>,
    output: Option<Value>,
}

impl Operation {
    /// Returns the operation of a call, with its output when the call
    /// completed as ok.
    pub(crate) fn new(name: String, args: Vec<Value>, output: Option<Value>) -> Self {
        Self { name, args, output }
    }

    /// Returns the call's operation.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the call's arguments.
    #[must_use]
    pub fn args(&self) -> &[Value] {
        &self.args
    }

    /// Returns `true` when the call completed as ok. A call whose outcome is
    /// unknown, and a pending call, is not known.
    #[must_use]
    pub fn known(&self) -> bool {
        self.output.is_some()
    }

    /// Returns what the call returned, for a known call.
    #[must_use]
    pub fn output(&self) -> Option<&Value> {
        self.output.as_ref()
    }

    /// Returns `true` when the call may have returned `value`. A call that
    /// is not known may have taken effect with any output, so it may have
    /// returned any value. A known call returned `value` when its output
    /// equals it, as `==` compares two values.
    ///
    /// ```
    /// use dokimi_assert::Value;
    /// use dokimi_assert::history::{Operation, Spec};
    ///
    /// /// A register: a write stores its argument, and a read returns it.
    /// struct Register;
    ///
    /// impl Spec for Register {
    ///     type State = Value;
    ///
    ///     fn initial(&self) -> Value {
    ///         Value::Null
    ///     }
    ///
    ///     fn next(&self, state: &Value, op: &Operation) -> Vec<Value> {
    ///         match op.name() {
    ///             "write" => op.args().to_vec(),
    ///             _ if op.returned(state.clone()) => vec![state.clone()],
    ///             _ => Vec::new(),
    ///         }
    ///     }
    /// }
    /// ```
    #[must_use]
    pub fn returned(&self, value: impl Into<Value>) -> bool {
        self.output
            .as_ref()
            .is_none_or(|output| *output == value.into())
    }
}

/// The sequential specification of an object: its state before any call,
/// and the states that an operation may leave.
///
/// [`history::is_linearizable`](super::is_linearizable) searches for an
/// order of a history's calls that respects their real-time order and that
/// the spec accepts. The search merges two configurations whose states
/// [`Spec::equal`] reports equal, so `equal` must be exact: an equality
/// coarser than the spec's meaning turns a linearizable history into a
/// violated one. [`Spec::hash`] must agree with `equal`, because the search
/// compares only the states of one hash.
///
/// # Concurrency
///
/// Under [`workers`](super::workers) the search runs on more than one thread,
/// so a spec is `Sync` and its states are `Send`. A spec of pure functions
/// is.
pub trait Spec: Sync {
    /// The state of the object.
    type State: Clone + PartialEq + Debug + Send + Sync + 'static;

    /// Returns the state before any call.
    fn initial(&self) -> Self::State;

    /// Returns the states that may follow `state` when `operation` takes
    /// effect, in the order that the search tries them, and none when the
    /// spec rejects the operation in `state`. For a call whose outcome is
    /// unknown, it returns the states that the call leaves when it takes
    /// effect.
    fn next(&self, state: &Self::State, operation: &Operation) -> Vec<Self::State>;

    /// Returns `true` when two states are interchangeable. The default
    /// compares them with `==`.
    fn equal(&self, a: &Self::State, b: &Self::State) -> bool {
        a == b
    }

    /// Returns a hash that every state equal to `state` shares. The default
    /// hashes every state alike, so the search compares the states of every
    /// configuration of one set of calls. A spec of a long history states a
    /// hash of its own.
    fn hash(&self, state: &Self::State) -> u64 {
        let _ = state;
        0
    }

    /// Returns the steps of the budget that a call of `next` from `state`
    /// counts for.
    #[doc(hidden)]
    fn cost(&self, state: &Self::State) -> u64 {
        let _ = state;
        1
    }
}

/// The spec that [`spec_from`] returns.
struct SpecFrom<F> {
    factory: F,
}

impl<F, S> Spec for SpecFrom<F>
where
    F: Fn() -> S + Sync,
    S: FnMut(&str, &[Value]) -> Value,
{
    type State = Vec<Operation>;

    fn initial(&self) -> Vec<Operation> {
        Vec::new()
    }

    fn next(&self, state: &Vec<Operation>, operation: &Operation) -> Vec<Vec<Operation>> {
        let mut subject = (self.factory)();
        for applied in state {
            subject(&applied.name, &applied.args);
        }
        if !operation.returned(subject(&operation.name, &operation.args)) {
            return Vec::new();
        }
        let mut following = state.clone();
        following.push(Operation::new(
            operation.name.clone(),
            operation.args.clone(),
            None,
        ));
        vec![following]
    }

    fn equal(&self, a: &Vec<Operation>, b: &Vec<Operation>) -> bool {
        a.len() == b.len()
            && a.iter().zip(b).all(|(x, y)| {
                x.name == y.name
                    && x.args.len() == y.args.len()
                    && x.args
                        .iter()
                        .zip(&y.args)
                        .all(|(p, q)| canonical(p) == canonical(q))
            })
    }

    fn hash(&self, state: &Vec<Operation>) -> u64 {
        let mut hasher = DefaultHasher::new();
        for operation in state {
            operation.name.hash(&mut hasher);
            for arg in &operation.args {
                canonical(arg).hash(&mut hasher);
            }
        }
        hasher.finish()
    }

    fn cost(&self, state: &Vec<Operation>) -> u64 {
        u64::try_from(state.len())
            .unwrap_or(u64::MAX)
            .saturating_add(1)
    }
}

/// Returns the spec that the subject's own sequential behaviour states. A
/// check against it finds a call of a history that was not atomic.
///
/// The spec's state is the list of the calls applied so far, each as an
/// operation without an output. `next` builds a subject with `factory`,
/// applies the calls of the state in order, then applies the operation, and
/// accepts it when the call may have returned the subject's output. Two
/// states are equal when they list the same operations with equal arguments,
/// in one order. A step from a state of d calls applies d + 1 calls, and
/// counts for d + 1 steps of the budget.
///
/// The subject must return the same outputs for the same calls. A subject
/// with hidden state returns other outputs on a replay, and the check then
/// reports a violation that did not happen.
pub fn spec_from<F, S>(factory: F) -> impl Spec<State = Vec<Operation>>
where
    F: Fn() -> S + Sync,
    S: FnMut(&str, &[Value]) -> Value,
{
    SpecFrom { factory }
}
