# Depth-1 Mini-Announces: Two-Phase Compute

## Context

Mini-announces deliver ~400ms promise latency for injected TXs (down from avg ~6.4s). The current implementation (branch `vs/mini-announces`) creates depth-2 announce chains (base + 1 mini), requiring:
- Block-aware CDL counting in 7 functions (~327 lines)
- leaf_announces chain-tip filter
- is_same_block canonical event skip in compute
- ReadyForMoreAnnounces + VR deferral in subordinate
- replay_pending_events for gossip ordering
- Total: ~1000 lines across 9 files

**Depth-1 eliminates 93% of this code** by producing exactly ONE announce per block containing both canonical events and injected TXs.

## Problem

TX validation requires knowing which programs exist and their state. Canonical events (Mirror::sendMessage, Router program creation) change this state. Without computing canonical events first, `select_for_announce` validates TXs against the parent block's state, missing programs created in the current block.

## Solution: Two-Phase Compute

```
Block N arrives → Delay (producer_delay)
     │
     ▼
Phase 1: ComputeCanonicalEvents(block_hash)
  → Compute layer processes canonical events only (no TXs)
  → Returns ProgramStates (post-canonical state)
     │
     ▼
Phase 2: WaitingCanonicalComputed
  → Poll timer: select TXs against post-canonical ProgramStates
  → When pool empty or timeout:
      Build ONE announce (canonical events + all TXs)
      → Write to DB, gossip, ComputeAnnounce
      → WaitingAnnounceComputed → AggregateBatchCommitment → batch
```

Depth: exactly 1. Always. No chains. No CDL patches.

## Architecture Diagram

```
Producer State Machine:
  Delay → timer fires
    → ComputeCanonicalEvents(block_hash) → WaitingCanonicalComputed
    → canonical computed, ProgramStates returned
    → ReadyForTxCollection { program_states, poll_timer }
    → poll timer fires:
        TXs found → collect, restart timer
        Pool empty → build announce → WaitingAnnounceComputed
    → announce computed → AggregateBatchCommitment → Coordinator → Initial

Subordinate State Machine (UNCHANGED from pre-mini-announces):
  WaitingForAnnounce → receive announce → WaitingAnnounceComputed
    → computed → VR → Participant

Compute Layer:
  NEW: ComputeCanonicalEvents(block_hash) → process canonical events only
       → return CanonicalEventsComputed { program_states }
  EXISTING: ComputeAnnounce(announce) → process full announce (canonical + TXs)
```

## Changes By File

### NEW: Compute layer changes

#### `ethexe/compute/src/compute.rs`

Add a new method to process canonical events without a full announce:

```rust
/// Compute canonical events for a block without injected TXs.
/// Returns the resulting ProgramStates for TX validation.
pub fn compute_canonical_events(
    &self,
    db: &impl ComputeDatabase,
    block_hash: H256,
    parent_announce: HashOf<Announce>,
    canonical_quarantine: u32,
) -> Result<ProgramStates> {
    // Reuse prepare_executable_for_announce logic but with empty injected_transactions
    let events = find_canonical_events_post_quarantine(db, block_hash, canonical_quarantine)?;
    let requests: Vec<_> = events.into_iter().filter_map(|e| e.to_request()).collect();
    
    // Build a minimal ExecutableData with canonical events only, no TXs
    let executable = ExecutableData {
        block: SimpleBlockData { hash: block_hash, header: db.block_header(block_hash).ok_or(...)? },
        requests,
        injected_transactions: vec![],
    };
    
    // Execute and return resulting program states
    let result = self.execute(db, executable, parent_announce)?;
    Ok(result.program_states)
}
```

**Key reuse:** This should share code with `prepare_executable_for_announce` — extract the canonical event loading into a helper, call it from both paths.

**IMPORTANT:** This computation result must NOT be written as an announce to DB. It's ephemeral — used only by the producer to inform TX selection. The real announce (with TXs) is written later.

#### `ethexe/compute/src/service.rs`

Add handler for the new event:

```rust
ComputeEvent::ComputeCanonicalEvents { block_hash, parent_announce } => {
    let program_states = self.processor.compute_canonical_events(
        &db, block_hash, parent_announce, canonical_quarantine
    )?;
    // Return to consensus via new event
    ConsensusEvent::CanonicalEventsComputed { block_hash, program_states }
}
```

### `ethexe/common/src/consensus.rs` (or wherever ConsensusEvent lives)

Add new event variants:

```rust
pub enum ConsensusEvent {
    // ... existing ...
    ComputeCanonicalEvents { block_hash: H256, parent_announce: HashOf<Announce> },
    CanonicalEventsComputed { block_hash: H256, program_states: ProgramStates },
}
```

### `ethexe/consensus/src/validator/producer.rs`

#### State enum — replace ReadyForMiniAnnounce

```rust
enum State {
    Delay { timer: Option<Timer> },
    WaitingCanonicalComputed,  // NEW: waiting for canonical events to compute
    ReadyForTxCollection {     // NEW: replaces ReadyForMiniAnnounce
        parent_announce: HashOf<Announce>,
        program_states: ProgramStates,  // post-canonical states for TX validation
        collected_txs: Vec<SignedInjectedTransaction>,
        poll_timer: Timer,
    },
    WaitingAnnounceComputed(HashOf<Announce>),
    AggregateBatchCommitment { future: BoxFuture<'static, Result<Option<BatchCommitment>>> },
}
```

#### Producer flow

1. **Delay → timer fires:**
   - Find parent announce via `best_parent_announce`
   - Emit `ComputeCanonicalEvents { block_hash, parent_announce }`
   - Enter `WaitingCanonicalComputed`

2. **WaitingCanonicalComputed → CanonicalEventsComputed received:**
   - Store `program_states`
   - Start poll timer
   - Enter `ReadyForTxCollection`

3. **ReadyForTxCollection → poll timer fires:**
   - `select_for_announce_with_states(block, program_states)` — NEW variant that validates against provided states instead of DB announce
   - TXs found → collect, restart timer
   - Pool empty + collected non-empty → build announce, write to DB, gossip, compute → `WaitingAnnounceComputed`
   - Pool empty + collected empty → build announce with just canonical events → same path

4. **WaitingAnnounceComputed → computed:**
   - `AggregateBatchCommitment` (no ReadyForMiniAnnounce, no loop)

#### produce_announce — simplified

```rust
fn produce_announce(mut self) -> Result<ValidatorState> {
    let parent = announces::best_parent_announce(...)?;
    // Don't select TXs or build announce yet — wait for canonical compute
    self.ctx.output(ConsensusEvent::ComputeCanonicalEvents {
        block_hash: self.block.hash,
        parent_announce: parent,
    });
    self.state = State::WaitingCanonicalComputed;
    Ok(self.into())
}
```

#### build_announce — NEW, called from ReadyForTxCollection

```rust
fn build_announce(mut self) -> Result<ValidatorState> {
    let State::ReadyForTxCollection { parent_announce, collected_txs, .. } = &mut self.state
    else { unreachable!() };
    
    let announce = Announce {
        block_hash: self.block.hash,
        parent: *parent_announce,
        gas_allowance: Some(self.ctx.core.block_gas_limit),
        injected_transactions: std::mem::take(collected_txs),
    };
    
    // Write to DB, gossip, emit ComputeAnnounce
    // ... same as current produce_announce post-announce-construction ...
    
    self.state = State::WaitingAnnounceComputed(announce_hash);
    Ok(self.into())
}
```

### `ethexe/consensus/src/validator/tx_pool.rs`

Add a new selection method that takes ProgramStates directly:

```rust
/// Select TXs validated against provided program states (post-canonical).
/// Used by depth-1 flow where canonical events are computed before TX selection.
pub fn select_for_announce_with_states(
    &mut self,
    block: SimpleBlockData,
    parent_announce: HashOf<Announce>,
    program_states: &ProgramStates,
) -> Result<Vec<SignedInjectedTransaction>> {
    // Same as select_for_announce but TxValidityChecker uses
    // provided program_states instead of looking up from DB
    let tx_checker = TxValidityChecker::new_with_states(
        self.db.clone(), block, parent_announce, program_states
    )?;
    // ... rest same as select_for_announce ...
}
```

### `ethexe/consensus/src/validator/subordinate.rs`

**REVERT TO PRE-BRANCH STATE.** All mini-announce handling removed:
- Remove `ReadyForMoreAnnounces` state
- Remove `process_announce` for ReadyForMoreAnnounces
- Remove `process_validation_request` for ReadyForMoreAnnounces
- Remove VR deferral logic
- Remove replay_pending_events oldest-first ordering
- Remove non-validator VR drop
- Remove send_announce_for_computation helper
- Remove gossip reorder guard (not needed — only 1 announce, parent always from prev block)

The subordinate returns to its simple form:
```
WaitingForAnnounce → receive announce → compute → VR → Participant
```

### `ethexe/consensus/src/announces.rs`

**REVERT CDL PATCHES.** All block-aware counting reverted to hop counting:
- `propagate_one_base_announce`: simple `for i in 0..commitment_delay_limit`
- `best_announce`: simple `for _ in 0..commitment_delay_limit`
- `recover_announces_chain_if_needed`: simple `while count < commitment_delay_limit`
- `find_announces_common_predecessor`: simple `for _ in 0..commitment_delay_limit`
- Remove `leaf_announces` function + tests
- Remove `best_parent_announce` base-filter (no intra-block parents exist)

**KEEP** the off-by-one fix (`>` vs `>=`) — this was a genuine bug independent of mini-announces.

### `ethexe/consensus/src/validator/batch/utils.rs`

**REVERT** `calculate_batch_expiry` from block-aware to hop counting. Keep off-by-one fix.

### `ethexe/compute/src/compute.rs`

**REMOVE** `is_same_block` check in `prepare_executable_for_announce`. With depth-1, parent is always from a different block. Canonical events always loaded.

### `ethexe/consensus/src/validator/coordinator.rs`

**KEEP** next_block buffering and second new_head timeout. These are needed regardless of depth (they handle new ETH blocks during batch commitment).

## TX Selection Against Post-Canonical State

The key new component: `TxValidityChecker` must accept pre-computed ProgramStates instead of always looking them up from a DB announce.

Current: `TxValidityChecker::new_for_announce(db, block, announce_hash)` → looks up `announce_program_states(announce_hash)` from DB.

New: `TxValidityChecker::new_with_states(db, block, announce_hash, program_states)` → uses provided states directly. `announce_hash` is still needed for `collect_recent_included_txs` (duplicate detection).

The `program_states` come from the canonical compute phase. They represent the world AFTER canonical events but BEFORE injected TXs. This is exactly the right state for validating TXs.

## Edge Cases (from EDGE_CASES.md, cross-referenced)

### Network Event Ordering

| Edge Case | Depth-2 handling | Depth-1 handling |
|-----------|-----------------|-----------------|
| Child before parent | Gossip guard defers | N/A — only 1 announce, parent always from prev block |
| VR before announce | Pending + replay | Same (save VR to pending, replay after compute) |
| VR between base and mini | announce_meta.computed check | N/A — no mini |
| Duplicate delivery | newly_included | Same |

### Gossip Loss

| Edge Case | Depth-1 handling |
|-----------|-----------------|
| Announce lost | Subordinate waits until next block (12s). Same as depth-2 base loss. |
| VR lost | Fewer signatures. Same. |

### Producer Lifecycle

| Edge Case | Depth-1 handling |
|-----------|-----------------|
| Crash after canonical compute, before announce | Ephemeral state lost. Next block restarts. TXs stay in pool. |
| New head during WaitingCanonicalComputed | DefaultProcessing → Initial. Canonical compute result discarded. |
| New head during ReadyForTxCollection | Build announce with collected TXs → WaitingAnnounceComputed. Or discard and batch with no announce (if no TXs). Save next_block. |
| New head during WaitingAnnounceComputed | DefaultProcessing → Initial. Announce in DB, recovered by next block. |
| producer_delay=0 | Canonical compute fires immediately. Poll timer fires immediately. Selects all TXs. Builds announce. 1 announce, no loop. |
| Compute event lost | DefaultProcessing on new_head → Initial. Recovers on next block. |

### TX Pool

| Edge Case | Depth-1 handling |
|-----------|-----------------|
| TXs arrive during canonical compute | Stay in pool. Selected after canonical computes (in ReadyForTxCollection). |
| TXs arrive during announce compute | Stay in pool. Picked up next block. |
| TXs targeting same-block programs | WORK — canonical compute establishes program state BEFORE TX selection. This is the key advantage of two-phase. |
| Cumulative limits | Single select_for_announce_with_states call. Natural limits. |
| Pool volatile on crash | By design. Repopulated from gossip. |

### CDL / Announce Chain

| Edge Case | Depth-1 handling |
|-----------|-----------------|
| CDL counting | Hop counting = block counting. No patches needed. |
| is_same_block | N/A — parent always from different block. |
| leaf_announces | N/A — 1 announce per block, always the leaf. |
| best_parent_announce | Simple — no base-filter needed. |

### Consensus Safety

| Edge Case | Depth-1 handling |
|-----------|-----------------|
| Batch commitment loss | Same pre-existing issue (new_head during compute). |
| Deterministic hashes | Same — announce is deterministic from fields. |
| Canonical compute non-determinism | NEW RISK — the canonical compute must produce identical ProgramStates on all validators. Since it uses the same inputs (block events + parent state), it should be deterministic. But this is a new code path that needs testing. |
| Canonical compute stored in DB? | NO — ephemeral. Only the full announce is stored. If the producer crashes between phases, the canonical result is lost. No orphaned state. |

### Subordinate (simplified)

| Edge Case | Depth-1 handling |
|-----------|-----------------|
| ReadyForMoreAnnounces | N/A — removed. Subordinate receives 1 announce. |
| VR deferral | Simplified — VR's head is the one announce. If not computed, defer. Standard pending mechanism. |
| replay_pending_events | Simplified — no ordering concern. 1 announce per block. |
| Pending queue growth | Less exposure — no mini-announce flood vector. Only VRs and single announces in pending. |
| Non-validator stuck | N/A — no ReadyForMoreAnnounces to get stuck in. |

## Implementation Order

1. **Compute layer:** Add `compute_canonical_events` method + `ComputeCanonicalEvents` event type
2. **TX validation:** Add `TxValidityChecker::new_with_states` + `select_for_announce_with_states`
3. **Producer:** Replace state machine (remove ReadyForMiniAnnounce, add WaitingCanonicalComputed + ReadyForTxCollection)
4. **Subordinate:** Revert to pre-branch state
5. **Announces:** Revert CDL patches to hop counting (keep off-by-one fix)
6. **Compute:** Remove is_same_block
7. **Batch:** Revert to hop counting (keep off-by-one fix)
8. **Tests:** Update all tests, remove mini-announce tests, add two-phase tests
9. **Verify:** Full test suite, clippy, fmt

## Verification

```bash
cargo nextest run -p ethexe-consensus --no-fail-fast
cargo nextest run -p ethexe-compute --no-fail-fast
cargo clippy -p ethexe-consensus -p ethexe-compute
cargo fmt --check -p ethexe-consensus -p ethexe-compute
```

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Canonical compute non-determinism | Low | High (consensus split) | Same compute layer, same inputs. Test with multiple validators. |
| Canonical compute performance | Low | Medium (added latency) | Canonical events are typically few per block. Compute is fast (~100ms). |
| TxValidityChecker with external states | Low | Medium (wrong validation) | States come from the same compute layer. Unit test state plumbing. |
| Ephemeral state loss on crash | Medium | Low (recovers next block) | TXs stay in pool. No DB corruption. |

## Lines Estimate

| Removed | Lines |
|---------|-------|
| CDL patches (announces.rs) | ~320 |
| ReadyForMiniAnnounce + produce_mini (producer.rs) | ~390 |
| Subordinate mini-announce handling | ~250 |
| is_same_block (compute.rs) | ~15 |
| Block-aware expiry (batch/utils.rs) | ~30 |
| **Total removed** | **~1005** |

| Added | Lines |
|-------|-------|
| compute_canonical_events method | ~30 |
| ComputeCanonicalEvents event + handler | ~20 |
| WaitingCanonicalComputed + ReadyForTxCollection states | ~15 |
| Producer two-phase flow | ~60 |
| select_for_announce_with_states | ~20 |
| TxValidityChecker::new_with_states | ~15 |
| Tests | ~80 |
| **Total added** | **~240** |

**Net: ~765 lines deleted.** The branch goes from +1051/-114 to approximately +240/-114.
