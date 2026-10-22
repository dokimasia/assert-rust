// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `stateful::machine`, the machine and its actions.

use crate::helpers::{Register, panic_of, read, write};
use dokimi_assert::prop::Case;
use dokimi_assert::stateful::{self, Action, Machine};
use dokimi_assert::{Recorder, check, prop};
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

/// Runs `body` as a property of 20 cases with the seed 1 on a recorder, and
/// returns the recorder.
fn run(body: impl Fn(&Case<'_>) + Send + Sync) -> Recorder {
    let seat = Recorder::new();
    prop::for_all(
        &seat,
        "the machine holds",
        prop::hermetic().seed(1).cases(20),
        body,
    );
    seat
}

mod action {
    use super::*;

    #[test]
    fn states_its_name_its_weight_and_its_settings() {
        let action = Action::<i64>::new("reset", async |_, _| {})
            .weight(3)
            .drain()
            .enabled(|_| true);
        assert_eq!(action.name(), "reset");
        assert_eq!(
            format!("{action:?}"),
            r#"Action { name: "reset", weight: 3, drain: true, enabled: true, .. }"#
        );
    }

    #[test]
    fn counts_a_weight_of_0_as_1() {
        let action = Action::<i64>::with_input("put", |_, _| 1, async |_, _, _: i32| {}).weight(0);
        assert_eq!(
            format!("{action:?}"),
            r#"Action { name: "put", weight: 1, drain: false, enabled: false, .. }"#
        );
    }

    #[test]
    fn takes_an_action_of_a_greater_weight_more_often() {
        let (light, heavy) = (AtomicUsize::new(0), AtomicUsize::new(0));
        let seat = run(|c| {
            let machine = Machine::<i64>::new([
                Action::new("light", async |_, _| {
                    _ = light.fetch_add(1, Ordering::SeqCst);
                }),
                Action::new("heavy", async |_, _| {
                    _ = heavy.fetch_add(1, Ordering::SeqCst);
                })
                .weight(50),
            ]);
            stateful::steps(c, &machine, stateful::swarm(false));
        });
        assert!(!seat.failed());
        assert!(heavy.load(Ordering::SeqCst) > 10 * light.load(Ordering::SeqCst));
    }
}

mod machine {
    use super::*;

    #[test]
    fn states_its_actions_and_whether_it_has_each_check() {
        let machine = Machine::new([Action::new("read", async |_, _| {})])
            .spec(Register)
            .settle(|_, _| {});
        assert_eq!(
            format!("{machine:?}"),
            r#"Machine { actions: [Action { name: "read", weight: 1, drain: false, enabled: false, .. }], spec: true, invariant: false, settle: true }"#
        );
    }

    #[test]
    fn panics_for_two_actions_of_one_name() {
        let raised = panic_of(|| {
            let _ = Machine::<i64>::new([
                Action::new("read", async |_, _| {}),
                Action::new("read", async |_, _| {}),
            ]);
        });
        assert_eq!(
            raised.as_deref(),
            Some(r#"stateful: Machine::new of actions that name "read" twice"#)
        );
    }

    #[test]
    fn checks_the_invariant_in_the_state_of_the_spec() {
        let seat = run(|c| {
            let register = AtomicI64::new(0);
            let machine = Machine::new([
                Action::with_input(
                    "write",
                    |c: &Case<'_>, _: &i64| c.draw(&prop::integer(0..=9_i64), "v"),
                    async |c: &Case<'_>, client: usize, value: i64| {
                        register.store(value, Ordering::SeqCst);
                        write(c.history(), client, value);
                    },
                ),
                Action::new("read", async |c: &Case<'_>, client: usize| {
                    read(c.history(), client, register.load(Ordering::SeqCst));
                }),
            ])
            .spec(Register)
            .invariant(|c: &Case<'_>, state: &i64| {
                check::is_true(c, *state < 5, "the register stays below 5");
            });
            stateful::steps(c, &machine, stateful::max(10));
        });
        let failure = &seat.failures()[0];
        let case = failure
            .case_failure()
            .expect("the run found a counterexample");
        assert_eq!(case.contract(), "the register stays below 5");
    }

    #[test]
    fn settles_once_in_each_case() {
        let (cases, settled) = (AtomicUsize::new(0), AtomicUsize::new(0));
        let seat = run(|c| {
            cases.fetch_add(1, Ordering::SeqCst);
            let machine =
                Machine::<i64>::new([Action::new("tick", async |_, _| {})]).settle(|_, _| {
                    settled.fetch_add(1, Ordering::SeqCst);
                });
            stateful::steps(c, &machine, stateful::max(3));
        });
        assert!(!seat.failed());
        assert_eq!(settled.load(Ordering::SeqCst), cases.load(Ordering::SeqCst));
    }

    #[test]
    fn passes_a_machine_whose_subject_follows_its_spec() {
        let seat = run(|c| {
            let register = AtomicI64::new(0);
            let machine = Machine::new([
                Action::new("write", async |c: &Case<'_>, client: usize| {
                    register.store(3, Ordering::SeqCst);
                    write(c.history(), client, 3);
                }),
                Action::new("read", async |c: &Case<'_>, client: usize| {
                    read(c.history(), client, register.load(Ordering::SeqCst));
                }),
            ])
            .spec(Register);
            stateful::steps(c, &machine, stateful::Options::default());
        });
        assert!(!seat.failed(), "{}", seat.message());
    }
}
