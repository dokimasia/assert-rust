<!--
  ~ Copyright Dokimasia B.V. 2026
  ~ SPDX-License-Identifier: MIT
-->

# dokimi-assert-tokio

The assertions of [dokimi-assert](https://github.com/dokimasia/assert-rust)
whose subject is a future, for tests that run on tokio.

The crates are not published on crates.io. Cargo takes them from their
repository:

```toml
[dev-dependencies]
dokimi-assert = { git = "https://github.com/dokimasia/assert-rust" }
dokimi-assert-tokio = { git = "https://github.com/dokimasia/assert-rust" }
tokio = { version = "1.54", features = ["macros", "rt", "test-util", "time"] }
```

`#[tokio::test]` needs the feature `macros`, and `start_paused` needs
`test-util`.

```rust
use dokimi_assert::Collector;
use dokimi_assert_tokio::check;
use std::time::Duration;

#[tokio::test(start_paused = true)]
async fn the_job_ends_within_a_second() {
    let t = Collector::new();
    let job = tokio::time::sleep(Duration::from_millis(200));

    check::completes_within(&t, Duration::from_secs(1), job, "the job ends within a second").await;
}
```

Each assertion returns a future that the test awaits. `check` ends the test
at the first failure, and `soft` records the failure and lets the test
continue.

- `honours_cancellation` and `honours_deadline` hand the subject a stopped
  `Cancel`. An asynchronous subject awaits `Cancel::stopped`.
- `completes_within` drops a future that has not completed in time, as
  `tokio::time::timeout` drops it.
- `eventually` and `eventually_true` retry on tokio's clock, so a test that
  pauses the clock does not spend real time.
- `no_task_leaks` fails when a task that the body spawned on the runtime
  still runs half a second after the body completes.

The first five have the names of their synchronous counterparts in
`dokimi-assert`. `no_task_leaks` has no synchronous counterpart, because
Rust's standard library cannot list the threads of a process.

The crate needs Rust 1.85 or later and is licensed under MIT.
