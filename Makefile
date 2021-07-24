.PHONY: all
all:
	@cargo build --workspace

.PHONY: check
check:
	@cargo check --workspace

.PHONY: clean
clean:
	@rm -rf target
	@rm -rf examples/target

.PHONY: core-test
core-test:
	@cargo test --package gear-core --package gear-core-backend --package gear-core-runner

.PHONY: examples
examples:
	@./scripts/build-wasm.sh

.PHONY: gstd-test
gstd-test:
	@cargo test --package gstd

.PHONY: gtest
gtest:
	@./scripts/test.sh

.PHONY: init
init:
	@./scripts/init.sh

.PHONY: fmt
fmt:
	@cargo fmt --all

.PHONY: node
node:
	@WASM_BUILD_TOOLCHAIN=nightly-2020-10-05 cargo build --package gear-node

.PHONY: node-release
node-release:
	@WASM_BUILD_TOOLCHAIN=nightly-2020-10-05 cargo build --package gear-node --release

.PHONY: node-run
node-run:
	@WASM_BUILD_TOOLCHAIN=nightly-2020-10-05 cargo run --package gear-node --release -- --dev --tmp

.PHONY: node-test
node-test:
	@SKIP_WASM_BUILD=1 cargo test --package gear-node

.PHONY: pre-commit
pre-commit:
	@./scripts/pre-commit.sh

.PHONY: release
release:
	@cargo build --workspace --release

.PHONY: test
test:
	@cargo test --workspace

.PHONY: toolchain
toolchain:
	@rustup show
	@echo targets
	@echo -------
	@echo
	@rustup target list --installed
	@echo
	@echo nightly targets
	@echo ---------------
	@echo
	@rustup +nightly target list --installed
