//! The run of a property form: the draws of its inputs, its examples, and
//! the record of its assertion.
//!
//! A form runs as `for_all` runs a body. Each case draws the inputs of the
//! form from the input's generator under the form's labels, and runs the
//! form's assertion on them with the case as its seat. The record of the run
//! has the form's id, such as `prop-equal`, and the record of each call in a
//! case is at the call site of the form.

use super::arbitrary::Arbitrary;
use super::case::Case;
use super::engine::runner::Example;
use super::generator::Generator;
use super::options::Options;
use super::property::{Call, Property, Refused, fault};
use super::registry::generator_of;
use super::scope;
use crate::seat::Seat;
use crate::verdict::Run;
use std::any::Any;
use std::fmt::Debug;

/// The label of the input of a form over functions and of a relation of one
/// input.
pub(crate) const INPUT: [&str; 1] = ["input"];

/// The labels of the two inputs of a commutative form.
pub(crate) const PAIR: [&str; 2] = ["a", "b"];

/// The labels of the three inputs of an associative form.
pub(crate) const TRIPLE: [&str; 3] = ["a", "b", "c"];

/// What a property form states: its operation, which each fault's text
/// starts with, its id, and whether its cases run one at a time.
pub(crate) struct Form {
    pub(crate) op: &'static str,
    pub(crate) id: &'static str,
    pub(crate) serial: bool,
}

/// Runs the form on `seat` as `for_all` runs a body. Each case draws one
/// input for each of `labels`, in order, and `check` runs the form's
/// assertion on them with the case as its seat.
///
/// The input's generator is the generator of its type under the generators
/// that `using` states, as a draw of `prop::of` picks it. The call ends with
/// a fault, without a run, for a type whose shape does not read, for an
/// example of another number of values than the form generates, for a value
/// of an example that the generator does not produce, and for each fault for
/// which `for_all` ends its call.
#[track_caller]
pub(crate) fn run<T, const N: usize>(
    seat: &dyn Seat,
    form: &Form,
    labels: [&'static str; N],
    msg: &str,
    mut options: Options<T>,
    check: impl Fn(&Case<'_>, [T; N]) + Sync,
) where
    T: Arbitrary + Debug + Send,
{
    seat.helper();
    let run = Run::begin(seat);
    let call = Call {
        op: form.op,
        assertion: form.id,
        contract: msg,
        pinned: true,
    };
    let using = options.using.generators();
    let _run = scope::enter(using.clone());
    let read = match &using {
        Some(generators) => generators.read::<T>(),
        None => generator_of::<T>(),
    };
    let generator = match read {
        Ok(generator) => generator,
        Err(refused) => return fault(&run, call, &Refused::new("", refused.reason())),
    };
    let examples = match examples_of(&generator, N, std::mem::take(&mut options.examples)) {
        Ok(examples) => examples,
        Err(refused) => return fault(&run, call, &refused),
    };
    if form.serial {
        options.workers = 1;
    }
    let property = match Property::of(seat, call, options) {
        Ok(property) => property,
        Err(refused) => return fault(&run, call, &refused),
    };
    let body = |case: &Case<'_>| {
        let inputs = std::array::from_fn(|at| case.draw(&generator, labels[at]));
        check(case, inputs);
    };
    property.run(seat, &run, &body, examples);
}

/// Returns `count` with the singular or the plural of `word`.
fn counted(count: usize, word: &str) -> String {
    if count == 1 {
        format!("1 {word}")
    } else {
        format!("{count} {word}s")
    }
}

/// Returns the cases of the examples, in order: the choices that decode to
/// each case's values, or the values themselves where the generator has no
/// inverse for one of them, such as a generator that `map` returns.
fn examples_of<T: Send + 'static>(
    generator: &Generator<T>,
    inputs: usize,
    examples: Vec<Vec<T>>,
) -> Result<Vec<Example>, Refused> {
    let mut cases = Vec::with_capacity(examples.len());
    for (at, values) in examples.into_iter().enumerate() {
        let place = format!("example[{at}]");
        if values.len() != inputs {
            return Err(Refused::new(
                place,
                format!(
                    "the example states {}, and the form generates {}",
                    counted(values.len(), "value"),
                    counted(inputs, "input")
                ),
            ));
        }
        let mut choices = Vec::new();
        let mut valued = false;
        for (index, value) in values.iter().enumerate() {
            let Some(literal) = generator.literal(value) else {
                valued = true;
                break;
            };
            match generator.invert(&literal) {
                Ok(inverted) => choices.extend(inverted),
                Err(refused) if refused.is_no_inverse() => {
                    valued = true;
                    break;
                }
                Err(refused) => {
                    return Err(Refused::new(
                        format!("{place}[{index}]"),
                        format!(
                            "the input's generator does not produce the value: {}",
                            refused.reason()
                        ),
                    ));
                }
            }
        }
        cases.push(if valued {
            Example::Values(
                values
                    .into_iter()
                    .map(|value| Box::new(value) as Box<dyn Any + Send>)
                    .collect(),
            )
        } else {
            Example::Choices(choices)
        });
    }
    Ok(cases)
}
