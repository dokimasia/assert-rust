//! The spec of `prop::engine::trace`, a case stated as its draws and its
//! steps.

use dokimi_assert::prop::engine::trace::{Refusal, TraceError};

mod trace_error {
    use super::*;

    #[test]
    fn states_the_entry_the_name_and_the_refusal() {
        let error = TraceError {
            entry: 2,
            name: "n".to_owned(),
            refusal: Refusal::Value,
        };
        assert_eq!(error.to_string(), "prop: trace entry 2 (n): value");
    }
}

mod refusal {
    use super::*;

    #[test]
    fn names_each_refusal_as_a_vector_states_it() {
        let names: Vec<&str> = [Refusal::Label, Refusal::Value, Refusal::Step]
            .into_iter()
            .map(Refusal::name)
            .collect();
        assert_eq!(names, ["label", "value", "step"]);
    }
}
