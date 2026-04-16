# Ethexe Consensus Edge Case Checklist

Use this checklist when developing new features or protocol changes that touch the validator state machine, announce processing, or network gossip.

Validated by 3 independent AI models (Claude structured, Claude adversarial, Codex gpt-5.4). Last updated 2026-04-16.

## Network Event Ordering (Gossip Reordering)

- [ ] **Child announce arrives before parent** — Gossipsub doesn't guarantee order. If announce B (parent=A) arrives before A, the subordinate must defer B to pending (not reject). Rejection permanently loses the announce since gossipsub won't redeliver. *Depth-2: fixed via WaitingForAnnounce guard (is_announce_included check). Depth-1: N/A, only 1 announce per block, parent always from prev block.*
- [ ] **VR arrives before announce** — The VR's `head` hash may reference an announce the subordinate hasn't received yet. Must defer VR (save to pending), not reject. Handled by `head_computed` check + pending replay. *Applies to both depth-1 and depth-2.*
- [ ] **VR arrives between base and mini** — *Depth-2 only.* Base is computed, mini hasn't arrived. VR.head = mini_hash. Must defer until mini computes. Handled by `announce_meta(h).computed` check. *Depth-1: N/A.*
- [ ] **Announces from stale/wrong block** — `block_hash` mismatch → falls to DefaultProcessing → saved to pending. *Applies to both.*
- [ ] **Announces from wrong producer** — Address mismatch → DefaultProcessing → saved or dropped. *Applies to both.*
- [ ] **Duplicate announce delivery** — `accept_announce` checks `newly_included`. Duplicate returns `AlreadyIncluded`. *Applies to both.*

## Gossip Loss (Message Never Arrives)

- [ ] **Announce lost** — Subordinate stuck in WaitingForAnnounce until next ETH block (12s max). VR is deferred. Block has reduced validator participation. Recovers on next block. *Applies to both. Depth-2: squashed announce loss leaves subordinate in ReadyForMoreAnnounces.*
- [ ] **Base announce lost (depth-2 only)** — Subordinate stays in WaitingForAnnounce. Mini (if it arrives) is deferred to pending (parent unknown). Both lost if base never comes. Recovers on next block.
- [ ] **VR lost** — Subordinate never transitions to Participant. OK, batch commitment proceeds with fewer signatures if threshold is met. *Applies to both.*
- [ ] **All gossip lost** — Subordinate sits idle. Recovers on next ETH block. No permanent damage. *Applies to both.*

## Producer Lifecycle Edge Cases

- [ ] **Producer crash after base gossip, before mini (depth-2)** — Subordinate has base, no mini, no VR. Waits until next block. *Depth-1: N/A.*
- [ ] **Producer crash after canonical compute, before announce (depth-1)** — Ephemeral canonical state lost. TXs stay in pool. Next block restarts cleanly. No orphaned DB state.
- [ ] **Producer crash during announce compute** — Announce in DB uncomputed. TXs removed from in-memory pool. Pool is volatile by design (repopulated from network gossip on restart). Pre-existing issue for any announce type. *Applies to both.*
- [ ] **New ETH head during ReadyForMiniAnnounce (depth-2)** — process_new_head → AggregateBatchCommitment with last_announce_hash. Saves next_block. *Depth-1: N/A.*
- [ ] **New ETH head during ReadyForTxCollection (depth-1)** — Build announce with collected TXs immediately (if any), or AggregateBatchCommitment with no announce. Save next_block.
- [ ] **New ETH head during WaitingCanonicalComputed (depth-1)** — DefaultProcessing → Initial. Canonical result discarded (ephemeral). TXs stay in pool.
- [ ] **New ETH head during WaitingAnnounceComputed** — Falls to DefaultProcessing → Initial. Batch commitment for this block skipped. Announces recovered by next block's `collect_not_committed_predecessors`. Pre-existing, TODO exists. *Applies to both.*
- [ ] **Compute event lost (service crash)** — Any WaitingComputed state escapes to Initial on next new_head. Announce (if written to DB) recovered by next block. Pool TXs lost (volatile). *Applies to both.*
- [ ] **Second new head during Coordinator** — Coordinator gives up and transitions to Initial. Batch for that block lost. codes_queue, validator/reward commitments for that specific block can be missed. Pre-existing, confirmed by all 3 models. *Applies to both.*
- [ ] **Batch commitment loss is permanent** — Unlike announce chains (recovered by `collect_not_committed_predecessors`), per-block code/validator/reward commitments have no recovery path. Confirmed by all 3 models. Pre-existing issue widened by time spent in compute states. *Applies to both.*

## TX Pool Edge Cases

- [ ] **producer_delay=0** — *Depth-2 (cap-at-1):* tick 1 produces mini, tick 2 batches. Max 1 mini. *Depth-1:* canonical compute fires immediately, poll fires immediately, selects all TXs, builds announce. 1 announce, no loop. Both fixed.
- [ ] **TXs arrive during canonical compute (depth-1)** — Stay in pool. Selected after canonical computes (in ReadyForTxCollection). Not lost.
- [ ] **TXs arrive during announce compute** — Stay in pool. Picked up next block. Not lost. *Applies to both.*
- [ ] **TXs targeting programs created by current block** — *Depth-2:* These programs exist after base computes. `select_for_announce(block, base_hash)` validates against post-base state. Works. *Depth-1 Option A (pre-canonical):* REJECTED as UnknownDestination. Wait 1 block. *Depth-1 Option B (two-phase):* WORKS — canonical compute establishes program state before TX selection.
- [ ] **Cumulative TX size/program limits** — *Depth-2:* Accumulated TXs stay in pool, re-counted by select_for_announce each tick. Dedup filter strips. *Depth-1:* Single select call. Natural limits.
- [ ] **Base announce already includes TXs** — `collect_recent_included_txs` marks them as Duplicate in subsequent calls. No double-counting. *Depth-2 only (depth-1 has no base/mini split).*
- [ ] **Pool volatile on crash** — `InjectedTxPool::new()` starts empty. No restart scan from DB. TXs lost on crash repopulated from network gossip. By design, confirmed by Codex. *Applies to both.*
- [ ] **select_for_announce iteration order** — Pool uses HashSet (unordered). Size/program limits applied in iteration order. Non-deterministic which TXs are included when pool exceeds limits. Deterministic announce hash requires deterministic TX selection. *Applies to both. Verify HashSet iteration is consistent within a single call.*

## Subordinate State Machine Edge Cases

- [ ] **Multiple announces in pending (depth-2)** — `replay_pending_events` processes oldest-first (reverses the deque). Base must be processed before mini. *Depth-1: simpler — only 1 announce + VR in pending.*
- [ ] **replay_pending_events state escape** — If an event causes transition to Initial (e.g., rejected announce), remaining pending events are processed by Initial's handlers, not Subordinate's. Harmless (events saved or dropped by DefaultProcessing) but logically imprecise. *Applies to both (depth-2 has more pending events).*
- [ ] **Pending queue unbounded after create** — `MAX_PENDING_EVENTS=10` only enforced in `Subordinate::create()`. After creation, `pending()` has no cap. Byzantine producer can flood orphan announces. Mitigated by gossipsub signature validation (only real validators send announces) and 12s block reset. PRE-EXISTING: TODO #4641. *Depth-1: less exposure (no mini-announce flood vector).*
- [ ] **Non-validator receives VR** — *Depth-2:* Dropped silently in ReadyForMoreAnnounces to avoid recycle loop. *Depth-1:* Standard DefaultProcessing handles it.
- [ ] **Non-validator stuck in ReadyForMoreAnnounces (depth-2)** — Computes every mini-announce but has no validation role. Exits only on next new_head. Unnecessary work but not harmful. *Depth-1: N/A — no ReadyForMoreAnnounces state.*

## Announce Chain Invariants

- [ ] **Depth per block** — *Depth-2 (cap-at-1):* max 2 (base + 1 mini). *Depth-1:* exactly 1. Always.
- [ ] **CDL counting** — *Depth-2:* Must count BLOCK transitions, not announce hops. Block-aware CDL in 5 functions. *Depth-1:* Hop counting = block counting. No patches needed. Revert to simple loops. KEEP the off-by-one fix (`>` vs `>=`).
- [ ] **CDL safety bound** — `propagate_one_base_announce` has `blocks_seen > CDL*2` guard. If hit, breaks and propagates anyway. May be too permissive. *Depth-1: revert to simple loop, this guard becomes unnecessary.*
- [ ] **is_same_block** — *Depth-2:* Compute must skip canonical events for same-block announces. Mini's parent is base (same block). *Depth-1:* N/A — parent always from different block. Remove check.
- [ ] **leaf_announces** — *Depth-2:* Filters announce set to chain tips. Returns mini (not base) for {base, mini} blocks. *Depth-1:* N/A — 1 announce per block, always the leaf. Remove function.
- [ ] **best_parent_announce** — *Depth-2:* Prefers base announces for parent selection. Falls back to leaf_announces. *Depth-1:* Simple — no base-filter needed, all announces are "base."
- [ ] **gas_allowance in mini/squashed announce** — Set to full `block_gas_limit`. Base already consumed gas for canonical events. Compute layer handles gas budget per-announce, not cumulative. *Depth-1: N/A — single announce gets full budget.*

## Consensus Safety

- [ ] **Deterministic announce hashes** — Producer and subordinate must produce identical announce hashes for the same content. Announce hashed deterministically from fields. *Applies to both.*
- [ ] **Canonical compute determinism (depth-1 specific)** — NEW RISK. Two-phase compute must produce identical ProgramStates on all validators. Since it uses the same inputs (block events + parent state) and the same compute layer, it should be deterministic. But this is a new code path. Test with multiple validators.
- [ ] **Ephemeral canonical state (depth-1 specific)** — The canonical compute result is NOT stored in DB. If the producer crashes between phases, it's lost. No orphaned state. TXs stay in pool. Clean restart.
- [ ] **Batch commitment integrity** — `create_batch_commitment` uses the latest announce hash. *Depth-2:* squashed/mini hash. *Depth-1:* the single announce hash. Same mechanism.
- [ ] **Signature threshold** — Batch requires N-of-M signatures. Reduced participation (lost gossip) may prevent threshold. System waits and retries next block. *Applies to both.*
- [ ] **Era boundaries** — Announce era_index comes from block timestamp. Ensure era transitions don't split a block's announces across eras. *Depth-2: could happen if base and mini span an era boundary. Depth-1: N/A — single announce, single era.*

## Deployment & Config

- [ ] **genesis-state-dump removal** — `NodeParams` uses `deny_unknown_fields`. Any config file with the removed `genesis-state-dump` field will fail to parse on upgrade. From master merge, not mini-announces branch. *Applies to both.*
- [ ] **producer_delay config** — Production uses `Duration::ZERO`. *Depth-2 (cap-at-1):* safe (1 mini max). *Depth-1:* safe (poll fires immediately, selects all TXs, builds 1 announce).

## Depth-1 Specific: Two-Phase Compute

- [ ] **Canonical compute returns wrong states** — Would cause TX validation against incorrect state. TXs might be incorrectly included or excluded. Test: verify ProgramStates match what a full announce compute would produce.
- [ ] **Canonical compute takes too long** — New head arrives during WaitingCanonicalComputed. DefaultProcessing → Initial. Block's TXs deferred. Acceptable (same as current WaitingAnnounceComputed behavior).
- [ ] **TX pool changes between phases** — TXs may arrive or become invalid between canonical compute and TX selection. Not a problem — select_for_announce re-validates each time.
- [ ] **Two compute calls per block** — Performance concern. Phase 1 (canonical only) is lightweight (~100ms, few events). Phase 2 (full announce) processes TXs. Total ~500ms. Current depth-2 also does 2 computes (base + mini). No regression.
- [ ] **Canonical compute + announce compute must produce same final state** — The announce includes the same canonical events that were pre-computed. The compute layer processes them again during ComputeAnnounce. The final state must be identical. This is guaranteed if compute is deterministic (same inputs → same outputs). Test explicitly.
- [ ] **select_for_announce_with_states correctness** — New code path using provided ProgramStates instead of DB lookup. Must produce identical results to select_for_announce(block, announce_hash) when given the same states. Unit test this equivalence.

## Findings From Multi-Model Review (carry forward to depth-1)

These were found by Claude structured review, Claude adversarial, and Codex (gpt-5.4) during the depth-2 PR review. Issues marked "pre-existing" affect depth-1 equally.

| Finding | Source | Severity | Depth-1 Status |
|---------|--------|----------|----------------|
| Batch commitment loss on new_head during compute | All 3 models | HIGH | Pre-existing. Same risk. |
| Pending queue unbounded after create (TODO #4641) | All 3 models | MEDIUM | Less exposure (no mini flood) |
| Pool volatile, TXs lost on crash | Codex | HIGH | Same. By design. |
| Coordinator second new_head kills batch | Codex | HIGH | Same. Pre-existing. |
| replay_pending_events state escape | Claude adversarial | MEDIUM | Simpler (fewer pending events) |
| Non-validator stuck computing | Claude adversarial | MEDIUM | Eliminated (no ReadyForMoreAnnounces) |
| CDL safety bound too permissive | Claude adversarial | LOW | Eliminated (no CDL patches) |
| gas_allowance fresh budget for mini | Claude adversarial | LOW | Eliminated (single announce) |
| genesis-state-dump config break | Codex | MEDIUM | Same. From master merge. |
