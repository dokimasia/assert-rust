# Managed by ergon init, with .ergon/local/Makefile merged in. Change the repository's settings there and run ergon init sync.
#
# make check is the gate of the repository. lint-common lints the Markdown files, and lint-github
# lints the workflows. Each language adds its own fmt, lint, test, generate, audit and check
# targets as prerequisites of the targets below. CI runs make check-<language> per language. It
# runs lint-common and lint-github in jobs of their own. Every target runs its tools through ergon
# tool run, which installs the version that .ergon.yaml names. make help lists the targets in
# groups. A line ##@ <name> starts a group: the common targets, the targets of the GitHub files and
# of spec, then the targets of each language, and the targets of .ergon/local/Makefile after a line
# of its own.

.DEFAULT_GOAL := check

# The command that runs ergon for the tools of the targets. The key ergon of the section common of
# .ergon.yaml sets it.
ERGON ?= ergon

# The tools of the common targets, which ergon installs at the versions of the section common.
RUMDL := $(ERGON) tool run common.rumdl --

# verify-generated runs the command of the variable $(1) and fails when the run changes a file of
# the repository, with the target $(2) that updates the files in its message. It compares the
# changes of the working tree before and after the run, so it also runs on a tree with uncommitted
# changes.
verify-generated = before="$$(git diff HEAD --binary 2>/dev/null; git ls-files --others --exclude-standard)"; \
	$($(1)) || exit 1; \
	after="$$(git diff HEAD --binary 2>/dev/null; git ls-files --others --exclude-standard)"; \
	if [ "$$before" != "$$after" ]; then \
	echo "$(2): the generated files are out of date, so run make $(2) and commit the result:" >&2; \
	git status --short >&2; exit 1; fi

.PHONY: help fmt lint test generate audit check lint-common

##@ Common

help: ## List the targets in groups
	@awk 'BEGIN {FS = ":.*## "} /^##@ / {group = substr($$0, 5); next} /^[a-z][a-z-]*:.*## / {if (group != shown) {printf "%s%s\n", (shown == "" ? "" : "\n"), group; shown = group} printf "  %-28s %s\n", $$1, $$2}' $(MAKEFILE_LIST)

fmt: ## Format the sources of every language
lint: ## Lint the Markdown files, the workflows and the sources of every language
test: ## Run the tests of every language
generate: ## Run the generators of every language
audit: ## Run the vulnerability scan of every language
check: ## Run the gate of every language, and lint the Markdown files and the workflows
lint: lint-common
check: lint-common

lint-common: ## Lint the Markdown files with rumdl and the rules of .markdownlint.yml
	$(RUMDL) check --no-cache .

##@ GitHub
#
# The section github of .ergon.yaml sets the tools of the GitHub targets.

# The tools of the GitHub targets, which ergon installs at the versions of the section github.
ACTIONLINT := $(ERGON) tool run github.actionlint --

.PHONY: lint-github
lint: lint-github
check: lint-github

# actionlint runs shellcheck and pyflakes on the scripts of the steps, and the flags turn both off.
# ergon pins neither program, so actionlint would run the versions that the machine has.
lint-github: ## Lint the workflows with actionlint and the configuration of .github/actionlint.yaml
	$(ACTIONLINT) -shellcheck= -pyflakes=

##@ Spec
#
# The section spec of .ergon.yaml sets the roles of the repository and the release of spec. spec
# reads its own configuration from .spec.yaml.

# The program spec, which ergon installs at the version of the section spec.
SPEC := $(ERGON) tool run spec.spec --

.PHONY: check-spec
check: check-spec

check-spec: ## Check the copies of the definitions against their manifests with spec
	$(SPEC) check

##@ Rust
#
# The section rust of .ergon.yaml sets the tools and the options of the Rust targets, and the steps
# of check-rust. One run overrides an option on the command line, such as
# make test-rust RUST_TEST_ARGS=--lib.

# The tools of the Rust targets, which ergon installs at the versions of the section rust. The
# targets of the baseline do not run CARGO_LLVM_COV. A target of .ergon/local/Makefile runs it to
# measure the coverage of the tests, such as
# $(CARGO_LLVM_COV) llvm-cov --workspace --fail-under-lines 90.
CARGO_AUDIT := $(ERGON) tool run rust.cargo-audit --
CARGO_LLVM_COV := $(ERGON) tool run rust.cargo-llvm-cov --
CARGO_HACK := $(ERGON) tool run rust.cargo-hack --

# The options of the section rust.
RUST_TEST_ARGS ?= --workspace --all-targets --all-features
RUST_AUDIT_IGNORE ?= RUSTSEC-2026-0009

.PHONY: fmt-rust lint-rust test-rust msrv-rust audit-rust check-rust
fmt: fmt-rust
lint: lint-rust
test: test-rust
audit: audit-rust
check: check-rust

fmt-rust: ## Format the Rust sources
	cargo fmt --all
lint-rust: ## Check the Rust sources with clippy.toml, the pedantic lints and the documentation lints as errors
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets --all-features -- -D warnings -D clippy::pedantic -D missing_docs \
		-D missing_debug_implementations -D unsafe_code -D unused_qualifications
	RUSTDOCFLAGS="-D warnings -D rustdoc::private_doc_tests -D rustdoc::unescaped_backticks" \
		cargo doc --workspace --all-features --no-deps
test-rust: ## Run the Rust tests, then the doc tests of the library crates
	cargo test $(RUST_TEST_ARGS)
	@if cargo metadata --no-deps --format-version 1 | grep -Eq '"kind":\[[^]]*"(lib|rlib|proc-macro)"'; then \
		cargo test --workspace --doc; \
	fi
msrv-rust: ## Check each published crate on the Rust of its rust-version
	@if cargo metadata --no-deps --format-version 1 | grep -q '"rust_version":"'; then \
		$(CARGO_HACK) hack check --rust-version --workspace --ignore-private --all-targets --all-features; \
	fi
audit-rust: ## Scan Cargo.lock for known vulnerabilities
	$(CARGO_AUDIT) audit $(foreach id,$(RUST_AUDIT_IGNORE),--ignore $(id))
check-rust: lint-rust test-rust msrv-rust audit-rust ## Run the gate of Rust

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
