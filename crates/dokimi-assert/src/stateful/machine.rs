//! Machines: the sequential specification of a subject, and the actions that
//! the steps of a case take on it.

use super::scheduler::Local;
use crate::history::Spec;
use crate::prop::Case;
use std::collections::HashSet;
use std::fmt;

/// The run of one step, planned with its input: it calls the subject on a
/// client and returns the future of the call.
pub(crate) type Planned<'s> = Box<dyn FnOnce(&'s Case<'s>, usize) -> Local<'s> + Send + 's>;

/// Whether an action may take a step in a state of the spec.
pub(crate) type Enabled<'a, S> = Box<dyn Fn(&S) -> bool + Send + Sync + 'a>;

/// A check of a case in a state of the spec.
pub(crate) type Check<'a, S> = Box<dyn Fn(&Case<'_>, &S) + 'a>;

/// The input and the run of an action, with the type of its input erased.
trait Plan<S>: Send + Sync {
    /// Requests the input of a step from the case in `state`, and returns the
    /// run of the step with that input.
    fn plan<'s>(&'s self, case: &Case<'_>, state: &S) -> Planned<'s>;
}

/// The plan of an action whose steps take no input.
struct Plain<F> {
    run: F,
}

impl<S, F> Plan<S> for Plain<F>
where
    F: AsyncFn(&Case<'_>, usize) + Send + Sync,
{
    fn plan<'s>(&'s self, _: &Case<'_>, _: &S) -> Planned<'s> {
        Box::new(move |case, client| Box::pin((self.run)(case, client)))
    }
}

/// The plan of an action whose steps take an input of type `I`.
struct Typed<G, F> {
    input: G,
    run: F,
}

impl<S, I, G, F> Plan<S> for Typed<G, F>
where
    I: Send + 'static,
    G: Fn(&Case<'_>, &S) -> I + Send + Sync,
    F: AsyncFn(&Case<'_>, usize, I) + Send + Sync,
{
    fn plan<'s>(&'s self, case: &Case<'_>, state: &S) -> Planned<'s> {
        let input = (self.input)(case, state);
        Box::new(move |case, client| Box::pin((self.run)(case, client, input)))
    }
}

/// One named action of a [`Machine`].
///
/// A step of the action calls the subject on a client and records each call
/// in the case's history. Its run is an async closure, because a step of a
/// concurrent section can run as a task of a [`Scheduler`](super::Scheduler).
/// A step outside a task runs to its end before the next step starts.
pub struct Action<'a, S> {
    pub(crate) name: String,
    pub(crate) weight: u64,
    pub(crate) drain: bool,
    pub(crate) enabled: Option<Enabled<'a, S>>,
    plan: Box<dyn Plan<S> + 'a>,
}

impl<S> fmt::Debug for Action<'_, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Action")
            .field("name", &self.name)
            .field("weight", &self.weight)
            .field("drain", &self.drain)
            .field("enabled", &self.enabled.is_some())
            .finish_non_exhaustive()
    }
}

impl<'a, S: 'a> Action<'a, S> {
    /// Returns the action named `name`, whose step `run` calls the subject
    /// on a client, without an input.
    pub fn new(name: &str, run: impl AsyncFn(&Case<'_>, usize) + Send + Sync + 'a) -> Self {
        Self {
            name: name.to_owned(),
            weight: 1,
            drain: false,
            enabled: None,
            plan: Box::new(Plain { run }),
        }
    }

    /// Returns the action named `name`, whose step requests its input from
    /// the case with `input`, such as a draw, and then `run` calls the
    /// subject on a client with the input. The bounds of the input's requests
    /// must not depend on results that the subject can return differently on
    /// a replay of the case.
    pub fn with_input<I: Send + 'static>(
        name: &str,
        input: impl Fn(&Case<'_>, &S) -> I + Send + Sync + 'a,
        run: impl AsyncFn(&Case<'_>, usize, I) + Send + Sync + 'a,
    ) -> Self {
        Self {
            name: name.to_owned(),
            weight: 1,
            drain: false,
            enabled: None,
            plan: Box::new(Typed { input, run }),
        }
    }

    /// States how often a random step takes the action, relative to the other
    /// actions that the step lists. A weight of 0 counts as 1, the default.
    #[must_use]
    pub fn weight(mut self, weight: u64) -> Self {
        self.weight = weight.max(1);
        self
    }

    /// Makes the action take the steps of the drain, whether the swarm kept
    /// it or not.
    #[must_use]
    pub fn drain(mut self) -> Self {
        self.drain = true;
        self
    }

    /// States whether a sequential or a drain step may take the action in a
    /// state. An action without it is enabled in every state, and only such
    /// an action takes the steps of a concurrent section. What it returns
    /// must not depend on results that the subject can return differently on
    /// a replay of the case.
    #[must_use]
    pub fn enabled(mut self, enabled: impl Fn(&S) -> bool + Send + Sync + 'a) -> Self {
        self.enabled = Some(Box::new(enabled));
        self
    }

    /// Returns the name of the action.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Requests the input of a step and returns its run.
    pub(crate) fn plan<'s>(&'s self, case: &Case<'_>, state: &S) -> Planned<'s> {
        self.plan.plan(case, state)
    }
}

/// The sequential specification of a subject and the actions that the steps
/// of a case take on it. `S` is the type of the spec's state.
///
/// The functions of a machine and of its actions receive a state of the
/// spec: the first of the states that the last check of the history left. A
/// machine without a spec receives `S::default()`.
///
/// Those states follow what the subject returned, and a replay of a case
/// requests the same choices only when its steps list the same actions and
/// its requests state the same bounds. When a subject can return other
/// results on a replay, as one that refuses a call for room does, the
/// actions' `enabled` and input read a state of the machine's own in place
/// of the spec's: one that only the inputs of its steps change. Otherwise a
/// replay can take other steps than the case took, and the run ends as
/// flaky.
pub struct Machine<'a, S> {
    pub(crate) actions: Vec<Action<'a, S>>,
    pub(crate) spec: Option<Box<dyn Spec<State = S> + 'a>>,
    pub(crate) invariant: Option<Check<'a, S>>,
    pub(crate) settle: Option<Check<'a, S>>,
}

impl<S> fmt::Debug for Machine<'_, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Machine")
            .field("actions", &self.actions)
            .field("spec", &self.spec.is_some())
            .field("invariant", &self.invariant.is_some())
            .field("settle", &self.settle.is_some())
            .finish()
    }
}

impl<'a, S> Machine<'a, S> {
    /// Returns the machine of `actions`, in order, the simpler first,
    /// without a spec.
    ///
    /// # Panics
    ///
    /// Panics for two actions with one name.
    pub fn new(actions: impl IntoIterator<Item = Action<'a, S>>) -> Self {
        let actions: Vec<Action<'a, S>> = actions.into_iter().collect();
        let mut names = HashSet::new();
        for action in &actions {
            assert!(
                names.insert(action.name.as_str()),
                "stateful: Machine::new of actions that name {:?} twice",
                action.name
            );
        }
        Self {
            actions,
            spec: None,
            invariant: None,
            settle: None,
        }
    }

    /// States the sequential specification that the history of the steps is
    /// checked against after every step, with every call in one partition.
    #[must_use]
    pub fn spec(mut self, spec: impl Spec<State = S> + 'a) -> Self {
        self.spec = Some(Box::new(spec));
        self
    }

    /// States a check that runs after setup, after every sequential and
    /// drain step, and after settle.
    #[must_use]
    pub fn invariant(mut self, invariant: impl Fn(&Case<'_>, &S) + 'a) -> Self {
        self.invariant = Some(Box::new(invariant));
        self
    }

    /// States a check that runs once after the drain, of what must be true
    /// once the subject has recovered, such as that every accepted write is
    /// readable.
    #[must_use]
    pub fn settle(mut self, settle: impl Fn(&Case<'_>, &S) + 'a) -> Self {
        self.settle = Some(Box::new(settle));
        self
    }
}
