# Contributing to assert-rust

## The gate

`make check` runs every stage that CI runs: rustfmt, clippy, the tests,
the documentation, the check on Rust 1.85, the coverage, cargo-deny, the
packages and the check of the vendored definition. A change that passes
`make check` on Linux passes the same stages in CI, which also runs the
tests on macOS and Windows.

`make coverage` needs [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov),
and `make deny` needs [cargo-deny](https://github.com/EmbarkStudios/cargo-deny).

## The definition

The crates implement the definition of
[assert-spec](https://github.com/dokimasia/assert-spec).
`crates/dokimi-assert-conformance` contains a copy of the definition, which
`make spec-sync` writes from a commit of assert-spec. Nobody edits the copy
by hand.

## Changes

- Open a pull request against `main`.
- Write each commit message as a Conventional Commit: `type(scope): subject`.
- Follow `docs/standards/testing.md` for every test.
