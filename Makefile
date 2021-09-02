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
	@cargo test --package gear-core --package gear-core-backend --package gear-core-runner -- --nocapture

.PHONY: examples
examples:
	@./scripts/build-wasm.sh

.PHONY: gstd-async-test
gstd-async-test:
	@cargo test --package gstd-async -- --nocapture

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
	@cargo +nightly fmt --all
	@cargo +nightly fmt --all --manifest-path examples/Cargo.toml -- --config=license_template_path=""

.PHONY: node
node:
	@cargo build --package gear-node

.PHONY: node-release
node-release:
	@cargo build --package gear-node --release

.PHONY: node-run
node-run:
	@cargo run --package gear-node --release -- --dev --tmp

.PHONY: node-test
node-test:
	@SKIP_WASM_BUILD=1 cargo test --package gear-node

.PHONY: ntest
ntest:
	@./scripts/build-wasm.sh
	@cargo run --package gear-node --release -- runtests ./gtest/spec/*.yaml

.PHONY: pre-commit
pre-commit:
	@./scripts/pre-commit.sh

.PHONY: release
release:
	@cargo build --workspace --release

.PHONY: rpc-test
rpc-test:
	@./scripts/build-wasm.sh
	@node rpc-tests/index.js ./gtest/spec/*.yaml

.PHONY: test
test:
	@cargo test --workspace
	@cargo test -p gear-core-backend --no-default-features --features wasmi_backend

.PHONY: test-release
test-release:
	@cargo test --workspace --release

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
