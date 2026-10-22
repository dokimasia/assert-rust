# Testing

A test specifies behaviour that a caller can observe. Every rule in this
document applies to every crate of the workspace. A change brings each
test file that it touches up to these rules.

## Black-box tests

- A spec calls only the public API of its crate, as a dependent crate
  calls it.
- `src/` contains no test: no `#[cfg(test)]` module and no `#[test]`
  function.
- A crate does not make an item public for its specs alone.
- A spec tests an internal module through the public items that use it.
  The spec of `pattern.rs` runs the parser through `check::matches`, whose
  failure states the parser's refusal.

## One spec per source file

- The spec of `src/a/b.rs` is `tests/a/b.rs`. The spec tests the
  behaviour that `b.rs` implements.
- The tests of one source file's behaviour are all in its spec.
- Every source file has a spec, except a file that only declares modules,
  such as `src/matcher.rs`. `tests/matcher.rs` declares the specs of those
  modules in the same way.
- The specs of a crate build as one test binary. `Cargo.toml` sets
  `autotests = false` and declares the binary `spec`, whose root is
  `tests/main.rs`. `tests/main.rs` is the spec of `lib.rs`.
- A second binary exists only for a process state that the first binary
  cannot have. `tests/uncounted.rs` runs the allocation ceilings without
  the counting allocator, because a process has one global allocator.
- A test file without a source file of its name does not exist, except
  `tests/helpers.rs` and a second binary.

## Helpers

- `tests/helpers.rs` contains the helpers, types and fixtures that more
  than one spec uses.
- A helper that one spec uses is declared in that spec.

## Names

- A spec has a `mod` for each export under test, named after the export:
  `mod equal` for `check::equal`, and `mod contract` for `bench::Contract`.
- Each `#[test]` function is one case. Its name states one behaviour in
  snake case: a verb in the third person, the result, and the condition,
  as `states_null_for_a_value_without_a_length`. Together with the name of
  its module, it reads as a sentence.
- A case's name is one clause. It never starts with `should`, and it never
  contains `test`, `correctly`, `properly` or `as_expected`.
- A table of cases is named `tests`, and each case is `tt`. Besides the
  case's `name`, its fields start with `give` and `want`.

## Determinism and isolation

- libtest runs the tests of one binary on parallel threads. A test does
  not change the environment variables or the working directory of the
  process.
- A test that needs `DOKIMI_ASSERT_RECORD` or another variable of the
  process runs an ignored test of its own binary in a child process. The
  child process runs with the variable set, and the test reads its output.
- Every child process of a binary writes the one artifact of that binary.
  A test that runs such a child process takes the lock that every test of
  the artifact takes.
- A test that needs a permission to cause an error, such as a store
  directory that refuses a write, runs on Unix alone. The specs run as a
  user other than root, because no permission refuses root a write.
- A run is the same on every machine. Seeds are fixed, no assertion reads
  the wall clock, and no result depends on the local time zone.
- A case whose subject waits, retries or measures gives the seat a
  `Controlled` clock. Such a case of the tokio crate runs on a runtime
  whose clock starts paused. The case then does not spend real time.
- A test waits on a channel or a join, never on `thread::sleep`.
- A test writes its files in a directory of its own under the system's
  temporary directory. The test removes the directory when it ends.
- No test calls the network.

## Coverage

- Every line and every function of each published crate runs in a spec.
  CI and `make coverage` measure the coverage with `cargo llvm-cov`. They
  fail when a line or a function did not run in any spec.
- A line counts as covered when any instantiation of its function runs
  it. llvm-cov's line percentage counts a generic function's lines in its
  best instantiation alone, so the gate counts uncovered lines instead of
  a percentage.
- Code that no spec can execute through the public API is dead code or a
  missing spec. Delete the code, or write the spec.
- No line of a published crate is excluded from coverage.
- The checks in the conformance crate find defects of the library. A
  correct library never triggers them, so the crate is outside the
  coverage gate. `make coverage` runs the tests of the crate and leaves its
  lines out of the report. A spec calls each check with an observation
  that disagrees with its case, and asserts the variant that the check
  returns.

## Mutation

- dokimasia has no mutation engine for Rust, so coverage is the only
  measure of these specs.
- A new check proves by the construction of its test that it can fail.
  The test asserts the field or the variant that separates the failure
  from its neighbours. A gate, a validator and a comparison of the
  conformance crate are such checks.

## Allocation ceilings

- The spec binary installs `CountingAllocator`, so every ceiling that a
  spec states counts.
- The library states no allocation contract for its own exports, so no
  spec states a ceiling on one.

## Verdicts

- The specs of the matchers in `tests/matcher/`, of the surfaces
  `tests/check.rs` and `tests/soft.rs`, of the seats in `tests/seat.rs`,
  and of the conformance crate use `assert!` and `assert_eq!` alone. These
  modules compare the values and deliver the verdict. A defect in them
  could pass a test that is written with the library.
- The conformance crate decodes the definition's typed literals with a
  decoder of its own. It compares JSON through `serde_json`.
- A test compares the values that the library reports: a failure's record
  in `Recorder::failures`, a call record in `Recorder::records`, and the
  text of a fault. It compares a failure's sentence only where the
  sentence is the subject: in the specs of `Failure`, of the seats, and of
  a seat that takes no records.

## Test data

- The data of a crate's specs is under `testdata/` of the crate, which the
  package leaves out. `golden::matches` resolves a name under
  `testdata/golden`.
- Each entry of `testdata/pattern/verdicts.json` is the verdict of the
  definition's reference parser on the entry's pattern.
- The vendored definition is in `crates/dokimi-assert-conformance`, as
  `.spec.yaml` declares. `make spec-sync` refreshes it, and nobody edits
  it by hand.

## Enforcement

A tool checks a rule wherever one can. Review checks every other rule.

| Rule | Checked by |
|---|---|
| Formatting | rustfmt, through `make fmt-check` |
| Lint | clippy at the pedantic level, through `make lint` |
| 100% coverage of each published crate | `cargo llvm-cov`, through `make coverage` |
| The public names match the definition's naming table | The spec of `lib.rs` in `tests/main.rs` |
| Every other rule of this document | Review |
