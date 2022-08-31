# Common section
.PHONY: show
show:
	@ ./scripts/gear.sh show

.PHONY: pre-commit
pre-commit: fmt check-spec clippy test

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

.PHONY: build-wat-examples
build-wat-examples:
	@ ./scripts/gear.sh build wat-examples

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

.PHONY: proc-examples
proc-examples: wasm-proc
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

# Collator
.PHONY: collator
collator:
	@ ./scripts/gear.sh build collator

.PHONY: collator-release
collator-release:
	@ ./scripts/gear.sh build collator --release

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
	@ RUST_LOG="gear_core_processor=debug,gwasm=debug,pallet_gas=debug,pallet_gear=debug" ./scripts/gear.sh run node -- --dev -l0

.PHONY: run-dev-node-release
run-dev-node-release:
	@ RUST_LOG="gear_core_processor=debug,gwasm=debug,pallet_gas=debug,pallet_gear=debug" ./scripts/gear.sh run node --release -- --dev -l0

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
test: test-gear test-js gtest # There should be no release builds (e.g. `rtest`) for fast checking.

.PHONY: test-release
test-release: test-gear-release test-js gtest rtest test-runtime-upgrade test-client-weights

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
gtest: init-js gear-test-release examples
	@ ./scripts/gear.sh test gtest yamls="$(yamls)"

.PHONY: rtest
rtest: init-js node-release examples
	@ ./scripts/gear.sh test rtest yamls="$(yamls)"

.PHONY: rtest-vara
rtest-vara: init-js vara-release examples
	@ ./scripts/gear.sh test rtest yamls="$(yamls)"

.PHONY: rtest-rococo
rtest-rococo: init-js collator-release examples
	@ ./scripts/gear.sh test collator_runtime yamls="$(yamls)"

.PHONY: test-pallet
test-pallet:
	@ ./scripts/gear.sh test pallet

.PHONY: test-pallet-release
test-pallet-release:
	@ ./scripts/gear.sh test pallet --release

.PHONY: test-runtime-upgrade
test-runtime-upgrade: init-js examples node-release
	@ ./scripts/gear.sh test runtime-upgrade

.PHONY: test-client-weights
test-client-weights: init-js examples node-release
	@ ./scripts/gear.sh test client-weights

# Misc section
.PHONY: doc
doc:
	@ RUSTDOCFLAGS="--enable-index-page -Zunstable-options" cargo +nightly doc --no-deps \
		-p galloc -p gcore -p gear-backend-common -p gear-backend-sandbox \
		-p gear-core -p gear-core-processor -p gear-lazy-pages -p gear-core-errors \
		-p gstd -p gtest -p gear-wasm-builder -p gear-common
	@ cp -f images/logo.svg target/doc/rust-logo.svg

.PHONY: fuzz
fuzz:
	@ ./scripts/gear.sh test fuzz $(target)

.PHONY: fuzz-vara
fuzz-vara:
	@ ./scripts/gear.sh test fuzz --features=vara-native,lazy-pages --no-default-features $(target)
