# Copyright Dokimasia B.V. 2026
# SPDX-License-Identifier: MIT

# The targets of assert-rust, which ergon init appends to the Makefile. coverage fails when a line or
# a function of a published crate did not run in a test, and package builds each published crate
# from the files of its package. deny checks the dependencies against deny.toml with cargo-deny,
# which the job deny of ci.yml installs.

##@ Repository

.PHONY: coverage deny package

# llvm-cov's line percentage counts a generic function's lines in its best instantiation alone, so
# the gate counts the lines that did not run in any test. The tests of the conformance crate run,
# because its vectors call the library, and the report leaves out the crate's own lines. The report
# also leaves out the source of the standard library, which a toolchain built from source states by
# its real path.
coverage: ## Fail when a line or a function of a published crate did not run in a test
	$(CARGO_LLVM_COV) llvm-cov --workspace --exclude-from-report dokimi-assert-conformance \
		--all-features --ignore-filename-regex '/rustlib/src/rust/library/' \
		--fail-uncovered-lines 0 --fail-uncovered-functions 0 --show-missing-lines

deny: ## Check the licences, the advisories, the bans and the sources of the dependencies
	cargo deny check

package: ## Build each published crate from the files of its package
	cargo package --workspace

# A test that a system skips leaves code that no test runs there, and the packages are the same on
# every system. check-rust checks both on Linux alone.
ifeq ($(shell uname -s),Linux)
check-rust: coverage package
endif
