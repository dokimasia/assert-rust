// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The generator: a domain, and how a case decodes a value of it from its
//! choices.

use super::case::{Case, rendered};
use super::engine::canonical::literal_any;
use super::engine::case::{AsAny, Erased, MAX_CHOICES, Record, Rendered, Replaying, Signal, Stop};
use super::engine::choice::{Choice, IntegerBounds};
use super::engine::inverse::{self, CannotInvert, Emitted};
use crate::value::Value;
use std::fmt::{self, Debug};
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::Arc;

/// How one kind of generator decodes, renders and inverts its values. A flag
/// reads a node as the kind that it rebuilds through [`AsAny`].
pub(crate) trait Node<T>: Send + Sync + AsAny + 'static {
    /// Returns the definition's id of the generator, such as `integer`.
    fn id(&self) -> &'static str;

    /// Asks the case for choices and returns the value that they decode to.
    fn decode(&self, case: &Case<'_>) -> Result<T, Stop>;

    /// Returns the typed literal of a value of the generator, where the value
    /// has one. The default states the literal of a value of a type that the
    /// library knows, such as a [`Value`] or an integer.
    fn literal(&self, value: &T) -> Option<Value>
    where
        T: 'static,
    {
        literal_any(value)
    }

    /// Returns the steps that the generator's decode asks for to decode
    /// `value`, and the value that they decode to.
    fn emit(&self, value: &Value) -> Result<Emitted, CannotInvert> {
        let _ = value;
        Err(CannotInvert::no_inverse(self.id()))
    }

    /// Returns the bounds of an integer generator, through maps, whose draw
    /// the explain phase steps towards its target. Such a generator decodes
    /// its value from one integer choice.
    fn integer(&self) -> Option<IntegerBounds> {
        None
    }
}

/// A domain, and how a case decodes a value of it from its choices.
///
/// A generator never reads a random source. It asks its case for choices
/// with bounds, so one generator generates a value, replays a recorded value
/// and shrinks it. Cloning a generator is cheap: the clones share one
/// description, and a generator is safe to share between threads.
///
/// ```
/// use dokimi_assert::prop;
///
/// let digits = prop::list(prop::integer(0..=9_u8), 1..=3);
/// let words = prop::string(..=8).alphabet("ab");
/// let pairs = prop::integer(0..100).bind(|n| prop::integer(n..=100).map(move |m| (n, m)));
/// # let _ = (digits, words, pairs);
/// ```
pub struct Generator<T> {
    node: Arc<dyn Node<T>>,
}

impl<T> Clone for Generator<T> {
    fn clone(&self) -> Self {
        Self {
            node: Arc::clone(&self.node),
        }
    }
}

impl<T: 'static> Debug for Generator<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Generator").field(&self.node.id()).finish()
    }
}

impl<T: 'static> Generator<T> {
    /// Returns the generator of a node.
    pub(crate) fn from_node(node: impl Node<T>) -> Self {
        Self {
            node: Arc::new(node),
        }
    }

    /// Returns the generator of a node that its owner shares.
    pub(crate) fn from_shared(node: Arc<dyn Node<T>>) -> Self {
        Self { node }
    }

    /// Returns the node of the generator.
    pub(crate) fn node(&self) -> &dyn Node<T> {
        &*self.node
    }

    /// Returns the node of a flag's generator, which must be of the kind
    /// `N`.
    ///
    /// # Panics
    ///
    /// Panics when the generator is of another kind, with a message that
    /// names the flag.
    pub(crate) fn flagged<N: 'static>(&self, flag: &str) -> &N {
        // The node of the generator, not the `Arc` that holds it.
        let node: &dyn Node<T> = &*self.node;
        AsAny::as_any(node).downcast_ref::<N>().unwrap_or_else(|| {
            panic!(
                "prop: {flag} applies to no generator of the kind {}",
                self.node.id()
            )
        })
    }

    /// Asks `case` for choices and returns the value that they decode to.
    pub(crate) fn decode(&self, case: &Case<'_>) -> Result<T, Stop> {
        self.node.decode(case)
    }

    /// Returns the typed literal of a value of the generator.
    pub(crate) fn literal(&self, value: &T) -> Option<Value> {
        self.node.literal(value)
    }

    /// Returns the steps that decode to `value`, and the value that they
    /// decode to.
    pub(crate) fn emit(&self, value: &Value) -> Result<Emitted, CannotInvert> {
        self.node.emit(value)
    }

    /// Returns the choices that decode to `value`.
    pub(crate) fn invert(&self, value: &Value) -> Result<Vec<Choice>, CannotInvert> {
        inverse::invert(self, value)
    }

    /// Returns a generator of `f` applied to each value of this one.
    ///
    /// The result makes this generator's choices and opens no span of its
    /// own, so a value of it shrinks as the value that it was mapped from. A
    /// mapped integer states its nearest passing value as `f` of the value
    /// one step towards the target. A value of a mapped generator has no
    /// inverse, so an example of it states its value.
    #[must_use]
    pub fn map<U: 'static>(self, f: impl Fn(T) -> U + Send + Sync + 'static) -> Generator<U> {
        Generator::from_node(Mapped {
            of: self,
            f: Arc::new(f),
        })
    }

    /// Returns a generator of the values of this one that `keep` returns
    /// `true` for.
    ///
    /// Each attempt decodes in a span labelled `filter`. A rejected attempt is
    /// removed from the case's record, so a replay decodes the kept value at
    /// its first attempt. After three attempts that `keep` rejects, the
    /// filter rejects the case.
    #[must_use]
    pub fn filter(self, keep: impl Fn(&T) -> bool + Send + Sync + 'static) -> Self {
        Self::from_node(Filtered {
            of: self,
            keep: Arc::new(keep),
        })
    }

    /// Returns a generator that decodes a value of this one, then a value of
    /// the generator that `f` returns for it, in one span labelled `bind`.
    #[must_use]
    pub fn bind<U: 'static>(
        self,
        f: impl Fn(T) -> Generator<U> + Send + Sync + 'static,
    ) -> Generator<U> {
        Generator::from_node(Bound {
            of: self,
            f: Arc::new(f),
        })
    }
}

impl<T: Debug + 'static> Generator<T> {
    /// Returns a value of the generator as a report shows it.
    pub(crate) fn render(&self, value: &T) -> Rendered {
        rendered(value, self.node.literal(value))
    }
}

impl<T: Debug + 'static> Erased for Generator<T> {
    fn fill(&self, case: &Case<'_>) -> Result<(), Stop> {
        self.decode(case).map(drop)
    }

    fn integer(&self) -> Option<IntegerBounds> {
        self.node.integer()
    }

    /// Returns the value that the generator decodes from the one integer
    /// choice `choice`, as a report shows it.
    fn nearest(&self, choice: i128) -> Option<Rendered> {
        let replayed = Box::new(Replaying::new(vec![Choice::Integer(choice)]));
        let case = Case::new(Record::new(replayed, MAX_CHOICES), None);
        let value = self.decode(&case).ok()?;
        Some(self.render(&value))
    }
}

/// The generator of `f` applied to each value of another.
struct Mapped<T, U> {
    of: Generator<T>,
    f: Arc<dyn Fn(T) -> U + Send + Sync>,
}

impl<T: 'static, U: 'static> Node<U> for Mapped<T, U> {
    fn id(&self) -> &'static str {
        "map"
    }

    fn decode(&self, case: &Case<'_>) -> Result<U, Stop> {
        self.of.decode(case).map(|value| (self.f)(value))
    }

    fn integer(&self) -> Option<IntegerBounds> {
        self.of.node.integer()
    }
}

/// The generator of the values of another that a predicate keeps.
struct Filtered<T> {
    of: Generator<T>,
    keep: Arc<dyn Fn(&T) -> bool + Send + Sync>,
}

/// The attempts that a filter makes in all before it rejects the case.
const FILTER_ATTEMPTS: usize = 3;

impl<T: 'static> Filtered<T> {
    fn attempt(&self, case: &Case<'_>) -> Result<T, Stop> {
        let _span = case.span("filter", None);
        self.of.decode(case)
    }
}

impl<T: 'static> Node<T> for Filtered<T> {
    fn id(&self) -> &'static str {
        "filter"
    }

    fn decode(&self, case: &Case<'_>) -> Result<T, Stop> {
        let mark = case.mark();
        let mut value = self.attempt(case)?;
        for _ in 1..FILTER_ATTEMPTS {
            if (self.keep)(&value) {
                return Ok(value);
            }
            case.rewind(mark);
            value = self.attempt(case)?;
        }
        if (self.keep)(&value) {
            Ok(value)
        } else {
            Err(Stop::Rejected)
        }
    }

    fn literal(&self, value: &T) -> Option<Value> {
        self.of.literal(value)
    }

    /// Returns the steps of the source. The inverse replays them through the
    /// filter, which refuses a value that the predicate rejects.
    fn emit(&self, value: &Value) -> Result<Emitted, CannotInvert> {
        self.of.emit(value)
    }
}

/// The generator of a value of another, then of the generator that a
/// function returns for it.
struct Bound<T, U> {
    of: Generator<T>,
    f: Arc<dyn Fn(T) -> Generator<U> + Send + Sync>,
}

impl<T: 'static, U: 'static> Node<U> for Bound<T, U> {
    fn id(&self) -> &'static str {
        "bind"
    }

    fn decode(&self, case: &Case<'_>) -> Result<U, Stop> {
        let _span = case.span("bind", None);
        let first = self.of.decode(case)?;
        (self.f)(first).decode(case)
    }
}

/// The generator whose value a function returns from the case it receives.
pub(crate) struct Composite<T> {
    pub(crate) f: Arc<dyn Fn(&Case<'_>) -> T + Send + Sync>,
}

impl<T: 'static> Node<T> for Composite<T> {
    fn id(&self) -> &'static str {
        "composite"
    }

    /// Runs the function inside a span labelled `composite`. A draw of the
    /// function that ends the case ends the decode, and any other panic of
    /// the function passes on.
    fn decode(&self, case: &Case<'_>) -> Result<T, Stop> {
        let _span = case.span("composite", None);
        match catch_unwind(AssertUnwindSafe(|| (self.f)(case))) {
            Ok(value) => Ok(value),
            Err(payload) => match payload.downcast::<Signal>() {
                Ok(signal) => Err(signal.0),
                Err(payload) => resume_unwind(payload),
            },
        }
    }
}
