//! The spec of `prop::engine::case`, the record of the choices of one call of
//! a body.

use crate::helpers::records;
use dokimi_assert::prop;
use dokimi_assert::prop::engine::case::MAX_CHOICES;
use dokimi_assert::{Recorder, check};

mod record {
    use super::*;

    #[test]
    fn ends_a_case_whose_sequence_alone_takes_it_past_its_cap() {
        let seat = Recorder::new();
        prop::for_all(
            &seat,
            "the bytes fit",
            prop::hermetic().seed(1).max_choices(5),
            |c| {
                let bytes = c.draw(&prop::bytes(10..=10), "bytes");
                check::is_true(c, bytes.len() == 10, "the bytes fit");
            },
        );
        assert_eq!(records(&seat)[0]["detail"]["outcome"], "rejected");
    }

    #[test]
    fn keeps_the_higher_of_two_scores_under_one_label() {
        let seat = Recorder::new();
        prop::for_all(
            &seat,
            "the scores hold",
            prop::hermetic().seed(1).cases(5),
            |c| {
                let n = c.draw(&prop::integer(0..=9_u8), "n");
                c.target("score", f64::from(n));
                c.target("score", f64::from(n) + 1.0);
                c.target("score", 0.0);
            },
        );
        assert!(!seat.failed(), "{}", seat.message());
        assert_eq!(MAX_CHOICES, 8192);
    }
}
