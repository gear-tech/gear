# Common section
.PHONY: show
show:
	@ ./scripts/gear.sh show

.PHONY: pre-commit
pre-commit: fmt clippy test-gear # check-spec

.PHONY: check-spec
check-spec:
	@ ./scripts/check-spec.sh

.PHONY: clean
clean:
	@ cargo clean --manifest-path=./Cargo.toml
	@ cargo clean --manifest-path=./examples/Cargo.toml

.PHONY: clean-examples
clean-examples:
	@ rm -rf ./target/wasm32-unknown-unknown
	@ rm -rvf target/release/build/demo-*
	@ cargo clean --manifest-path=./examples/Cargo.toml

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

.PHONY: gear-test
gear-test:
	@ ./scripts/gear.sh build gear-test

.PHONY: gear-test-release
gear-test-release:
	@ ./scripts/gear.sh build gear-test --release

.PHONY: examples
examples: build-examples proc-examples

.PHONY: build-examples
build-examples:
	@ ./scripts/gear.sh build examples yamls="$(yamls)"

.PHONY: wasm-proc
wasm-proc:
	@ ./scripts/gear.sh build wasm-proc

.PHONY: wat-examples
wat-examples:
	@ ./scripts/gear.sh build wat-examples

.PHONY: proc-examples
proc-examples: wasm-proc
	@ ./scripts/gear.sh build examples-proc

.PHONY: node
node:
	@ ./scripts/gear.sh build node

.PHONY: node-release
node-release:
	@ ./scripts/gear.sh build node --release

.PHONY: node-release-rtest
node-release-rtest:
	@ ./scripts/gear.sh build node --release --no-default-features --features=gear-native,lazy-pages,runtime-test

.PHONY: vara
vara:
	@ ./scripts/gear.sh build node --no-default-features --features=vara-native,lazy-pages

.PHONY: vara-release
vara-release:
	@ ./scripts/gear.sh build node --release --no-default-features --features=vara-native,lazy-pages

.PHONY: vara-release-rtest
vara-release-rtest:
	@ ./scripts/gear.sh build node --release --no-default-features --features=runtime-test,vara-native,lazy-pages

# Check section
.PHONY: check
check: check-gear check-examples

.PHONY: check-release
check-release: check-gear-release check-examples

.PHONY: check-gear
check-gear:
	@ ./scripts/gear.sh check gear

.PHONY: check-gear-release
check-gear-release:
	@ ./scripts/gear.sh check gear --release

.PHONY: check-examples
check-examples:
	@ ./scripts/gear.sh check examples

# Clippy section
.PHONY: clippy
clippy: clippy-gear clippy-examples

.PHONY: clippy-release
clippy-release: clippy-gear-release clippy-examples

.PHONY: clippy-gear
clippy-gear:
	@ ./scripts/gear.sh clippy gear --all-targets --all-features

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
	There should be no release builds (e.g. `rtest`) for fast checking.
test: test-gear test-js gtest

.PHONY: test-doc
test-doc:
	@ ./scripts/gear.sh test doc

.PHONY: test-release
test-release: test-gear-release test-js gtest rtest

.PHONY: test-gear
test-gear: init-js examples # \
	We use lazy-pages feature for pallet-gear-debug due to cargo building issue \
	and fact that pallet-gear default is lazy-pages.
	@ ./scripts/gear.sh test gear --exclude gclient --exclude gcli --features pallet-gear-debug/lazy-pages

.PHONY: test-gear-release
test-gear-release: init-js examples # \
	We use lazy-pages feature for pallet-gear-debug due to cargo building issue \
	and fact that pallet-gear default is lazy-pages.
	@ ./scripts/gear.sh test gear --release --exclude gclient --exclude gcli --features pallet-gear-debug/lazy-pages

.PHONY: test-gcli
test-gcli: node
	@ ./scripts/gear.sh test gcli

.PHONY: test-gcli-release
test-gcli-release: node-release
	@ ./scripts/gear.sh test gcli --release

.PHONY: test-js
test-js: init-js
	@ ./scripts/gear.sh test js

.PHONY: gtest
gtest: init-js gear-test-release examples
	@ ./scripts/gear.sh test gtest yamls="$(yamls)"

.PHONY: rtest
rtest: init-js node-release-rtest examples
	@ ./scripts/gear.sh test rtest gear yamls="$(yamls)"

.PHONY: rtest-vara
rtest-vara: init-js vara-release-rtest examples
	@ ./scripts/gear.sh test rtest vara yamls="$(yamls)"

.PHONY: test-pallet
test-pallet:
	@ ./scripts/gear.sh test pallet

.PHONY: test-pallet-release
test-pallet-release:
	@ ./scripts/gear.sh test pallet --release

.PHONY: test-client
test-client: node-release examples wat-examples
	@ ./scripts/gear.sh test client --run-node

.PHONY: test-syscalls-integrity
test-syscalls-integrity:
	@ ./scripts/gear.sh test syscalls

.PHONY: test-syscalls-integrity-release
test-syscalls-integrity-release:
	@ ./scripts/gear.sh test syscalls --release

# Misc section
.PHONY: doc
doc:
	@ RUSTDOCFLAGS="--enable-index-page -Zunstable-options" cargo +nightly doc --no-deps \
		-p galloc -p gclient -p gcore -p gear-backend-common -p gear-backend-sandbox \
		-p gear-core -p gear-core-processor -p gear-lazy-pages -p gear-core-errors \
		-p gstd -p gtest -p gear-wasm-builder -p gear-common \
		-p pallet-gear -p pallet-gear-gas -p pallet-gear-messenger -p pallet-gear-payment \
		-p pallet-gear-program -p pallet-gear-rpc-runtime-api -p pallet-gear-rpc -p pallet-gear-scheduler -p gsdk
	@ cp -f images/logo.svg target/doc/rust-logo.svg

.PHONY: fuzz
fuzz:
	@ ./scripts/gear.sh test fuzz $(target)

.PHONY: fuzz-vara #TODO 2434 test it works
fuzz-vara:
	@ ./scripts/gear.sh test fuzz --features=vara-native,lazy-pages --no-default-features $(target)

.PHONY: kill
kill:
	@ pkill -f 'gear |gear$' -9

.PHONY: kill-rust
kill-rust:
	@ pgrep -f "rust" | sudo xargs kill -9

.PHONY: install
install:
	@ cargo install --path ./node/cli --force --locked
