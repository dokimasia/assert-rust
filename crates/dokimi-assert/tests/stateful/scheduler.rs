// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `stateful::scheduler`, the task scheduler.

use dokimi_assert::prop::Case;
use dokimi_assert::stateful::{self, Scheduler, Strategy};
use dokimi_assert::{Recorder, prop};
use std::collections::BTreeSet;
use std::future::{Future, poll_fn};
use std::pin::pin;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, Waker};

/// Takes a lock whose poison a panicking task left.
fn locked<T>(lock: &Mutex<T>) -> MutexGuard<'_, T> {
    lock.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A flag that one task awaits and another sets, which wakes the waiting
/// task.
#[derive(Default)]
struct Flag {
    state: Mutex<(bool, Option<Waker>)>,
}

impl Flag {
    fn set(&self) {
        let mut state = locked(&self.state);
        state.0 = true;
        if let Some(waker) = state.1.take() {
            waker.wake();
        }
    }

    fn wait(&self) -> impl Future<Output = ()> + '_ {
        poll_fn(|context: &mut Context<'_>| {
            let mut state = locked(&self.state);
            if state.0 {
                return Poll::Ready(());
            }
            state.1 = Some(context.waker().clone());
            Poll::Pending
        })
    }
}

/// Runs `body` as a property of 30 cases with the seed 1 on a recorder, and
/// returns the recorder.
fn run(body: impl Fn(&Case<'_>) + Send + Sync) -> Recorder {
    let seat = Recorder::new();
    prop::for_all(
        &seat,
        "the tasks run",
        prop::hermetic().seed(1).cases(30),
        body,
    );
    seat
}

/// Returns the first entries of the logs of two tasks that each log their
/// name, yield and log it again, under `strategy`, one per case.
fn first_entries(strategy: Strategy) -> BTreeSet<&'static str> {
    let firsts = Mutex::new(BTreeSet::new());
    let seat = run(|c| {
        let log = Mutex::new(Vec::new());
        let scheduler = Scheduler::new(c, strategy);
        for name in ["a", "b", "c"] {
            let log = &log;
            scheduler.spawn(async move |s| {
                locked(log).push(name);
                s.yield_now().await;
                locked(log).push(name);
            });
        }
        scheduler.run();
        let log = locked(&log);
        assert_eq!(log.len(), 6);
        locked(&firsts).insert(log[0]);
    });
    assert!(!seat.failed(), "{}", seat.message());
    firsts.into_inner().unwrap_or_else(PoisonError::into_inner)
}

mod scheduler {
    use super::*;

    #[test]
    fn releases_the_tasks_in_orders_that_the_case_chooses() {
        assert_eq!(
            first_entries(stateful::uniform()),
            BTreeSet::from(["a", "b", "c"])
        );
    }

    #[test]
    fn releases_the_tasks_by_priority_under_pct() {
        assert_eq!(
            first_entries(stateful::pct(3)),
            BTreeSet::from(["a", "b", "c"])
        );
    }

    #[test]
    fn rejects_a_case_whose_releases_pass_its_cap_on_choices() {
        for strategy in [stateful::uniform(), stateful::pct(2)] {
            let seat = Recorder::new();
            prop::for_all(
                &seat,
                "the tasks run",
                prop::hermetic().seed(1).cases(5).max_choices(1),
                |c| {
                    let scheduler = Scheduler::new(c, strategy);
                    for _ in 0..3 {
                        scheduler.spawn(async move |s| s.yield_now().await);
                    }
                    scheduler.run();
                },
            );
            let record: serde_json::Value =
                serde_json::from_str(&seat.records()[0]).expect("a call record is JSON");
            assert_eq!(record["detail"]["outcome"], "rejected", "{record}");
        }
    }

    #[test]
    fn continues_a_task_that_another_task_woke() {
        let seat = run(|c| {
            let (flag, log) = (Flag::default(), Mutex::new(Vec::new()));
            let scheduler = Scheduler::new(c, stateful::uniform());
            let (flag, log) = (&flag, &log);
            scheduler.spawn(async move |_| {
                flag.wait().await;
                locked(log).push("woken");
            });
            scheduler.spawn(async move |s| {
                s.yield_now().await;
                flag.set();
            });
            scheduler.run();
            assert_eq!(*locked(log), ["woken"]);
        });
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn ends_the_case_with_a_fault_at_a_task_that_no_task_wakes() {
        let seat = run(|c| {
            let scheduler = Scheduler::new(c, stateful::uniform());
            scheduler.spawn(async |_| std::future::pending::<()>().await);
            scheduler.run();
        });
        assert_eq!(
            seat.message(),
            "prop::for_all: stateful: the tasks 0 wait on a future that no task of the scheduler wakes"
        );
    }

    #[test]
    fn raises_the_panic_of_a_task_on_the_thread_of_the_run() {
        let seat = run(|c| {
            let scheduler = Scheduler::new(c, stateful::uniform());
            scheduler.spawn(async |_| panic!("the task fails"));
            scheduler.run();
        });
        let failure = &seat.failures()[0];
        assert_eq!(
            failure
                .case_failure()
                .map(|case| case.contract().to_owned())
                .as_deref(),
            Some("the task fails")
        );
    }

    #[test]
    fn panics_when_a_task_runs_its_own_scheduler() {
        let seat = run(|c| {
            let scheduler = Scheduler::new(c, stateful::uniform());
            scheduler.spawn(async |s| s.run());
            scheduler.run();
        });
        let failure = &seat.failures()[0];
        assert_eq!(
            failure
                .case_failure()
                .map(|case| case.contract().to_owned())
                .as_deref(),
            Some("stateful: a task called run of its own scheduler")
        );
    }

    #[test]
    fn makes_a_yield_outside_a_turn_ready_at_once() {
        let seat = run(|c| {
            c.draw(&prop::boolean(), "flag");
            let scheduler = Scheduler::new(c, stateful::uniform());
            let mut yielded = pin!(scheduler.yield_now());
            let mut context = Context::from_waker(Waker::noop());
            assert_eq!(yielded.as_mut().poll(&mut context), Poll::Ready(()));
            assert_eq!(
                format!("{scheduler:?}"),
                "Scheduler { strategy: Strategy { depth: 0 }, .. }"
            );
        });
        assert!(!seat.failed(), "{}", seat.message());
    }
}
