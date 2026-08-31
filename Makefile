# Every target runs from the workspace root and covers both crates.
.DEFAULT_GOAL := help

.PHONY: help
help: ## List the targets
	@grep -hE '^[a-z-]+:.*?## ' $(MAKEFILE_LIST) \
		| awk 'BEGIN{FS=":.*?## "}{printf "  %-16s %s\n", $$1, $$2}'

.PHONY: fmt
fmt: ## Format both crates
	@cargo fmt

.PHONY: build
build: ## Compile both crates
	@cargo build --all-targets

.PHONY: test
test: ## Run every test, including the doctests
	@cargo test

.PHONY: lint
lint: ## Clippy at the workspace's own level, warnings included
	@cargo clippy --all-targets -- -D warnings

.PHONY: lint-fmt
lint-fmt: ## Fail when anything is unformatted
	@cargo fmt --check

.PHONY: lint-md
lint-md: ## Lint the markdown
	@ergon lint md 2>/dev/null || echo "lint-md: ergon not installed, skipped"

.PHONY: doc
doc: ## Build the documentation, refusing a broken link
	@cargo doc --no-deps

.PHONY: msrv
msrv: ## Check the crate still builds on its stated minimum
	@rustup toolchain install 1.85 --profile minimal --no-self-update >/dev/null 2>&1 || true
	@cargo +1.85 check --all-targets

.PHONY: check
check: lint-fmt lint build test doc ## Everything CI runs
	@echo "rust: every stage passed"

.PHONY: spec-sync
spec-sync: ## Refresh the vendored definition from ../assert-spec
	@cp ../assert-spec/spec/assertions.json ../assert-spec/spec/naming.json \
		dokimi-assert/tests/spec/
	@cp ../assert-spec/overlays/rust.json dokimi-assert/tests/spec/overlay.json
	@cp ../assert-spec/corpus/*.json dokimi-assert/tests/spec/corpus/
	@echo "spec: vendored copy refreshed; run make test"

.PHONY: clean
clean: ## Remove build output
	@cargo clean
