# Third-Party Notices

## Polkadot SDK Copied Sources

Some local crates contain copied or modified source files from `paritytech/polkadot-sdk`:

- `substrate/sp-allocator` (`sp-allocator`, published by Gear as `gsp-allocator`)
- `substrate/runtime-executor/common` (`sc-executor-common`, published by Gear as `gsc-executor-common`)
- `substrate/runtime-executor/polkavm` (`sc-executor-polkavm`, published by Gear as `gsc-executor-polkavm`)
- `substrate/runtime-executor/wasmtime` (`sc-executor-wasmtime`, published by Gear as `gsc-executor-wasmtime`)
- `substrate/runtime-executor` (`sc-executor`, published by Gear as `gsc-executor`)
- `substrate/substrate-wasm-builder` (`substrate-wasm-builder`, published by Gear as `gsubstrate-wasm-builder`)

Source reference: <https://github.com/paritytech/polkadot-sdk/tree/298f676c91d64f15f38ea7fd78f125c5889ab09c>

These copied source files retain the upstream copyright notices and their original SPDX headers. See [`substrate/README.md`](substrate/README.md) for the shared fork, provenance, and publishing notice.

Related Gear-authored compatibility crate: `substrate/sp-wasm-interface-common` keeps the upstream-compatible local package name `sp-wasm-interface-common` and is published by Gear as `gsp-wasm-interface-common`, but its source files are not copied Polkadot SDK source.

Apache-2.0 license text: <https://www.apache.org/licenses/LICENSE-2.0>
GPL-3.0-or-later WITH Classpath-exception-2.0 license text: [`LICENSE`](LICENSE)
