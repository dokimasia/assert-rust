<!--
  ~ Copyright Dokimasia B.V. 2026
  ~ SPDX-License-Identifier: MIT
-->

# dokimi-assert

Test assertions, property-based tests, history checks and state machines for
Rust. A language-neutral standard defines what each one checks and what its
failure reports, and this repository runs the standard's cases against the
library.

[![CI](https://github.com/dokimasia/assert-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/dokimasia/assert-rust/actions/workflows/ci.yml)
[![Licence](https://img.shields.io/badge/licence-MIT-blue)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-blue)](https://www.rust-lang.org/)

The crates are not published on crates.io. Cargo takes them from this
repository:

```toml
[dev-dependencies]
dokimi-assert = { git = "https://github.com/dokimasia/assert-rust", features = ["derive"] }
# The assertions whose subject is a future, for tests on tokio.
dokimi-assert-tokio = { git = "https://github.com/dokimasia/assert-rust" }
```

The crates need Rust 1.85 or later, and they use edition 2024. The features
of `dokimi-assert` add optional parts:

- `derive` adds `#[derive(prop::Arbitrary)]`.
- `chrono`, `jiff`, `time`, `uuid` and `rust_decimal` implement
  `prop::Arbitrary` for the types of those crates.
- `rand_core` lets the random source of a property's case serve as a
  `rand_core::Rng`.

## Contents

- [Use cases](#use-cases)
- [Getting started](#getting-started)
- [Two surfaces](#two-surfaces)
- [Seats](#seats)
- [The assertions](#the-assertions)
- [Files](#files)
- [Golden files](#golden-files)
- [Properties](#properties)
- [Histories](#histories)
- [Machines](#machines)
- [Futures](#futures)
- [Benchmark ceilings](#benchmark-ceilings)
- [Equality](#equality)
- [Call records](#call-records)
- [The standard](#the-standard)
- [Development](#development)

## Use cases

`assert_eq!` compares two values, and
[pretty_assertions](https://crates.io/crates/pretty_assertions) adds a diff
to its failure. Use this library for the checks that Rust has no other way
to state:

- **Soft assertions.** `assert_eq!` stops at the first failure. `soft`
  records each failure and lets the test continue, so one run reports every
  failure with its line.
- **Assertions about behaviour.** A subject stops when its caller cancels it,
  leaves state alone, runs without a handle, returns its input through a
  round trip, and allocates no more than a ceiling allows.
- **Files and golden output.** A test writes a tree of files into a
  directory of its own and compares the directory with the tree that it
  wants. A golden file records output, and the next run compares its output
  with the file.
- **Properties.** `prop` runs a body against generated inputs, shrinks a
  failing case to the smallest one that fails the same way, and stores it
  for the next run.
- **Concurrency.** `history` checks a recorded history of concurrent calls
  for linearizability, serializability and snapshot isolation. `stateful`
  drives a subject through generated steps, on threads or as tasks whose
  order replays.
- **The same meaning in every language.** Go, Python, TypeScript, Java,
  Kotlin and Rust implement one definition, so a Rust test and its Go port
  check the same thing.

## Getting started

```rust
use dokimi_assert::{Collector, check};

#[test]
fn trim_removes_the_padding() {
    let t = Collector::new();
    let got = "  widget  ".trim();

    check::is_not_empty(&t, got, "the trimmed name is not empty");
    check::equal(&t, got, "widget", "trim removes the padding");
}
```

Each assertion takes the seat first and the message last. The message is
the contract under test, and a failure starts with it:

```text
trim removes the padding: want "widget", got "gadget"
```

Every assertion has `#[track_caller]`, so the location of a failure is the
line of the test, not a line of the library.

`check::rejects` takes the message before its body. A golden comparison
takes no message, because the library builds its contract from the path of
the golden file. A benchmark contract takes the message in
`Contract::new`.

## Two surfaces

`check` ends the test at the first failure. `soft` records the failure and
lets the test continue. Both modules have the same assertions under the
same names, except `rejects`, which only `check` has.

```rust
use dokimi_assert::{Collector, check, soft};

#[test]
fn the_reply_lists_every_item() {
    let t = Collector::new();
    let (status, body, items) = (200, "[1,2]", vec![1, 2]);

    check::equal(&t, status, 200, "the request succeeds");
    soft::has_prefix(&t, body, "{", "the body is JSON");
    soft::length(&t, &items, 3, "every item comes back");
}
```

The collector drops at the end of the test and reports both failures, each
with its line:

```text
2 failures:
  1. the body is JSON: got "[1,2]", prefix "{"
     at tests/api.rs:9
  2. every item comes back: want 3, got 2
     at tests/api.rs:10
```

## Seats

An assertion reports each failure to the `Seat` that the test passes,
instead of calling a test framework or panicking. What a failure does
depends on the seat:

| Seat | A failure on `check` | A failure on `soft` |
|---|---|---|
| `Collector` | Panics | Is collected, and the collector panics with every collected failure when it drops |
| `Standard` | Panics | Panics |
| `Recorder` | Is kept | Is kept |

- `Collector` is the seat of a test. A collector that drops while its thread
  unwinds from another panic does not panic again, because a second panic
  aborts the process. `Collector::flush` reports the collected failures at a
  point that the test chooses.
- `Standard` is the seat of code without a test, such as a fuzz target. A
  failure on `soft` panics there too. A collected failure needs the end of a
  test, and this seat has none.
- `Recorder` is the seat of a test of an assertion. Nothing that runs on it
  fails the test. `failed`, `message`, `messages`, `failures`, `faults` and
  `records` return what the assertion reported. `Recorder::with_clock` sets
  the clock that the timed assertions read, and
  `Recorder::with_cancellation` sets the cancellation handle of the test.
- A type of your own implements `Seat` with `fail` and `record`. The trait's
  default methods return the platform clock and a cancellation handle that
  never stops, and they render the sentence of each failure.

A fault is an argument or an environment that a call refuses, such as a
pattern that does not compile. The seat receives the fault as the error it
is through `Seat::report_fault`. A `Recorder` keeps it, and the default
method sends its text through `fail`. `dokimi_assert::fault` ends the call
of a helper of your own the same way:

```rust
use dokimi_assert::{Seat, fault};
use std::io;

fn restart(seat: &dyn Seat, restartable: bool) {
    if !restartable {
        fault(seat, io::Error::other("the service cannot restart"));
        return;
    }
    // …
}
```

## The assertions

`check` has 49 assertions, and `soft` has the same names without `rejects`.
Each signature in this section leaves out `seat: &dyn Seat`, the first
parameter, and `msg: &str`, the last.

**Equality.** `equal` and `not_equal` compare with `==`, so they accept any
pair of types that `PartialEq` relates, such as a `String` and a `&str`.

```rust
check::equal<G, W>(got: G, want: W) where G: PartialEq<W> + Debug, W: Debug
check::not_equal<G, W>(got: G, want: W) where G: PartialEq<W> + Debug, W: Debug
```

**Truth and absence.**

```rust
check::is_true(condition: bool)
check::is_false(condition: bool)
check::is_none<T: Debug>(got: Option<T>)
check::is_some<T: Debug>(got: Option<T>)
```

**Size.** Each takes a `Container`: `str`, `String`, a slice, an array,
`Vec`, `VecDeque`, `LinkedList`, `HashMap`, `BTreeMap`, `HashSet`,
`BTreeSet`, or a reference, a `Box` or an `Option` of one. The length of
text is its number of Unicode scalar values. `None` has no length and
fails all three.

```rust
check::length<C: Container + ?Sized>(got: &C, want: usize)
check::is_empty<C: Container + ?Sized>(got: &C)
check::is_not_empty<C: Container + ?Sized>(got: &C)
```

**Containment.** Text contains a substring, a sequence contains an
element, and a map or a set contains a key. `is_permutation` compares
elements with `PartialEq` alone. It does not need `Ord` or `Hash`.

```rust
check::contains<H, N>(haystack: &H, needle: &N) where H: Contains<N> + Debug + ?Sized, N: Debug + ?Sized
check::not_contains<H, N>(haystack: &H, needle: &N) where H: Contains<N> + Debug + ?Sized, N: Debug + ?Sized
check::contains_in_order(got: &str, needles: &[&str])
check::is_permutation<T: PartialEq + Debug>(got: &[T], want: &[T])
```

**Text.** The patterns of `matches` are the portable subset of regular
expressions. Every implementation interprets that subset the same way. In
it `\d`, `\w` and `\s` are ASCII classes, `.` matches no line terminator,
and `$` matches at the end of the text alone. A pattern passes when it
matches anywhere in the text.

```rust
check::has_prefix(got: &str, prefix: &str)
check::has_suffix(got: &str, suffix: &str)
check::matches(got: &str, pattern: &str)
```

**Numbers.** Every integer and float type is a `Number`, so a test passes a
`u64` or an `f32` without a cast. The tolerance is an absolute difference.
NaN is outside every tolerance and every range.

```rust
check::close_to<N: Number>(got: N, want: f64, tolerance: f64)
check::in_range<N: Number>(got: N, low: f64, high: f64)
```

**Order.** `pairwise` covers sorted, unique and strictly increasing,
because each of those is a relation between neighbours.

```rust
check::pairwise<T: Debug>(items: &[T], predicate: impl Fn(&T, &T) -> bool)
```

**Errors.** Rust puts failure in the type, so these assertions take a
`Result` or an error. `error_is`, `error_is_not` and `error_as` search the
chain of `Error::source`.

```rust
check::no_error<T, E: Debug>(got: &Result<T, E>)
check::has_error<T: Debug, E>(got: &Result<T, E>)
check::error_is<T>(error: &(dyn Error + 'static), target: &T) where T: Error + PartialEq + 'static
check::error_is_not<T>(error: &(dyn Error + 'static), target: &T) where T: Error + PartialEq + 'static
check::error_as<'e, T: Error + 'static>(error: &'e (dyn Error + 'static)) -> Option<&'e T>
```

**Panics.** `panics` returns the message of the panic. The assertion does
not print a panic that it expects.

```rust
check::panics(body: impl FnOnce()) -> Option<String>
check::does_not_panic(body: impl FnOnce())
```

**Behaviour.** A subject that can stop takes an `Option<&Cancel>`. `Cancel`
is the handle that the subject reads to learn that it should stop, and why:
`Stop::Cancelled` or `Stop::DeadlineExceeded`. The subject returns the
reason as its error or as the source of its error. Synchronous code reads
`Cancel::reason`. Asynchronous code awaits `Cancel::stopped`, which needs no
particular runtime.

```rust
use dokimi_assert::{Cancel, Collector, Stop, check};

/// Returns a page, or the reason of the handle when it has stopped.
fn fetch(cancel: Option<&Cancel>) -> Result<String, Stop> {
    if let Some(reason) = cancel.and_then(Cancel::reason) {
        return Err(reason);
    }
    Ok("fetched".to_owned())
}

#[test]
fn fetch_stops_when_asked() {
    let t = Collector::new();
    check::honours_cancellation(&t, |cancel| fetch(cancel).map(drop), "fetch stops on a cancelled handle");
    check::honours_deadline(&t, |cancel| fetch(cancel).map(drop), "fetch stops on an expired handle");
    check::none_handle_safe(&t, |cancel| fetch(cancel).map(drop), "fetch runs without a handle");
}
```

```rust
check::honours_cancellation<E: Error + 'static>(body: impl FnOnce(Option<&Cancel>) -> Result<(), E>)
check::honours_deadline<E: Error + 'static>(body: impl FnOnce(Option<&Cancel>) -> Result<(), E>)
check::completes_within<E: Debug + Send + 'static>(within: Duration, body: impl FnOnce(Option<&Cancel>) -> Result<(), E> + Send + 'static)
check::none_handle_safe<E: Debug>(body: impl FnOnce(Option<&Cancel>) -> Result<(), E>)
check::is_pure<S: PartialEq + Debug>(observe: impl Fn() -> S, body: impl FnOnce())
check::is_not_pure<S: PartialEq + Debug>(observe: impl Fn() -> S, body: impl FnOnce())
```

`completes_within` runs the body on a thread of its own, with a handle that
expires when the duration passes. The assertion does not wait for a body
that has not returned by then, so the body is `Send + 'static`.

**Relations.** Each relation calls the subject with the inputs that the test
passes:

```rust
check::is_idempotent<I, S, E>(call: impl FnMut(&I) -> Result<(), E>, input: I, observe: impl FnMut() -> S) where S: PartialEq + Debug, E: Debug
check::accumulates<I, E: Debug>(call: impl FnMut(&I) -> Result<(), E>, input: I, observe: impl FnMut() -> i64)
check::is_deterministic<I, O, E>(call: impl FnMut(&I) -> Result<O, E>, input: I) where O: PartialEq + Debug, E: Debug
check::is_commutative<T, R>(combine: impl FnMut(&T, &T) -> R, a: T, b: T) where R: PartialEq + Debug
check::is_associative<T>(combine: impl FnMut(&T, &T) -> T, a: T, b: T, c: T) where T: PartialEq + Debug
check::round_trip<I, O, E, F>(forward: impl FnOnce(&I) -> Result<O, E>, inverse: impl FnOnce(&O) -> Result<I, F>, input: I) where I: PartialEq + Debug, E: Debug, F: Debug
check::has_stable_order<T, E>(iterate: impl FnMut() -> Result<Vec<T>, E>) where T: PartialEq + Debug, E: Debug
check::no_duplicates<T, E>(iterate: impl FnOnce() -> Result<Vec<T>, E>) where T: PartialEq + Debug, E: Debug
check::is_monotonic<N, E>(observe: impl FnMut() -> N, advance: impl FnMut() -> Result<(), E>, steps: usize) where N: PartialOrd + Debug, E: Debug
check::is_total<I, E: Debug>(call: impl FnMut(&I) -> Result<(), E>, domain: &[I])
check::fails_after_close<T, E, C>(close: impl FnOnce() -> Result<(), C>, call: impl FnOnce() -> Result<(), E>, sentinel: &T) where T: Error + PartialEq + 'static, E: Error + 'static, C: Debug
check::is_poisoned<E: Debug>(induce: impl FnOnce(), observe: impl FnMut() -> Result<(), E>)
```

| Assertion | Fails when |
|---|---|
| `is_idempotent` | A second call with `input` leaves `observe` returning other than the first call left it |
| `accumulates` | The first call leaves the count unchanged, or a second call changes it by another amount |
| `is_deterministic` | One of 32 calls returns a result other than the first |
| `is_commutative` | `combine(a, b)` differs from `combine(b, a)` |
| `is_associative` | `combine(combine(a, b), c)` differs from `combine(a, combine(b, c))` |
| `round_trip` | `inverse` of `forward` of `input` does not return `input` |
| `has_stable_order` | One of 32 iterations yields a sequence other than the first |
| `no_duplicates` | One iteration yields an element twice |
| `is_monotonic` | The reading of `observe` falls, or is NaN, while `advance` runs `steps` times |
| `is_total` | `call` returns an error or panics for an element of `domain` |
| `fails_after_close` | `call` after `close` does not return an error whose chain contains `sentinel` |
| `is_poisoned` | One of 32 readings of `observe` after `induce` returns `Ok` |

**Retrying.** `eventually` retries a body, and `eventually_true` retries a
predicate, until something outside the test makes the condition true or the
timeout passes. Each attempt of `eventually` runs on a seat of its own, so a
failure ends the attempt and not the test. `eventually_true` retries with a
backoff that starts at a millisecond and doubles, up to a quarter of the
timeout. Both read the seat's clock. On the platform clock they spend real
time. On a `Controlled` clock they advance the clock between attempts and do
not spend real time.

```rust
check::eventually(timeout: Duration, interval: Duration, body: impl Fn(&dyn Seat))
check::eventually_true(timeout: Duration, predicate: impl Fn() -> bool)
```

**Allocations.** `max_allocs` and `max_allocs_with_setup` end the test
when one call makes more heap allocations than the ceiling allows. They run
in an ordinary test, not in a benchmark.

- The assertion warms the body with one call and counts the allocations of
  the next 100 calls on the calling thread, so tests on other threads do not
  change the count.
- A ceiling of 0 admits one allocation in all. Any other ceiling admits an
  average per call that rounds to at most the ceiling.
- A count above the ceiling is taken again, ten times at most.
- `max_allocs_with_setup` builds the input of each call with `setup`, and
  does not count the allocations of `setup`.
- Both take their counts from `CountingAllocator`, as the
  [benchmark ceilings](#benchmark-ceilings) do.
  `dokimi_assert::allocs_checked()` returns the reason when the test binary
  does not install it.

```rust
check::max_allocs(body: impl FnMut(), ceiling: u64)
check::max_allocs_with_setup<T>(setup: impl FnMut() -> T, body: impl FnMut(T), ceiling: u64)
```

**Testing an assertion.** `rejects` runs a check on a seat of its own and
returns the check's failures. It ends the test when the check does not fail.
A fault that ends a call of the check ends the call of `rejects` with the
same fault. Only `check` has it, because a check on `soft` does not stop.
Its signature keeps the message, which comes before the body.

```rust
check::rejects(msg: &str, body: impl FnOnce(&dyn Seat)) -> Vec<Failure>
```

## Files

`files` writes trees of files for a test, and checks the files that the code
under test reads and writes.

```rust
use dokimi_assert::Collector;
use dokimi_assert::files::{self, Tree};

#[test]
fn a_rename_moves_the_draft() {
    let t = Collector::new();
    let dir = files::workspace(&t, &Tree::from([("draft.txt", files::text("hello\n"))]));

    std::fs::rename(dir.path().join("draft.txt"), dir.path().join("final.txt")).unwrap();

    files::equal(&t, &dir, &Tree::from([("final.txt", files::text("hello\n"))]), "the draft becomes the final file");
    files::absent(&t, dir.path().join("draft.txt"), "no draft is left");
}
```

- `files::text`, `bytes`, `executable`, `directory` and `link` build the
  entries of a `Tree`. Each entry is at a path relative to the tree's root,
  with `/` between the names. `Entry::with_mode` sets the permission bits of
  an entry where they are part of the contract.
- `files::workspace` writes a tree into a new directory below the system's
  temporary directory. The `Workspace` removes the directory when it drops.
- `files::write` writes a tree over a directory that exists, and
  `files::read` returns the text of a file.
- `equal`, `contains` and `unchanged` compare a directory with a tree.
  `absent`, `is_file`, `is_dir`, `links_to`, `has_content` and `has_mode`
  check the entry at one path.
- A comparison follows no link, and it compares content as bytes. It
  compares a mode only where the wanted entry has one, and otherwise the
  owner's execute bit of a file.
- Windows does not record permission bits, so the library does not read a
  mode there, and `has_mode` ends the call with a fault.
- Every assertion of `files` ends the test at a failure.

The signatures leave out the seat and the message. `workspace`, `write` and
`read` take no message.

```rust
files::workspace(tree: &Tree) -> Workspace
files::write(dir: impl AsRef<Path>, tree: &Tree)
files::read(path: impl AsRef<Path>) -> String
files::equal(dir: impl AsRef<Path>, want: &Tree)
files::contains(dir: impl AsRef<Path>, want: &Tree)
files::unchanged(dir: impl AsRef<Path>, body: impl FnOnce())
files::absent(path: impl AsRef<Path>)
files::is_file(path: impl AsRef<Path>)
files::is_dir(path: impl AsRef<Path>)
files::links_to(path: impl AsRef<Path>, target: &str)
files::has_content(path: impl AsRef<Path>, want: impl AsRef<[u8]>)
files::has_mode(path: impl AsRef<Path>, mode: u32)
```

## Golden files

```rust
use dokimi_assert::{Collector, golden};

#[test]
fn the_report_is_unchanged() {
    let t = Collector::new();
    let report = format!("generated at {}\ntotal: 3\n", "2026-10-22T09:30:00Z");

    golden::matches(&t, "report.txt", &report, golden::should_update(), &[golden::scrub_timestamps()]);
}
```

The file `testdata/golden/report.txt` in the package's root contains the
recorded output. `DOKIMI_ASSERT_UPDATE_GOLDEN=1 cargo test` writes it,
because `golden::should_update` returns `true` when the variable is set to
anything other than the empty string and `0`. A comparison applies its
scrubbers to both sides before it compares them, so the recorded file
contains `generated at SCRUBBED_TIMESTAMP`.

The golden comparisons take no message, and each ends the test at a
failure. The signatures leave out the seat.

```rust
golden::matches(name: &str, got: impl AsRef<str>, update: bool, scrubbers: &[Scrubber])
golden::matches_at(path: impl AsRef<Path>, got: impl AsRef<str>, update: bool, scrubbers: &[Scrubber])
golden::matches_tree(name: &str, got: impl AsRef<Path>, update: bool, scrubbers: &[Scrubber])
golden::matches_json_field(path: impl AsRef<Path>, field: &str, got: impl AsRef<str>, update: bool, scrubbers: &[Scrubber])
golden::should_update() -> bool
golden::scrub_timestamps() -> Scrubber
golden::scrub_hashes() -> Scrubber
golden::scrub_run_ids() -> Scrubber
golden::scrub_json_fields(fields: &[&str]) -> Scrubber
golden::Scrubber::new(replace: impl Fn(&str) -> String + Send + Sync + 'static) -> Scrubber
```

- `matches` and `matches_tree` resolve a name against `testdata/golden`,
  and `matches_at` takes a path.
- `matches_tree` compares a directory with a golden directory. An update
  makes the golden directory equal the output.
- `matches_json_field` compares a JSON text with one field of a golden JSON
  object. A difference of layout passes.

## Properties

```rust
use dokimi_assert::{Collector, check, prop};

#[test]
fn sorting_keeps_every_element() {
    let t = Collector::new();
    prop::for_all(&t, "sorting keeps every element", prop::Options::default(), |c| {
        let mut items = c.draw(&prop::list(prop::integer(0..=100_u8), ..=20), "items");
        let before = items.len();
        items.sort_unstable();
        check::length(c, &items, before, "the sorted list has every element");
    });
}
```

`prop::for_all` runs the body against generated cases. The body takes its
inputs from its `prop::Case` with `Case::draw`, and it asserts on the case
as a test asserts on its seat. A failing case shrinks to the smallest case that fails the
same way:

```rust
use dokimi_assert::{Collector, check, prop};

#[test]
fn reversing_keeps_the_first_element() {
    let t = Collector::new();
    prop::for_all(&t, "a reversed list starts with its first element", prop::Options::default(), |c| {
        let items = c.draw(&prop::list(prop::integer(0..=9_u8), ..=10), "items");
        let reversed: Vec<u8> = items.iter().rev().copied().collect();
        check::equal(c, reversed.first(), items.first(), "the reverse starts with the first element");
    });
}
```

```text
a reversed list starts with its first element: counterexample after 1 valid and 0 rejected cases, seed 9394425613992845869
  items: [0, 1], any value fails
failure of equal at reverse.rs:9: the reverse starts with the first element: want Some(0), got Some(1)
replay: prop::replay("prop1:AAEAAAABAAEAAA") or DOKIMI_ASSERT_PROP_REPLAY=prop1:AAEAAAABAAEAAA
```

- A failing run writes its smallest case to the store of the test under
  `testdata/prop`, and later runs replay it first.
- A form, such as `prop::equal` or `prop::round_trip`, runs one assertion
  on every generated input.
- `prop::of::<T>()` returns the generator of a type that implements
  `prop::Arbitrary`. `#[derive(prop::Arbitrary)]` implements it for a
  struct or an enum, and `prop::of_shape` returns the generator of a shape
  file.
- `prop::fuzz` replays the stored cases of a property and returns the
  target of a fuzzer, such as the body of `fuzz_target!`.
- `DOKIMI_ASSERT_PROP_SEED`, `DOKIMI_ASSERT_PROP_PROFILE`,
  `DOKIMI_ASSERT_PROP_BUDGET` and `DOKIMI_ASSERT_PROP_REPLAY` set the
  defaults of every property of a run. The profile is `default`, `ci` or
  `campaign`. The option `prop::hermetic()` makes a run ignore the four
  variables.
- `prop::around` runs each run of the body inside a wrapper, such as a
  runtime or a frozen clock. A wrapper that does not call `run`, calls it a
  second time or panics ends the call with a fault.
- `prop::using(&generator)` makes `generator` the generator of its type in
  one run. A draw of `prop::of::<T>()` of that type decodes with it, and so
  does each value of that type inside a derived type. Inside `generator`, a
  draw of its own type decodes without the setting, so a run can narrow a
  type with `prop::of::<T>().filter(…)`.
- `prop::values(&generator, seed, n, options)` returns the values of the
  first random cases of a seed outside a property. A benchmark then
  measures the same inputs in every run. `options` takes `using` alone, so
  the benchmark and a run of the property draw from one domain. A call
  without it passes `Options::default()`.

A form takes a type with a derived generator:

```rust
use dokimi_assert::{Collector, prop};

#[derive(Debug, Clone, PartialEq, prop::Arbitrary)]
struct Line {
    #[prop(min = 1, max = 99)]
    quantity: u32,
    sku: String,
}

fn encode(line: &Line) -> Result<String, String> {
    Ok(format!("{}:{}", line.quantity, line.sku))
}

fn decode(text: &String) -> Result<Line, String> {
    let (quantity, sku) = text.split_once(':').ok_or("no separator")?;
    let quantity = quantity.parse().map_err(|_| "no quantity")?;
    Ok(Line { quantity, sku: sku.to_owned() })
}

#[test]
fn decoding_undoes_encoding() {
    let t = Collector::new();
    prop::round_trip(&t, encode, decode, "decode undoes encode", prop::cases(200));
}
```

## Histories

```rust
use dokimi_assert::history::{self, History, Operation, Spec};
use dokimi_assert::{Collector, Value};

/// A register of integers, 0 before the first write.
struct Register;

impl Spec for Register {
    type State = i64;

    fn initial(&self) -> i64 {
        0
    }

    fn next(&self, state: &i64, op: &Operation) -> Vec<i64> {
        match (op.name(), op.args()) {
            ("write", [Value::Int(value)]) => vec![i64::try_from(*value).unwrap_or_default()],
            _ if op.returned(*state) => vec![*state],
            _ => Vec::new(),
        }
    }
}

#[test]
fn the_register_is_linearizable() {
    let t = Collector::new();
    let calls = History::new();
    calls.invoke(0, "write", [Value::from(1)], ["x".into()]).ok(Value::Null);
    calls.invoke(1, "read", [], ["x".into()]).ok(0);
    history::is_linearizable(&t, &calls, &Register, "the register is linearizable", history::Options::default());
}
```

The read returns 0 after the write of 1 completed, so no order of the two
calls fits the register. The failure contains the longest order of calls
that the specification accepts, and the call that it rejects there:

```text
the register is linearizable: violated in the partition of "x"
    steps 2, partitions 1, calls 2, concurrency 1
    linearized: call 0 write(1) → None
    states: 1
    rejected: call 2 read() → 0
```

- A test records each call with `History::invoke` before the call starts.
  After the call returns, the test completes it with `Call::ok`,
  `Call::fail` or `Call::unknown`.
- `history::concurrently` starts the clients together, and
  `history::from_intervals` builds a history from a log of timed calls.
- Calls that share a key form one partition, and the check searches each
  partition on its own.
- `history::spec_from` builds a specification from a factory of the
  subject.
- `history::is_serializable` and `history::has_snapshot_isolation` check a
  history of list-append transactions.

## Machines

```rust
use dokimi_assert::prop::{self, Case};
use dokimi_assert::stateful::{self, Action, Machine};
use dokimi_assert::{Collector, Value};
use std::sync::atomic::{AtomicI64, Ordering};

#[test]
fn the_register_keeps_its_last_write() {
    let t = Collector::new();
    prop::for_all(&t, "the register keeps its last write", prop::Options::default(), |c| {
        let register = AtomicI64::new(0);
        let machine = Machine::new([
            Action::with_input(
                "write",
                |c: &Case<'_>, _: &i64| c.draw(&prop::integer(0..=9_i64), "v"),
                async |c: &Case<'_>, client: usize, value: i64| {
                    let call = c.history().invoke(client, "write", [value.into()], []);
                    register.store(value, Ordering::SeqCst);
                    call.ok(Value::Null);
                },
            ),
            Action::new("read", async |c: &Case<'_>, client: usize| {
                let call = c.history().invoke(client, "read", [], []);
                call.ok(register.load(Ordering::SeqCst));
            }),
        ])
        .spec(Register);
        stateful::steps(c, &machine, stateful::clients(2));
    });
}
```

`Register` is the specification of the previous section.

- A machine has the sequential specification of a subject and its actions,
  with the simpler actions first. `Action::new` takes the run of an action,
  and `Action::with_input` takes the input that the run receives as well.
- `stateful::steps` takes the steps of a case and makes each decision a
  choice of the case. A failing case then shrinks to the steps that the
  failure needs, and it replays from its token.
- The clients of a concurrent section run on threads by default. A case
  then runs up to `repeat` times, because threads do not replay.
- With the option `stateful::tasks`, the clients run as tasks of a
  `stateful::Scheduler`. The releases of the scheduler are choices of the
  case, so a race replays and shrinks. A task awaits
  `Scheduler::yield_now` at each point where another task can run.

## Futures

`dokimi-assert-tokio` has the assertions whose subject is a future, for
tests that run on tokio. Each returns a future that the test awaits.

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

The signatures leave out the seat and the message.

```rust
check::honours_cancellation<'a, E: Error + 'static>(body: impl AsyncFnOnce(Option<&Cancel>) -> Result<(), E> + 'a)
check::honours_deadline<'a, E: Error + 'static>(body: impl AsyncFnOnce(Option<&Cancel>) -> Result<(), E> + 'a)
check::completes_within<'a>(within: Duration, body: impl Future + 'a)
check::eventually<'a>(timeout: Duration, interval: Duration, body: impl AsyncFn(&dyn Seat) + 'a)
check::eventually_true<'a>(timeout: Duration, predicate: impl AsyncFn() -> bool + 'a)
check::no_task_leaks<'a>(body: impl AsyncFnOnce() + 'a)
```

- Each returns `impl Future<Output = ()> + 'a`, and `soft` has the same
  six.
- `honours_cancellation`, `honours_deadline`, `completes_within`,
  `eventually` and `eventually_true` have the names of their synchronous
  counterparts.
- `no_task_leaks` fails when a task that the body spawned on the runtime
  still runs half a second after the body completes. Rust's standard library
  cannot list the threads of a process, so the assertion does not see a
  thread from `std::thread`.
- The assertions that wait read tokio's clock. A test that pauses the
  clock, as `#[tokio::test(start_paused = true)]` does, does not spend real
  time on a retry or a timeout.
- `completes_within` drops a future that has not completed in time, as
  `tokio::time::timeout` drops it.

The dependencies of a test on tokio are in
[crates/dokimi-assert-tokio/README.md](crates/dokimi-assert-tokio/README.md).

## Benchmark ceilings

A benchmark that prints numbers reports what happened. With a contract, the
test fails when a measurement crosses a ceiling.

```rust
use dokimi_assert::Collector;
use dokimi_assert::bench::{Contract, CountingAllocator};
use std::time::Duration;

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator::new();

#[test]
fn parsing_a_port_is_quick() {
    let t = Collector::new();
    Contract::new(&t, "parsing a port is quick")
        .warmup(100)
        .max_latency(Duration::from_micros(50))
        .max_allocs(0)
        .run(10_000, || {
            std::hint::black_box("8080".parse::<u16>().unwrap());
        })
        .check();
}
```

- Each method returns the contract, and nothing is asserted until `check`
  runs. `check` reports each ceiling as one call of its assertion.
- `max_latency` sets a ceiling on the p99 latency of one iteration. With
  fewer than 100 iterations, the ceiling applies to the slowest one.
  `max_mean` sets a ceiling on the mean latency.
- `max_allocs` sets a ceiling on the mean allocations of one iteration,
  rounded to the nearest whole number with a half rounded up. `max_bytes`
  sets a ceiling on the bytes that one iteration requests from the
  allocator, rounded down.
- `warmup` sets the number of iterations that the run makes before the
  measured ones.
- `run` measures a body. `measuring` builds the input of each iteration with
  a setup closure, outside the measurement. `run_parallel` runs the body on
  scoped threads that make the iterations together.
- The allocation ceilings take their counts from `CountingAllocator`, which
  the test binary installs as its global allocator. A program has one global
  allocator, so the library cannot install it for the caller. Without it, an
  allocation ceiling ends the call with a fault.

## Equality

`equal` compares with `==`, as `assert_eq!` does. The definition has three
rules of equality. NaN is unequal to itself, `0.0` equals `-0.0`, and
containers compare element by element. Rust's floats and collections follow
all three rules, so the library does not add a comparison of its own. A
pair of types that `PartialEq` does not relate fails to compile.

The definition offers three relaxations, and this library declines all
three:

- `equate-empty`: an absent container and an empty one have different
  types, such as `Option<Vec<T>>` and `Vec<T>`.
- `equate-nans`: a test that needs NaN equal to NaN compares
  `f64::to_bits`.
- `by-identity`: `PartialEq` compares values, so a test compares two
  references with `std::ptr::eq`.

## Call records

With `DOKIMI_ASSERT_RECORD=1`, the seat of each test writes the call record
of every assertion as one line of JSON. The file is beside the test binary
and has the binary's name with the suffix `.dokimi-assert.jsonl`, such as
`target/debug/deps/api-0a1b2c3d4e5f6a7b.dokimi-assert.jsonl`.

- Each line contains the definition's version, the sequence number of the
  call in its test, the assertion, the contract, the verdict, the file and
  line of the call, and the test. The line of a failure also contains its
  detail.
- Any other value than the empty string, `0` and `1` makes every call end
  with a fault.
- A `Recorder` keeps its records whatever the variable states, and
  `Recorder::records` returns them.

## The standard

[assert-spec](https://github.com/dokimasia/assert-spec) defines the
assertions. The unpublished crate `dokimi-assert-conformance` contains a
copy of the definition. `ergon tool run spec.spec -- sync` writes the copy
from a commit of assert-spec, and `make check-spec` verifies it. The
crate's tests run the definition's cases and vectors through the public API
of both crates. They compare JSON with `serde_json`, never with the library
under test.

- 161 of the corpus's 195 cases apply to Rust. Of the other 34, 7 skip
  Rust and 27 use a relaxation that the library declines.
- Each case runs on `check` and `soft` of each crate that has its
  assertion. The driver compares the verdict, the failure's sentence and
  detail, and the call record of each run with the case.
- The vectors run too: 57 for files, 80 for histories, 430 for properties
  and 14 for machines. A platform that does not record permission bits
  skips the files vectors that state a mode.
- The crates implement all 110 assertions of the definition.
  `dokimi-assert-tokio` has `no_task_leaks`, and `dokimi-assert` has the
  others. The spec of `lib.rs` names every Rust name of the naming table
  in code, so the build fails when one is missing.

The overlay, `crates/dokimi-assert-conformance/overlays/rust.json`, contains
the differences between Rust and the definition. Besides the limits and the
declined relaxations of the earlier sections, these differences include
seven names of the definition's surface that the library replaces:

- `Reporter`, `Clocked` and `FaultReporter`: the `Seat` trait gives
  `report`, `clock` and `report_fault` defaults, so no second trait is
  needed.
- `Contract.excluding`: `Contract::measuring` takes the setup as a closure
  of its own.
- `assertion` and `that`: the functions make every check that a chain would
  make, and a chain would borrow the subject across every method of the
  chain.
- `cancellation-of`: `Seat::cancellation` returns the handle of any seat.

The design records of the library and their statuses are in
[docs/rfc](docs/rfc/README.md).

## Development

ergon manages the Makefile, the workflows and the configuration of the
tools. `ergon init sync` writes them again from its baseline and from the
files under `.ergon/local`.

```sh
make check      # the gate of the repository
make test       # every test, then the doctests of the library crates
make lint       # rustfmt, clippy with the pedantic lints, and the documentation
make coverage   # fails on a line or a function of a published crate that no test ran
make deny       # the licences, advisories and sources of the dependencies
make help       # list every target
```

`make check` lints the Markdown files with rumdl and the workflows with
actionlint, and it checks the vendored definition with spec. For Rust, it
runs rustfmt, clippy, rustdoc, the tests, a check of each crate on Rust
1.85 and cargo-audit over `Cargo.lock`. On Linux, it also measures the
coverage and packages each crate. CI runs the gate of Rust on Linux, macOS
and Windows, and cargo-deny on Linux.

## Licence

MIT. See [LICENSE](LICENSE).
