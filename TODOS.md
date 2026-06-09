# TODOs

## Polkadot SDK stable2603 Follow-Ups

- Evaluate stable2603 `system_version >= 3` pending-code runtime-upgrade
  semantics in a dedicated branch. This migration intentionally keeps
  `system_version = 1`.
- Run try-runtime against production and development snapshots for the wired
  identity lazy migration and child-bounties v0-to-v1 migration, confirming
  storage versions, cardinality, and weight bounds.
- Revisit ethexe Malachite once its `libp2p` dependency can coexist with the
  stable2603 Polkadot SDK graph. The current blocker is the exact
  `libp2p-swarm-derive` conflict between stable2603 `sc-network` and
  Malachite's `libp2p` stack.
