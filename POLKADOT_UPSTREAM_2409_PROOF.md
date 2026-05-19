# Polkadot SDK Upstream 2409 Proof

Branch: `vs/polkadot-upstream-2409-proof`

Goal: prove that Gear can use upstream `paritytech/polkadot-sdk` stable2409 instead of the custom `gear-tech/polkadot-sdk` fork, while keeping Gear-specific SDK differences localized in this repository.

## Upstream Baseline

All workspace `polkadot-sdk` dependencies were moved from:

```text
https://github.com/gear-tech/polkadot-sdk.git
branch = "gear-polkadot-stable2409-wasm32v1-none"
```

to:

```text
https://github.com/paritytech/polkadot-sdk.git
rev = "298f676c91d64f15f38ea7fd78f125c5889ab09c"
```

No Cargo dependency on `gear-tech/polkadot-sdk` remains. The only remaining `gear-tech/polkadot-sdk` references are old source-link comments in `vara/pallets/gear-eth-bridge/src/tests.rs`.

## Localized Fork Diff

The fork-only pieces that Gear still needs were copied into local crates or local patches:

- `protocol/sp-allocator`
- `protocol/wasm-interface-common`
- `utils/substrate-wasm-builder`
- `protocol/runtime-executor`
- `protocol/runtime-executor/common`
- `protocol/runtime-executor/polkavm`
- `protocol/runtime-executor/wasmtime`

The local runtime executor crates are patched over upstream package identities with:

```toml
[patch."https://github.com/paritytech/polkadot-sdk.git"]
sc-executor = { path = "protocol/runtime-executor" }
sc-executor-common = { path = "protocol/runtime-executor/common" }
sc-executor-polkavm = { path = "protocol/runtime-executor/polkavm" }
sc-executor-wasmtime = { path = "protocol/runtime-executor/wasmtime" }
```

`gear-sandbox-interface` was adjusted to use upstream `sp-wasm-interface` plus the local `sc-executor` caller utilities for `host-api`.

## Removed Or Deleted

- Removed the custom `gear-tech/polkadot-sdk` Cargo dependency source.
- Kept `binary-merkle-tree` on crates.io (`16.1.1`) instead of copying it locally.
- Removed `sc-executor-wasmi` from workspace dependency wiring.
- Deleted the unused local `protocol/runtime-executor/wasmi` copy from the proof branch.
- Removed the fork-only `logger.with_max_level(log::LevelFilter::Info)` call from `vara/node/cli/src/command.rs`; upstream `LoggerBuilder` does not expose that method.
- Moved `gear-workspace-hack` out of wasm example target dependencies by making example usage native-only.

## Clippy And Build Hygiene

The copied upstream crates needed local clippy cleanup because Gear runs `-D warnings`:

- Fixed clippy warnings in `sp-allocator` and runtime executor crates.
- Added crate-level style lint allows in `utils/substrate-wasm-builder` for copied upstream code where the lints are formatting/style-only.
- Fixed Rust 2024 unsafe-op-in-unsafe-fn requirements in the local wasmtime executor code.
- Added `cfg(build_type, values("debug"))` to workspace check-cfg.

The clippy wrapper was also fixed:

- `scripts/src/common.sh` no longer fails when `TERM` is unset.
- `scripts/src/clippy.sh` now discovers examples relative to `workspace_root`, so `make clippy` works from this worktree path and not only from the canonical checkout path.

## Validation

Passing in this worktree:

```text
rtk cargo fmt
rtk make clippy
rtk git diff --check
```

Earlier focused checks also passed:

```text
rtk cargo check -p sp-wasm-interface-common --no-default-features
rtk cargo check -p substrate-wasm-builder
rtk cargo test -p sc-executor --lib --no-run
rtk cargo check -p gear-sandbox-interface --features host-api
rtk cargo check -p vara-runtime --features std
rtk cargo test -p pallet-gear-eth-bridge --lib
```

Known non-blockers:

- Separate `gear-service --features vara-native` check was intentionally skipped.
- `make clippy` still emits existing warnings from `trie-db v0.29.1` future incompatibility and a clang unused `-fstack-clash-protection` flag in `gear-stack-buffer`.
