---
rfc: 0001
title: The Rust assertion library
author: Roy Klopper <roy.klopper@stealthscale.io>
status: Accepted
created: 2026-08-30
updated: 2026-08-30
discussion: none
supersedes: none
superseded-by: none
produces-adr: none
---

# RFC-0001: The Rust assertion library

## Summary

`dokimi-assert` implements the standardized assertion set in Rust, and
`dokimi-assert-tokio` adds the ones whose subject is a future. It is the
first implementation with nothing absent: forty-one of forty-one, with
three recorded as partial rather than missing.

This records what Rust does differently from the other implementations,
and why each difference is forced rather than chosen.

## Motivation

The standard exists so the same test means the same thing in every
language. That only holds if each implementation is held to it, and if
the places it cannot comply are written down rather than quietly skipped.

Rust is the language where the standard asks for least and gets most.
Three of the rules the other implementations had to write code for are
already what the language does. Two of the mechanisms they rely on do not
exist here at all.

There is also a question the other four never had to answer. Rust has
`assert_eq!`, and it is better than anything this library offers for
comparing two values. Saying what this library is for, and what it is
not, is part of shipping it.

## Detailed design

### Two crates

```
dokimi-assert         check, soft, seats, golden, bench, the Cancel handle
dokimi-assert-tokio   the six assertions whose subject is a future
```

Thirty-five of the forty-one take a value, a closure or a duration, and a
Rust caller uses the core crate for all of them. The other six take work
that suspends, and a synchronous signature cannot accept a future without
either blocking on it or pulling a runtime into the core.

So the core stays synchronous and knows nothing about async. A project
that never writes `async` adds one crate and no runtime.

### Equality needed no code

The standard asks that NaN be unequal to itself, that `0.0` equal `-0.0`,
and that containers compare by their elements. Rust's derived `PartialEq`
already answers all three that way, and values of different types do not
compile rather than comparing unequal.

So `equal` is `!=` on a `T: PartialEq + Debug`. There is no comparison
routine in this library, no cycle detection, and no type table. The Java
implementation needed 223 lines to correct `Object.equals` on exactly
these three points, and Python needed a rule about `bool` subclassing
`int`.

The relaxations the other implementations offer are absent, and nothing
is lost. They exist to paper over dynamic typing: an absent collection
and an empty one are different types here, and the language already says
NaN is unequal to itself, which is what the standard wants by default.
Neither relaxation is part of the standard, and no corpus case uses one.

### Cancellation is a handle, not a dropped future

Go states cancellation with a `context.Context` in every signature. Rust
has no such value in its standard library, and the obvious substitute is
wrong.

Dropping a future cancels it. That is a property of the runtime rather
than a choice the subject made, so a subject that stops because it was
dropped never decided to stop. An assertion built on it would answer yes
for every subject, which is exactly the shape this assertion has shipped
broken in three other languages: arrange cancellation so early the
subject never runs, then read "it did not finish" as "it honoured the
signal".

So the handle is a value the subject reads:

```rust
pub struct Cancel { /* one atomic */ }

impl Cancel {
    pub fn cancelled() -> Self;
    pub fn expired() -> Self;
    pub fn stop(&self);
    pub fn expire(&self);
    pub fn stopped(&self) -> Option<Stop>;
}

pub enum Stop { Cancelled, DeadlineExceeded }
```

`honours_cancellation` hands the subject an already-cancelled handle and
requires `Stop::Cancelled` somewhere in the error it answers with.
Answering `Ok` fails, because the subject did the work. Answering a
different error fails too: failing for its own reasons and happening to
do so in time is not the same as reading the handle.

The tokio crate asks the same question of a `CancellationToken`, and for
the same reason: a future that merely stopped proves nothing.

### The caller's line, not the library's

Every other implementation calls `helper()` on the seat, which is what
Go's `testing.TB.Helper` does. Rust has a language feature for this, and
it works through dynamic dispatch: a failure reported through
`&dyn Seat` points at the caller's line.

`#[track_caller]` only propagates if every frame in the chain carries it,
so it is on the trait method, both seat implementations, the report
function, and every assertion. The seat's `helper()` is kept because the
standard states it, and because a framework that can hide frames some
other way has somewhere to do it.

### Soft failures report from a drop

Rust has no test-teardown hook, so nothing can flush a recording seat at
the end of a test. `Collector` reports from its own `Drop`, which is what
running out of scope means.

Two things make that safe. A collector already unwinding from another
panic stays quiet, because panicking twice aborts the process and the
first failure is the one worth reading. And each recorded failure
captures `Location::caller()` where the assertion was written, because a
drop has no caller of its own and a report that pointed at the library
would be useless.

### The completeness gate is a compile-time one

Python reaches for `getattr`, Java for reflection, TypeScript for the
keys of a module. Rust can ask nothing at run time.

So the gate names every assertion as a value of its own type:

```rust
let _: fn(&dyn Seat, &str, &str, &str) = check::has_prefix;
```

Renaming an assertion or changing its shape fails the build rather than a
test, which is stronger than what the others get. A second test reads the
vendored naming table and fails when it names something this file does
not pin, which is what stops the file falling behind the standard.

### Allocations are counted exactly

The standard states a ceiling on allocations per iteration and one on
bytes. Java reports bytes and no count. V8 reports neither in a form that
holds still.

Rust can count both, because a program chooses its allocator:

```rust
#[global_allocator]
static ALLOC: CountingAllocator = CountingAllocator::new();
```

Measured against a body allocating one `Vec` per iteration over a
thousand iterations, a ceiling of nought reports "1 allocations per
iteration" and a ceiling of one passes. That is an exact count, not an
estimate from a heap delta.

The price is that a library cannot install an allocator on a caller's
behalf. Without it the assertion reports that nothing counted rather than
passing quietly, and the overlay records that as a limit.

### One dependency, and where the line is

The core crate depends on `regex`, because Rust's standard library has no
regular expressions and the standard states an assertion that matches a
pattern. Nothing else is a runtime dependency.

That is a smaller compromise than it looks. This library is itself a
development dependency, so its own dependencies never reach a consumer's
shipped binary.

Reading the standard needs a JSON parser, which Rust also lacks. The
conformance run is a test and its parser is a development dependency, so
what is published carries neither.

### What the corpus reaches

Eighty-seven cases across twenty-five assertions, run against both
surfaces: a hundred and seventy-four case-runs, with no declared skips.
Seventeen of those cases name a behaviour rather than stating a value,
which is how a case reaches an assertion that takes a callable.

The other sixteen want a real duration, a real file or a real runtime,
and no corpus file can hold one, so they are covered by tests here and by
the completeness gate.

Those tests drive each assertion twice, with a subject that satisfies it
and one that does not. Returning early from the one function every
assertion reports through makes fifty-five of the ninety-four tests fail,
which is how the failing halves were shown to bite rather than merely
pass.

### Three limits, no divergences

`max_allocs` and `max_bytes` need the counting allocator installed.
`no_task_leaks` sees tasks spawned on the Tokio runtime the test is
running under, and does not see a thread started with `std::thread`.

That last one is why the assertion ships in the tokio crate. Rust's
standard library offers no way to enumerate threads: nothing in
`std::thread` lists them, so the core crate cannot answer the question at
all. This is the same shape as the Java implementation's virtual threads,
arrived at from the opposite direction.

## Alternatives considered

### A. One crate, with async behind a feature

A feature flag would spare a second crate and a second version to keep in
step.

Rejected because an assertion that vanishes under a feature is a
conformance hazard. The completeness gate would pass or fail depending on
which features were enabled, and a consumer could satisfy the standard on
paper while missing six assertions.

### B. `tokio-util`'s `CancellationToken` in the core

It is the closest thing Rust has to a shared cancellation type, and using
it would make the core and the tokio crate agree by construction.

Rejected because every consumer would inherit `tokio-util` to call
`equal`. The core's own handle is one atomic and sixty-five lines, and a
tokio user bridges it in one line.

### C. A macro API, matching `assert_eq!`

Rust assertion libraries are macros, and a macro could capture the seat
from scope and spare the caller passing it.

Rejected because the standard names free functions, and because a macro
hides the signature. The naming table maps an assertion to a name a user
types; a macro that took its seat implicitly would be a different call
shape from every other implementation.

### D. Offer the relaxations anyway

The other four implementations take options on `equal` and `contains`.
Offering them would match.

Rejected because they would do nothing. A relaxation reaching inside a
generic `T: PartialEq` is not expressible without a comparison routine
this library does not have, and the two the others offer answer questions
Rust's type system has already settled.

## Drawbacks

The seat is threaded through every call, which is more typing than
`assert_eq!` and unusual for Rust. For plain equality this library is
worse than `pretty_assertions`, which needs no seat and prints a coloured
diff. The README says so.

`equal` takes `&T` on both sides, so comparing integers reads `&1`.
Taking by value would move a non-`Copy` type, so the noise stays.

`missing_docs` is denied at the workspace, which applies to integration
tests as well as to the library. Every test file needs a module comment.

Two crates mean a user writing async adds two dependencies and has to
know which six assertions come from the second.

Three assertions of the forty-one are partial. A team relying on
allocation ceilings gets nothing until they install the allocator, and a
team leaking a `std::thread` is not told.

## Unresolved and future work

The golden-file assertions overlap
[insta](https://crates.io/crates/insta), which is more capable in every
way that matters: inline snapshots, a review command, redactions. What
this library offers is the same three assertions the other four have.
Whether that is worth having beside insta is not settled here.

An adapter for a test framework other than the built-in harness is not
proposed. What the harness does not give is a hook after the test body,
which is why `Collector` reports from a drop; a framework offering one
could report more directly.

### The failure record holds the caller's values as text

A failure carries named values, and here they are typed where the
library computed them and text where they came from the caller.

An assertion is generic over what it compares: `equal` takes any `T`
that is `PartialEq + Debug`. Requiring `T: Into<Detail>` would stop a
caller comparing their own types, which is the thing that makes the
library useful. So `want` and `got` on the comparing assertions hold
what the assertion would have printed.

Everything the library measures itself is a number. `length`, `index`,
`attempts` and the ends of a range are counts and reals rather than
renderings, and the corpus compares them as numbers.

That leaves a real limit on the fields holding a caller's value: two
values that print alike are not told apart. It is smaller than it was,
and it is the price of an assertion that compares anything.


## References

- The standard, its corpus and the overlay format:
  <https://github.com/dokimasia/assert-spec>
- `GlobalAlloc`, the trait the counting allocator implements:
  <https://doc.rust-lang.org/std/alloc/trait.GlobalAlloc.html>
- `#[track_caller]` and `Location::caller`:
  <https://doc.rust-lang.org/std/panic/struct.Location.html>
- `CancellationToken`, which the tokio crate takes as its handle:
  <https://docs.rs/tokio-util/latest/tokio_util/sync/struct.CancellationToken.html>
