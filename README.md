# dokimi-assert

Test assertions for Rust, defined by a language-neutral standard and
held to it on every run.

[![CI](https://github.com/dokimasia/assert-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/dokimasia/assert-rust/actions/workflows/ci.yml)
[![Licence](https://img.shields.io/badge/licence-MIT-blue)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-blue)](https://www.rust-lang.org/)

```toml
[dev-dependencies]
dokimi-assert = "0.1"
dokimi-assert-tokio = "0.1"   # the six that take a future
```

Rust 1.85 and up, edition 2024.

- [What this is for](#what-this-is-for)
- [Getting started](#getting-started)
- [Two surfaces](#two-surfaces)
- [The assertions](#the-assertions)
- [Equality](#equality)
- [Benchmark ceilings](#benchmark-ceilings)
- [The standard](#the-standard)

## What this is for

`assert_eq!` is better than this library for comparing two values, and
[pretty_assertions](https://crates.io/crates/pretty_assertions) is better
still. Reach for this when you want something Rust has no other way to
say:

- **Soft assertions.** `assert_eq!` stops at the first failure. `soft`
  records and carries on, so one run reports every property that failed,
  each with the line it was written on.
- **Assertions about behaviour.** Whether a subject honours cancellation,
  leaves state alone, survives a missing handle, or stays inside an
  allocation ceiling. Nothing else in the ecosystem asserts these.
- **The same meaning in another language.** A Go service and its Rust
  rewrite can run the same assertions and get the same answers.

## Getting started

```rust
use dokimi_assert::{check, seat::Collector};

#[test]
fn get_answers_the_stored_item() {
    let seat = Collector::new();
    let item = store.get("widget");

    check::is_some(&seat, item.as_ref(), "get answers the stored item");
    check::equal(&seat, &item.unwrap().name, "widget", "and it is the one stored");
}
```

Every assertion takes a seat first and a message last. The message states
the contract under test and is the first line of the failure:

```text
and it is the one stored: want "widget", got "gadget"
```

The failure points at your line, not at the library, because every
assertion carries `#[track_caller]`.

### What a seat is

The seat is where a failure goes. Assertions never call a test framework
and never panic on their own; they report to whatever seat they are
handed. That is what lets one assertion serve a real test, a benchmark,
and a test that checks the assertion itself.

| Seat | `check` does | `soft` does |
|---|---|---|
| `Collector` | panics | collects, reported when it is dropped |
| `Standard` | panics | panics |
| `Recorder` | collects | collects |

`Collector` is the one a real test wants. It reports what `soft`
collected when it drops, so nothing has to be called at the end and
nothing can be forgotten. A collector already unwinding from another
panic stays quiet, because panicking twice aborts the process and the
first failure is the one worth reading.

## Two surfaces

`check` stops at the first failure. `soft` records and carries on.

```rust
use dokimi_assert::{check, soft, seat::Collector};

let seat = Collector::new();
check::equal(&seat, &reply.status, &200, "the request succeeds");

soft::has_prefix(&seat, &reply.body, "{", "the body is JSON");
soft::length(&seat, &reply.items, 3, "every item comes back");
```

If both `soft` calls fail, both are reported together with their lines:

```text
2 failures:
  1. the body is JSON: "[1,2]" does not start with "{"
     at tests/api.rs:14
  2. every item comes back: want length 3, got 2
     at tests/api.rs:15
```

## The assertions

Thirty-three on `check` and thirty-two on `soft`, since only `check` can
drive an assertion to failure. Three more compare against a golden file
and four state benchmark ceilings, which is forty. The forty-first is
`no_task_leaks`, and it lives in the tokio crate because Rust's standard
library cannot count what is running.

Every signature below takes `seat: &dyn Seat` first and `msg: &str` last;
both are elided here to keep the shapes readable.

**Equality.** The language's own `==`, which is already what the standard
asks for.

```rust
check::equal<T: PartialEq + Debug + ?Sized>(got: &T, want: &T)
check::not_equal<T: PartialEq + Debug + ?Sized>(got: &T, want: &T)
```

**Truth and absence.** Rust states absence in the type, so there is no
typed nil to catch.

```rust
check::is_true(condition: bool)
check::is_false(condition: bool)
check::is_none<T: Debug>(got: Option<&T>)
check::is_some<T: Debug>(got: Option<&T>)
```

**Size.** Anything implementing `Container`: `str`, `String`, slices,
`Vec`, `VecDeque`, `HashMap`, `BTreeMap`, `HashSet`, `BTreeSet`. A value
with no length does not compile, so it cannot fail at run time.

```rust
check::length<C: Container + ?Sized>(got: &C, want: usize)
check::is_empty<C: Container + ?Sized>(got: &C)
check::is_not_empty<C: Container + ?Sized>(got: &C)
```

**Containment.** What holding means follows the haystack, decided by the
types rather than at run time: text holds a substring, a sequence holds an
element, a map holds a key.

```rust
check::contains<H: Holds<N> + Debug + ?Sized, N: Debug + ?Sized>(haystack: &H, needle: &N)
check::not_contains<H: Holds<N> + Debug + ?Sized, N: Debug + ?Sized>(haystack: &H, needle: &N)
check::contains_in_order(got: &str, needles: &[&str])
```

**Text.**

```rust
check::has_prefix(got: &str, prefix: &str)
check::has_suffix(got: &str, suffix: &str)
check::matches(got: &str, pattern: &str)
```

**Numbers.** Where exact equality is the wrong question.

```rust
check::close_to(got: f64, want: f64, tolerance: f64)
check::in_range(got: f64, low: f64, high: f64)
```

**Errors.** Rust states failure in the type, so these read a `Result`
rather than catching anything. Matching walks the chain of
`Error::source`.

```rust
check::no_error<T, E: Debug>(got: &Result<T, E>)
check::has_error<T: Debug, E>(got: &Result<T, E>)
check::error_is<T: PartialEq + Error + Debug + 'static>(error: &dyn Error, target: &T)
check::error_is_not<T: PartialEq + Error + Debug + 'static>(error: &dyn Error, target: &T)
check::error_as<'a, T: Error + 'static>(error: &'a dyn Error) -> Option<&'a T>
```

**Panicking.** A panic means a broken invariant. A failure a caller is
meant to handle is a `Result`, and the errors family covers that.

```rust
check::panics<F: FnOnce()>(body: F) -> Option<String>
check::does_not_panic<F: FnOnce()>(body: F)
```

**Ordering.** One assertion rather than sorted, unique and strictly
increasing, because each of those is a relation between neighbours.

```rust
check::pairwise<T: Debug, P: Fn(&T, &T) -> bool>(items: &[T], predicate: P)
```

**Behaviour.** `Cancel` is the handle a subject reads to learn it should
stop. Rust has nothing like `context.Context`, and dropping a future is
not the equivalent: a subject that stops because it was dropped never
chose to stop.

```rust
check::honours_cancellation<E: Error + 'static, F>(body: F)
    where F: FnOnce(Option<&Cancel>) -> Result<(), E>
check::honours_deadline<E: Error + 'static, F>(body: F)
    where F: FnOnce(Option<&Cancel>) -> Result<(), E>
check::completes_within<E: Debug, F>(within: Duration, body: F)
    where F: FnOnce(Option<&Cancel>) -> Result<(), E>
check::none_handle_safe<E: Debug, F>(body: F)
    where F: FnOnce(Option<&Cancel>) -> Result<(), E> + UnwindSafe
check::is_pure<S: PartialEq + Debug, O: Fn() -> S, F: FnOnce()>(observe: O, body: F)
```

**Retrying.** For a condition something outside the test makes true. Both
spend real time.

```rust
check::eventually<F: Fn(&Recorder)>(timeout: Duration, interval: Duration, body: F)
check::eventually_true<P: Fn() -> bool>(timeout: Duration, predicate: P)
```

**Testing an assertion.** On `check` only: `soft` cannot drive a check to
failure, because it does not stop.

```rust
check::rejects<F: FnOnce(&Recorder)>(body: F) -> String
```

**Golden files.** Recorded output, compared and rewritable with
`UPDATE_GOLDEN=1`.

```rust
golden::matches(name: &str, got: &str, scrubbers: &[Scrubber])
golden::matches_at(path: &Path, got: &str, scrubbers: &[Scrubber])
golden::matches_json_field(path: &Path, field: &str, got: &str, scrubbers: &[Scrubber])
golden::should_update() -> bool
golden::scrub_timestamps() -> Scrubber
golden::scrub_hashes() -> Scrubber
golden::scrub_run_ids() -> Scrubber
golden::scrub_json_fields(fields: &[&str]) -> Scrubber
```

**Coroutines**, from `dokimi-assert-tokio`, for the six a synchronous
signature cannot take. The subject is handed a `CancellationToken`.

```rust
check::honours_cancellation(body).await
check::honours_deadline(body).await
check::completes_within(within: Duration, body: impl Future).await
check::eventually(timeout, interval, body).await
check::eventually_true(timeout, predicate).await
check::no_task_leaks(body).await
```

## Equality

The standard asks that NaN be unequal to itself, that `0.0` equal `-0.0`,
and that containers compare by their elements. Rust's derived `PartialEq`
already answers all three that way, so this library adds no comparison of
its own. Values of different types never compare because they do not
compile.

That is the one place Rust made the work smaller rather than larger. The
Java implementation needed 223 lines to correct `Object.equals` on those
same three points.

## Benchmark ceilings

A benchmark that prints numbers tells you what happened. A ceiling tells
you whether it was acceptable.

```rust
use dokimi_assert::bench::{Contract, CountingAllocator};

#[global_allocator]
static ALLOC: CountingAllocator = CountingAllocator::new();

Contract::new(&seat, "get stays quick")
    .max_latency(Duration::from_millis(2))
    .max_allocs(4)
    .run(10_000, || { store.get(&id); })
    .check();
```

`max_allocs` and `max_bytes` need `CountingAllocator` installed as the
test binary's global allocator, and say so rather than passing quietly
when it is missing. Rust is the only implementation of this standard that
counts allocations exactly: the JVM reports bytes and no count, and V8
answers neither.

## The standard

The assertions are defined in
[assert-spec](https://github.com/dokimasia/assert-spec), language-neutral
and implemented in several languages. This library vendors the definition
and holds itself to it:

- 87 corpus cases state what each assertion must report, run against both
  surfaces. They are the same cases every other implementation runs.
- A completeness gate names every assertion as a value of its own type.
  Rust can look nothing up at run time, so a rename or a changed shape
  fails the build rather than a test.
- An overlay records what this language supplies only partly.

Rust is the first implementation with nothing absent: 41 of 41. Three are
recorded as partial. `max_allocs` and `max_bytes` need the allocator
installed, and `no_task_leaks` sees Tokio tasks but not a thread started
with `std::thread`, because Rust's standard library cannot enumerate
threads at all.

[docs/rfc/0001](docs/rfc/0001-the-rust-implementation.md) records what
Rust does differently from the other implementations, and why.

## Development

```sh
make check    # fmt, clippy, build, test, doc
make test
make msrv     # build on the declared 1.85 floor
```

## Licence

MIT. See [LICENSE](LICENSE).
