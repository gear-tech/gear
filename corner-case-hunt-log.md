# Corner-case vulnerability hunt log

Branch: `gsobol/ethexe/corner-case-hunt` (off `gsobol/ethexe/malachite-new`).
Goal: find latent vulnerabilities / corner-case bugs in the ethexe-malachite
layer through targeted unit tests. Each iteration: invent one hypothesis,
write a test, run it. If the test PASSES (no bug reproduces) — delete the
test. If it FAILS (bug suspected) — verify the test is correct and KEEP it
(marked `#[ignore]`) as a bug record.

## Skip list — already known / fixed / tracked

Do NOT re-test these areas. They are pinned in memory
`ethexe-malachite-pending-fixes.md`.

### Fixed (do not re-test)

| Area | Fix commit |
|---|---|
| `app.rs:115-149` StartedRound remove-before-validate | `f3c5639a1` |
| `app.rs:handle_app_msg` `?`-propagation kills app task | `cacf41ac1` |
| `app.rs:process_finalized` partial-finalize drift | `0ef199abd`, `cc3f4e3c6`, `e81a572c0` |
| `app.rs:process_received_proposal_part` future-height unbounded buffer | `42a0d6024` (FUTURE_HEIGHT_WINDOW = 4) |
| `externalities.rs:validate_block_above` quarantine-poll | `6d302a7a0` (post_quarantine_delay) |
| `externalities.rs:validate_block_above` missing strict-descendant | `1052391fa` |
| `mempool.rs:purge_expired` unresolved ref_block DoS | `d52c62e01` |
| `codec.rs:From<RawProposedValue>` Round::Nil aliasing | `503a3d43d` (TryFrom) |

### Known-open follow-ups (tracked as GitHub issues — do NOT add new tests for these)

| Issue | Area |
|---|---|
| #5473 | `PartStreamsMap` unbounded growth + caps |
| #5474 | Mempool per-signer quota |
| #5475 | Per-peer rate limit on `process_received_proposal_part` |
| #5476 | `ProposalFin` signature check before buffering future-height parts |
| #5477 | Shared helper for producer/validator EB-advance |
| #5478 | Upper-bound validation on `post_quarantine_delay` |
| #5479 | Metrics for `validate_block_above` abstains |
| #5480 | Validator peer-id allowlist |
| #5481 | Multi-validator integration test for `post_quarantine_delay` lagging observer |
| #5482 | Misc polish: chain_head==None test + TryFrom round-bound test + mempool insert doc |

## Iteration history

Format: each entry is one row in the table below. Add new entries APPEND-ONLY
(newest at bottom).

| # | UTC timestamp | Hypothesis | Area / file | Test name | Outcome | Notes |
|---|---|---|---|---|---|---|
| 0 | 2026-05-20T21:00:00Z | seed | — | — | — | log initialized |
