//! The spec of `history::concurrently`, which runs the clients of a history
//! together.

use crate::helpers::panic_of;
use dokimi_assert::history;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

mod concurrently {
    use super::*;

    #[test]
    fn returns_the_result_of_each_client_in_client_order() {
        let outcomes = history::concurrently(3, Duration::from_secs(60), |client| {
            if client == 1 {
                Err("refused")
            } else {
                Ok(client * 10)
            }
        });
        let results: Vec<(usize, Option<Result<usize, &str>>)> = outcomes
            .iter()
            .map(|outcome| (outcome.client, outcome.result))
            .collect();
        assert_eq!(
            results,
            [
                (0, Some(Ok(0))),
                (1, Some(Err("refused"))),
                (2, Some(Ok(20)))
            ]
        );
        assert!(outcomes.iter().all(history::Outcome::finished));
    }

    #[test]
    fn returns_no_result_for_a_client_that_runs_past_the_time() {
        let (release, released) = mpsc::channel::<()>();
        let released = Arc::new(Mutex::new(released));
        let outcomes = history::concurrently(2, Duration::from_millis(500), move |client| {
            if client == 1 {
                let _ = released.lock().map(|waiting| waiting.recv());
            }
            Ok::<usize, ()>(client)
        });
        let _ = release.send(());
        assert_eq!(outcomes[0].result, Some(Ok(0)));
        assert_eq!(outcomes[1].result, None);
        assert!(!outcomes[1].finished());
    }

    #[test]
    fn raises_the_panic_of_the_lowest_client_that_panicked() {
        let raised = panic_of(|| {
            history::concurrently(3, Duration::from_secs(60), |client| {
                assert!(client == 0, "client {client} panics");
                Ok::<(), ()>(())
            });
        });
        assert_eq!(raised.as_deref(), Some("client 1 panics"));
    }

    #[test]
    fn panics_for_no_client() {
        let raised = panic_of(|| {
            drop(history::concurrently(0, Duration::from_secs(1), |_| {
                Ok::<(), ()>(())
            }));
        });
        assert_eq!(
            raised.as_deref(),
            Some("history: concurrently(0, 1s) starts no client")
        );
    }
}
