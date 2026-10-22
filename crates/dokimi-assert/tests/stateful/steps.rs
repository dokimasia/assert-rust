//! The spec of `stateful::steps`, which runs the steps of a machine in a
//! case.

use crate::helpers::Register;
use dokimi_assert::history::{Operation, Spec};
use dokimi_assert::prop::{self, Case, Options};
use dokimi_assert::stateful::{self, Action, Machine, Scheduler};
use dokimi_assert::{Recorder, Value, check};
use std::future::poll_fn;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::task::{Poll, Waker};

/// The contract of the record of a failing check of a machine's steps.
const CONTRACT: &str = "the history of the machine's steps is linearizable";

/// Takes a lock whose poison a panicking step left.
fn locked<T>(lock: &Mutex<T>) -> MutexGuard<'_, T> {
    lock.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Runs `body` as a property of 30 cases with the seed 1 under `options`,
/// on a recorder, and returns the recorder.
fn run(options: Options, body: impl Fn(&Case<'_>) + Send + Sync) -> Recorder {
    let seat = Recorder::new();
    prop::for_all(
        &seat,
        "the machine holds",
        options.hermetic().seed(1).cases(30),
        body,
    );
    seat
}

/// Returns the machine of a register that `subject` holds: write stores a
/// value of 0 to 9, and read returns the stored value. A lossy register
/// drops each write of 7.
fn register(subject: &AtomicI64, lossy: bool) -> Machine<'_, i64> {
    Machine::new([
        Action::with_input(
            "write",
            |c: &Case<'_>, _: &i64| c.draw(&prop::integer(0..=9_i64), "v"),
            async move |c: &Case<'_>, client: usize, value: i64| {
                let call = c.history().invoke(client, "write", [value.into()], []);
                if !(lossy && value == 7) {
                    subject.store(value, Ordering::SeqCst);
                }
                call.ok(Value::Null);
            },
        ),
        Action::new("read", async move |c: &Case<'_>, client: usize| {
            let call = c.history().invoke(client, "read", [], []);
            call.ok(subject.load(Ordering::SeqCst));
        }),
    ])
    .spec(Register)
}

/// Returns the machine of a counter that logs the name and the client of
/// each step into `log`.
fn logged(log: &Mutex<Vec<String>>) -> Machine<'_, i64> {
    Machine::new([
        Action::new("increment", async move |_: &Case<'_>, client: usize| {
            locked(log).push(format!("increment on {client}"));
        }),
        Action::new("reset", async move |_: &Case<'_>, client: usize| {
            locked(log).push(format!("reset on {client}"));
        }),
    ])
}

/// Returns the fault that ends the run of `machine`'s steps under `steps`
/// after the trace `draws`.
fn refusal(draws: &str, steps: stateful::Options<'static>) -> String {
    let seat = run(prop::draws(draws), |c| {
        let log = Mutex::new(Vec::new());
        stateful::steps(c, &logged(&log), steps);
    });
    seat.message()
}

/// A spec whose `next` panics.
struct Panicking;

impl Spec for Panicking {
    type State = i64;

    fn initial(&self) -> i64 {
        0
    }

    fn next(&self, _: &i64, _: &Operation) -> Vec<i64> {
        panic!("the spec has no next state")
    }
}

/// A spec whose initial state panics.
struct Unstarted;

impl Spec for Unstarted {
    type State = i64;

    fn initial(&self) -> i64 {
        panic!("the spec has no initial state")
    }

    fn next(&self, state: &i64, _: &Operation) -> Vec<i64> {
        vec![*state]
    }
}

mod steps {
    use super::*;

    #[test]
    fn passes_a_subject_that_follows_its_spec() {
        let seat = run(Options::default(), |c| {
            let subject = AtomicI64::new(0);
            stateful::steps(c, &register(&subject, false), stateful::max(10));
        });
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn fails_with_the_record_of_linearizable_for_a_subject_that_breaks_its_spec() {
        let seat = run(Options::default(), |c| {
            let subject = AtomicI64::new(0);
            stateful::steps(c, &register(&subject, true), stateful::max(10));
        });
        let case = seat.failures()[0]
            .case_failure()
            .cloned()
            .expect("the run found a counterexample");
        assert_eq!(case.assertion(), Some("linearizable"));
        assert_eq!(case.contract(), CONTRACT);
    }

    #[test]
    fn rejects_a_case_whose_steps_pass_its_cap_on_choices() {
        for cap in 1..=4 {
            let seat = run(prop::max_choices(cap), |c| {
                let subject = AtomicI64::new(0);
                stateful::steps(c, &register(&subject, false), stateful::max(10));
            });
            let record: serde_json::Value =
                serde_json::from_str(&seat.records()[0]).expect("a call record is JSON");
            assert!(
                record["detail"]["rejected"]
                    .as_u64()
                    .is_some_and(|rejected| rejected > 0),
                "{cap}: {record}"
            );
        }
    }

    #[test]
    fn runs_a_concurrent_section_on_threads() {
        let seat = run(Options::default(), |c| {
            let subject = AtomicI64::new(0);
            stateful::steps(c, &register(&subject, false), stateful::clients(2).max(4));
        });
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn runs_a_case_with_a_section_on_threads_up_to_its_repeats() {
        let calls = |repeats: usize| {
            let count = AtomicUsize::new(0);
            run(Options::default(), |c| {
                count.fetch_add(1, Ordering::SeqCst);
                let subject = AtomicI64::new(0);
                stateful::steps(
                    c,
                    &register(&subject, false),
                    stateful::clients(2).repeat(repeats),
                );
            });
            count.into_inner()
        };
        assert!(calls(3) > calls(1));
    }

    #[test]
    fn raises_the_panic_of_a_client_on_a_thread() {
        let seat = run(Options::default(), |c| {
            let machine =
                Machine::<i64>::new([Action::new("crash", async |_: &Case<'_>, client: usize| {
                    assert!(client == 0, "client {client} crashed");
                })]);
            stateful::steps(c, &machine, stateful::clients(2).swarm(false));
        });
        let case = seat.failures()[0]
            .case_failure()
            .cloned()
            .expect("the run found a counterexample");
        assert!(case.contract().ends_with("crashed"), "{}", case.contract());
    }

    #[test]
    fn runs_the_clients_as_tasks_of_a_scheduler() {
        let seat = run(Options::default(), |c| {
            let subject = AtomicI64::new(0);
            let scheduler = Scheduler::new(c, stateful::uniform());
            stateful::steps(
                c,
                &register(&subject, false),
                stateful::clients(3).tasks(&scheduler),
            );
        });
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn drains_until_no_drain_action_is_enabled_and_then_settles() {
        let seat = run(Options::default(), |c| {
            let buffer = AtomicUsize::new(0);
            let machine = Machine::<i64>::new([
                Action::new("put", async |_: &Case<'_>, _: usize| {
                    _ = buffer.fetch_add(1, Ordering::SeqCst);
                }),
                Action::new("flush", async |_: &Case<'_>, _: usize| {
                    _ = buffer.fetch_sub(1, Ordering::SeqCst);
                })
                .enabled(|_| buffer.load(Ordering::SeqCst) > 0)
                .drain(),
            ])
            .settle(|c: &Case<'_>, _: &i64| {
                check::equal(
                    c,
                    buffer.load(Ordering::SeqCst),
                    0,
                    "the drain empties the buffer",
                );
            });
            stateful::steps(c, &machine, stateful::max(20));
        });
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn ends_the_run_with_a_fault_at_a_spec_that_panics() {
        let seat = run(Options::default(), |c| {
            let machine =
                Machine::new([Action::new("tick", async |c: &Case<'_>, client: usize| {
                    c.history().invoke(client, "tick", [], []).ok(Value::Null);
                })])
                .spec(Panicking);
            stateful::steps(c, &machine, stateful::swarm(false));
        });
        assert_eq!(
            seat.message(),
            r#"prop::for_all: stateful::steps: calls[0]: the spec's next panics on "tick": the spec has no next state"#
        );
    }

    #[test]
    fn ends_the_run_with_a_fault_at_a_spec_whose_initial_state_panics() {
        let seat = run(Options::default(), |c| {
            let tick = Action::new("tick", async |_: &Case<'_>, _: usize| {});
            stateful::steps(
                c,
                &Machine::new([tick]).spec(Unstarted),
                stateful::swarm(false),
            );
        });
        assert_eq!(
            seat.message(),
            "prop::for_all: stateful::steps: the spec's initial panics: the spec has no initial state"
        );
    }

    #[test]
    fn waits_for_a_step_that_another_thread_completes() {
        let seat = run(Options::default().cases(3), |c| {
            let machine =
                Machine::<i64>::new([Action::new("await", async |_: &Case<'_>, _: usize| {
                    let shared = Arc::new((Mutex::new((false, None::<Waker>)), Condvar::new()));
                    let other = Arc::clone(&shared);
                    std::thread::spawn(move || {
                        let (state, registered) = &*other;
                        let mut held = registered
                            .wait_while(locked(state), |held| held.1.is_none())
                            .unwrap_or_else(PoisonError::into_inner);
                        held.0 = true;
                        if let Some(waker) = held.1.take() {
                            waker.wake();
                        }
                    });
                    poll_fn(|context| {
                        let (state, registered) = &*shared;
                        let mut held = locked(state);
                        if held.0 {
                            return Poll::Ready(());
                        }
                        held.1 = Some(context.waker().clone());
                        registered.notify_all();
                        Poll::Pending
                    })
                    .await;
                })]);
            stateful::steps(c, &machine, stateful::max(2).swarm(false));
        });
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn rejects_a_case_past_its_cap_on_choices() {
        let seat = run(prop::max_choices(1), |c| {
            let log = Mutex::new(Vec::new());
            stateful::steps(c, &logged(&log), stateful::swarm(false));
        });
        assert_eq!(
            seat.failures()[0].field("outcome"),
            Some(&Value::from("rejected"))
        );
    }
}

mod traces {
    use super::*;

    #[test]
    fn takes_the_steps_of_a_trace_in_its_first_case() {
        let first = Mutex::new(None);
        let seat = run(
            prop::draws(r#"[{"step": "increment"}, {"step": "reset"}]"#),
            |c| {
                let log = Mutex::new(Vec::new());
                stateful::steps(c, &logged(&log), stateful::max(5));
                let mut first = locked(&first);
                if first.is_none() {
                    *first = Some(locked(&log).clone());
                }
            },
        );
        assert!(!seat.failed(), "{}", seat.message());
        let taken = first.into_inner().unwrap_or_else(PoisonError::into_inner);
        assert_eq!(
            taken,
            Some(vec!["increment on 0".to_owned(), "reset on 0".to_owned()])
        );
    }

    #[test]
    fn takes_a_concurrent_step_of_a_trace_on_its_client() {
        let first = Mutex::new(None);
        let seat = run(
            prop::draws(r#"[{"step": "increment", "client": 2}]"#),
            |c| {
                let log = Mutex::new(Vec::new());
                stateful::steps(c, &logged(&log), stateful::clients(2));
                let mut first = locked(&first);
                if first.is_none() {
                    *first = Some(locked(&log).clone());
                }
            },
        );
        assert!(!seat.failed(), "{}", seat.message());
        let taken = first.into_inner().unwrap_or_else(PoisonError::into_inner);
        assert_eq!(taken, Some(vec!["increment on 2".to_owned()]));
    }

    #[test]
    fn refuses_a_step_whose_action_the_machine_lacks() {
        assert_eq!(
            refusal(r#"[{"step": "made-up"}]"#, stateful::max(5)),
            r#"prop::for_all: draws[0].step: the machine cannot take the step "made-up" there"#
        );
    }

    #[test]
    fn refuses_a_step_past_the_most_steps() {
        let refused = refusal(
            r#"[{"step": "increment"}, {"step": "increment"}]"#,
            stateful::max(1),
        );
        assert!(
            refused.starts_with("prop::for_all: draws[1].step: "),
            "{refused}"
        );
    }

    #[test]
    fn refuses_a_concurrent_step_of_a_machine_with_one_client() {
        let refused = refusal(r#"[{"step": "increment", "client": 1}]"#, stateful::max(5));
        assert!(
            refused.starts_with("prop::for_all: draws[0].step: "),
            "{refused}"
        );
    }

    #[test]
    fn refuses_a_concurrent_step_of_a_client_outside_the_clients() {
        let refused = refusal(
            r#"[{"step": "increment", "client": 5}]"#,
            stateful::clients(2),
        );
        assert!(
            refused.starts_with("prop::for_all: draws[0].step: "),
            "{refused}"
        );
    }

    #[test]
    fn leaves_a_sequential_step_after_the_concurrent_section_untaken() {
        let first = Mutex::new(None);
        let seat = run(
            prop::draws(r#"[{"step": "increment", "client": 1}, {"step": "reset"}]"#),
            |c| {
                let log = Mutex::new(Vec::new());
                stateful::steps(c, &logged(&log), stateful::clients(2));
                let mut first = locked(&first);
                if first.is_none() {
                    *first = Some(locked(&log).clone());
                }
            },
        );
        assert!(!seat.failed(), "{}", seat.message());
        let taken = first.into_inner().unwrap_or_else(PoisonError::into_inner);
        assert_eq!(taken, Some(vec!["increment on 1".to_owned()]));
    }

    #[test]
    fn takes_and_refuses_the_drain_steps_of_a_trace() {
        let machine = |c: &Case<'_>| {
            let buffer = AtomicUsize::new(0);
            let machine = Machine::<i64>::new([
                Action::new("put", async |_: &Case<'_>, _: usize| {
                    _ = buffer.fetch_add(1, Ordering::SeqCst);
                }),
                Action::new("flush", async |_: &Case<'_>, _: usize| {
                    buffer.store(0, Ordering::SeqCst);
                })
                .enabled(|_| buffer.load(Ordering::SeqCst) > 0)
                .drain(),
            ]);
            stateful::steps(c, &machine, stateful::max(5));
        };
        let taken = run(
            prop::draws(r#"[{"step": "put"}, {"step": "flush", "drain": true}]"#),
            machine,
        );
        assert!(!taken.failed(), "{}", taken.message());
        let refused = run(
            prop::draws(r#"[{"step": "flush", "drain": true}]"#),
            machine,
        );
        assert!(
            refused
                .message()
                .starts_with("prop::for_all: draws[0].step: "),
            "{}",
            refused.message()
        );
    }
}
