//! The spec of `prop::engine::runner`: the phases, the counts and the outcome
//! of a run.

use crate::helpers::records;
use dokimi_assert::prop::engine::choice::Choice;
use dokimi_assert::prop::engine::runner::{DEFAULT_CASES, Example, Kind};
use dokimi_assert::{Recorder, check, prop};

mod example {
    use super::*;

    #[test]
    fn formats_its_choices_or_the_number_of_its_values() {
        assert_eq!(
            format!("{:?}", Example::Choices(vec![Choice::Integer(1)])),
            "Choices([Integer(1)])"
        );
        let values = Example::Values(vec![Box::new(1_u8), Box::new(2_u8)]);
        assert_eq!(format!("{values:?}"), "Values(2)");
    }
}

mod kind {
    use super::*;

    #[test]
    fn names_each_kind_as_a_record_states_it() {
        let kinds = [
            Kind::Passed,
            Kind::Counterexample,
            Kind::Flaky,
            Kind::Rejected,
            Kind::CoverageUnmet,
            Kind::Vacuous,
        ];
        let names: Vec<&str> = kinds.into_iter().map(Kind::name).collect();
        assert_eq!(
            names,
            [
                "passed",
                "counterexample",
                "flaky",
                "rejected",
                "coverage-unmet",
                "vacuous"
            ]
        );
        assert_eq!(DEFAULT_CASES, 100);
    }
}

mod run {
    use super::*;

    #[test]
    fn passes_a_requirement_that_the_first_check_meets() {
        let seat = Recorder::new();
        prop::for_all(
            &seat,
            "every case counts",
            prop::require("all", 0.9).hermetic().seed(1),
            |c| {
                let _ = c.draw(&prop::boolean(), "flag");
                c.classify("all");
            },
        );
        assert_eq!(records(&seat)[0]["detail"]["outcome"], "passed");
    }

    #[test]
    fn fails_a_property_that_rejects_more_than_ten_cases_for_each_valid_one() {
        let seat = Recorder::new();
        prop::for_all(
            &seat,
            "most cases are valid",
            prop::hermetic().seed(1),
            |c| {
                let n = c.draw(&prop::integer(0..=100_u8), "n");
                c.assume(n == 0);
                check::is_true(c, true, "most cases are valid");
            },
        );
        assert_eq!(records(&seat)[0]["detail"]["outcome"], "rejected");
    }
}
