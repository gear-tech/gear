# Gear-maintained Polkadot SDK crates

This directory contains selected Polkadot SDK crates copied into the Gear workspace and refreshed for Polkadot SDK `polkadot-stable2603-3`, source reference [`e3737178ec726cffe506c907263aaaa417893fd0`](https://github.com/paritytech/polkadot-sdk/tree/e3737178ec726cffe506c907263aaaa417893fd0), plus Gear-local compatibility crates needed to isolate the remaining fork delta.

Copied crates are modified under the terms of their upstream open-source licenses. Original SPDX headers and upstream copyright notices remain in the copied source files; original copyright ownership remains with the upstream rightsholders as indicated there, including Parity Technologies where present. Gear maintains local changes to isolate the remaining fork delta while the rest of the workspace depends on upstream Polkadot SDK.

Local Cargo package names intentionally stay compatible with upstream package names so `[patch]` can replace Polkadot SDK git dependencies. When these crates are prepared for crates.io, Gear publishes them under `g*` aliases for Gear ecosystem packages.

## Copied Polkadot SDK Crates

| Local path | Upstream package | Gear publish name | License |
| --- | --- | --- | --- |
| `substrate/sp-allocator` | local `sp-allocator` package name; derived from upstream `sc-allocator` | `gsp-allocator` | Apache-2.0 |
| `substrate/sp-wasm-interface` | `sp-wasm-interface` | `gsp-wasm-interface` | Apache-2.0 |
| `substrate/runtime-executor/common` | `sc-executor-common` | `gsc-executor-common` | GPL-3.0-or-later WITH Classpath-exception-2.0 |
| `substrate/runtime-executor/polkavm` | `sc-executor-polkavm` | `gsc-executor-polkavm` | GPL-3.0-or-later WITH Classpath-exception-2.0 |
| `substrate/runtime-executor/wasmtime` | `sc-executor-wasmtime` | `gsc-executor-wasmtime` | GPL-3.0-or-later WITH Classpath-exception-2.0 |
| `substrate/runtime-executor` | `sc-executor` | not published by Gear | GPL-3.0-or-later WITH Classpath-exception-2.0 |
| `substrate/substrate-wasm-builder` | `substrate-wasm-builder` | `gsubstrate-wasm-builder` | Apache-2.0 |

`sc-mixnet` is no longer copied locally. Gear resolves it from upstream
Polkadot SDK `polkadot-stable2603-3`.

## Gear Compatibility Crates

| Local path | Upstream-compatible package name | Gear publish name | License |
| --- | --- | --- | --- |
| `substrate/sp-wasm-interface-common` | `sp-wasm-interface-common` | `gsp-wasm-interface-common` | Apache-2.0 |

`substrate/sp-wasm-interface-common` is Gear-authored compatibility code, not copied upstream source. It keeps the upstream-compatible local package name so Gear can patch dependencies that previously resolved through the custom Polkadot SDK fork.

Publishing is handled by Gear maintainers through `utils/crates-io`; this README only documents the fork and naming policy.
