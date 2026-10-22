# Every target runs from the workspace root and covers every crate of the
# workspace. `make check` runs the stages that CI runs.
.DEFAULT_GOAL := help

# The oldest Rust that the crates support, as rust-version states it.
MSRV := 1.85

# The command that runs spec, which vendors the definition of assert-spec
# into the crate dokimi-assert-conformance, as .spec.yaml declares.
SPEC ?= spec

.PHONY: help
help: ## List the targets
	@grep -hE '^[a-z-]+:.*?## ' $(MAKEFILE_LIST) \
		| awk 'BEGIN{FS=":.*?## "}{printf "  %-16s %s\n", $$1, $$2}'

.PHONY: fmt
fmt: ## Format every crate
	@cargo fmt --all

.PHONY: fmt-check
fmt-check: ## Fail when a file is not formatted
	@cargo fmt --all --check

.PHONY: lint
lint: ## Run clippy over every target and feature, warnings included
	@cargo clippy --workspace --all-targets --all-features -- -D warnings

.PHONY: test
test: ## Run every test, the doctests included
	@cargo test --workspace --all-features

.PHONY: doc
doc: ## Build the documentation, refusing a warning or a broken link
	@RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features

.PHONY: msrv
msrv: ## Check every crate on the oldest supported Rust
	@rustup toolchain install $(MSRV) --profile minimal --no-self-update >/dev/null
	@rustup run $(MSRV) cargo check --workspace --all-targets --all-features

# llvm-cov's line percentage counts a generic function's lines in its best
# instantiation alone, so the gate counts the lines that did not run in any
# test. The tests of the conformance crate run, because its vectors call the
# library, and the report leaves out the crate's own lines. The report also
# leaves out the source of the standard library, which a toolchain built from
# source states by its real path.
.PHONY: coverage
coverage: ## Fail when a line or a function of a published crate did not run in a test
	@cargo llvm-cov --workspace --exclude-from-report dokimi-assert-conformance --all-features \
		--ignore-filename-regex '/rustlib/src/rust/library/' \
		--fail-uncovered-lines 0 --fail-uncovered-functions 0 --show-missing-lines

.PHONY: deny
deny: ## Check the licences, advisories and sources of the dependencies
	@cargo deny check

.PHONY: package
package: ## Build each published crate from its package
	@cargo package --workspace

.PHONY: lint-md
lint-md: ## Lint the markdown
	@ergon lint md

.PHONY: spec-sync
spec-sync: ## Vendor the definition of assert-spec at the head of its branch main
	@$(SPEC) sync

.PHONY: spec-check
spec-check: ## Check that the vendored definition is intact and comes from a commit of assert-spec
	@$(SPEC) check

.PHONY: check
check: fmt-check lint test doc msrv coverage deny package spec-check ## Run every stage that CI runs
	@echo "rust: every stage passed"

.PHONY: clean
clean: ## Remove the build output
	@cargo clean
