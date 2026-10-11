# Gear-maintained Polkadot SDK crates

This directory contains selected Polkadot SDK crates copied into the Gear workspace from Polkadot SDK `stable2409`, source reference [`298f676c91d64f15f38ea7fd78f125c5889ab09c`](https://github.com/paritytech/polkadot-sdk/tree/298f676c91d64f15f38ea7fd78f125c5889ab09c), plus Gear-local compatibility crates needed to isolate the remaining fork delta.

Copied crates are modified under the terms of their upstream open-source licenses. Original SPDX headers and upstream copyright notices remain in the copied source files; original copyright ownership remains with the upstream rightsholders as indicated there, including Parity Technologies where present. Gear maintains local changes to isolate the remaining fork delta while the rest of the workspace depends on upstream Polkadot SDK.
The license-header check maps the native ownership primitive/test-runtime paths
to Apache-2.0. Imported RPC files use the repository's minimal SPDX header format,
preserving their GPL classpath exception and original copyright attribution.

Local Cargo package names intentionally stay compatible with upstream package names so `[patch]` can replace Polkadot SDK git dependencies. When these crates are prepared for crates.io, Gear publishes them under `g*` aliases for Gear ecosystem packages.

## Copied Polkadot SDK Crates

| Local path | Upstream package | Gear publish name | License |
| --- | --- | --- | --- |
| `substrate/sp-allocator` | `sp-allocator`; derived from upstream `sc-allocator` | `gsp-allocator` | Apache-2.0 |
| `substrate/sp-wasm-interface` | `sp-wasm-interface` | `gsp-wasm-interface` | Apache-2.0 |
| `substrate/runtime-executor/common` | `sc-executor-common` | `gsc-executor-common` | GPL-3.0-or-later WITH Classpath-exception-2.0 |
| `substrate/runtime-executor/polkavm` | `sc-executor-polkavm` | `gsc-executor-polkavm` | GPL-3.0-or-later WITH Classpath-exception-2.0 |
| `substrate/runtime-executor/wasmtime` | `sc-executor-wasmtime` | `gsc-executor-wasmtime` | GPL-3.0-or-later WITH Classpath-exception-2.0 |
| `substrate/runtime-executor` | `sc-executor` | not published by Gear | GPL-3.0-or-later WITH Classpath-exception-2.0 |
| `substrate/cli` | `sc-cli` | not published by Gear | GPL-3.0-or-later WITH Classpath-exception-2.0 |
| `substrate/sc-consensus-beefy` | `sc-consensus-beefy` | not published by Gear | GPL-3.0-or-later WITH Classpath-exception-2.0 |
| `substrate/pallet-beefy` | `pallet-beefy` | not published by Gear | Apache-2.0 |
| `substrate/pallet-session` | `pallet-session` | not published by Gear | Apache-2.0 |
| `substrate/sp-application-crypto` | `sp-application-crypto` 38.0.0 | not published by Gear | Apache-2.0 |
| `substrate/sp-runtime` | `sp-runtime` 39.0.5 | not published by Gear | Apache-2.0 |
| `substrate/sp-session` | `sp-session` 36.0.0 | not published by Gear | Apache-2.0 |
| `substrate/rpc-api` | `sc-rpc-api` 0.44.0 | not published by Gear | GPL-3.0-or-later WITH Classpath-exception-2.0 |
| `substrate/rpc` | `sc-rpc` 40.0.0 | not published by Gear | GPL-3.0-or-later WITH Classpath-exception-2.0 |
| `substrate/test-runtime` | `substrate-test-runtime` 2.0.0 | not published by Gear | Apache-2.0 |
| `substrate/rpc-servers` | `sc-rpc-server` | not published by Gear | GPL-3.0-or-later WITH Classpath-exception-2.0 |
| `substrate/service` | `sc-service` | not published by Gear | GPL-3.0-or-later WITH Classpath-exception-2.0 |
| `substrate/substrate-wasm-builder` | `substrate-wasm-builder` | `gsubstrate-wasm-builder` | Apache-2.0 |

The local BEEFY client backports [Polkadot SDK #12812](https://github.com/paritytech/polkadot-sdk/pull/12812) so malformed justification requests are rejected without terminating the handler, including a nonzero penalty for empty requests.

The BEEFY pallet also backports [Polkadot SDK #11816](https://github.com/paritytech/polkadot-sdk/pull/11816), commit `71da30286be32e2368b4d948b5febd80c0b6a92d`: unsigned future-block voting reports convert to equivocation evidence before validation. Regression coverage exercises local/in-block acceptance, external rejection, dispatch and duplicate rejection.

Gear also carries raw-buffer gossip rebroadcast fixes and signed-proof/MMR regression coverage. Peer progress remains an untrusted discovery hint; returned proofs are verified against the requested round and authority set. Advancing hints wake historical-proof requests waiting for an eligible peer without an unrelated finality event. Buffered mandatory proofs drain across successive sessions while progress is possible. Cached-round proofs receive no reputation reward because their signatures are not rechecked.

After exhausting failed requests, the engine stays `Idle`, matching both the pinned SDK and [upstream at `05ddbee5c062`](https://github.com/paritytech/polkadot-sdk/blob/05ddbee5c062d49c8a2a072c529a2fc80aad243e/substrate/client/consensus/beefy/src/communication/request_response/outgoing_requests_engine.rs). The worker can retry a mandatory proof after a timeout, canceled response or invalid proof without requiring a newer peer progress hint; an in-flight request is not restarted. This restores upstream behavior rather than adding a new backport.

Offline `import-blocks` authenticates File-origin BEEFY proofs against imported post-state and retains them by block hash in auxiliary storage. Exports include that archive without treating it as GRANDPA finality; live serving and voter recovery use it only on canonical GRANDPA-finalized ancestry. This Gear-local fix is not an SDK upgrade: the pinned SDK and upstream `05ddbee5c062` still route imported proofs solely to the live voter, which offline commands do not run. Recovery requires replay into a fresh archive database; see the [offline recovery procedure](../vara/node/README.md#offline-beefy-proof-recovery).

Restart initialization reads the current finalized state before waiting for another finality notification. Recovery replays the exact persisted mandatory vote without re-signing, stops header catch-up at BEEFY genesis, and restores the finalized RPC head. These changes do not alter the persisted SCALE schema. The remaining SDK stays pinned to the source reference above.

The six native ownership patches backport the SessionKeys v2 and
`author_rotateKeysWithOwner` protocol from [Polkadot SDK #1739](https://github.com/paritytech/polkadot-sdk/pull/1739),
released at `db46f6f939f68b8b84ddd531e77dbe8771dc4a73`, onto the pinned source
without changing its package versions or editions. Ownership signs `POP_ || owner`
with each application key; ECDSA possession rejects noncanonical high-S signatures
without changing other ECDSA verification. Experimental aggregate schemes are
unsupported. The RPC uses the configured native keystore; the test-runtime patch
keeps the existing pinned test client coherent. No keystore, core, I/O or client
fork is introduced.
Native ECDSA possession uses the pinned SDK host functions
`ext_crypto_ecdsa_sign_version_1` and `ext_crypto_ecdsa_verify_version_2`;
the runtime-import allowlist includes both, without adding new host APIs.
The SDK test runtime is a dependency-only workspace exclusion: its optional
`disable-logging` mode must not leak into Gear through workspace `--all-features`.
Its manifest still inherits the pinned workspace dependencies explicitly.
The ownership crates retain the pinned SDK lint policy while remaining workspace
members for their dev-dependency tests; compiler-compatibility fixes preserve the
upstream public APIs.
The RPC backport omits the unused upstream `sc-network-common` dev-dependency;
production dependencies and APIs are unchanged.

The local BEEFY pallet adds a configurable argument-aware activation origin and
validation weight, avoiding a root-dispatch call-filter bypass. The local session
pallet validates every native ownership proof against the effective signed
account before registration, preserves ordinary validator/stash conversion and
key ownership accounting, and marks queued sets changed when a purged validator
shortens them. No custom registration protocol or persistent proof ledger remains.

For operators, [node installation](../vara/node/README.md#beefy-upgrade-and-later-activation)
precedes the runtime upgrade: new-node API v1 fallback supports only the old runtime,
not old nodes running the changed v2 ABI. SessionKeys API ID is
`0xab3c0572291feb8b` (the pinned macro's Blake2b-64 hash of `SessionKeys`).
Native generation uses the configured persistent keystore and seed None; its
password affects derivation, not file encryption. Five-key generation is
nontransactional, so a failed proof/signing request may leave unused private keys.

Source activation allows actual active/queued committees up to **1000** independently of
destination binding; bound actual/queued and desired capacity is **256**. Cutover requires
separate approval; readiness guards do not automatically pause/reset state or guarantee signing.
Follow the [operator contract](../beefy-deployment.md) and [qualification gates](../beefy-migration.md)
for election, admission, cutover and evidence; superseded custom-contract counts do not qualify release.

## Gear Compatibility Crates

| Local path | Upstream-compatible package name | Gear publish name | License |
| --- | --- | --- | --- |
| `substrate/sp-wasm-interface-common` | `sp-wasm-interface-common` | `gsp-wasm-interface-common` | Apache-2.0 |

`substrate/sp-wasm-interface-common` is Gear-authored compatibility code, not copied upstream source. It keeps the upstream-compatible local package name so Gear can patch dependencies that previously resolved through the custom Polkadot SDK fork.

Publishing is handled by Gear maintainers through `utils/crates-io`; this README only documents the fork and naming policy.
