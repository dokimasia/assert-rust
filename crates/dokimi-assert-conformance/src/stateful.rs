//! The machines vectors of the definition, run against the steps of a
//! machine of `dokimi-assert`.
//!
//! A machines vector names a subject of the definition, a store, a queue or
//! a counter with a fault or without one. It states the detail of the
//! property's call record that a run of the subject's machine ends with, or
//! the entry of its trace that the run refuses.

pub mod subjects;
pub mod vectors;
