//! The spec of `cancel`, the handle that tells a subject to stop.

use dokimi_assert::{Cancel, Stop};
use std::pin::pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll, Wake, Waker};

/// A waker that counts how often it is woken.
#[derive(Default)]
struct Wakes(AtomicUsize);

impl Wake for Wakes {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

impl Wakes {
    fn count(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }
}

/// Returns a waker that counts how often it is woken, with its counter.
fn counting_waker() -> (Waker, Arc<Wakes>) {
    let counter = Arc::new(Wakes::default());
    (Waker::from(Arc::clone(&counter)), counter)
}

mod reason {
    use super::*;

    #[test]
    fn returns_none_for_a_new_handle() {
        assert_eq!(Cancel::new().reason(), None);
        assert!(!Cancel::new().is_stopped());
    }

    #[test]
    fn returns_cancelled_after_cancel() {
        let handle = Cancel::new();
        handle.cancel();
        assert_eq!(handle.reason(), Some(Stop::Cancelled));
        assert!(handle.is_stopped());
    }

    #[test]
    fn returns_the_deadline_after_expire() {
        assert_eq!(Cancel::expired().reason(), Some(Stop::DeadlineExceeded));
        assert_eq!(Cancel::cancelled().reason(), Some(Stop::Cancelled));
    }

    #[test]
    fn keeps_the_first_reason() {
        let handle = Cancel::cancelled();
        handle.expire();
        assert_eq!(handle.reason(), Some(Stop::Cancelled));
    }

    #[test]
    fn returns_the_reason_of_a_clone() {
        let handle = Cancel::new();
        let clone = handle.clone();
        clone.expire();
        assert_eq!(handle.reason(), Some(Stop::DeadlineExceeded));
    }
}

mod child {
    use super::*;

    #[test]
    fn stops_when_its_parent_stops() {
        let parent = Cancel::new();
        let child = parent.child();
        assert_eq!(child.reason(), None);
        parent.expire();
        assert_eq!(child.reason(), Some(Stop::DeadlineExceeded));
    }

    #[test]
    fn stops_on_its_own_without_stopping_its_parent() {
        let parent = Cancel::new();
        let child = parent.child();
        child.cancel();
        assert_eq!(child.reason(), Some(Stop::Cancelled));
        assert_eq!(parent.reason(), None);
    }

    #[test]
    fn keeps_its_own_reason_when_its_parent_stops_later() {
        let parent = Cancel::new();
        let child = parent.child();
        child.cancel();
        parent.expire();
        assert_eq!(child.reason(), Some(Stop::Cancelled));
    }
}

mod stopped {
    use super::*;

    #[test]
    fn completes_with_the_reason_of_a_stopped_handle() {
        let mut cx = Context::from_waker(Waker::noop());
        let stopped = pin!(Cancel::expired().stopped());
        assert_eq!(stopped.poll(&mut cx), Poll::Ready(Stop::DeadlineExceeded));
    }

    #[test]
    fn wakes_the_task_when_the_handle_stops() {
        let (waker, counter) = counting_waker();
        let mut cx = Context::from_waker(&waker);
        let handle = Cancel::new();
        let mut stopped = pin!(handle.stopped());
        assert_eq!(stopped.as_mut().poll(&mut cx), Poll::Pending);
        handle.cancel();
        assert_eq!(counter.count(), 1);
        assert_eq!(stopped.poll(&mut cx), Poll::Ready(Stop::Cancelled));
    }

    #[test]
    fn wakes_the_task_when_a_parent_stops() {
        let (waker, counter) = counting_waker();
        let mut cx = Context::from_waker(&waker);
        let parent = Cancel::new();
        let child = parent.child().child();
        let mut stopped = pin!(child.stopped());
        assert_eq!(stopped.as_mut().poll(&mut cx), Poll::Pending);
        parent.expire();
        assert_eq!(counter.count(), 1);
        assert_eq!(stopped.poll(&mut cx), Poll::Ready(Stop::DeadlineExceeded));
    }

    #[test]
    fn wakes_a_task_once_however_often_it_polls() {
        let (waker, counter) = counting_waker();
        let mut cx = Context::from_waker(&waker);
        let handle = Cancel::new();
        let mut stopped = pin!(handle.stopped());
        assert_eq!(stopped.as_mut().poll(&mut cx), Poll::Pending);
        assert_eq!(stopped.as_mut().poll(&mut cx), Poll::Pending);
        handle.cancel();
        handle.expire();
        assert_eq!(counter.count(), 1);
    }
}

mod stop {
    use super::*;

    #[test]
    fn renders_each_reason() {
        assert_eq!(Stop::Cancelled.to_string(), "cancelled");
        assert_eq!(Stop::DeadlineExceeded.to_string(), "deadline exceeded");
    }

    #[test]
    fn is_an_error_that_a_chain_can_contain() {
        let error: Box<dyn std::error::Error> = Box::new(Stop::Cancelled);
        assert_eq!(error.downcast_ref::<Stop>(), Some(&Stop::Cancelled));
    }
}

mod debug {
    use super::*;

    #[test]
    fn shows_the_reason() {
        assert_eq!(
            format!("{:?}", Cancel::cancelled()),
            "Cancel { reason: Some(Cancelled), .. }"
        );
    }
}
