---
rfc: 0002
title: Definition 8.0 in Rust
author: Roy Klopper <roy.klopper@stealthscale.io>
status: Withdrawn
created: 2026-10-11
updated: 2026-10-21
discussion: https://github.com/dokimasia/assert-rust/issues/7
supersedes: none
superseded-by: RFC-0003
produces-adr: none
---

<!--
  ~ Copyright Dokimasia B.V. 2026
  ~ SPDX-License-Identifier: MIT
-->

# RFC-0002: Definition 8.0 in Rust

## Summary

The library moves from definition 1.1.0 to 8.0.0 in one step. It gains
the 68 assertions and the 193 Rust names that the definition added since
1.1.0:

- the property engine, with its generators, its property forms and its
  derived inputs
- the 14 relations
- the history seam and its three checks
- machines and the task scheduler
- trees of files, the file assertions and `golden::matches_tree`
- call records, faults, and the accessors of the failure record
- `warmup` and `run_parallel` on the bench contract

The definition fixes the names, the rules and the vectors. This proposal
decides only what Rust must decide:

- **Crates.** A third crate, `dokimi-assert-derive`, supplies the derive
  macro that reads a type into a shape. The core crate is still
  synchronous.
- **Values.** The detail of a failure becomes a typed literal. A value
  of a caller's own type is an opaque literal with its `Debug` text,
  because an assertion is generic over what it compares.
- **Options.** A generator and a property take their optional parameters
  as a trailing list of option values, with the names of Go's options.
- **Records.** The call records of a test binary go to a file beside the
  binary, as the overlay states.
- **Tasks.** The task scheduler is an executor of futures in the core
  crate. A subject spawns its tasks through it.
- **JSON.** The core crate reads and writes JSON through `serde_json`, for
  the store and the call records.
- **Time.** `SystemTime` and `Duration` are the only date and time types
  without a feature. The types of `chrono`, `time`, `jiff`, `uuid` and
  `rust_decimal` read behind features of the derive crate.
- **The definition.** Definition 8.0.0 adds two limits for time zones to
  the Rust overlay. The library vendors 8.0.0.

## Motivation

The library vendors definition 1.1.0. It implements all 42 of its
assertions, and names 79 of the 272 Rust names of 7.2.1. Its completeness
gate fails on every row that it neither implements nor declines, so the
library cannot vendor a later version until it implements the rest.

The shared vectors pin each addition: 430 property vectors, 80 history
vectors, 14 machine vectors and 57 file vectors. The corpus grew from 87
cases at 1.1.0 to 195 at 7.2.1. The library runs none of the vectors. The
Go and TypeScript libraries run them, so Rust disagrees with both wherever
it has not caught up:

- `matches` compiles the pattern with `Regex::new`, so it accepts every
  pattern of the `regex` crate, and `\d` matches every Unicode digit
  (`dokimi-assert/src/matcher/text.rs`, lines 49 to 51).
- `rejects` returns the first message of the check, and the definition
  returns its failure records (`dokimi-assert/src/check.rs`, line 410).
- The detail of a failure contains text where the definition states typed
  literals, so a call record cannot state it
  (`dokimi-assert/src/failure.rs`, lines 28 to 43).

## Detailed design

### Components

| Component | Modules | Responsibility |
|---|---|---|
| Values | `value.rs` | `Value`, the typed literals of the definition, and the opaque literal |
| Call records | `record/` | Numbers calls, writes each record to the file of the binary, and reads `DOKIMI_ASSERT_RECORD` once per process |
| Verdicts | `matcher/verdict.rs` | Reports a pass, a failure and a fault, and writes the call record of each |
| Failure record | `failure.rs` | `Failure`, with `want`, `got` and `case_failure` |
| Seats | `seat.rs` | `cancellation`, `Recorder::records` and `Recorder::with_cancellation` |
| Portable patterns | `pattern/` | Parses the portable subset for `matches` and `prop::string_matching` |
| Relations | `matcher/relation.rs`, `check.rs`, `soft.rs` | The 14 relations on both surfaces |
| Property engine | `prop/` | The public module `prop`: the source, the decoder, the shrinker, coverage, tokens, the store, campaigns, shapes, the forms and the fuzz bridge |
| Histories | `history/` | The public module `history`: the seam, specs, `concurrently` and the three checks |
| Machines | `stateful/` | The public module `stateful`: machines, their steps and the task scheduler |
| Files | `files/`, `golden.rs` | The public module `files`, and `golden::matches_tree` |
| Bench | `bench.rs` | `warmup` and `run_parallel` |
| Derive | `dokimi-assert-derive` | `#[derive(Shaped)]` and the attribute `#[prop(...)]` |
| Conformance | `tests/conformance/` | A driver for each vector family, and the corpus driver for the 8.0.0 corpus |

### The definition: 8.0.0

A zone resolves through the time-zone database of `jiff`, behind its
feature. That database can be of another release than tzdata 2026e. The
definition states that the overlay declares the difference, and the Rust
overlay states nothing about zones, so the change goes to assert-spec
first:

| Change | Where | Reason |
|---|---|---|
| A limit on `prop.of`: a zone resolves through the time-zone database of `jiff`, behind the feature `jiff`. A value in a zone can then show another wall time than tzdata 2026e states | `overlays/rust.json` | Rust's standard library has no time zones, and `jiff` reads the system's database or a copy of its own |
| A limit on `prop.of-shape`, with the zone text of the `prop.of` entry | `overlays/rust.json` | The same reason |

Definition 8.0.0 also changes the count of `max-allocs`. The vendored
copy then comes from `main` of assert-spec at 8.0.0.

### Values

```rust
/// A value of a failure's detail or of a call record, as the definition's typed
/// literals state it.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i128),
    Float(f64),
    Text(String),
    Bytes(Vec<u8>),
    List(Vec<Value>),
    Map(Vec<(Value, Value)>),
    Record(Vec<(String, Value)>),
    Variant { name: String, payload: Option<Box<Value>> },
    Tree(Vec<TreeEntry>),
    /// A value outside the typed literals, with the text that its `Debug` writes.
    Opaque(String),
}
```

- An assertion that takes a value of a known type states it as its
  literal: a `&str` as `Text`, a count as `Int`, a tolerance as `Float`.
- An assertion that is generic over `T: PartialEq + Debug` states the
  caller's value as `Opaque`, with the text of `{:?}`. The value of a call
  record is then the text that the failure prints.
- `Failure::detail` becomes `Vec<(&'static str, Value)>`, and the type
  `Detail` is removed.

The corpus driver compares a record with a case through the same rule. A
case value of a generic field becomes the `Debug` text of the Rust value
that the driver decoded.

### The failure record

```rust
impl Failure {
    /// The value of the field want, `None` when the assertion does not declare it,
    /// and `Some(&Value::Null)` when it declares it without a value.
    #[must_use]
    pub fn want(&self) -> Option<&Value>;

    /// The value of the field got, with the rule of `want`.
    #[must_use]
    pub fn got(&self) -> Option<&Value>;

    /// The failure record of a failing case of a property, and `None` for any
    /// other failure.
    #[must_use]
    pub fn case_failure(&self) -> Option<Failure>;
}
```

`rejects` returns the failure records of its check, in call order:

```rust
#[track_caller]
pub fn rejects<F>(seat: &dyn Seat, body: F, msg: &str) -> Vec<Failure>
where
    F: FnOnce(&Recorder);
```

### Faults

An assertion that cannot run reports a fault, such as a malformed shape, a
spec function that panics, or the switch `DOKIMI_ASSERT_RECORD` set to a
value that the definition does not define. A fault ends the call with the
verdict `error`. The seat receives the text of the fault through `fail`,
and the call record states the text in the field `error`.

### Call records

`DOKIMI_ASSERT_RECORD=1` records every assertion call. The library reads
the variable once per process.

- The records of a test binary go to a file beside the binary, named after
  it with the suffix `.dokimi-assert.jsonl`. The first record of a run
  creates the file anew. Each record is one line.
- Each record states the test in the field `test`, as libtest names the
  thread of the test, such as `nested::fails`.
- A `Recorder` keeps its records whatever the switch states, and
  `Recorder::records` returns them as JSON lines.

### The seat

```rust
pub trait Seat {
    fn helper(&self) {}
    fn fail(&self, message: &str);
    fn record(&self, message: &str);
    fn report(&self, _failure: &Failure, _aborting: bool) {}
    fn clock(&self) -> &dyn Clock { /* the platform clock */ }
    /// The cancellation handle of the test, and a `Cancel` that is never stopped
    /// for a seat that states none.
    fn cancellation(&self) -> &Cancel { /* a Cancel that is never stopped */ }
}

impl Recorder {
    /// The call records of the calls on this recorder, in the order of their seq,
    /// as JSON lines.
    #[must_use]
    pub fn records(&self) -> Vec<String>;

    /// Make cancellation the handle of this recorder.
    #[must_use]
    pub fn with_cancellation(self, cancellation: Cancel) -> Self;
}
```

`Cancel` gains a parent, so the handle that `rejects` and `eventually`
hand a body derives from the handle of the assertion's seat and stops
with it.

### Relations

The 14 relations are in `check` and `soft`:

```rust
#[track_caller]
pub fn is_idempotent<I, O, C, B>(seat: &dyn Seat, call: C, input: I, observe: B, msg: &str)
where
    C: FnMut(&I),
    B: FnMut() -> O,
    O: PartialEq + Debug;

#[track_caller]
pub fn is_permutation<T>(seat: &dyn Seat, got: &[T], want: &[T], msg: &str)
where
    T: PartialEq + Debug;
```

- A callable that panics fails the relation, unless the relation requires
  a failure, as `fails_after_close` and `is_poisoned` do.
- `is_permutation` pairs elements through a maximum bipartite matching
  over `PartialEq`, because a type that is only `PartialEq` has no order
  and no hash.

### The property engine

```rust
pub fn for_all<F>(seat: &dyn Seat, contract: &str, body: F, options: impl IntoIterator<Item = Opt>)
where
    F: Fn(&Case) + Send + Sync;

impl Case {
    pub fn draw<T>(&self, generator: &Generator<T>, label: &str) -> T;
    pub fn assume(&self, condition: bool);
    pub fn classify(&self, label: &str);
    pub fn note(&self, message: &str);
    pub fn rand(&self) -> Source<'_>;
    pub fn observe(&self, fingerprint: u64);
    pub fn cleanup(&self, f: impl FnOnce() + Send + 'static);
    pub fn cancellation(&self) -> &Cancel;
    pub fn history(&self) -> &History;
    pub fn target(&self, label: &str, score: f64);
}

impl<T> Generator<T> {
    pub fn map<U>(self, f: impl Fn(T) -> U + Send + Sync + 'static) -> Generator<U>;
    pub fn filter(self, keep: impl Fn(&T) -> bool + Send + Sync + 'static) -> Generator<T>;
    pub fn bind<U>(self, f: impl Fn(T) -> Generator<U> + Send + Sync + 'static) -> Generator<U>;
}
```

`Source` is the trait that the definition lets Rust state for a random
source. It returns 64-bit choices of the case through `next_u64`.

A generator takes its required parameters as arguments and its optional
ones as a trailing list of `Param` values, with the names of Go's options:

```rust
pub fn integer<T: Integral>(lo: T, hi: T) -> Generator<T>;
pub fn float<T: Floating>(lo: T, hi: T, params: impl IntoIterator<Item = Param>) -> Generator<T>;
pub fn boolean(params: impl IntoIterator<Item = Param>) -> Generator<bool>;
pub fn list<T>(of: Generator<T>, params: impl IntoIterator<Item = Param>) -> Generator<Vec<T>>;
pub fn string(params: impl IntoIterator<Item = Param>) -> Generator<String>;

pub fn min_size(n: usize) -> Param;
pub fn max_size(n: usize) -> Param;
pub fn unique() -> Param;
pub fn allow_nan() -> Param;
pub fn odds(numerator: u64, denominator: u64) -> Param;
pub fn alphabet(characters: &str) -> Param;
pub fn max_leaves(n: usize) -> Param;
```

```rust
let values = prop::list(prop::integer(-1000i64, 1000), [prop::max_size(16)]);
```

- `float` takes its width from its type: `f32` is width 32 and `f64` width
  64.
- A generator panics for a parameter that it does not take, as it panics
  for arguments that state no domain. The message names the generator and
  the parameter.
- Each generator keeps the encoder of its values, so the record of a draw
  states a typed literal. `map` and `bind` produce values of another type,
  and their draws are `Opaque` with the text of `Debug`.

A property form takes the arguments of its assertion, with a closure of
the input in place of each value:

```rust
pub fn equal<T, U>(seat: &dyn Seat, got: impl Fn(&T) -> U, want: impl Fn(&T) -> U, msg: &str, options: impl IntoIterator<Item = FormOpt>)
where
    T: Shaped,
    U: PartialEq + Debug;
```

The input's generator is `prop::of::<T>()`, unless `prop::using` states
another one.

#### Derived inputs

```rust
#[derive(Debug, Clone, prop::Shaped)]
struct Line {
    #[prop(pattern = "[A-Z]{3}-[0-9]{4}")]
    sku: String,
    #[prop(min = 1, max = 99)]
    quantity: u32,
    placed: std::time::SystemTime,
}
```

- `#[derive(Shaped)]` implements `prop::Shaped` for a struct, a tuple
  struct and an enum, in declaration order. It reads the types of the
  definition's table for Rust.
- `#[prop(...)]` states the constraints of the constraint table with
  their keys: `min`, `max`, `min_size`, `max_size`, `pattern`, `alphabet`,
  `allow_nan`, `allow_infinity`, `unit`, `scale` and `version`. An
  unknown key fails the compilation.
- The features `chrono`, `time`, `jiff`, `uuid` and `rust_decimal` of the
  derive crate implement `Shaped` for the types of those crates.
- `prop::of_shape(text)` returns a `Generator<Value>`, because a shape
  file states no Rust type. An instant decodes to the record of its
  seconds and units, as the vectors state it.
- A registration after the first property of the test binary has started
  panics, because the definition applies a registration to the whole test
  process.

#### The rules of a run

- **Workers.** `prop::workers(n)` runs cases on n scoped threads, and a run
  reports what a run on one worker reports.
- **The end of a case.** `assume`, a repeated label and a fatal failure end
  the case with a panic of the engine, which the engine catches. A body
  that catches it with `catch_unwind` continues past the end of its case,
  and every later call of the case panics again.
- **The store.** A run stores its failures under
  `testdata/prop/<target>/<test path>/`, relative to the package root,
  where cargo test runs a test. `<target>` is the name of the test binary
  without its hash, and the test path splits at `::`.
- **Variables.** A run that is not hermetic reads
  `DOKIMI_ASSERT_PROP_SEED`, `DOKIMI_ASSERT_PROP_PROFILE`,
  `DOKIMI_ASSERT_PROP_BUDGET` and `DOKIMI_ASSERT_PROP_REPLAY`.
- **The store's depth.** The reader refuses an entry deeper than 64 levels
  as damaged, as the definition states. `serde_json` refuses only a 128th
  level, so the reader checks the depth itself.

`prop::fuzz` replays the stored cases and the seed files on the seat, and
returns a target for libFuzzer:

```rust
pub fn fuzz<'a, F>(seat: &'a dyn Seat, contract: &'a str, body: F, options: impl IntoIterator<Item = Opt>) -> impl Fn(&[u8]) + 'a
where
    F: Fn(&Case) + Send + Sync + 'a;
```

The library does not depend on `libfuzzer-sys`. A fuzz target of
`cargo fuzz` calls the returned function from `fuzz_target!`.

### Histories

```rust
impl History {
    pub fn new() -> Self;
    pub fn invoke(&self, client: usize, operation: &str, args: Vec<Value>) -> Call<'_>;
    pub fn events(&self) -> Vec<Event>;
}

pub fn concurrently<F>(clients: usize, within: Duration, body: F) -> Vec<Outcome>
where
    F: Fn(usize) + Sync;

pub fn is_linearizable<S: Spec>(seat: &dyn Seat, history: &History, spec: &S, msg: &str, options: impl IntoIterator<Item = HistoryOpt>);
```

- `concurrently` runs one scoped thread per client and releases them at
  once through a `Barrier`.
- The overlay keeps its limit on `history`. Threads run on more than one
  core, and the recorder's counter can supply a barrier that the subject
  lacks.

### Machines and the task scheduler

```rust
pub struct Scheduler { /* ... */ }

impl Scheduler {
    pub fn new(case: &Case, strategy: Strategy) -> Self;
    pub fn spawn(&self, task: impl Future<Output = ()> + 'static);
    pub fn yield_now(&self) -> impl Future<Output = ()> + '_;
    pub fn run(&self);
}
```

- The scheduler is an executor in the core crate, on `std::task` alone.
  The subject spawns its tasks through it, as the definition states for
  Rust.
- A task's turn ends at `yield_now().await` or when the task returns
  `Pending`. The strategy chooses the next ready task. A task is ready
  when its waker has been woken.
- A task that waits on a future that no task of the scheduler wakes stops
  the run, and `run` ends with a fault that names the waiting tasks.

### Trees of files and golden trees

The file helpers and assertions use `std::fs`. A path is a `&str`, as the
definition states for Rust.

- A write reads every parent with `symlink_metadata` before it creates an
  entry, so it does not follow a link out of the directory. The standard
  library has no call that creates a file relative to an open directory.
- A mode is set after the file is written, with `set_permissions`, so the
  umask does not decide it.
- On Windows a mode is not recorded, so `has_mode` ends with a fault, and
  the conformance driver skips the vectors with a mode.
- `golden::matches` and `golden::matches_tree` resolve a name against
  `testdata/golden` in the package root.

### Bench

```rust
impl Contract {
    #[must_use]
    pub fn warmup(self, iterations: usize) -> Self;

    #[must_use]
    pub fn run_parallel<F: Fn() + Sync>(self, workers: usize, iterations: usize, body: F) -> Self;
}
```

`warmup` panics after `run` or `measuring` has run, and `run_parallel`
panics for 0 workers, as the definition states.

### Dependencies

| Crate | Dependency | Why |
|---|---|---|
| `dokimi-assert` | `regex` | `matches` and `string_matching` run the parsed pattern on it, as today |
| `dokimi-assert` | `serde_json`, with `float_roundtrip` | The store and the call records are JSON, and a float of a literal reads back to the same bits |
| `dokimi-assert-derive` | `syn`, `quote`, `proc-macro2` | The derive macro reads a type at compile time |

The library is a development dependency of its callers, so none of these
ends up in a caller's shipped binary.

### Tests and CI

- **Drivers.** A driver for each vector family runs every vector of 8.0.0,
  and states its verdict with `assert!`, never with the library.
- **The completeness gate.** The gate pins the type of every new function
  at compile time, as it pins the existing ones.
- **Coverage.** `cargo llvm-cov` checks 100% of the lines and functions of
  both library crates.
- **CI.** CI runs stable and 1.85 on Linux, and adds a job each on macOS
  and Windows on stable.

### Bounds

- The search of `is_linearizable` stops at the budget of 10,000,000 steps
  per partition, at its memo limit and at its time limit.
- A property stops shrinking at 2,000 runs or 30 seconds.

## Alternatives considered

### A. Typed literals through a trait bound

Every assertion would require `T: Literal`, and the library would
implement the trait for the standard types.

**Why not:** a caller could no longer compare a type of their own without
implementing a trait of this library, and the comparison of any type
that is `PartialEq` is what makes the library useful.

### B. The detail as text, as today

The record would state every value as text.

**Why not:** the vectors pin the typed literals of the values that the
engine generates, and the definition states the record of a call in
typed literals. Text loses the type of a count and of a draw.

### C. A JSON module of the library's own

The core would read and write JSON with code of its own.

**Why not:** the store reads files that another language wrote, and a
reader of its own would repeat a parser that `serde_json` already
provides, tested.

### D. A runtime crate as the task scheduler

The scheduler would run on tokio's current-thread runtime.

**Why not:** tokio chooses the next task itself, so the strategy could not
choose it, and the schedule would not replay from a stored case.

### E. Generator parameters as ranges and booleans

`prop::list(of, 0..=16)` would state the sizes as a range, and a flag as
a `bool` argument.

**Why not:** a bare `true` at a call site does not say which flag it sets,
and the generators take seven optional parameters between them. The
names of Go's options keep the two libraries readable side by side.

## Drawbacks

- **Size.** The TypeScript library implements definition 7.2.1 in 33,888
  source lines and 42,997 test lines, measured on 2026-10-11. The Rust work
  is likely of the same order.
- **A third crate.** A caller who derives inputs adds the derive crate, and
  the library publishes three crates in step.
- **Opaque values.** A value of a caller's type is text in a record, so two
  values that print alike are not told apart.
- **Changes for callers.** `rejects` returns failure records, and
  `Failure::detail` is a list of `Value`. `matches` refuses patterns
  outside the portable subset, and every option list adds a trailing
  argument.
- **Links.** A write checks each parent before it creates an entry, so a
  link that another process creates between the check and the write is
  followed.
- **Mutation testing.** dokimasia has no mutation engine for Rust, so
  coverage is the only measure of the tests.

## References

| What | Where |
|---|---|
| The definition, its corpus, its vectors and the overlay format | <https://github.com/dokimasia/assert-spec> |
| The table of type readers, the constraint table and the second crate for the derive macro | assert-spec RFC-0011: <https://github.com/dokimasia/assert-spec/blob/main/docs/rfc/0011-property-forms-and-derived-inputs.md> |
| The fuzz bridge and the depth of a store entry | assert-spec RFC-0010: <https://github.com/dokimasia/assert-spec/blob/main/docs/rfc/0010-properties-over-generated-inputs.md> |
| The Rust overlay, its records and its limits | <https://github.com/dokimasia/assert-spec/blob/main/overlays/rust.json> |
| The cancellation handle of a Rust seat | assert-spec RFC-0029: <https://github.com/dokimasia/assert-spec/blob/main/docs/rfc/0029-the-cancellation-handle-of-a-seat.md> |
| The options of Go's generators | <https://github.com/dokimasia/assert-go/tree/main/prop> |
| The TypeScript implementation of the same definition, and its size | <https://github.com/dokimasia/assert-typescript/blob/main/docs/rfc/0003-definition-7-2-in-typescript.md>; `src` and `tests` counted with `wc -l` on 2026-10-11 |

## Withdrawal

RFC-0003 replaces this proposal. It implements definition 9.0.0, which
this proposal predates. It states the settings of a run as builders and
the bounds of a generator as ranges, where this proposal states both as
lists of option values.
