# TODOS

## Ethexe: Guard batch commitment during mini-announce computation

**What:** When a new ETH block arrives while the producer is in `WaitingAnnounceComputed` (computing a mini-announce), `DefaultProcessing::new_head` transitions to Initial without creating a batch commitment for the current block. Block N's batch is lost and must be recovered by block N+1's `collect_not_committed_predecessors`.

**Why:** Mini-announces increase the time spent in `WaitingAnnounceComputed` (one cycle per mini-announce vs one per block before), making this window more likely to be hit. The batch commitment for block N is only triggered from `ReadyForMiniAnnounce` → `process_new_head`, so a new head arriving mid-computation skips it.

**Impact:** Low in practice. `create_batch_commitment` is mostly DB reads (~100ms). The 12s ETH block interval makes collision unlikely. If it does happen, block N's announces are still in the DB and will be included in the next block's batch via `collect_not_committed_predecessors`. Only validators/rewards/code commitments queued specifically for block N could be missed.

**Fix options:**
- Override `process_new_head` for `WaitingAnnounceComputed` to also trigger `AggregateBatchCommitment` with `next_block` saved (same pattern as `ReadyForMiniAnnounce`)
- Or: accept the current behavior since `collect_not_committed_predecessors` provides recovery

**Files:** `ethexe/consensus/src/validator/producer.rs:154-172`

**Origin:** Flagged by Gemini, Claude bot, and Codex in PR #5321 review. Pre-existing behavior, not introduced by mini-announces, but window is wider now.

## ~~Ethexe: Squash mini-announce chains into single announce in DB~~ DONE

Implemented as producer-only squash: accumulated TXs in-memory, single squashed announce written to DB. Depth capped at 2 (base + squashed). No subordinate changes needed. CDL patches and `is_same_block` kept as correct safety nets for depth-2 chains.

## Ethexe: Depth-1 optimization — eliminate base announce

**What:** Currently blocks have `{base, squashed}` (depth 2). True depth-1 would mean a single announce per block containing both canonical events and injected TXs. This would allow reverting block-aware CDL patches and `leaf_announces`.

**Why:** With depth-1, hop-counting = block-counting, so CDL simplification becomes safe. About ~80 lines of block-aware counting code could be removed.

**Blocker:** TX validation requires the base to be computed first — `select_for_announce` validates TXs against the state at the parent announce. Without a computed base, TXs targeting programs created by canonical events (current block) would be incorrectly rejected as "unknown destination."

**Fix options:**
- Two-phase compute: compute canonical events first (without an announce), then select TXs, then build single announce
- Accept the limitation: TXs targeting current-block programs are deferred to next block

**Priority:** P3 — the depth-2 solution works well. This is a nice-to-have simplification.

**Depends on:** Squash landing and proving stable in production.

**Files:** `ethexe/consensus/src/validator/producer.rs`, `ethexe/consensus/src/announces.rs`, `ethexe/compute/src/compute.rs`

**Origin:** CEO review of squash plan, 2026-04-15.

## ~~Ethexe: producer_delay=0 causes tight mini-announce loop~~ MITIGATED

With squashing, `delay=0` means "accumulate on first tick, squash on second tick." No more compute-per-mini cycle. The producer does at most 2 compute calls per block regardless of delay setting. The busy spin is eliminated.
