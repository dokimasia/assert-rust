//! The property vectors of the definition, run against the property engine
//! of `dokimi-assert`.
//!
//! A vector states a generator, a body or the settings of a run as JSON,
//! with the outputs that the definition's reference computes for them. The
//! drivers build each generator from the library's constructors, and compare
//! each output with the vector's as JSON. The kinds of the engine run on
//! `dokimi_assert::prop::engine`, and a behaviour vector and a recording
//! vector run with `prop::for_all` on a recorder. A fixtures vector reads the
//! shape of a Rust type of its fixture, and a forms vector runs a property
//! form of the library.

pub mod body;
pub mod choices;
pub mod fixtures;
pub mod forms;
pub mod generator;
pub mod predicate;
pub mod vectors;
