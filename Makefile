# Ethexe section
.PHONY: ethexe-pre-commit
ethexe-pre-commit: ethexe-contracts-pre-commit ethexe-pre-commit-no-contracts

.PHONY: ethexe-pre-commit-no-contracts
ethexe-pre-commit-no-contracts: fmt clippy
	@ echo " >>> Testing ethexe" && cargo nextest run -p "ethexe-*" --no-fail-fast

# Building ethexe contracts
.PHONY: ethexe-contracts-pre-commit
ethexe-contracts-pre-commit:
	@ echo " > Cleaning contracts" && forge clean --root ethexe/contracts
	@ echo " > Formatting contracts" && forge fmt --root ethexe/contracts
	@ echo " > Building contracts" && forge build --root ethexe/contracts
	@ echo " > Testing contracts" && forge test --root ethexe/contracts -vvv
	@ echo " > Copying Middleware artifact" && cp ./ethexe/contracts/out/Middleware.sol/Middleware.json ./ethexe/ethereum
	@ echo " > Copying Mirror artifact" && cp ./ethexe/contracts/out/Mirror.sol/Mirror.json ./ethexe/ethereum
	@ echo " > Copying Router artifact" && cp ./ethexe/contracts/out/Router.sol/Router.json ./ethexe/ethereum
	@ echo " > Copying TransparentUpgradeableProxy artifact" && cp ./ethexe/contracts/out/TransparentUpgradeableProxy.sol/TransparentUpgradeableProxy.json ./ethexe/ethereum
	@ echo " > Copying WrappedVara artifact" && cp ./ethexe/contracts/out/WrappedVara.sol/WrappedVara.json ./ethexe/ethereum

# Common section
.PHONY: show
show:
	@ ./scripts/gear.sh show

.PHONY: workspace-hack
workspace-hack:
	@ cargo hakari generate && ./scripts/hakari-post-process.sh

.PHONY: pre-commit # Here should be no release builds to keep checks fast.
pre-commit: fmt typos workspace-hack clippy test check-runtime-imports

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
	@ ./scripts/gear.sh build node --no-default-features --features=vara-native

.PHONY: vara-release
vara-release:
	@ ./scripts/gear.sh build node --release --no-default-features --features=vara-native

.PHONY: gear-replay
gear-replay:
	@ ./scripts/gear.sh build gear-replay

.PHONY: gear-replay-vara-native
gear-replay-vara-native:
	@ ./scripts/gear.sh build gear-replay --no-default-features --features=std,vara-native

# Check section
.PHONY: check
check:
	@ ./scripts/gear.sh check gear

.PHONY: check-release
check-release:
	@ ./scripts/gear.sh check gear --release

.PHONY: check-runtime-imports
check-runtime-imports:
	@ ./scripts/gear.sh check runtime-imports

# Clippy section
.PHONY: clippy
clippy: clippy-gear clippy-examples

.PHONY: clippy-release
clippy-release: clippy-gear-release clippy-examples-release

.PHONY: clippy-gear
clippy-gear:
	@ ./scripts/gear.sh clippy gear --all-targets --all-features

.PHONY: clippy-examples
clippy-examples:
	@ ./scripts/gear.sh clippy examples --all-targets

.PHONY: clippy-gear-release
clippy-gear-release:
	@ ./scripts/gear.sh clippy gear --release

.PHONY: clippy-examples-release
clippy-examples-release:
	@ ./scripts/gear.sh clippy examples --all-targets --release

# Docker section
.PHONY: docker-run
docker-run:
	@ ./scripts/gear.sh docker run

# Format section
.PHONY: fmt
fmt:
	@ ./scripts/gear.sh format gear

.PHONY: fmt-check
fmt-check:
	@ ./scripts/gear.sh format gear --check

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
.PHONY: test # Here should be no release builds to keep checks fast.
test: test-gear

.PHONY: test-release
test-release: test-gear-release

.PHONY: test-doc
test-doc:
	@ ./scripts/gear.sh test docs

.PHONY: test-gear
test-gear: # Crates are excluded to significantly decrease time.
	@ ./scripts/gear.sh test gear \
		--exclude gear-authorship \
		--exclude pallet-gear-staking-rewards \
		--exclude gear-wasm-gen \
		--exclude demo-stack-allocations

.PHONY: test-gear-release
test-gear-release:
	@ ./scripts/gear.sh test gear --release

.PHONY: test-gsdk
test-gsdk: node-release
	@ ./scripts/gear.sh test gsdk

.PHONY: test-gsdk-release
test-gsdk-release: node-release
	@ ./scripts/gear.sh test gsdk --release

.PHONY: test-gcli
test-gcli: node-release
	@ ./scripts/gear.sh test gcli

.PHONY: test-gcli-release
test-gcli-release: node-release
	@ ./scripts/gear.sh test gcli --release

.PHONY: test-gbuild
test-gbuild: node
	@ ./scripts/gear.sh test gbuild

.PHONY: test-gbuild-release
test-gbuild-release: node-release
	@ ./scripts/gear.sh test gbuild --release

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
.PHONY: kill-gear
kill:
	@ pkill -f 'gear |gear$' -9

.PHONY: kill-rust
kill-rust:
	@ pgrep -f "rust" | sudo xargs kill -9

.PHONY: install
install:
	@ cargo install --path ./node/cli --force --locked

.PHONY: typos
typos:
	@ ./scripts/gear.sh test typos

.PHONY: ethexe-remappings
ethexe-remappings:
	@ cd ethexe/contracts && forge remappings > remappings.txt && cd ../..
