# Polkadot SDK stable2603-3 Migration Notes

Target SDK revision: `e3737178ec726cffe506c907263aaaa417893fd0`
(`polkadot-stable2603-3`).

This note records the maintainer decisions for the direct migration from the
stable2409-era dependency graph to stable2603-3.

## Release Checklist

The migration scope covers every non-RC stable release note from stable2412
through stable2603-3:

- `stable2412`, `stable2412-1` through `stable2412-11`
- `stable2503`, `stable2503-1` through `stable2503-11`
- `stable2506`, `stable2506-1`, `stable2506-2`, `stable2506-3`, `stable2506-4`,
  `stable2506-5`, `stable2506-6`, `stable2506-7`, `stable2506-9`,
  `stable2506-10`, `stable2506-11`
- `stable2509`, `stable2509-1` through `stable2509-8`
- `stable2512`, `stable2512-1`, `stable2512-2`, `stable2512-3`, `stable2512-5`
- `stable2603`, `stable2603-1`, `stable2603-2`, `stable2603-3`

Standalone-chain impact areas checked in this branch:

- Runtime version field rename from `state_version` to `system_version`
- `SignedExtension` to `TransactionExtension` for custom payment and staking
  blacklist extensions
- Stable2603 transaction pool and BABE proposer APIs
- Stable2603 offchain bare transaction creation API
- FRAME config additions across balances, transaction payment, session,
  scheduler, identity, proxy, multisig, treasury, staking, bags-list,
  nomination pools, and election provider multi-phase
- Runtime API shape changes for block builder lazy blocks and session keys
- Local copied executor/runtime-interface/wasm-builder crate compatibility
- Removal of local `substrate/sc-mixnet` in favor of upstream `sc-mixnet`

## Source Identity

The workspace is pinned to the Polkadot SDK git source:

```text
git+https://github.com/paritytech/polkadot-sdk.git?rev=e3737178ec726cffe506c907263aaaa417893fd0
```

`cargo tree -p gear-cli --all-targets --locked -i sc-mixnet` resolves
`sc-mixnet v0.26.0` from that upstream source only.

## Runtime Semantics

`vara-runtime` intentionally keeps:

```rust
system_version: 1
```

This preserves the pre-stable2603 pending-code runtime-upgrade semantics for
this branch. Do not raise this to `3` or higher without a dedicated runtime
upgrade review and try-runtime evidence.

The runtime migration set remains limited to the existing Gear migrations plus
the stable2603-compatible FRAME migrations whose storage versions are proven by
snapshot or live-state evidence. Unversioned cleanups remain out of scope.

In this branch:

- `pallet_migrations` is wired at runtime pallet index `32` and runs
  `pallet_identity::migration::v2::LazyMigrationV1ToV2<Runtime>` as the
  multi-block identity migration.
- `pallet_child_bounties::migration::MigrateV0ToV1` is wired into the
  single-block runtime migration tuple with a transfer-weight guard based on
  balances transfer weight plus one storage read.
- Staking slashes resolve to Treasury and election rewards are routed through
  the Gear staking-rewards pool instead of being left as no-op handlers.

## Local Fork Matrix

| Local path | Upstream package | stable2603 action |
| --- | --- | --- |
| `substrate/sc-mixnet` | `sc-mixnet` | Deleted; use upstream stable2603-3 |
| `substrate/sp-allocator` | `sc-allocator` | Refreshed for stable2603 compatibility; keep `sp-allocator` package name locally |
| `substrate/runtime-executor` | `sc-executor` | Keep Gear fork for sandbox/lazy-pages compatibility |
| `substrate/runtime-executor/common` | `sc-executor-common` | Refreshed for stable2603 compatibility |
| `substrate/runtime-executor/polkavm` | `sc-executor-polkavm` | Refreshed for stable2603 compatibility |
| `substrate/runtime-executor/wasmtime` | `sc-executor-wasmtime` | Refreshed for stable2603 compatibility |
| `substrate/sp-runtime-interface-proc-macro` | `sp-runtime-interface-proc-macro` | Refreshed with Gear runtime-interface compatibility |
| `substrate/sp-wasm-interface` | `sp-wasm-interface` | Keep compatibility-only fork |
| `substrate/sp-wasm-interface-common` | local compatibility crate | Keep compatibility-only fork |
| `substrate/substrate-wasm-builder` | `substrate-wasm-builder` | Refreshed for stable2603 compatibility |

The local diff against `e3737178ec726cffe506c907263aaaa417893fd0` was
re-audited after the ethexe Malachite restore. The remaining `substrate/`
source deltas are intentional Gear compatibility changes: allocator package
renaming and shared wasm-interface types, static Wasmtime host-function
registration for the current `wasmtime` API, Gear executor host-state and
memory-wrapper plumbing, `get_global_const` support, wasm32v1/RISC-V builder
handling, and omission of upstream-only benches/runtime-test fixtures. The
manual ethexe host registration for
`ext_gear_ri_pre_process_memory_accesses_version_2` was updated separately to
match stable2603's `PassFatPointerAndReadWrite<&mut [u8]>` FFI shape.

## Deferred Items

- Evaluate `system_version >= 3` pending-code runtime-upgrade semantics.
- ethexe Malachite uses upstream Circle Malachite at
  `circlefin/malachite` commit
  `1fe7961aca933cefad8e4d9a52f50eda565288e7`. Substrate stable2603 may keep
  its own libp2p line, while ethexe networking and Malachite stay aligned with
  upstream Malachite on libp2p 0.56.
- Port prior Malachite hardening work after the stable2603 dependency update:
  validator identity/peer gating, bounded stream cleanup, injected transaction
  caps and duplicate checks, quarantine liveness, and typed-hash audit items.
- Run try-runtime against production and development snapshots before shipping
  the wired identity and child-bounties runtime migrations.
