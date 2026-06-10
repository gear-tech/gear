# Third-Party Notices

## Polkadot SDK Copied Sources

Some local crates contain copied or modified source files from `paritytech/polkadot-sdk`
at <https://github.com/paritytech/polkadot-sdk/tree/e3737178ec726cffe506c907263aaaa417893fd0>:

- `substrate/sp-allocator` (`sp-allocator`, published by Gear as `gsp-allocator`; derived from upstream `sc-allocator`)
- `substrate/runtime-executor/common` (`sc-executor-common`, published by Gear as `gsc-executor-common`)
- `substrate/runtime-executor/polkavm` (`sc-executor-polkavm`, published by Gear as `gsc-executor-polkavm`)
- `substrate/runtime-executor/wasmtime` (`sc-executor-wasmtime`, published by Gear as `gsc-executor-wasmtime`)
- `substrate/runtime-executor` (`sc-executor`, not published by Gear)
- `substrate/substrate-wasm-builder` (`substrate-wasm-builder`, published by Gear as `gsubstrate-wasm-builder`)

`substrate/sp-allocator` keeps the historical local package name for dependency
compatibility, while its source is refreshed from upstream
`substrate/client/allocator`.

These copied source files retain the upstream copyright notices and their original SPDX headers. See [`substrate/README.md`](substrate/README.md) for the shared fork, provenance, and publishing notice.

Related Gear-authored compatibility crate: `substrate/sp-wasm-interface-common` keeps the upstream-compatible local package name `sp-wasm-interface-common` and is published by Gear as `gsp-wasm-interface-common`, but its source files are not copied Polkadot SDK source.

`substrate/sc-mixnet` was removed during the stable2603-3 migration; Gear now
uses upstream `sc-mixnet` from the Polkadot SDK source reference above.

Additional third-party file: `substrate/runtime-executor/wasmtime/src/test-guard-page-skip.wat` is a modified WebAssembly testsuite fixture from <https://github.com/WebAssembly/testsuite/blob/01efde81028c5b0d099eb836645a2dc5e7755449/skip-stack-guard-page.wast>, licensed under Apache-2.0.

## libp2p Copied Source

`third-party/libp2p-swarm-0.45.1` is copied from the crates.io
`libp2p-swarm` 0.45.1 package, originally published from
<https://github.com/libp2p/rust-libp2p>. Gear patches only its
`libp2p-swarm-derive` dependency to `=0.35.1` so the stable2603 Substrate
networking stack can coexist with ethexe's upstream libp2p 0.56 dependency
line. `libp2p-swarm` declares the MIT license in its package manifest.

Apache-2.0 license text: <https://www.apache.org/licenses/LICENSE-2.0>
MIT license text: <https://opensource.org/license/mit>
GPL-3.0-or-later WITH Classpath-exception-2.0 license text: [`LICENSE`](LICENSE)
