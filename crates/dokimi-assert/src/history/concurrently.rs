// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! Running the clients of a history together.

use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::mpsc;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

/// What one client of [`concurrently`] did.
#[derive(Debug)]
pub struct Outcome<O, E> {
    /// The client's number, from 0.
    pub client: usize,
    /// What the body returned, or `None` for a body that was still running
    /// when the time of `concurrently` passed.
    pub result: Option<Result<O, E>>,
}

impl<O, E> Outcome<O, E> {
    /// Returns `true` when the body returned before the time of
    /// `concurrently` passed.
    #[must_use]
    pub fn finished(&self) -> bool {
        self.result.is_some()
    }
}

/// How one body ended: its result, or the payload of its panic.
type Exit<O, E> = (usize, thread::Result<Result<O, E>>);

/// Runs `clients` copies of `body` on threads of their own, the i-th with
/// client number i, from 0, and returns one outcome per client, in client
/// order.
///
/// The bodies start together once every thread has started, and the call
/// waits until every body has returned or `within` has passed on the
/// platform clock. A body that is still running then has an outcome without
/// a result. Its thread runs on, because a thread cannot be stopped from
/// outside, and a call that it opened in a history is pending there. The
/// threads outlive the call, so `body` and its results are `'static`.
///
/// ```
/// use dokimi_assert::history;
/// use std::time::Duration;
///
/// let outcomes = history::concurrently(4, Duration::from_secs(10), |client| Ok::<_, ()>(client * 2));
/// assert!(outcomes.iter().all(|outcome| outcome.finished()));
/// ```
///
/// # Panics
///
/// Panics for no client. A body that panics before the time passes panics
/// again on the caller's thread, after the other bodies, with the payload of
/// the lowest-numbered client that panicked. The panic hook prints each
/// body's panic on its own thread, when it happens.
pub fn concurrently<O, E>(
    clients: usize,
    within: Duration,
    body: impl Fn(usize) -> Result<O, E> + Send + Sync + 'static,
) -> Vec<Outcome<O, E>>
where
    O: Send + 'static,
    E: Send + 'static,
{
    assert!(
        clients >= 1,
        "history: concurrently(0, {within:?}) starts no client"
    );
    let body = Arc::new(body);
    let release = Arc::new(Barrier::new(clients + 1));
    let (sender, exits) = mpsc::channel::<Exit<O, E>>();
    for client in 0..clients {
        let (body, release, sender) = (Arc::clone(&body), Arc::clone(&release), sender.clone());
        thread::spawn(move || {
            release.wait();
            let _ = sender.send((client, catch_unwind(AssertUnwindSafe(|| body(client)))));
        });
    }
    release.wait();
    let deadline = Instant::now() + within;
    let mut outcomes: Vec<Outcome<O, E>> = (0..clients)
        .map(|client| Outcome {
            client,
            result: None,
        })
        .collect();
    let mut raised: Option<(usize, Box<dyn std::any::Any + Send>)> = None;
    for _ in 0..clients {
        let Ok((client, exit)) =
            exits.recv_timeout(deadline.saturating_duration_since(Instant::now()))
        else {
            break;
        };
        match exit {
            Ok(result) => outcomes[client].result = Some(result),
            Err(payload) => {
                if raised.as_ref().is_none_or(|(lowest, _)| client < *lowest) {
                    raised = Some((client, payload));
                }
            }
        }
    }
    if let Some((_, payload)) = raised {
        resume_unwind(payload);
    }
    outcomes
}
