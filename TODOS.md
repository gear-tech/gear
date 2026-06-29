# TODOs

## Polkadot SDK stable2603 Follow-Ups

- Evaluate stable2603 `system_version >= 3` pending-code runtime-upgrade
  semantics in a dedicated branch. This migration intentionally keeps
  `system_version = 1`.
- Run try-runtime against production and development snapshots for the wired
  identity lazy migration and child-bounties v0-to-v1 migration, confirming
  storage versions, cardinality, and weight bounds.
- Port the prior ethexe Malachite hardening items on top of the stable2603
  dependency graph: validator identity/peer gating, bounded stream cleanup,
  injected transaction caps and duplicate checks, quarantine liveness, and
  typed-hash audit items.
