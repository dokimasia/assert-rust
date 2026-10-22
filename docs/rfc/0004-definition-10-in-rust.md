---
rfc: 0004
title: Definition 10.0.0 in Rust
author: Roy Klopper <roy.klopper@stealthscale.io>
status: Accepted
created: 2026-10-22
updated: 2026-10-22
discussion: https://github.com/dokimasia/assert-spec/issues/18
supersedes: none
superseded-by: none
produces-adr: none
---

# RFC-0004: Definition 10.0.0 in Rust

## Summary

The library moves from definition 9.0.0 to 10.0.0.

| Change | Rust API |
|---|---|
| A count of an allocation ceiling is taken up to ten times | `max_allocs`, `max_allocs_with_setup` and their property forms |
| A helper ends its call with a fault through the seat | `Seat::report_fault`, `dokimi_assert::fault` and `Recorder::faults` |
| A check that a fault ends ends the call of `rejects` with that fault | `check::rejects` |
| A test reads the values of a generator for a seed | `prop::values` |
| A test reads whether its run checks allocation ceilings | `dokimi_assert::allocs_checked` |
| A run of `for_all` and `fuzz` takes `using` | `Options::using` takes a generator of any type |

Every call record states 10.0.0.

## Motivation

The completeness gate fails on every row of the naming table that the
library neither names nor declines. Definition 10.0.0 adds the rows
`seat.report-fault`, `recorder-seat.faults`, `fault`, `allocs-checked` and
`prop.values`, so the library cannot vendor 10.0.0 without them.

`Options::using` states the generator of a property form's input alone.
`for_all` takes `Options<()>`, so a body that draws `prop::of::<Order>()`
cannot narrow the `Status` inside each order for one run. `register_values`
narrows it for the whole test process.

## Detailed design

### Ten counts

`COUNTS` of `matcher/allocations.rs` becomes 10. The warm-up, the 100
counted calls, the slack of a ceiling of 0, the rounding and the detail
fields do not change. The documentation of each ceiling states "ten times
at most".

### Faults through the seat

```rust
pub trait Seat: Send + Sync {
    /// Receives a fault: an error that ends the call without a verdict when
    /// `ending` is true, and that the call notes and runs on past when it is
    /// false. The default sends the text of an ending fault through `fail`,
    /// and prints the text of any other fault to the standard error.
    #[track_caller]
    fn report_fault(&self, fault: Arc<dyn Error + Send + Sync>, ending: bool) {
        if ending {
            self.fail(&fault.to_string());
        } else {
            eprintln!("{fault}");
        }
    }
}

/// Ends the call of a helper with `error`, a fault, through
/// `seat.report_fault(error, true)`. It does not write a call record.
#[track_caller]
pub fn fault(seat: &dyn Seat, error: impl Error + Send + Sync + 'static);

impl Recorder {
    /// Returns the faults that the recorder received, in order.
    #[must_use]
    pub fn faults(&self) -> Vec<Arc<dyn Error + Send + Sync>>;
}
```

- `Recorder` overrides `report_fault`. It keeps each fault. An ending fault
  fails the recorder with the text of the fault, as `fail` does. A noted
  fault does not fail it.
- The library passes each fault of its own to `report_fault` with `ending`
  true. The fault is an error of a private type whose text is the text that
  the seat received through `fail` in 9.0.0, so `Collector` and `Standard`
  panic with the same message. The faults of `Run::fault`, of the switch of
  recording, of a benchmark contract that measured nothing, and of
  `files::workspace`, `files::write` and `files::read` take this path.
- A property passes each fault that it notes, such as a file of its store
  that it skips, with `ending` false. The default prints it to the standard
  error, as the library did in 9.0.0.
- The seat of an attempt of `eventually`, of the check of `rejects` and of
  a property's case overrides `report_fault`. An ending fault ends the body
  with the text of the fault, as `fail` ends it. A noted fault passes to the
  seat of the assertion.
- `fault` and `allocs_checked` are members of the crate root, beside the
  seats, as the naming table gives them no module.

### The fault of a check

The seat of the check that `rejects` drives keeps the first fault that ends
the check. `rejects` then ends its own call with that fault: the call record
states the verdict `error` and the text of the fault, the seat of `rejects`
receives the same `Arc` through `report_fault`, and `rejects` returns an
empty list. `Run` gains the crate-private `fault_with`, which takes the
fault as an `Arc` and keeps its identity.

### The values of a seed

```rust
/// Returns `n` values of `generator` for `seed`: the values of the random
/// cases of `seed`, in order, without the cases that the generator rejects.
///
/// # Panics
///
/// Panics when the generator rejects more than ten cases for each value.
#[must_use]
pub fn values<T: 'static>(generator: &Generator<T>, seed: u64, n: usize) -> Vec<T>;
```

- Case k decodes from the random source of `seed + k`, as a random case of
  a run does.
- The call does not run a property. It does not try the simplest case or a
  boundary case, record an assertion, write a stored case or read a
  variable of the engine. A draw of `of` uses the registration or the shape
  of its type, also inside a run that states `using`.
- A panic of the generator passes through.

### Whether a run checks allocation ceilings

```rust
/// Returns `Ok(())` when `max_allocs`, `max_allocs_with_setup`, their
/// property forms and the allocation ceilings of a benchmark contract check
/// their ceilings in this test binary.
///
/// # Errors
///
/// Returns the reason when the test binary does not install
/// `bench::CountingAllocator` as its global allocator.
pub fn allocs_checked() -> Result<(), &'static str>;
```

The reason is the text of the fault of a ceiling in such a binary.

### Using in every run

```rust
impl<T> Options<T> {
    /// Makes `generator` the generator of `U` in the run that takes the
    /// settings.
    pub fn using<U: Arbitrary + Debug>(self, generator: &Generator<U>) -> Self;
}

pub fn using<T, U: Arbitrary + Debug>(generator: &Generator<U>) -> Options<T>;
```

`for_all`, `fuzz` and every property form take the option. A later `using`
of one type replaces an earlier one. A call of a form that states
`using(&g)` of its input type compiles as before, with `U` the input type.
The run's generators apply to the draws of `of`:

| Draw | Generator |
|---|---|
| `of::<U>()` of a type that the run states | The stated generator |
| `of::<V>()` of a type that contains a stated type | `V` read with the stated generators in place of their types |
| `of::<U>()` inside the stated generator of `U` | `U` without its own stated generator, with the run's other generators |
| The input of a property form | As a draw of `of` of the input type |
| Any other generator, such as `prop::integer(0..=9)` | The generator itself |

`of::<T>()` returns a generator whose node picks its generator where it is
used. A thread-local variable contains the generators of the run. The
property sets it on the thread of the call and around each run of the body,
so a case on a thread of a worker reads them. The client threads of a
concurrent section of `stateful::steps` take the generators of the thread
that starts them. A stated generator decodes inside a node that sets the
run's generators without its type. Each pick also applies to the literal,
the inverse and the integer bounds of the picked generator, so a report, an
entry of `draws`, an explained draw and an example of a form use the
generators of the run.

`Definitions` gains a private field with the run's generators. A read
under a run states a ref to each stated type, as it states a ref to a
registered generator. `of::<T>()` reads `T` without the run's generators,
and panics for a shape that does not read, as before. A draw whose type
does not read under the run's generators panics inside its case with the
message of `of`.

### Version

`DEFINITION` of `record.rs` becomes `10.0.0`, and every call record states
it. `make spec-sync` vendors the copy of 10.0.0.

## Alternatives considered

### A. A trait object of the run in each case

The case would carry the run's generators, and a draw would read them from
its case.

**Why not:** the inverse and the literal of a generator take no case, and
the explain phase decodes a filling on a case of its own. Each of them
would need the generators as a parameter, and a stated generator that draws
its own type would need a second parameter for the generators without that
type.

### B. Keep `using` on the forms alone

The overlay of Rust would declare that a run of `for_all` and `fuzz` does
not take `using`.

**Why not:** a run can give a type a generator, because Rust has a `TypeId`
for every `'static` type. Without the setting, a body of `for_all` narrows a
type through the registry alone. The registry applies to the whole test
process.

### C. A public type of the library's faults

The library would export the error type of its faults. A test could then
downcast a fault of the library.

**Why not:** the testing standard has a test compare the text of a fault. A
public type would also fix the fields of a fault in the API.

## Drawbacks

- `Seat` gains a member with a default. A seat of a caller's own that
  overrides it decides what a fault does.
- `using` requires `U: Arbitrary + Debug`, because the run reads a type that
  contains `U` through the shapes. A form's input already requires both.
- Each pick reads a thread-local variable, and a run with `using` reads each
  type that a draw of `of` names once more.
- A thread that a body starts with `std::thread::spawn` does not take the
  run's generators. A case borrows its body's environment, so such a thread
  cannot draw from the case.
- No vector covers the faults, `using` or `allocs_checked`, so the specs of
  the library alone test them.

## References

| What | Where |
|---|---|
| Ten counts of an allocation ceiling | <https://github.com/dokimasia/assert-spec/blob/main/docs/rfc/0032-ten-counts-of-an-allocation-ceiling.md> |
| Faults through the seat | <https://github.com/dokimasia/assert-spec/blob/main/docs/rfc/0033-faults-through-the-seat.md> |
| A check that ends with a fault | <https://github.com/dokimasia/assert-spec/blob/main/docs/rfc/0034-a-check-that-ends-with-a-fault.md> |
| The values of a generator for a seed | <https://github.com/dokimasia/assert-spec/blob/main/docs/rfc/0035-the-values-of-a-generator-for-a-seed.md> |
| Whether a run checks allocation ceilings | <https://github.com/dokimasia/assert-spec/blob/main/docs/rfc/0036-whether-a-run-checks-allocation-ceilings.md> |
| The option of a single property | <https://github.com/dokimasia/assert-spec/blob/main/docs/rfc/0011-property-forms-and-derived-inputs.md> |
