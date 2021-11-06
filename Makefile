# Common section
.PHONY: show
show:
	@ ./scripts/gear.sh show

.PHONY: pre-commit
pre-commit: fmt clippy test

.PHONY: clean
clean:
	@ cargo clean --manifest-path=./Cargo.toml
	@ cargo clean --manifest-path=./examples/Cargo.toml

.PHONY: clean-examples
clean-examples:
	@ rm -rf ./target/wasm32-unknown-unknown
	@ cargo clean --manifest-path=./examples/Cargo.toml

.PHONY: clean-node
clean-node:
	@ cargo clean -p gear-node

# Build section
.PHONY: all
all: gear examples

.PHONY: all-release
all-release: gear-release examples

.PHONY: gear
gear:
	@ ./scripts/gear.sh build gear

.PHONY: gear-release
gear-release:
	@ ./scripts/gear.sh build gear --release

.PHONY: examples
examples: build-examples proc-examples

.PHONY: build-examples
build-examples:
	@ ./scripts/gear.sh build examples

.PHONY: wasm-proc
wasm-proc:
	@ ./scripts/gear.sh build wasm-proc

.PHONY: proc-examples
proc-examples: wasm-proc
	@ ./scripts/gear.sh build examples-proc

.PHONY: node
node:
	@ ./scripts/gear.sh build node

.PHONY: node-release
node-release:
	@ ./scripts/gear.sh build node --release

# Check section
.PHONY: check
check: check-gear check-examples check-benchmark

.PHONY: check-release
check-release: check-gear-release check-examples check-benchmark-release

.PHONY: check-gear
check-gear:
	@ ./scripts/gear.sh check gear

.PHONY: check-gear-release
check-gear-release:
	@ ./scripts/gear.sh check gear --release

.PHONY: check-examples
check-examples:
	@ ./scripts/gear.sh check examples

.PHONY: check-benchmark
check-benchmark:
	@ ./scripts/gear.sh check benchmark

.PHONY: check-benchmark-release
check-benchmark-release:
	@ ./scripts/gear.sh check benchmark --release

# Clippy section
.PHONY: clippy
clippy: clippy-gear clippy-examples

.PHONY: clippy-release
clippy-release: clippy-gear-release clippy-examples

.PHONY: clippy-gear
clippy-gear:
	@ ./scripts/gear.sh clippy gear

.PHONY: clippy-gear-release
clippy-gear-release:
	@ ./scripts/gear.sh clippy gear --release

.PHONY: clippy-examples
clippy-examples:
	@ ./scripts/gear.sh clippy examples

# Docker section
.PHONY: docker-run
docker-run:
	@ ./scripts/gear.sh docker run

# Format section
.PHONY: fmt
fmt: fmt-gear fmt-examples fmt-doc

.PHONY: fmt-check
fmt-check: fmt-gear-check fmt-examples-check fmt-doc-check

.PHONY: fmt-gear
fmt-gear:
	@ ./scripts/gear.sh format gear

.PHONY: fmt-gear-check
fmt-gear-check:
	@ ./scripts/gear.sh format gear --check

.PHONY: fmt-examples
fmt-examples:
	@ ./scripts/gear.sh format examples

.PHONY: fmt-examples-check
fmt-examples-check:
	@ ./scripts/gear.sh format examples --check

.PHONY: fmt-doc
fmt-doc:
	@ ./scripts/gear.sh format doc

.PHONY: fmt-doc-check
fmt-doc-check:
	@ ./scripts/gear.sh format doc --check

# Init section
.PHONY: init
init: init-wasm init-cargo init-js

.PHONY: init-wasm
init-wasm:
	@ ./scripts/gear.sh init wasm

.PHONY: init-js
init-js:
	@ ./scripts/gear.sh init js

.PHONY: update-js
update-js:
	@ ./scripts/gear.sh init update-js

.PHONY: init-cargo
init-cargo:
	@ ./scripts/gear.sh init cargo

# Run section
.PHONY: run-node
run-node:
	@ ./scripts/gear.sh run node

.PHONY: run-node-release
run-node-release:
	@ ./scripts/gear.sh run node --release

.PHONY: run-dev-node
run-dev-node:
	@ ./scripts/gear.sh run node -- --dev

.PHONY: run-dev-node-release
run-dev-node-release:
	@ ./scripts/gear.sh run node --release -- --dev

.PHONY: purge-chain
purge-chain:
	@ ./scripts/gear.sh run purge-chain

.PHONY: purge-chain-release
purge-chain-release:
	@ ./scripts/gear.sh run purge-chain --release

.PHONY: purge-dev-chain
purge-dev-chain:
	@ ./scripts/gear.sh run purge-dev-chain

.PHONY: purge-dev-chain-release
purge-dev-chain-release:
	@ ./scripts/gear.sh run purge-dev-chain --release

# Test section
.PHONY: test
test: test-gear test-pallet test-js gtest

.PHONY: test-release
test-release: test-gear-release test-pallet-release test-js gtest ntest

.PHONY: test-gear
test-gear: init-js examples
	@ ./scripts/gear.sh test gear

.PHONY: test-gear-release
test-gear-release: init-js examples
	@ ./scripts/gear.sh test gear --release

.PHONY: test-js
test-js: init-js
	@ ./scripts/gear.sh test js

.PHONY: gtest
gtest: init-js examples
	@ ./scripts/gear.sh test gtest

.PHONY: ntest
ntest:
	@ ./scripts/gear.sh test ntest

.PHONY: test-pallet
test-pallet:
	@ ./scripts/gear.sh test pallet

.PHONY: test-pallet-release
test-pallet-release:
	@ ./scripts/gear.sh test pallet --release
