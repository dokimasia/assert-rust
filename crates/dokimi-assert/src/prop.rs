//! Properties: a body run against generated inputs, with failures shrunk.
//!
//! [`for_all`] runs a body against generated cases. The body draws its inputs
//! from its [`Case`], and it asserts on the case as a test asserts on its
//! seat. A failing case shrinks to the smallest case that fails the same way,
//! and the run reports it with its draws and a replay token:
//!
//! ```
//! use dokimi_assert::{Collector, check, prop};
//!
//! let t = Collector::new();
//! prop::for_all(&t, "a reversed list reverses back", prop::cases(50), |c| {
//!     let xs = c.draw(&prop::list(prop::integer(-100..=100), ..), "xs");
//!     let mut twice = xs.clone();
//!     twice.reverse();
//!     twice.reverse();
//!     check::equal(c, twice, xs, "reversing twice returns the list");
//! });
//! ```
//!
//! A run replays the stored failures of the test first, tries the simplest
//! input and the boundary inputs, then generates inputs that it has not
//! tested until it has run the stated number of cases or tested every input.
//!
//! # Forms and derived inputs
//!
//! A property form, such as [`equal`] or [`round_trip`], runs an assertion of
//! the root namespace on each input that a run generates. The input is a
//! value of a type that implements [`Arbitrary`], which states the type's
//! shape, and `#[derive(Arbitrary)]` implements it behind the feature
//! `derive`. [`of_shape`] reads a shape file of any language of the
//! definition.
//!
//! # Settings
//!
//! The settings of a run are an [`Options`] builder. A free function of the
//! name of each setting, such as [`cases`] or [`seed`], starts the builder,
//! and its methods of the same names extend it. [`using`] gives a type
//! another generator in one run.
//!
//! # Values outside a property
//!
//! [`values`] returns the values of a generator for a seed, so a benchmark
//! measures the same inputs in every run.
//!
//! # The environment
//!
//! `DOKIMI_ASSERT_PROP_SEED`, `DOKIMI_ASSERT_PROP_PROFILE`,
//! `DOKIMI_ASSERT_PROP_BUDGET` and `DOKIMI_ASSERT_PROP_REPLAY` set the
//! defaults of every property of a run. The profile is `default`, `ci` or
//! `campaign`. [`hermetic`] makes a run ignore the four variables.

mod arbitrary;
pub(crate) mod case;
pub(crate) mod detail;
pub(crate) mod directory;
#[doc(hidden)]
pub mod engine;
pub(crate) mod environment;
mod form;
mod forms;
mod generator;
mod generators;
mod native;
pub(crate) mod options;
pub(crate) mod property;
pub(crate) mod registry;
pub(crate) mod scope;
mod shape;
mod values;

pub use arbitrary::{Arbitrary, Definitions};
pub use case::{Case, Rand};
#[cfg(feature = "derive")]
pub use dokimi_assert_derive::Arbitrary;
pub use forms::{
    accumulates, close_to, contains, contains_in_order, does_not_panic, equal, error_as, error_is,
    error_is_not, has_error, has_prefix, has_suffix, honours_cancellation, honours_deadline,
    in_range, is_associative, is_commutative, is_deterministic, is_empty, is_false, is_idempotent,
    is_none, is_not_empty, is_not_pure, is_permutation, is_pure, is_some, is_true, length, matches,
    max_allocs, max_allocs_with_setup, no_error, none_handle_safe, not_contains, not_equal,
    pairwise, panics, round_trip,
};
pub use generator::Generator;
pub use generators::{
    Floating, Integral, boolean, bytes, composite, dict, duration, float, integer, just, list,
    one_of, optional, permutation, recursive, sampled_from, string, string_matching,
};
pub use options::{
    Options, around, cases, draws, example, examples, explain, hermetic, max_choices, replay,
    require, seed, shrink, shrink_time, store, using, workers,
};
pub use property::{for_all, fuzz};
pub use registry::{Variant, of, register, register_values, register_variants, shape_of};
pub use shape::{Shape, ShapeError, of_shape};
pub use values::values;
