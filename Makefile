# Common section
.PHONY: show
show:
	@ ./scripts/gear.sh show

.PHONY: pre-commit
pre-commit: fmt clippy test

.PHONY: check-spec
check-spec:
	@ ./scripts/check-spec.sh

.PHONY: clean
clean:
	@ cargo clean
	@ git clean -fdx

# Build section
.PHONY: gear
gear:
	@ ./scripts/gear.sh build gear

.PHONY: gear-release
gear-release:
	@ ./scripts/gear.sh build gear --release

.PHONY: examples
examples:
	@ ./scripts/gear.sh build examples

.PHONY: examples-release
examples-release:
	@ ./scripts/gear.sh build examples --release

.PHONY: wasm-proc
wasm-proc:
	@ ./scripts/gear.sh build wasm-proc

.PHONY: wasm-proc-release
wasm-proc-release:
	@ ./scripts/gear.sh build wasm-proc --release

.PHONY: examples-proc
examples-proc: wasm-proc-release
	@ ./scripts/gear.sh build examples-proc

.PHONY: node
node:
	@ ./scripts/gear.sh build node

.PHONY: node-release
node-release:
	@ ./scripts/gear.sh build node --release

.PHONY: vara
vara:
	@ ./scripts/gear.sh build node --no-default-features --features=vara-native,lazy-pages

.PHONY: vara-release
vara-release:
	@ ./scripts/gear.sh build node --release --no-default-features --features=vara-native,lazy-pages

# Check section
.PHONY: check
check:
	@ ./scripts/gear.sh check gear

.PHONY: check-release
check-release:
	@ ./scripts/gear.sh check gear --release

# Clippy section
.PHONY: clippy
clippy:
	@ ./scripts/gear.sh clippy gear --all-targets --all-features

.PHONY: clippy-release
clippy-release:
	@ ./scripts/gear.sh clippy gear --release

# Docker section
.PHONY: docker-run
docker-run:
	@ ./scripts/gear.sh docker run

# Format section
.PHONY: fmt
fmt: fmt-gear fmt-doc

.PHONY: fmt-check
fmt-check: fmt-gear-check fmt-doc-check

.PHONY: fmt-gear
fmt-gear:
	@ ./scripts/gear.sh format gear

.PHONY: fmt-gear-check
fmt-gear-check:
	@ ./scripts/gear.sh format gear --check

.PHONY: fmt-doc
fmt-doc:
	@ ./scripts/gear.sh format doc

.PHONY: fmt-doc-check
fmt-doc-check:
	@ ./scripts/gear.sh format doc --check

# Init section
.PHONY: init
init: init-wasm init-cargo

.PHONY: init-wasm
init-wasm:
	@ ./scripts/gear.sh init wasm

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
	@ RUST_LOG="gear_core_processor=debug,gwasm=debug,pallet_gas=debug,pallet_gear=debug" ./scripts/gear.sh run node -- --dev

.PHONY: run-dev-node-release
run-dev-node-release:
	@ RUST_LOG="gear_core_processor=debug,gwasm=debug,pallet_gas=debug,pallet_gear=debug" ./scripts/gear.sh run node --release -- --dev

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
.PHONY: test # \
	There should be no release builds to keep checks fast.
test: test-gear

.PHONY: test-release
test-release: test-gear-release

.PHONY: test-doc
test-doc:
	@ ./scripts/gear.sh test doc

.PHONY: test-gear
test-gear: #\
	We use lazy-pages feature for pallet-gear-debug due to cargo building issue \
	and fact that pallet-gear default is lazy-pages.
	@ ./scripts/gear.sh test gear --exclude gclient --exclude gcli --exclude gsdk --features pallet-gear-debug/lazy-pages

.PHONY: test-gear-release
test-gear-release: # \
	We use lazy-pages feature for pallet-gear-debug due to cargo building issue \
	and fact that pallet-gear default is lazy-pages.
	@ ./scripts/gear.sh test gear --release --exclude gclient --exclude gcli --exclude gsdk --features pallet-gear-debug/lazy-pages

.PHONY: test-gsdk
test-gsdk: node-release
	@ ./scripts/gear.sh test gsdk

.PHONY: test-gsdk-release
test-gsdk-release: node-release
	@ ./scripts/gear.sh test gsdk --release

.PHONY: test-gcli
test-gcli: node
	@ ./scripts/gear.sh test gcli

.PHONY: test-gcli-release
test-gcli-release: node-release
	@ ./scripts/gear.sh test gcli --release

.PHONY: test-pallet
test-pallet:
	@ ./scripts/gear.sh test pallet

.PHONY: test-pallet-release
test-pallet-release:
	@ ./scripts/gear.sh test pallet --release

.PHONY: test-client
test-client: node-release
	@ ./scripts/gear.sh test client

.PHONY: test-client-release
test-client-release: node-release
	@ ./scripts/gear.sh test client --release

.PHONY: test-syscalls-integrity
test-syscalls-integrity:
	@ ./scripts/gear.sh test syscalls

.PHONY: test-syscalls-integrity-release
test-syscalls-integrity-release:
	@ ./scripts/gear.sh test syscalls --release

# Misc section
.PHONY: doc
doc:
	@ RUSTDOCFLAGS="--enable-index-page -Zunstable-options -D warnings" cargo doc --no-deps \
		-p galloc -p gclient -p gcore -p gear-backend-common -p gear-backend-sandbox \
		-p gear-core -p gear-core-processor -p gear-lazy-pages -p gear-core-errors \
		-p gmeta -p gstd -p gtest -p gear-wasm-builder -p gear-common \
		-p pallet-gear -p pallet-gear-gas -p pallet-gear-messenger -p pallet-gear-payment \
		-p pallet-gear-program -p pallet-gear-rpc-runtime-api -p pallet-gear-rpc -p pallet-gear-scheduler -p gsdk
	@ cp -f images/logo.svg target/doc/rust-logo.svg

.PHONY: fuzz
fuzz:
	@ ./scripts/gear.sh test fuzz $(target)

.PHONY: fuzz-vara #TODO 2434 test it works
fuzz-vara:
	@ ./scripts/gear.sh test fuzz --features=vara-native,lazy-pages --no-default-features $(target)

.PHONY: kill-gear
kill:
	@ pkill -f 'gear |gear$' -9

.PHONY: kill-rust
kill-rust:
	@ pgrep -f "rust" | sudo xargs kill -9

.PHONY: install
install:
	@ cargo install --path ./node/cli --force --locked
