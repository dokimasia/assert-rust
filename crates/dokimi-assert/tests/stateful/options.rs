//! The spec of `stateful::options`, the settings of a run of steps.

use crate::helpers::panic_of;
use dokimi_assert::stateful::{self, Options, Scheduler};
use dokimi_assert::{Collector, prop};

mod options {
    use super::*;

    #[test]
    fn states_the_defaults_of_the_definition() {
        assert_eq!(
            format!("{:?}", Options::default()),
            "Options { mean: 30, max: 100, swarm: true, clients: 1, concurrent: 16, tasks: false, repeat: 4 }"
        );
    }

    #[test]
    fn starts_a_chain_with_the_function_of_each_setting() {
        let tests = [
            (stateful::mean(5), "mean: 5"),
            (stateful::max(6), "max: 6"),
            (stateful::swarm(false), "swarm: false"),
            (stateful::clients(3), "clients: 3"),
            (stateful::concurrent(7), "concurrent: 7"),
            (stateful::repeat(2), "repeat: 2"),
            (
                stateful::mean(5)
                    .max(6)
                    .swarm(false)
                    .clients(3)
                    .concurrent(7)
                    .repeat(2),
                "concurrent: 7",
            ),
        ];
        for (give, want) in tests {
            let stated = format!("{give:?}");
            assert!(stated.contains(want), "{stated} states {want}");
        }
    }

    #[test]
    fn makes_the_clients_run_as_tasks_of_a_scheduler() {
        let t = Collector::new();
        prop::for_all(
            &t,
            "the settings name the tasks",
            prop::hermetic().cases(1),
            |c| {
                c.draw(&prop::boolean(), "flag");
                let scheduler = Scheduler::new(c, stateful::uniform());
                assert!(format!("{:?}", stateful::tasks(&scheduler)).contains("tasks: true"));
                assert!(
                    format!("{:?}", Options::default().tasks(&scheduler)).contains("tasks: true")
                );
            },
        );
    }

    #[test]
    fn panics_for_no_client() {
        assert_eq!(
            panic_of(|| _ = stateful::clients(0)).as_deref(),
            Some("stateful: clients(0) is no count of 1 or more")
        );
    }

    #[test]
    fn panics_for_no_run() {
        assert_eq!(
            panic_of(|| _ = stateful::repeat(0)).as_deref(),
            Some("stateful: repeat(0) is no count of 1 or more")
        );
    }
}
