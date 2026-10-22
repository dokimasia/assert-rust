---
rfc: 0003
title: Definition 9.0.0 in Rust
author: Roy Klopper <roy.klopper@stealthscale.io>
status: Accepted
created: 2026-10-21
updated: 2026-10-22
discussion: https://github.com/dokimasia/assert-rust/issues/7
supersedes: RFC-0002
superseded-by: none
produces-adr: none
---

# RFC-0003: Definition 9.0.0 in Rust

## Summary

The library moves from definition 1.1.0 to 9.0.0 in one step. It
implements every assertion of the definition and every Rust name of its
surface: 110 assertions and 163 names. It also changes its API to the
conventions of Rust's standard library and of the Rust API Guidelines:

- **Crates.** The new crate `dokimi-assert-derive` supplies
  `#[derive(Arbitrary)]`, beside `dokimi-assert` and
  `dokimi-assert-tokio`.
- **Comparisons.** `equal` and `not_equal` compare any pair that
  `PartialEq` relates, as `assert_eq!` does.
- **Settings.** The settings of a property, a history check and a
  machine are builders. The naming table's free functions, such as
  `prop::cases`, start a builder, and its methods of the same names
  extend it.
- **Domains and sizes.** A generator takes its bounds and its sizes as
  ranges, such as `prop::integer(0..=100)` and `prop::list(of, ..=16)`.
- **Paths.** Every path parameter takes `impl AsRef<Path>`.
- **Records.** The detail of a failure is a list of typed values, and
  `Failure` has private fields with accessors.

A property with a derived input reads like this:

```rust
use dokimi_assert::{Collector, prop};

#[derive(Debug, Clone, PartialEq, prop::Arbitrary)]
struct Order {
    #[prop(min = 1, max = 99)]
    quantity: u32,
    note: Option<String>,
}

#[test]
fn decoding_undoes_encoding() {
    let t = Collector::new();
    prop::round_trip(&t, encode, decode, "decode undoes encode", Default::default());
}
```

## Motivation

### The library implements a third of the definition

The library vendors definition 1.1.0 and implements its 42 assertions.
Definition 9.0.0 states 110. The completeness gate fails on every row of
the naming table that the library neither implements nor declines, so the
library cannot vendor a later version until it implements the rest.

The vectors pin each addition: 430 property vectors, 80 history vectors,
14 machine vectors and 57 file vectors. The Go and TypeScript libraries
run them. The Rust library runs none of them, and it disagrees with the
definition in three places:

- `matches` compiles a pattern with `Regex::new`, so it accepts every
  pattern of the `regex` crate, and `\d` matches every Unicode digit. The
  definition accepts the portable subset alone, where `\d` is the ASCII
  digits.
- `rejects` returns the first message of the check. The definition
  returns the failure records of the check.
- The detail of a failure contains text. The definition states typed
  values, so a call record of the library cannot state its detail.

### The API differs from Rust's conventions

A Rust test author meets these differences from the standard library and
from the Rust API Guidelines:

| Today | Rust's convention |
|---|---|
| `check::equal(&seat, &got, &want, msg)` takes `&T` on both sides of one `T`, so `check::equal(&seat, &name, "widget", msg)` compiles only through a deref coercion | `assert_eq!` compares any pair of types that `PartialEq` relates, such as `String` with `&str` |
| RFC-0002 states the options of a property as a trailing list of values named after Go's functional options | `std::thread::Builder`, `std::fs::OpenOptions` and `std::process::Command` state optional settings with builders |
| `golden::matches_at` takes `&Path` | `std::fs::read` and every other function of `std::fs` take `impl AsRef<Path>` |
| The module `matcher` is public and exports `Mode`, `fail` and `report`, which the surfaces call internally | The Rust API Guidelines keep implementation details out of the documentation (C-HIDDEN) |
| `Failure` exposes its fields | The Rust API Guidelines give a struct private fields (C-STRUCT-PRIVATE) |
| `UPDATE_GOLDEN` rewrites the golden files | Python and TypeScript read `DOKIMI_ASSERT_UPDATE_GOLDEN`, and every variable of the definition starts with `DOKIMI_ASSERT_` |

The library has not been released, so a change of its API does not
break a caller.

## Detailed design

### Components

| Component | Where | Responsibility |
|---|---|---|
| Values | `crates/dokimi-assert/src/value.rs` | `Value`, the typed literals of the definition and the opaque literal, and their JSON |
| Failure record | `failure.rs` | `Failure` and `Where`, with the accessors of the naming table |
| Seats | `seat.rs` | `Seat`, `Standard`, `Recorder` and `Collector` |
| Cancellation | `cancel.rs` | `Cancel` and `Stop`, with derived handles |
| Clocks | `clock.rs` | `Clock`, `System` and `Controlled` |
| Call records | `record.rs` | The switch, the numbering, the records of a recorder and the artifact of a test binary |
| Verdicts | `verdict.rs` | Reports a pass, a failure and a fault, and writes the call record of each |
| Portable patterns | `pattern.rs` | Parses the portable subset for `matches` and `prop::string_matching` |
| Surfaces | `check.rs`, `soft.rs` | The 48 assertions of the root namespace that both surfaces state, and `rejects` on `check` alone |
| Matchers | `matcher/` | The comparison of each family, shared by both surfaces and by the property forms |
| Golden files | `golden.rs` | The four golden assertions and the scrubbers |
| Trees of files | `files/` | `Tree`, `Entry`, workspaces and the nine file assertions |
| Benchmarks | `bench.rs`, `alloc.rs` | `Contract` and `CountingAllocator` |
| Properties | `prop/` | The engine, the generators, the case, the forms, the shapes and the fuzz bridge |
| Histories | `history/` | The history, the specs, `concurrently` and the three checks |
| Machines | `stateful/` | Machines, their steps and the task scheduler |
| Derive | `dokimi-assert-derive` | `#[derive(Arbitrary)]` and the attribute `#[prop(...)]` |
| Futures | `dokimi-assert-tokio` | The assertions whose subject is a future, and `no_task_leaks` |
| Conformance | `dokimi-assert-conformance`, not published | Drives every corpus case and vector against both libraries |

### The workspace

```text
assert-rust/
├── Cargo.toml                      the workspace: its crates, shared dependencies and lints
├── Makefile                        make check runs every stage that CI runs
├── deny.toml                       licences, advisories and sources of the dependencies
├── .spec.yaml                      the copy of the definition that spec sync writes and spec check verifies
├── docs/rfc/
├── docs/standards/testing.md       the testing rules, stated for Rust
└── crates/
    ├── dokimi-assert/              the library
    │   ├── src/                    code only, one file per module
    │   ├── tests/                  the specs: one file per source file, through the public API
    │   └── testdata/               golden files and the pattern verdicts
    ├── dokimi-assert-derive/       #[derive(Arbitrary)]
    │   └── testdata/ui/            each misuse of the attribute and its compile error
    ├── dokimi-assert-tokio/        the assertions whose subject is a future
    └── dokimi-assert-conformance/  the definition, run against both libraries; not published
        ├── VERSION                 the version of the vendored definition
        ├── manifest.json           the digest of each vendored file
        ├── corpus/                 the cases of every assertion, and the vectors
        ├── overlays/rust.json      the declines and the limits of this library
        ├── spec/                   the assertions, the naming table and the zone table
        ├── src/                    the drivers of the corpus and the vectors
        └── tests/                  the specs of the drivers, which run every case and vector
```

The crates are under `crates/`, as in the workspaces of ripgrep, uv and
rust-analyzer. The root of the repository then contains the files of the
workspace alone. Each crate has the same layout. `src/` contains code
alone, and `tests/` contains the specs at the paths of the source files
that they specify. The workspace states every shared dependency once in
`[workspace.dependencies]`. Every crate inherits the workspace's lints.

`dokimi-assert` has these features:

| Feature | Default | Adds |
|---|---|---|
| `derive` | Off | Re-exports `#[derive(Arbitrary)]` from `dokimi-assert-derive` as `prop::Arbitrary` |
| `chrono`, `time`, `jiff`, `uuid`, `rust_decimal` | Off | `prop::Arbitrary` for the date, time, UUID and decimal types of those crates |
| `rand_core` | Off | `rand_core::TryRng` for the random source of a case, and through it `rand_core::Rng` |

The implementations for another crate's types are features of
`dokimi-assert` and not of the derive crate, because Rust's orphan rule
allows an implementation of `prop::Arbitrary` only in the crate that
declares the trait or the type. The derive macro does not add a feature
of its own. docs.rs builds the documentation with every feature.

### The seat

The seat is the explicit first argument of every assertion. A test
creates one and hands it to each call:

```rust
use dokimi_assert::{Collector, check, soft};

#[test]
fn get_returns_the_stored_item() {
    let t = Collector::new();
    let item = store().get("widget");
    check::is_some(&t, item.as_ref(), "get returns the stored item");
    soft::equal(&t, &item.unwrap().name, "widget", "the name is kept");
}
```

```rust
/// Receives the failures of the assertions that a test calls.
pub trait Seat: Send + Sync {
    /// Marks the calling frame as a frame of a helper, as `testing.TB.Helper`
    /// does in Go. The default does nothing, because `#[track_caller]` already
    /// reports the caller's line.
    fn helper(&self) {}

    /// Reports a failure that ends the test.
    #[track_caller]
    fn fail(&self, message: &str);

    /// Reports a failure after which the test continues.
    #[track_caller]
    fn record(&self, message: &str);

    /// Receives the record of a failure. The default renders the record and
    /// calls `fail` when `aborting` is true and `record` otherwise.
    #[track_caller]
    fn report(&self, failure: &Failure, aborting: bool) { /* … */ }

    /// Returns the clock that the timed assertions read. The default is the
    /// platform clock.
    fn clock(&self) -> &dyn Clock { /* … */ }

    /// Returns the cancellation handle of the seat's test. The default is a
    /// handle that never stops, because libtest gives a test no handle.
    fn cancellation(&self) -> &Cancel { /* … */ }

    /// Returns the calls whose records the seat keeps or writes.
    #[doc(hidden)]
    fn calls(&self) -> Option<Arc<Calls>> { None }

    /// Returns the path of the test that the seat belongs to.
    #[doc(hidden)]
    fn test(&self) -> Option<String> { None }
}
```

- `Seat` requires `Send + Sync`, because the definition requires that a
  seat accept failures from more than one thread.
- The default `report` replaces the member `takes_records`. A seat that
  keeps records overrides `report`.
- `calls` and `test` are hidden members with defaults. `Standard`,
  `Recorder` and `Collector` override them to number and write the call
  records of their test. A seat that a caller implements does not write
  a call record.
- `Recorder` gains `records`, which returns its call records as JSON
  lines, and `with_cancellation`. `Standard`, `Recorder` and `Collector`
  implement `Default`.

### Values and the failure record

```rust
/// A value of a failure's detail or of a call record, as a typed literal of
/// the definition states it.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Value {
    Null,
    Bool(bool),
    Int(i128),
    /// An unsigned 128-bit integer above the range of `i128`. A smaller one
    /// is an `Int`.
    Uint(u128),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
    List(Vec<Value>),
    Map(Vec<(Value, Value)>),
    Record(Vec<(String, Value)>),
    Variant { name: String, payload: Option<Box<Value>> },
    Tree(files::Tree),
    /// A value outside the typed literals, as its `Debug` renders it.
    Opaque(String),
}
```

```rust
impl Failure {
    pub fn assertion(&self) -> Option<&'static str>;
    pub fn contract(&self) -> &str;
    pub fn detail(&self) -> &[(&'static str, Value)];
    pub fn field(&self, name: &str) -> Option<&Value>;
    pub fn want(&self) -> Option<&Value>;
    pub fn got(&self) -> Option<&Value>;
    pub fn case_failure(&self) -> Option<&Failure>;
    pub fn location(&self) -> Option<Where>;
}

impl fmt::Display for Failure { /* the sentence: "contract: want 2, got 1" */ }
```

- An assertion over a parameter of a known type states the value as its
  literal: a `&str` as `String`, a count as `Int` and a tolerance as
  `Float`.
- An assertion that is generic over `T: Debug` states the caller's value
  as `Opaque`, with the text of `{:?}`. Rust has no reflection, and a
  trait bound that converts a value would stop a caller comparing a type
  of their own.
- `assertion` returns `None` for the record of a message that a body
  passed to the `fail` or `record` of a property's case. No assertion
  reported that message.
- `Value` implements `From` for the integers, the floats, `bool`, `char`,
  `&str`, `String`, `&[u8]`, `Vec<u8>`, `Vec<Value>` and `Option<T>` of a
  convertible `T`. A `u128` above the range of `i128` converts to `Uint`.

### The assertions of the root namespace

`check` and `soft` state the same 48 assertions with the same
signatures, and `check` adds `rejects`. Every assertion takes the seat
first and the message last, except `rejects`, which takes its body last,
as the definition orders its arguments.

```rust
// values
pub fn equal<G, W>(seat: &dyn Seat, got: G, want: W, msg: &str)
where G: PartialEq<W> + Debug, W: Debug;
pub fn not_equal<G, W>(seat: &dyn Seat, got: G, want: W, msg: &str)
where G: PartialEq<W> + Debug, W: Debug;
pub fn is_true(seat: &dyn Seat, condition: bool, msg: &str);
pub fn is_false(seat: &dyn Seat, condition: bool, msg: &str);
pub fn is_none<T: Debug>(seat: &dyn Seat, got: Option<T>, msg: &str);
pub fn is_some<T: Debug>(seat: &dyn Seat, got: Option<T>, msg: &str);

// sizes and containment
pub fn length<C: Container + ?Sized>(seat: &dyn Seat, got: &C, want: usize, msg: &str);
pub fn is_empty<C: Container + ?Sized>(seat: &dyn Seat, got: &C, msg: &str);
pub fn is_not_empty<C: Container + ?Sized>(seat: &dyn Seat, got: &C, msg: &str);
pub fn contains<H, N>(seat: &dyn Seat, haystack: &H, needle: &N, msg: &str)
where H: Contains<N> + Debug + ?Sized, N: Debug + ?Sized;
pub fn not_contains<H, N>(seat: &dyn Seat, haystack: &H, needle: &N, msg: &str)
where H: Contains<N> + Debug + ?Sized, N: Debug + ?Sized;
pub fn contains_in_order(seat: &dyn Seat, got: &str, needles: &[&str], msg: &str);
pub fn is_permutation<T: PartialEq + Debug>(seat: &dyn Seat, got: &[T], want: &[T], msg: &str);

// text and numbers
pub fn has_prefix(seat: &dyn Seat, got: &str, prefix: &str, msg: &str);
pub fn has_suffix(seat: &dyn Seat, got: &str, suffix: &str, msg: &str);
pub fn matches(seat: &dyn Seat, got: &str, pattern: &str, msg: &str);
pub fn close_to<N: Number>(seat: &dyn Seat, got: N, want: f64, tolerance: f64, msg: &str);
pub fn in_range<N: Number>(seat: &dyn Seat, got: N, low: f64, high: f64, msg: &str);
pub fn pairwise<T: Debug>(seat: &dyn Seat, items: &[T], predicate: impl Fn(&T, &T) -> bool, msg: &str);

// errors and panics
pub fn no_error<T, E: Debug>(seat: &dyn Seat, got: &Result<T, E>, msg: &str);
pub fn has_error<T: Debug, E>(seat: &dyn Seat, got: &Result<T, E>, msg: &str);
pub fn error_is<T>(seat: &dyn Seat, error: &(dyn Error + 'static), target: &T, msg: &str)
where T: Error + PartialEq + 'static;
pub fn error_is_not<T>(seat: &dyn Seat, error: &(dyn Error + 'static), target: &T, msg: &str)
where T: Error + PartialEq + 'static;
pub fn error_as<'e, T: Error + 'static>(seat: &dyn Seat, error: &'e (dyn Error + 'static), msg: &str)
    -> Option<&'e T>;
pub fn panics(seat: &dyn Seat, body: impl FnOnce(), msg: &str) -> Option<String>;
pub fn does_not_panic(seat: &dyn Seat, body: impl FnOnce(), msg: &str);

// behaviour
pub fn honours_cancellation<E: Error + 'static>(
    seat: &dyn Seat, body: impl FnOnce(Option<&Cancel>) -> Result<(), E>, msg: &str);
pub fn honours_deadline<E: Error + 'static>(
    seat: &dyn Seat, body: impl FnOnce(Option<&Cancel>) -> Result<(), E>, msg: &str);
pub fn completes_within<E: Debug + Send + 'static>(
    seat: &dyn Seat, within: Duration,
    body: impl FnOnce(Option<&Cancel>) -> Result<(), E> + Send + 'static, msg: &str);
pub fn is_pure<S: PartialEq + Debug>(seat: &dyn Seat, observe: impl Fn() -> S, body: impl FnOnce(), msg: &str);
pub fn is_not_pure<S: PartialEq + Debug>(seat: &dyn Seat, observe: impl Fn() -> S, body: impl FnOnce(), msg: &str);
pub fn none_handle_safe<E: Debug>(
    seat: &dyn Seat, body: impl FnOnce(Option<&Cancel>) -> Result<(), E>, msg: &str);

// relations
pub fn is_idempotent<I, S, E>(seat: &dyn Seat, call: impl FnMut(&I) -> Result<(), E>, input: I,
    observe: impl FnMut() -> S, msg: &str) where S: PartialEq + Debug, E: Debug;
pub fn accumulates<I, E: Debug>(seat: &dyn Seat, call: impl FnMut(&I) -> Result<(), E>, input: I,
    observe: impl FnMut() -> i64, msg: &str);
pub fn is_deterministic<I, O, E>(seat: &dyn Seat, call: impl FnMut(&I) -> Result<O, E>, input: I, msg: &str)
where O: PartialEq + Debug, E: Debug;
pub fn is_commutative<T, R>(seat: &dyn Seat, combine: impl FnMut(&T, &T) -> R, a: T, b: T, msg: &str)
where R: PartialEq + Debug;
pub fn is_associative<T>(seat: &dyn Seat, combine: impl FnMut(&T, &T) -> T, a: T, b: T, c: T, msg: &str)
where T: PartialEq + Debug;
pub fn round_trip<I, O, E, F>(seat: &dyn Seat, forward: impl FnOnce(&I) -> Result<O, E>,
    inverse: impl FnOnce(&O) -> Result<I, F>, input: I, msg: &str)
where I: PartialEq + Debug, E: Debug, F: Debug;
pub fn has_stable_order<T, E>(seat: &dyn Seat, iterate: impl FnMut() -> Result<Vec<T>, E>, msg: &str)
where T: PartialEq + Debug, E: Debug;
pub fn no_duplicates<T, E>(seat: &dyn Seat, iterate: impl FnOnce() -> Result<Vec<T>, E>, msg: &str)
where T: PartialEq + Debug, E: Debug;
pub fn is_monotonic<N, E>(seat: &dyn Seat, observe: impl FnMut() -> N,
    advance: impl FnMut() -> Result<(), E>, steps: usize, msg: &str)
where N: PartialOrd + Debug, E: Debug;
pub fn is_total<I, E: Debug>(seat: &dyn Seat, call: impl FnMut(&I) -> Result<(), E>,
    domain: &[I], msg: &str);
pub fn fails_after_close<T, E, C>(seat: &dyn Seat, close: impl FnOnce() -> Result<(), C>,
    call: impl FnOnce() -> Result<(), E>, sentinel: &T, msg: &str)
where T: Error + PartialEq + 'static, E: Error + 'static, C: Debug;
pub fn is_poisoned<E: Debug>(seat: &dyn Seat, induce: impl FnOnce(),
    observe: impl FnMut() -> Result<(), E>, msg: &str);

// waiting and allocations
pub fn eventually(seat: &dyn Seat, timeout: Duration, interval: Duration,
    body: impl Fn(&dyn Seat), msg: &str);
pub fn eventually_true(seat: &dyn Seat, timeout: Duration, predicate: impl Fn() -> bool, msg: &str);
pub fn max_allocs(seat: &dyn Seat, body: impl FnMut(), ceiling: u64, msg: &str);
pub fn max_allocs_with_setup<T>(seat: &dyn Seat, setup: impl FnMut() -> T, body: impl FnMut(T),
    ceiling: u64, msg: &str);

// testing an assertion, on check alone
pub fn rejects(seat: &dyn Seat, msg: &str, body: impl FnOnce(&dyn Seat)) -> Vec<Failure>;
```

- `equal` compiles for every pair that `PartialEq` relates, which is the
  rule of `assert_eq!`. A pair of types that does not compare, such as
  `i32` and `f64`, fails to compile.
- A callable returns `Result` where the definition reads its failure, and
  Go's callable returns an `error`. A callable that panics fails the
  relation, unless the relation requires a failure.
- `is_permutation` pairs the elements through a maximum bipartite
  matching over `PartialEq`, because a type that is only `PartialEq` has
  no order and no hash.
- `completes_within` runs its body on a thread of its own, with a handle
  that expires when the duration passes. It fails when the duration
  passes and does not wait for the body, as the definition requires. A
  body that runs on another thread is `Send + 'static`.
- `is_none` and `is_some` take the option by value. A test that keeps the
  option passes `option.as_ref()`, and a lookup such as `map.get(key)`
  passes as it is.
- `close_to` and `in_range` take any `Number`: every integer and float
  type, and a `Value` that is an `Int` or a `Float`. A test passes an
  `f32` or a `u64` without a cast.
- The body of `eventually` and the check of `rejects` run on a seat of
  their own. A failure on `check` ends the attempt or the check, and not
  the test.
- `Contains<N>` replaces the trait `Holds<N>`. It is implemented for
  `str`, `String`, slices, arrays, `Vec`, `VecDeque`, `LinkedList`, the
  maps by key, the sets and `Value`. `Container` is implemented for the
  same types, and for a `Box` and an `Option` of a container.

### Call records and faults

`DOKIMI_ASSERT_RECORD=1` records every assertion call. The library reads
the variable once per process.

- The call records of a test binary go to a file beside the binary,
  named after it with the suffix `.dokimi-assert.jsonl`. The first record
  of a run creates the file anew, and each record is one line.
- Each record states the test in the field `test`, as libtest names the
  thread of the test, such as `nested::fails`. A `Collector` belongs to
  the test of the thread that created it, and `Standard` to the test of
  the thread that calls it.
- A `Recorder` keeps its call records whatever the switch states, and
  `Recorder::records` returns them as JSON lines.
- An assertion that cannot run ends with a fault: a malformed shape, a
  spec that panics, or a value of `DOKIMI_ASSERT_RECORD` that the
  definition does not define. The seat receives the text of the fault
  through `fail`, and the call record states it in the field `error`.

### Settings as builders

The naming table lists each setting as a free function, such as
`prop::cases`, `history::budget` and `stateful::mean`. Each such function
returns the settings with that one setting applied. The returned value has a method
of the same name for each setting, so a caller chains them:

```rust
prop::for_all(&t, "a sorted list is ordered", prop::cases(500).seed(7), |c| {
    let xs = c.draw(&prop::list(prop::integer(-100..=100), ..), "xs");
    check::pairwise(c, &sort(xs), |a, b| a <= b, "sort orders the list");
});

prop::for_all(&t, "a sorted list is ordered", Default::default(), |c| { /* … */ });
```

```rust
/// The settings of a property over inputs of type `T`.
#[derive(Clone)]
#[must_use]
pub struct Options<T = ()> { /* private fields */ }

impl<T> Default for Options<T> { /* 100 cases, 2,000 shrink runs, 30 seconds, … */ }

impl<T> Options<T> {
    pub fn cases(self, n: usize) -> Self;
    pub fn seed(self, seed: u64) -> Self;
    pub fn replay(self, token: &str) -> Self;
    pub fn require(self, label: &str, share: f64) -> Self;
    pub fn shrink(self, runs: usize) -> Self;
    pub fn shrink_time(self, limit: Duration) -> Self;
    pub fn max_choices(self, n: usize) -> Self;
    pub fn store(self, dir: impl AsRef<Path>) -> Self;
    pub fn explain(self, enabled: bool) -> Self;
    pub fn workers(self, n: usize) -> Self;
    pub fn hermetic(self) -> Self;
    pub fn around(self, wrapper: impl Fn(&mut dyn FnMut()) + Send + Sync + 'static) -> Self;
    pub fn draws(self, entries: &str) -> Self;
    pub fn using(self, generator: &Generator<T>) -> Self;
    pub fn example(self, values: impl IntoIterator<Item = T>) -> Self;
    pub fn examples(self, values: impl IntoIterator<Item = T>) -> Self;
}

pub fn cases<T>(n: usize) -> Options<T>;
pub fn using<T>(generator: &Generator<T>) -> Options<T>;
// … one free function for each method
```

- A property takes its settings before its body, so that the body is the
  last argument and rustfmt keeps the closure on the line of the call. A
  property form takes its settings last, after its message.
- `T` is the input of a property form. `using`, `example` and `examples`
  state values of it, so the compiler checks them against the form. A
  `prop::for_all` takes `Options<()>`.
- A setting outside its domain, such as `cases(0)`, panics, as
  `Vec::with_capacity` panics on a capacity overflow. The message of the
  panic contains the call.

`history::Options` and `stateful::Options` follow the same rule, for the
settings `budget`, `memo_limit`, `time_limit` and `workers`, and `mean`,
`max`, `swarm`, `clients`, `concurrent`, `tasks` and `repeat`.

### Generators

```rust
/// A domain, and how a case decodes a value of it from its choices.
/// Cloning a generator is cheap: the clones share one description.
pub struct Generator<T> { /* private fields */ }

impl<T> Clone for Generator<T> { /* … */ }

impl<T: 'static> Generator<T> {
    pub fn map<U: 'static>(self, f: impl Fn(T) -> U + Send + Sync + 'static) -> Generator<U>;
    pub fn filter(self, keep: impl Fn(&T) -> bool + Send + Sync + 'static) -> Self;
    pub fn bind<U: 'static>(self, f: impl Fn(T) -> Generator<U> + Send + Sync + 'static) -> Generator<U>;
}

pub fn integer<T: Integral>(range: impl RangeBounds<T>) -> Generator<T>;
pub fn float<T: Floating>(range: impl RangeBounds<T>) -> Generator<T>;
pub fn boolean() -> Generator<bool>;
pub fn just<T: Clone + Send + Sync + 'static>(value: T) -> Generator<T>;
pub fn sampled_from<T: Clone + Send + Sync + 'static>(values: impl IntoIterator<Item = T>) -> Generator<T>;
pub fn one_of<T: 'static>(generators: impl IntoIterator<Item = Generator<T>>) -> Generator<T>;
pub fn optional<T: 'static>(of: Generator<T>) -> Generator<Option<T>>;
pub fn list<T: 'static>(of: Generator<T>, sizes: impl RangeBounds<usize>) -> Generator<Vec<T>>;
pub fn dict<K: Eq + Hash + 'static, V: 'static>(keys: Generator<K>, values: Generator<V>,
    sizes: impl RangeBounds<usize>) -> Generator<HashMap<K, V>>;
pub fn string(sizes: impl RangeBounds<usize>) -> Generator<String>;
pub fn bytes(sizes: impl RangeBounds<usize>) -> Generator<Vec<u8>>;
pub fn duration(range: impl RangeBounds<Duration>) -> Generator<Duration>;
pub fn permutation<T: Clone + Send + Sync + 'static>(values: impl IntoIterator<Item = T>)
    -> Generator<Vec<T>>;
pub fn string_matching(pattern: &str) -> Generator<String>;
pub fn recursive<T: 'static>(base: Generator<T>,
    extend: impl Fn(Generator<T>) -> Generator<T> + Send + Sync + 'static) -> Generator<T>;
pub fn composite<T: 'static>(f: impl Fn(&Case<'_>) -> T + Send + Sync + 'static) -> Generator<T>;

impl<T: Floating> Generator<T> { pub fn allow_nan(self) -> Self; }
impl Generator<bool> { pub fn odds(self, numerator: u64, denominator: u64) -> Self; }
impl<T: PartialEq + 'static> Generator<Vec<T>> { pub fn unique(self) -> Self; }
impl Generator<String> { pub fn alphabet(self, characters: &str) -> Self; }
impl<T: 'static> Generator<T> { pub fn max_leaves(self, n: usize) -> Self; }
```

- A range states the bounds of a domain, as `rand::random_range` and
  proptest's strategies take them. `prop::integer(..)` is the type's whole
  range, and `prop::list(of, 1..)` states at least one element.
- `Integral` and `Floating` are sealed traits. `Integral` covers `i8` to
  `i128`, `u8` to `u128`, `isize` and `usize`, and `Floating` covers
  `f32` and `f64`.
- An empty range panics, and so does a flag on a generator of another
  kind, such as `unique` on a list that `map` returns. Both are mistakes
  in a test, and the message of the panic contains the call.
- `float` takes its width from its type: `f32` has width 32 and `f64`
  width 64.
- `duration` generates `std::time::Duration`, which has no negative
  values. A vector that states a negative duration is a declared skip.
- One generator type serves every generator, so `one_of`, `recursive`,
  `prop::register` and `prop::using` take generators of different origins
  without boxing.

### Properties and cases

```rust
pub fn for_all(seat: &dyn Seat, contract: &str, options: Options,
    body: impl Fn(&Case<'_>) + Send + Sync);

pub fn fuzz<'a>(seat: &'a dyn Seat, contract: &'a str, options: Options,
    body: impl Fn(&Case<'_>) + Send + Sync + 'a) -> impl Fn(&[u8]) + 'a;

impl<'s> Case<'s> {
    pub fn draw<T: Debug + 'static>(&self, generator: &Generator<T>, label: &str) -> T;
    pub fn assume(&self, condition: bool);
    pub fn classify(&self, label: &str);
    pub fn note(&self, message: &str);
    pub fn observe(&self, fingerprint: u64);
    pub fn target(&self, label: &str, score: f64);
    pub fn cleanup(&self, f: impl FnOnce() + Send + 'static);
    pub fn rand(&self) -> Rand<'_, 's>;
    pub fn history(&self) -> &History;
}

impl Seat for Case<'_> { /* fail, record, report, clock and cancellation of the case */ }

impl Rand<'_, '_> {
    pub fn next_u64(&mut self) -> u64;
    pub fn next_u32(&mut self) -> u32;
    pub fn fill_bytes(&mut self, dest: &mut [u8]);
}

#[cfg(feature = "rand_core")]
impl rand_core::TryRng for Rand<'_, '_> { type Error = Infallible; /* … */ }
```

- `assume`, a draw that repeats a tested case and a failure on `check`
  end the case through `std::panic::resume_unwind`, which the engine
  catches. `resume_unwind` does not call the panic hook, so these ends do
  not print a message.
- `workers(n)` runs cases on n scoped threads, and the run reports what a
  run on one worker reports.
- A run stores its failures under `testdata/prop/<target>/<test path>/`
  relative to the package root. `<target>` is the name of the test binary
  without its hash, and the test path splits at `::`.
- `fuzz` returns the target of a libFuzzer binary. The library does not
  depend on `libfuzzer-sys`, and a target of `cargo fuzz` calls the
  returned function from `fuzz_target!`.

### Property forms and derived inputs

The 39 property forms are functions of `prop` with the names of their
assertions. Each takes a closure of the input in place of each value, and
its settings last:

```rust
pub fn equal<T, U, W>(seat: &dyn Seat, got: impl Fn(&T) -> U + Sync, want: impl Fn(&T) -> W + Sync,
    msg: &str, options: Options<T>)
where T: Arbitrary + Debug + Send, U: PartialEq<W> + Debug, W: Debug;

pub fn round_trip<I, O, E, F>(seat: &dyn Seat, forward: impl Fn(&I) -> Result<O, E> + Sync,
    inverse: impl Fn(&O) -> Result<I, F> + Sync, msg: &str, options: Options<I>)
where I: Arbitrary + PartialEq + Debug + Send, E: Debug, F: Debug;
```

The input's generator is `prop::of::<T>()`, unless `using` states
another one.

```rust
/// A type whose values a property generates from the type's shape.
pub trait Arbitrary: Sized + 'static {
    /// Returns the shape of the type. A type that can refer to itself states
    /// its shape through `Definitions::define`.
    fn shape(definitions: &mut Definitions) -> Shape;
    /// Builds a value from the value that a generator of the shape decodes.
    fn from_value(value: Value) -> Result<Self, ShapeError>;
    /// Returns the value that `from_value` turns back into `self`.
    fn to_value(&self) -> Value;

    /// The shape and the conversions of a `Vec` of the type. The defaults
    /// state a list, and `u8` states bytes.
    fn list_shape(definitions: &mut Definitions) -> Shape { /* … */ }
    fn list_from_value(value: Value) -> Result<Vec<Self>, ShapeError> { /* … */ }
    fn list_to_value(items: &[Self]) -> Value { /* … */ }
}

impl Definitions {
    pub fn shape<T: Arbitrary>(&mut self) -> Shape;
    pub fn define<T: 'static>(&mut self, build: impl FnOnce(&mut Self) -> Shape) -> Shape;
}

/// The description of a type's values that a shape file states.
#[derive(Debug, Clone, PartialEq)]
#[must_use]
pub struct Shape { /* private fields */ }

impl Shape {
    pub fn boolean() -> Self;
    pub fn int(width: u32, signed: bool) -> Self;
    pub fn float(width: u32) -> Self;
    pub fn character() -> Self;
    pub fn string() -> Self;
    pub fn bytes() -> Self;
    pub fn list(of: Self) -> Self;
    pub fn fixed_list(of: Self, size: usize) -> Self;
    pub fn set(of: Self) -> Self;
    pub fn map(key: Self, of: Self) -> Self;
    pub fn optional(of: Self) -> Self;
    pub fn record<N: Into<String>>(fields: impl IntoIterator<Item = (N, Self)>) -> Self;
    pub fn enumeration<N: Into<String>>(variants: impl IntoIterator<Item = (N, Option<Self>)>)
        -> Self;
    pub fn literal(values: impl IntoIterator<Item = Value>) -> Self;
    pub fn reference(name: &str) -> Self;
    pub fn uuid() -> Self;
    pub fn ip_address() -> Self;
    pub fn decimal() -> Self;
    pub fn instant() -> Self;
    pub fn date() -> Self;
    pub fn time_of_day() -> Self;
    pub fn local_date_time() -> Self;
    pub fn duration() -> Self;
    pub fn offset() -> Self;
    pub fn zone() -> Self;
    pub fn zoned_date_time() -> Self;
    pub fn wall_time() -> Self;

    pub fn min(self, bound: impl Into<Value>) -> Self;
    pub fn max(self, bound: impl Into<Value>) -> Self;
    pub fn min_size(self, n: usize) -> Self;
    pub fn max_size(self, n: usize) -> Self;
    pub fn pattern(self, pattern: &str) -> Self;
    pub fn alphabet(self, characters: &str) -> Self;
    pub fn allow_nan(self) -> Self;
    pub fn allow_infinity(self) -> Self;
    pub fn unit(self, unit: &str) -> Self;
    pub fn scale(self, digits: u32) -> Self;
    pub fn version(self, version: u8) -> Self;
}

/// A shape that does not read, or a value that is no value of a type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapeError { /* private fields */ }

impl ShapeError {
    pub fn new(reason: impl Into<String>) -> Self;
}

pub fn of<T: Arbitrary>() -> Generator<T>;
pub fn of_shape(text: &str) -> Result<Generator<Value>, ShapeError>;
pub fn shape_of<T: Arbitrary>() -> Result<String, ShapeError>;
pub fn register<T: Arbitrary + Debug>(generator: Generator<T>);
pub fn register_values<T: Arbitrary + Clone + Send + Sync>(values: impl IntoIterator<Item = T>);
pub fn register_variants<I: Arbitrary>(variants: impl IntoIterator<Item = Variant<I>>);

impl<I: 'static> Variant<I> {
    pub fn of<V: Arbitrary>(name: &str, into: fn(V) -> I, from: fn(&I) -> Option<&V>) -> Self;
    pub fn unit(name: &str, value: fn() -> I, is: fn(&I) -> bool) -> Self;
}
```

- `#[derive(Arbitrary)]` implements the trait for a struct, a tuple
  struct and an enum, with fields in declaration order. The attribute
  `#[prop(...)]` on a field states the constraints of the definition's
  constraint table with its keys: `min`, `max`, `min_size`, `max_size`,
  `pattern`, `alphabet`, `allow_nan`, `allow_infinity`, `unit`, `scale`
  and `version`. `#[prop(rename = "...")]` names a field or a variant in
  the shape. An unknown key fails to compile, and the UI tests of the
  derive crate pin each such error with trybuild.
- A constraint applies to the shape that takes it, and otherwise to the
  content of a list, a fixed-list, a set or an optional.
  `Shape::list(Shape::int(8, false)).max(9)` bounds the elements. A
  constraint that applies to no part of the shape makes the shape fail
  to read.
- The trait is implemented for the types of the definition's reading
  table for Rust: the primitives with `i128`, `u128` and `char`,
  `String`, `Vec`, arrays, tuples as records, `HashMap`, `BTreeMap`,
  `HashSet`, `BTreeSet`, `Option`, `Box`, `SystemTime`, `Duration`,
  `IpAddr`, `Ipv4Addr` and `Ipv6Addr`, and the types of the five optional
  crates. A `Vec<u8>` is bytes.
- `Value` implements the trait without a shape of its own. A property
  over values states their generator with `using`, such as one that
  `of_shape` returns.
- A form requires `T: Arbitrary`, as proptest's `proptest!` requires
  `Arbitrary` for an argument without a strategy. A test of a type from
  another crate derives `Arbitrary` on a newtype, or draws the type in
  `prop::for_all` with a generator of its own.
- A derived type refuses a record whose fields differ from its own and a
  variant that the enum lacks. A type whose values are fewer than the
  values of its shape, such as `jiff::Zoned`, refuses the others. Each
  refused value makes its case invalid, as a case that `assume` rejects
  is.
- `of` panics when the shape of `T` does not read, with the reason. A
  shape that a caller states can fail only at run time, and a test that
  meets the panic has a mistake in its shape.
- `register` makes a generator the generator that `of::<T>()` returns,
  and the generator of `T` wherever `T` occurs in a derived input.
  `register_values` makes the shape of `T` a literal over its values, and
  `register_variants` makes it an enum of its variants. A second
  registration of `T` panics, and so does a registration of `T` after a
  read of the registry used the shape of `T`. The definition applies a
  registration to the whole test process, so a later registration would
  change values that a run already generated.
- `of_shape(text)` returns a `Generator<Value>`, because a shape file
  states no Rust type. A record decodes to `Value::Record` and a variant
  to `Value::Variant`. `shape_of::<T>()` returns the shape file of `T`,
  which `of_shape` reads in any language of the definition.

### Histories

```rust
impl History {
    pub fn new() -> Self;
    pub fn invoke(&self, client: usize, operation: &str, args: impl IntoIterator<Item = Value>,
        keys: impl IntoIterator<Item = Value>) -> Call<'_>;
    pub fn events(&self) -> Vec<Event>;
}

impl Call<'_> {
    pub fn ok(self, output: impl Into<Value>);
    pub fn fail(self, error: impl Display);
    pub fn unknown(self, error: impl Display);
}

/// One recorded event of a history.
pub struct Event { pub index: usize, pub call: usize, pub client: usize, pub process: usize,
    pub kind: Kind }

pub enum Kind {
    Invoke { operation: String, args: Vec<Value>, keys: Vec<Value> },
    Ok { output: Value },
    Fail { error: String },
    Unknown { error: String },
}

/// The sequential specification of an object.
pub trait Spec: Sync {
    type State: Clone + PartialEq + Debug + Send + Sync + 'static;
    fn initial(&self) -> Self::State;
    fn next(&self, state: &Self::State, operation: &Operation) -> Vec<Self::State>;
    /// The default compares the states with `==`.
    fn equal(&self, a: &Self::State, b: &Self::State) -> bool { /* … */ }
    /// The default hashes every state alike.
    fn hash(&self, state: &Self::State) -> u64 { /* … */ }
}

impl Operation {
    pub fn name(&self) -> &str;
    pub fn args(&self) -> &[Value];
    pub fn known(&self) -> bool;
    pub fn output(&self) -> Option<&Value>;
    pub fn returned(&self, value: impl Into<Value>) -> bool;
}

pub fn spec_from<F, S>(factory: F) -> impl Spec<State = Vec<Operation>>
where F: Fn() -> S + Sync, S: FnMut(&str, &[Value]) -> Value;

pub fn is_linearizable<S: Spec>(seat: &dyn Seat, history: &History, spec: &S, msg: &str,
    options: Options);
pub fn is_serializable(seat: &dyn Seat, history: &History, msg: &str);
pub fn has_snapshot_isolation(seat: &dyn Seat, history: &History, msg: &str);

pub fn concurrently<O, E>(clients: usize, within: Duration,
    body: impl Fn(usize) -> Result<O, E> + Send + Sync + 'static) -> Vec<Outcome<O, E>>
where O: Send + 'static, E: Send + 'static;

pub struct Outcome<O, E> { pub client: usize, pub result: Option<Result<O, E>> }

pub fn from_intervals(entries: &[Interval]) -> Result<History, IntervalError>;
```

- `invoke` takes the keys that a call touches beside its arguments. Two
  calls that share a key are in one partition of the check of
  linearizability, and a call without keys touches every key.
- `Spec::equal` must be exact, because the search merges two
  configurations whose states it reports equal. `Spec::hash` must agree
  with `equal`. A spec of a long history states a hash of its own.
  `Spec` also has a hidden member, `cost`, which the spec of `spec_from`
  uses to count the calls that it replays.
- `spec_from` builds the spec from the subject itself. A check against
  it finds a call of a history that was not atomic.
- `concurrently` runs each client on a thread of its own, and releases
  the clients at once through a `Barrier`. It waits until every body has
  returned or `within` has passed on the platform clock. A body that is
  still running then has an outcome without a result. Its thread runs
  on, because a thread cannot be stopped from outside, so the body and
  its results are `'static`.
- `from_intervals` builds the history of calls that a log of a system
  records with a start and an end on one clock.
- `history::Options` states the budget, the memo limit, the time limit
  and the workers of the search. `is_linearizable` with more than one
  worker searches on scoped threads.
- The overlay keeps its limit on `history`: threads run on more than one
  core, and the recorder's counter can supply a barrier that the subject
  lacks.

### Machines and the task scheduler

`Machine` and `Action` are builders, because most of their members are
optional. The run of an action is an async closure, because a step of a
concurrent section can run as a task of a scheduler:

```rust
let machine = Machine::new([
    Action::with_input("put", |c, _| c.draw(&prop::integer(0..=9_i64), "v"),
        async |c, client, v| { /* … */ }).weight(3),
    Action::new("get", async |c, client| { /* … */ }),
])
.spec(QueueSpec::new(4))
.invariant(|c, state| { /* … */ });

stateful::steps(c, &machine, stateful::mean(20).clients(2));
```

```rust
impl<'a, S: 'a> Action<'a, S> {
    pub fn new(name: &str, run: impl AsyncFn(&Case<'_>, usize) + Send + Sync + 'a) -> Self;
    pub fn with_input<I: Send + 'static>(name: &str,
        input: impl Fn(&Case<'_>, &S) -> I + Send + Sync + 'a,
        run: impl AsyncFn(&Case<'_>, usize, I) + Send + Sync + 'a) -> Self;
    pub fn weight(self, weight: u64) -> Self;
    pub fn drain(self) -> Self;
    pub fn enabled(self, enabled: impl Fn(&S) -> bool + Send + Sync + 'a) -> Self;
    pub fn name(&self) -> &str;
}

impl<'a, S> Machine<'a, S> {
    pub fn new(actions: impl IntoIterator<Item = Action<'a, S>>) -> Self;
    pub fn spec(self, spec: impl Spec<State = S> + 'a) -> Self;
    pub fn invariant(self, invariant: impl Fn(&Case<'_>, &S) + 'a) -> Self;
    pub fn settle(self, settle: impl Fn(&Case<'_>, &S) + 'a) -> Self;
}

pub fn steps<S>(case: &Case<'_>, machine: &Machine<'_, S>, options: Options<'_>)
where S: Clone + PartialEq + Debug + Default + Send + Sync + 'static;

impl<'s> Options<'s> {
    pub fn mean(self, n: usize) -> Self;
    pub fn max(self, n: usize) -> Self;
    pub fn swarm(self, on: bool) -> Self;
    pub fn clients(self, n: usize) -> Self;
    pub fn concurrent(self, n: usize) -> Self;
    pub fn tasks(self, scheduler: &'s Scheduler<'_>) -> Self;
    pub fn repeat(self, n: usize) -> Self;
}

impl<'c> Scheduler<'c> {
    pub fn new(case: &'c Case<'_>, strategy: Strategy) -> Self;
    pub fn spawn(&self, task: impl AsyncFnOnce(&Scheduler<'c>) + Send + 'c);
    pub fn yield_now(&self) -> YieldNow<'_, 'c>;
    pub fn run(&self);
}

pub fn uniform() -> Strategy;
pub fn pct(depth: u64) -> Strategy;
```

- The functions of a machine and of its actions receive a state of the
  spec: the first of the states that the last check of the history left.
  A machine without a spec receives `S::default()`. `steps` requires
  `S: Default` for that state.
- The draws of a step's input are choices of the case, so the input
  shrinks with the case. `with_input` takes the input from the case
  before the run.
- A concurrent section runs its clients on threads, and a case whose
  section runs on threads runs up to `repeat` times, because threads do
  not replay. Under `tasks` the clients run as tasks of a `Scheduler`,
  whose releases are choices of the case, so a race replays and shrinks.

The task scheduler is an executor of futures on `std::task` alone. A
subject spawns its tasks through it, and the strategy chooses the next
ready task at each `yield_now().await`: `uniform` by a choice of the
case, and `pct` by the priorities of probabilistic concurrency testing.
A task that awaits a future of something other than the scheduler waits
until another task wakes it. When no task is ready and a task waits, the
run ends the case with a fault that lists the waiting tasks.

### Trees of files and golden files

```rust
let tree = Tree::from([
    ("bin/run", files::executable("#!/bin/sh\necho ok\n")),
    ("keys", files::directory().with_mode(0o700)),
]);
let dir = files::workspace(&t, &tree);
files::equal(&t, &dir, &tree, "the build leaves the tree as written");
```

```rust
/// A tree of files, each entry at a path with `/` between its names, in the
/// order of the bytes of the paths.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Tree { /* private fields */ }

impl Tree {
    pub fn new() -> Self;
    pub fn insert(&mut self, path: impl Into<String>, entry: Entry) -> Option<Entry>;
    pub fn get(&self, path: &str) -> Option<&Entry>;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
}
impl<P: Into<String>, const N: usize> From<[(P, Entry); N]> for Tree { /* … */ }
impl<P: Into<String>> FromIterator<(P, Entry)> for Tree { /* … */ }

/// A file, a directory or a symbolic link. It prints as the expression that
/// states it, such as `text("secret\n").with_mode(0o600)`.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Entry { /* private fields */ }

impl Entry {
    /// Panics for a bit beyond the nine permission bits and for a link.
    pub fn with_mode(self, mode: u32) -> Self;
}

pub fn text(text: impl Into<String>) -> Entry;
pub fn bytes(content: impl Into<Vec<u8>>) -> Entry;
pub fn executable(text: impl Into<String>) -> Entry;
pub fn directory() -> Entry;
pub fn link(target: impl Into<String>) -> Entry;

/// A directory below the system's temporary directory, which its drop removes.
#[derive(Debug)]
pub struct Workspace { /* private fields */ }

impl Workspace {
    pub fn path(&self) -> &Path;
}
impl AsRef<Path> for Workspace { /* … */ }

pub fn workspace(seat: &dyn Seat, tree: &Tree) -> Workspace;
pub fn write(seat: &dyn Seat, dir: impl AsRef<Path>, tree: &Tree);
pub fn read(seat: &dyn Seat, path: impl AsRef<Path>) -> String;

pub fn equal(seat: &dyn Seat, dir: impl AsRef<Path>, want: &Tree, msg: &str);
pub fn contains(seat: &dyn Seat, dir: impl AsRef<Path>, want: &Tree, msg: &str);
pub fn unchanged(seat: &dyn Seat, dir: impl AsRef<Path>, body: impl FnOnce(), msg: &str);
pub fn absent(seat: &dyn Seat, path: impl AsRef<Path>, msg: &str);
pub fn is_file(seat: &dyn Seat, path: impl AsRef<Path>, msg: &str);
pub fn is_dir(seat: &dyn Seat, path: impl AsRef<Path>, msg: &str);
pub fn links_to(seat: &dyn Seat, path: impl AsRef<Path>, target: &str, msg: &str);
pub fn has_content(seat: &dyn Seat, path: impl AsRef<Path>, want: impl AsRef<[u8]>, msg: &str);
pub fn has_mode(seat: &dyn Seat, path: impl AsRef<Path>, mode: u32, msg: &str);

// in golden
pub fn matches_tree(seat: &dyn Seat, name: &str, got: impl AsRef<Path>, update: bool,
    scrubbers: &[Scrubber]);
```

- Every path parameter takes `impl AsRef<Path>`, and the assertions of
  the module end the test at a failure, as the golden comparisons do.
- `files::workspace` returns a `Workspace`, a temporary directory that
  its drop removes, as `tempfile::TempDir` does. The drop first gives the
  owner every permission on each directory, so it also removes a
  directory without write permission.
- A write creates each entry in path order, with a call that refuses an
  entry that is there. `files::write` reads each path of the tree and each
  parent with `symlink_metadata` before it writes anything. Neither
  follows a link out of the directory.
- A mode is set with `set_permissions` after the file is written, so the
  umask does not decide it. Windows does not record a mode, so `has_mode`
  ends with a fault there, and a write sets the read-only attribute of a
  file whose mode lacks the owner's write bit.
- `Value::Tree` states a tree in a record. A record states a file longer
  than 65,536 bytes by its size and its SHA-256 digest. The crate computes
  the digest itself in 152 lines, because the standard library has no
  digest and one function does not justify a dependency.
- The golden assertions take the decision to update as an argument,
  `golden::should_update()`, as Go and TypeScript do. The variable is
  `DOKIMI_ASSERT_UPDATE_GOLDEN`. `golden::matches_tree` resolves a name
  against `testdata/golden` in the working directory, as
  `golden::matches` does, so the conformance crate runs each vector of
  `golden-match-tree` in a child process inside a directory of its own.

### Benchmarks

`Contract` gains `warmup(iterations)` and `run_parallel(workers,
iterations, body)`. `warmup` after `run` panics, and so does
`run_parallel` with no worker, as the definition states.

### The tokio crate

`dokimi-assert-tokio` states the six assertions whose subject is a
future, under the names of the synchronous crate, on the surfaces `check`
and `soft`. Each function returns a future that the test awaits:

```rust
pub fn honours_cancellation<'a, E: Error + 'static>(seat: &'a dyn Seat,
    body: impl AsyncFnOnce(Option<&Cancel>) -> Result<(), E> + 'a, msg: &'a str)
    -> impl Future<Output = ()> + 'a;
pub fn honours_deadline<'a, E: Error + 'static>(seat: &'a dyn Seat,
    body: impl AsyncFnOnce(Option<&Cancel>) -> Result<(), E> + 'a, msg: &'a str)
    -> impl Future<Output = ()> + 'a;
pub fn completes_within<'a>(seat: &'a dyn Seat, within: Duration, body: impl Future + 'a,
    msg: &'a str) -> impl Future<Output = ()> + 'a;
pub fn eventually<'a>(seat: &'a dyn Seat, timeout: Duration, interval: Duration,
    body: impl AsyncFn(&dyn Seat) + 'a, msg: &'a str) -> impl Future<Output = ()> + 'a;
pub fn eventually_true<'a>(seat: &'a dyn Seat, timeout: Duration,
    predicate: impl AsyncFn() -> bool + 'a, msg: &'a str) -> impl Future<Output = ()> + 'a;
pub fn no_task_leaks<'a>(seat: &'a dyn Seat, body: impl AsyncFnOnce() + 'a, msg: &'a str)
    -> impl Future<Output = ()> + 'a;
```

- A subject takes the core crate's `Cancel` and awaits
  `Cancel::stopped`, a future that any executor can poll. tokio-util's
  `CancellationToken` states only that it stopped, so a subject could not
  tell a cancellation from an expired deadline.
- An assertion takes an async closure when it hands the body an argument,
  runs the body more than once, or reads the runtime before the body
  starts. `completes_within` only times its body, so it takes a future, as
  `tokio::time::timeout` does.
- Each function is `#[track_caller]` and reads its call site before it
  returns the future. On an `async fn` the attribute has no effect, as the
  lint `ungated_async_fn_track_caller` of rustc states.
- The assertions that wait read tokio's clock, so a test that pauses the
  clock does not spend real time. `honours_cancellation` and
  `honours_deadline` do not read a clock, and they run on any executor.
- `no_task_leaks` compares `RuntimeMetrics::num_alive_tasks` before and
  after the body. Tokio's stable API counts the alive tasks and does not
  list them, so the detail `leaked` is the number of tasks. Go, Python and
  TypeScript state a list of names.
- The count is read up to 100 times, 5 ms apart, so a task that ends
  within half a second is not a leak, as in Go and Python. The readings
  also cover the count of the multi-thread runtime, which tokio documents
  as not strongly consistent. Outside a tokio runtime the call ends with a
  fault.
- The crate takes the internal reporting of the core crate through
  `dokimi_assert::__private`, a module that the documentation hides.

### Conformance

The crate `dokimi-assert-conformance` contains the vendored definition at
its root, as `.spec.yaml` declares: `VERSION`, `manifest.json`,
`corpus/`, `overlays/rust.json` and `spec/`. `spec sync` writes the copy
from a commit of assert-spec, and `spec check` fails for a file that
differs from the manifest. `spec sync` also writes the definition's zone
table into the shapes of `dokimi-assert`, which embed it. The drivers of
the corpus and the vectors are in `src/`.

Its specs run every corpus case on both surfaces of each library that
states the case's assertion. They also run every vector of the property
engine, the histories, the machines and the files. The drivers depend on
`dokimi-assert` and `dokimi-assert-tokio` as a test of a user depends on
them:

- The history drivers run an invocation that the history refuses inside
  `quietly` of `__private`, which keeps the panic hook from printing the
  panic. Apart from that call, a corpus case, a history vector, a machine
  vector and a file vector run through the public API.
- A behaviour vector and a recording vector run `prop::for_all` on a
  recorder. A replay token states the choices of a case, and the call
  record of a property states its counterexample and its choices.
- A fixtures vector reads the shape of a Rust type, and a forms vector
  runs a property form.
- The kinds of the engine run on `prop::engine`: decoding, generation,
  the fuzz bridge, the inverse, draws, shrinking, coverage, the replay
  token and the store. The definition pins each of these algorithms, and
  no public function exposes them. `prop::engine` is public and hidden
  from the documentation. So are `Case::fail_as` and `Case::fail_with`,
  which fail a case as a vector states, and `Generator::valued`, which
  returns the values of a generator as `Value`s.

The drivers decode the definition's typed literals with a decoder of
their own. Their verdicts compare JSON through `serde_json::Value`, and
never through the library under test. The crate is not published, so no
package of the library contains the definition.

### Tests and CI

- Every source file has its spec, at the same path under the crate's
  `tests/`. A spec calls the public API alone, and `src/` contains no
  test. `docs/standards/testing.md` states the testing rules for Rust.
- The specs of a crate build as one test binary, `tests/main.rs`, which
  installs the counting allocator. A second binary checks the ceilings
  without it.
- `cargo llvm-cov` fails when a line or a function of a published crate
  did not run in a test, in CI and through `make coverage`. llvm-cov's
  line percentage counts a generic function's lines in its best
  instantiation alone, so the gate counts uncovered lines over every
  instantiation. The tests of the conformance crate run in the
  measurement, because its vectors call the library. The report leaves
  out the crate's own lines.
- The spec of `lib.rs` imports every Rust name of the naming table and
  calls every member that the table names. A missing name then fails to
  compile. A test of the spec reads the vendored table and the overlay.
  It fails for a row whose name the spec does not contain.
- CI runs the jobs `lint`, `test`, `msrv`, `coverage`, `deny`, `package`
  and `spec`. The tests run on stable on Linux, macOS and Windows, and
  `msrv` checks 1.85 on Linux. `make check` runs the same stages.
- `make msrv` runs `rustup run 1.85 cargo check`, which works with every
  installation of cargo. `cargo +1.85` works only with rustup's proxy.

### The definition

The library vendors definition 9.0.0, after these corrections in
assert-spec:

| Change | Where | Reason |
|---|---|---|
| Seven Rust names with a `.`, such as `golden.Scrubber`, take `::` | `spec/naming.yaml` | The table states that Rust separates the parts of a name with two colons |
| The limits on `prop.of` and `prop.of-shape` name the feature `jiff` of `dokimi-assert` | `overlays/rust.json` | The orphan rule puts the implementations for jiff's types in the crate that declares the trait |
| The overlay declines `assertion` and `that` because `equal` compares through `PartialEq`, so a pair that does not compare fails to compile | `overlays/rust.json` | `equal` takes two type parameters |

## Alternatives considered

### A. Settings as a trailing list of option values

RFC-0002 proposed `prop::for_all(seat, contract, body, [prop::cases(500)])`,
with the options named after Go's functional options. A Go property and
its Rust port would then read alike.

**Why not:** a form's `using` states a generator of the form's input,
and a list of option values of one type cannot state the input's type. A
list would check `using` at run time. Rust's standard library states
optional settings with builders, and a Rust caller expects one.

### B. A seat that the test does not pass

The assertions would report to a seat in a thread-local variable, as
googletest's `expect_that!` reports to the test that
`#[gtest]` sets up. `check::equal(got, want, msg)` would then read like
`assert_eq!`.

**Why not:** a thread-local seat is absent on every thread that a test
starts, and the definition requires a seat that accepts failures from
more than one thread. The body of a property, of `eventually` and of
`rejects` would each replace the thread's seat for the time of a call,
and a body that starts a thread would report nowhere. An explicit seat
has neither problem. RFC-0001 rejected a macro that captures the seat for
the same reasons.

### C. A trait per generator, as proptest's `Strategy`

Each generator would be a type of its own that implements a trait
`Generator`, with flags as methods of each type and `boxed()` for a
generator of an unnamed type.

**Why not:** `one_of`, `recursive`, the registry, the shapes and
`using` all take generators of different origins, and each would need a
boxed generator. A caller would then name `BoxedGenerator<T>` in most of
their signatures. One generator type with flags as methods costs one
runtime check per flag.

### D. Typed literals from `type_name` and the `Debug` text

The library would read `std::any::type_name::<T>()` and parse the
`Debug` text of the common types into typed literals, so that a call
record of `equal(1, 2)` states two ints.

**Why not:** the standard library documents that `type_name` is not a
stable identifier and that `Debug` output is not stable. A record would
depend on the compiler's version.

### E. Crates at the root of the repository

Each crate would be a directory at the root of the repository, next to
`docs/`. tokio, serde and proptest lay out their crates this way.

**Why not:** the root would then list four crates among the files of the
workspace, and a reader could not tell the published crates from the
repository's own files. Under `crates/` the root lists the workspace
alone.

### F. Conformance as a test module of the core crate

The drivers would be a module of `dokimi-assert` that only its tests
compile, as the conformance package of the Go library is a package of
its module. The drivers could then call the engine's crate-internal
functions, and `prop::engine` would not be public.

**Why not:** a module of the core crate cannot run the definition
against `dokimi-assert-tokio`, which depends on the core crate. The
vendored definition would be part of the core crate's directory. Every
package of the crate would then need to leave it out. A crate of its own
costs one hidden public module for the vectors of the engine's
algorithms.

### G. Unit tests beside each module

Each module would end with `#[cfg(test)] mod tests`, inline or in a
`tests.rs` of its own. The standard library keeps its unit tests this
way.

**Why not:** a unit test sees the private items of its module, so it
tests the implementation rather than the contract. The testing standard
allows black-box tests alone. `src/` would also mix the code with its
specs.

## Drawbacks

- **Size.** The TypeScript library implements definition 9.0.0 in 35,274
  source lines and 45,842 test lines, and the Go library in 58,929 and
  70,259, measured on 2026-10-21. The Rust crates implement it in 40,387
  source lines and 22,387 test lines, the conformance crate included,
  measured on 2026-10-22.
- **A hidden module.** `prop::engine` is public, so a caller can import
  it. The documentation hides it, and its items change without notice.
- **A third published crate.** A caller who derives inputs adds the
  feature `derive`, and the library publishes three crates in step.
- **Opaque values.** A value of a caller's type is text in a record, so
  two values that print alike are not told apart.
- **Breaking changes.** `rejects` returns failure records in place of a
  message, and the golden assertions take the decision to update as an
  argument. `Failure` loses its public fields, `Holds` becomes
  `Contains`, and the module `matcher` is no longer public. A call of
  `equal`, `is_none` or `is_some` with references still compiles. No
  release contains the old API.
- **The error type of a closure that cannot fail.** Such a closure
  states its error type when it returns, as in `Ok::<_, ()>(value)`.
- **Panics on misuse.** A flag on a generator of another kind panics when
  the test builds the generator, and not when it compiles.
- **Links.** A write checks each parent before it creates an entry, so a
  link that another process creates between the check and the write is
  followed.
- **Mutation testing.** dokimasia has no mutation engine for Rust, so
  coverage is the only measure of the tests.
- **Spawned assertions.** rustc 1.99 rejects the `Send` check of the
  future of `eventually` when its body is an async closure over
  `&dyn Seat`. A test awaits that future, and cannot hand it to
  `tokio::spawn`.
- **Leaked tasks.** `no_task_leaks` counts the tasks of the whole runtime.
  A task that ends while the body runs hides a leaked task, and a task of
  another test on the same runtime counts as a leak.

## References

| What | Where |
|---|---|
| The definition, its corpus, its vectors and the overlay format | <https://github.com/dokimasia/assert-spec> |
| The Rust API Guidelines, C-BUILDER, C-HIDDEN and C-STRUCT-PRIVATE | <https://rust-lang.github.io/api-guidelines/checklist.html> |
| `assert_eq!`, which compares through `PartialEq` | <https://doc.rust-lang.org/std/macro.assert_eq.html> |
| `std::any::type_name`, whose output is not guaranteed | <https://doc.rust-lang.org/std/any/fn.type_name.html> |
| `std::panic::resume_unwind`, which does not call the panic hook | <https://doc.rust-lang.org/std/panic/fn.resume_unwind.html> |
| googletest-rust, `expect_that!` and `#[gtest]` | <https://github.com/google/googletest-rust> |
| proptest, `Strategy`, `BoxedStrategy` and `Arbitrary` | <https://github.com/proptest-rs/proptest> |
| The Go and TypeScript libraries, counted with `wc -l` on 2026-10-21 | <https://github.com/dokimasia/assert-go>, <https://github.com/dokimasia/assert-typescript> |
