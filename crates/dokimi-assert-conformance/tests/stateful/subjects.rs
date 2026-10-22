//! The spec of `stateful::subjects`, the machine subjects of the definition.

use dokimi_assert::prop;
use dokimi_assert::stateful::{self, Options};
use dokimi_assert::{Recorder, Value};
use dokimi_assert_conformance::stateful::subjects::{self, Setup};

/// Returns the setup of a subject's steps, with two clients under the
/// uniform strategy.
fn setup(steps: Options<'static>) -> Setup {
    Setup {
        steps,
        clients: 2,
        concurrent: None,
        strategy: stateful::uniform(),
    }
}

/// Runs the subject `name` under `setup` with the seed 1, and returns the
/// failures that the recorder kept.
fn failures(name: &str, setup: &Setup) -> Vec<dokimi_assert::Failure> {
    let subject = subjects::subject(name).expect("the definition states the subject");
    let seat = Recorder::new();
    prop::for_all(
        &seat,
        "the subject",
        prop::hermetic().seed(1).store(""),
        |c| subject(c, setup),
    );
    seat.failures()
}

mod subject {
    use super::*;

    #[test]
    fn returns_none_for_a_name_that_the_definition_does_not_state() {
        assert!(subjects::subject("made-up").is_none());
    }

    #[test]
    fn fails_the_queue_that_loses_on_wrap() {
        let found = failures("queue-loses-on-wrap", &setup(Options::default()));
        let case = found[0]
            .case_failure()
            .expect("the property found a counterexample");
        assert_eq!(case.assertion(), Some("linearizable"));
    }

    #[test]
    fn passes_the_correct_queue() {
        assert_eq!(failures("correct-queue", &setup(Options::default())), []);
    }

    #[test]
    fn reports_the_lost_write_of_the_store() {
        let found = failures("store-loses-on-crash", &setup(Options::default()));
        let case = found[0]
            .case_failure()
            .expect("the property found a counterexample");
        assert_eq!(case.assertion(), Some("lost-write"));
        assert_eq!(case.contract(), "the store keeps every acknowledged put");
        assert_eq!(case.field("lost"), Some(&Value::Int(1)));
    }

    #[test]
    fn fails_the_racy_counter_on_two_clients() {
        let found = failures("racy-counter", &setup(Options::default()));
        let case = found[0]
            .case_failure()
            .expect("the property found a counterexample");
        assert_eq!(case.assertion(), Some("linearizable"));
    }

    #[test]
    fn passes_the_correct_counter_with_a_bounded_section() {
        let bounded = Setup {
            concurrent: Some(3),
            ..setup(Options::default())
        };
        assert_eq!(failures("correct-counter", &bounded), []);
    }

    #[test]
    fn fails_the_counter_that_overflows() {
        let found = failures("counter-overflows", &setup(Options::default()));
        let case = found[0]
            .case_failure()
            .expect("the property found a counterexample");
        assert_eq!(case.assertion(), Some("linearizable"));
    }

    #[test]
    fn makes_the_run_of_the_refusing_counter_flaky() {
        let found = failures(
            "counter-refuses-every-third",
            &setup(stateful::swarm(false)),
        );
        assert_eq!(found[0].field("outcome"), Some(&Value::from("flaky")));
    }
}
