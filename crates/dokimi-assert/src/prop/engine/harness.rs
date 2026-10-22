//! The entry points that run the engine on the inputs of the definition's
//! vectors: a decode from stated choices, from a seed or from a fuzzer's
//! bytes, the inverse of a generator, and a run of a body.
//!
//! The conformance crate calls them. They are no part of the public API.

use super::bridge::Bridging;
use super::case::{Failed, MAX_CHOICES, Provider, Record, Replaying, Stop};
use super::choice::{Choice, IntegerBounds};
use super::execution::{Execution, Fault, Phase};
use super::inverse::CannotInvert;
use super::runner::{self, Outcome, Run, Settings};
use super::source::case_source;
use crate::failure::Failure;
use crate::prop::generators::{Integer, Pairs};
use crate::prop::{Case, Generator};
use crate::seat::Seat;
use crate::value::Value;
use std::fmt::Debug;
use std::sync::Arc;

/// Decodes a value of `generator` on a case of `provider`, and returns the
/// choices that the case recorded and the value, or `None` when the decode
/// rejected the case.
fn decoded<T: 'static>(
    generator: &Generator<T>,
    provider: Box<dyn Provider>,
) -> (Vec<Choice>, Option<T>) {
    let case = Case::new(Record::new(provider, MAX_CHOICES), None);
    let value = generator.decode(&case).ok();
    let (record, _) = case.into_parts();
    (record.choices, value)
}

/// Decodes a value of `generator` from stated choices.
#[must_use]
pub fn decode<T: 'static>(
    generator: &Generator<T>,
    choices: Vec<Choice>,
) -> (Vec<Choice>, Option<T>) {
    decoded(generator, Box::new(Replaying::new(choices)))
}

/// Decodes a value of `generator` from random case `index` of `seed`.
#[must_use]
pub fn generate<T: 'static>(
    generator: &Generator<T>,
    seed: u64,
    index: u64,
) -> (Vec<Choice>, Option<T>) {
    decoded(
        generator,
        Box::new(super::case::Generating::new(case_source(seed, index))),
    )
}

/// Decodes a value of `generator` from a fuzzer's bytes.
#[must_use]
pub fn bridge<T: 'static>(generator: &Generator<T>, data: &[u8]) -> (Vec<Choice>, Option<T>) {
    decoded(generator, Box::new(Bridging::new(data)))
}

/// Returns the choices that decode to `value`.
///
/// # Errors
///
/// Returns the refusal when the generator cannot produce `value`.
pub fn invert<T: 'static>(
    generator: &Generator<T>,
    value: &Value,
) -> Result<Vec<Choice>, CannotInvert> {
    generator.invert(value)
}

/// Runs `body` under `settings`, and passes each call of the body to
/// `observer` with its phase.
///
/// # Errors
///
/// Returns the fault that ends the run.
pub fn run(
    body: &(dyn Fn(&Case<'static>) + Sync),
    settings: Settings,
    observer: &mut dyn FnMut(Phase, &Execution),
) -> Result<Outcome, Fault> {
    let mut engine = Run {
        body,
        context: None,
        observer,
    };
    runner::run(&mut engine, settings)
}

impl Execution {
    /// Returns the choices that the case recorded.
    #[must_use]
    pub fn choices(&self) -> &[Choice] {
        &self.record.choices
    }

    /// Returns the label and the typed literal of each draw of the case.
    #[must_use]
    pub fn draws(&self) -> Vec<(String, Option<Value>)> {
        self.record
            .draws
            .iter()
            .map(|drawn| (drawn.label.clone(), drawn.value.literal.clone()))
            .collect()
    }
}

impl Case<'_> {
    /// Fails the case with `identity`, as the body of a vector fails, and ends
    /// it when `aborting`. A record of the run states the identity as the
    /// case's failure.
    #[doc(hidden)]
    pub fn fail_as(&self, identity: &str, aborting: bool) {
        if let Err(stop) = self.check() {
            self.end(stop);
        }
        let failed = self.keep(Failed::of(identity));
        if aborting {
            self.end(Stop::Failed(Box::new(failed)));
        }
    }

    /// Ends the case with the record of `assertion`, whose contract is
    /// `contract` and whose detail is `detail`, as a subject of the
    /// definition reports an assertion of its own. The record has no call
    /// site, so the run keeps its failure by the assertion and the contract.
    #[doc(hidden)]
    pub fn fail_with(
        &self,
        assertion: &'static str,
        contract: &str,
        detail: Vec<(&'static str, Value)>,
    ) {
        self.report(&Failure::new(assertion, contract, detail, None), true);
    }
}

impl<T: Debug + 'static> Generator<T> {
    /// Returns a generator of the values of this one as [`Value`]s, with the
    /// same choices and the same inverse.
    #[doc(hidden)]
    #[must_use]
    pub fn valued(self) -> Generator<Value> {
        Generator::from_node(Valued { of: self })
    }
}

/// The generator of another's values as their typed literals.
struct Valued<T> {
    of: Generator<T>,
}

impl<T: Debug + 'static> crate::prop::generator::Node<Value> for Valued<T> {
    fn id(&self) -> &'static str {
        self.of.node().id()
    }

    fn decode(&self, case: &Case<'_>) -> Result<Value, Stop> {
        let value = self.of.decode(case)?;
        Ok(self
            .of
            .literal(&value)
            .unwrap_or_else(|| Value::Opaque(format!("{value:?}"))))
    }

    fn emit(&self, value: &Value) -> Result<super::inverse::Emitted, CannotInvert> {
        self.of.emit(value)
    }

    fn integer(&self) -> Option<IntegerBounds> {
        self.of.node().integer()
    }
}

/// Returns the generator of durations in nanoseconds in `[lo, hi]`, as the
/// vocabulary's `duration` states them, which may be negative.
///
/// # Errors
///
/// Returns the refusal of bounds outside the 64-bit ranges.
pub fn duration_nanos(lo: i128, hi: i128) -> Result<Generator<Value>, super::choice::BoundsError> {
    Ok(Generator::from_node(Integer::<Value> {
        id: "duration",
        bounds: IntegerBounds::new(lo, hi)?,
        from: Value::Int,
    }))
}

/// Returns the generator of maps from `keys`' values to `values`' values,
/// with distinct keys under the definition's equality.
#[must_use]
pub fn pairs(
    keys: Generator<Value>,
    values: Generator<Value>,
    sizes: super::collection::Sizes,
) -> Generator<Value> {
    let entries = Generator::from_node(Pairs {
        keys,
        values,
        sizes,
        same: Arc::new(super::canonical::same),
    });
    Generator::from_node(Valued { of: entries })
}
