// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The values of a generator for a seed, outside a property.
//!
//! A benchmark takes its inputs from a generator, so every run measures the
//! same inputs, and the inputs cover more paths than one literal does. The
//! generators that `using` states apply as they apply in a run, so the
//! benchmark and a run of the property draw from one domain.

use super::case::Case;
use super::engine::case::{Failed, MAX_CHOICES, Record, Stop};
use super::engine::runner::generating;
use super::generator::Generator;
use super::options::Options;
use super::scope;

/// The most cases that [`values`] rejects for each value that it returns. A
/// run of a property gives up at the same ratio.
const REJECTIONS_PER_VALUE: usize = 10;

/// Returns `n` values of `generator` for `seed`: the values of the random
/// cases of `seed`, in order, without the cases that the generator rejects.
///
/// Case k decodes from the random source of `seed + k`, as a random case of
/// a run does, so the values are the outputs of the generation and shapes
/// vectors of the definition for the seed. Two calls with one seed return the
/// same values, and a call with a larger `n` returns the same first values.
///
/// `values` does not run a property. It does not try the simplest case or a
/// boundary case, shrink, record an assertion or write a stored case, and it
/// does not read the variables `DOKIMI_ASSERT_PROP_*`. A panic of the
/// generator passes through.
///
/// Each generator that `options` states through
/// [`using`](super::Options::using) applies as it applies in a run. A draw of
/// [`of::<U>()`](super::of) decodes with the stated generator of `U`, and a
/// draw of `of` of a type that contains `U` generates each `U` with it. Inside
/// the stated generator of `U`, a draw of `of::<U>()` decodes without it.
/// Without `using`, a draw of `of` uses the registration or the shape of its
/// type, also inside a run that gives the type a generator.
///
/// ```
/// use dokimi_assert::prop::{self, Options};
///
/// let inputs = prop::values(&prop::string(..=16), 42, 100, Options::default());
/// assert_eq!(inputs, prop::values(&prop::string(..=16), 42, 100, Options::default()));
///
/// let digits = prop::of::<u16>().filter(|n| *n < 10);
/// let lists = prop::values(&prop::of::<Vec<u16>>(), 42, 100, prop::using(&digits));
/// assert!(lists.iter().flatten().all(|n| *n < 10));
/// ```
///
/// # Panics
///
/// Panics when a setting of `options` other than `using` differs from its
/// default, because no other setting of a run applies to the call. Panics
/// when the generator rejects more than ten cases for each of the `n` values.
/// A case past 8,192 choices counts as rejected. Panics with the reason when
/// the generator ends its case in another way, such as a check of a composite
/// that fails on its case.
#[must_use]
pub fn values<T: 'static>(
    generator: &Generator<T>,
    seed: u64,
    n: usize,
    options: Options,
) -> Vec<T> {
    assert!(
        options.states_using_alone(),
        "prop: values(generator, {seed}, {n}) states a setting other than using"
    );
    let Options { using, .. } = options;
    let _run = scope::enter(using.generators());
    let mut out = Vec::with_capacity(n);
    let mut rejected = 0;
    let mut index = 0;
    while out.len() < n {
        let case = Case::new(Record::new(generating(seed, index), MAX_CHOICES), None);
        index += 1;
        match generator.decode(&case) {
            Ok(value) => out.push(value),
            Err(Stop::Rejected | Stop::Overrun) => {
                rejected += 1;
                assert!(
                    rejected <= REJECTIONS_PER_VALUE * n,
                    "prop: values(generator, {seed}, {n}) stops after the generator rejects \
                     {rejected} cases for {n} values"
                );
            }
            Err(Stop::Failed(failed)) => {
                let Failed {
                    identity, record, ..
                } = *failed;
                let reason = record.map_or(identity, |record| record.to_string());
                panic!(
                    "prop: values(generator, {seed}, {n}): the generator fails its case: {reason}"
                );
            }
            Err(stop) => {
                panic!(
                    "prop: values(generator, {seed}, {n}): the generator ends its case: {stop:?}"
                )
            }
        }
    }
    out
}
