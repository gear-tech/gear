# Third-Party Notices

## Polkadot SDK Copied Sources

Some local crates contain copied or modified source files from `paritytech/polkadot-sdk`:

- `substrate/sp-allocator` (`sp-allocator`, published by Gear as `gsp-allocator`; derived from upstream `sc-allocator` through the Gear Polkadot SDK fork)
- `substrate/runtime-executor/common` (`sc-executor-common`, published by Gear as `gsc-executor-common`)
- `substrate/runtime-executor/polkavm` (`sc-executor-polkavm`, published by Gear as `gsc-executor-polkavm`)
- `substrate/runtime-executor/wasmtime` (`sc-executor-wasmtime`, published by Gear as `gsc-executor-wasmtime`)
- `substrate/runtime-executor` (`sc-executor`, not published by Gear)
- `substrate/substrate-wasm-builder` (`substrate-wasm-builder`, published by Gear as `gsubstrate-wasm-builder`)

Source reference: <https://github.com/paritytech/polkadot-sdk/tree/298f676c91d64f15f38ea7fd78f125c5889ab09c>

Current migration source reference: <https://github.com/paritytech/polkadot-sdk/tree/e3737178ec726cffe506c907263aaaa417893fd0>

`substrate/sp-allocator` was sourced from the Gear Polkadot SDK fork `gear-polkadot-stable2409-wasm32v1-none` at `1d1b394647eb26c094cf50c759b900dc5faa3b80`, derived from Parity Polkadot SDK `sc-allocator`.

These copied source files retain the upstream copyright notices and their original SPDX headers. See [`substrate/README.md`](substrate/README.md) for the shared fork, provenance, and publishing notice.

Related Gear-authored compatibility crate: `substrate/sp-wasm-interface-common` keeps the upstream-compatible local package name `sp-wasm-interface-common` and is published by Gear as `gsp-wasm-interface-common`, but its source files are not copied Polkadot SDK source.

`substrate/sc-mixnet` was removed during the stable2603-3 migration; Gear now
uses upstream `sc-mixnet` from the Polkadot SDK source reference above.

Additional third-party file: `substrate/runtime-executor/wasmtime/src/test-guard-page-skip.wat` is a modified WebAssembly testsuite fixture from <https://github.com/WebAssembly/testsuite/blob/01efde81028c5b0d099eb836645a2dc5e7755449/skip-stack-guard-page.wast>, licensed under Apache-2.0.

Apache-2.0 license text: <https://www.apache.org/licenses/LICENSE-2.0>
GPL-3.0-or-later WITH Classpath-exception-2.0 license text: [`LICENSE`](LICENSE)
