# Ethexe Consensus Edge Cases & Constraints

Verified constraints for the validator state machine, announce processing, processor execution,
and mini-announces. Based on code tracing, test results, and cross-model review (Claude, Codex gpt-5.4).

Last updated 2026-04-16.

## Hard Constraints (System Invariants)

### Program Lifecycle
- Programs ONLY initialize via Ethereum canonical events (Router::ProgramCreated + Mirror init message)
- Until a program is initialized via canonical execution, injected TXs targeting it will fail
- Injected TXs cannot trigger program initialization. Period.
- `requires_init_message()` returns false if ANY queue is non-empty (canonical or injected)

### Processor Event Ordering
- `handle_injected_and_events` processes: injected TXs first, then canonical events
- This is correct. Injected TXs have execution priority over canonical messages
- `process_queues` (the actual execution) also runs injected queue first, then canonical
- DO NOT reorder. Changing registration order changes which messages get queued first
- Router events (ProgramCreated, CodeValidated) only register programs, they don't enqueue messages
- Mirror events (MessageQueueingRequested) enqueue canonical messages

### CDL (Commitment Delay Limit)
- CDL is defined in BLOCKS, not announce hops (S1 in announces.rs theory)
- With mini-announces (depth-2), one block can have 2 announces (base + mini)
- All CDL-bounded loops MUST count block transitions: track `prev_block_hash`, increment `blocks_seen` only when `block_hash` changes
- Boundary condition: use `>=` not `>` when checking `blocks_seen` against CDL. The announce AT the boundary must be examined before breaking.
- Off-by-one here causes expired branches to win parent selection or batch expiry to be wrong by 1 block

### Announce Chain
- Base announces cross block boundaries (parent is from previous block)
- Mini-announces chain within the same block (parent is base or previous mini)
- `leaf_announces()` filters to chain tips. For {base, mini} blocks, the leaf is the mini
- `best_parent_announce()` must prefer base announces for cross-block parent selection

## Producer State Machine

### Mini-Announces Flow
```
Delay → produce_announce (base, no TXs) → WaitingAnnounceComputed
  → base computed → ReadyForMiniAnnounce (poll timer)
  → timer fires:
      pool empty → AggregateBatchCommitment
      TXs found → produce_mini_announce → WaitingAnnounceComputed
        → mini computed → AggregateBatchCommitment (cap at 1 mini)
  → AggregateBatchCommitment → Coordinator → Initial
```

### Edge Cases
- **new_head during ReadyForMiniAnnounce**: Creates batch commitment with last_announce_hash, saves next_block. Correct.
- **new_head during AggregateBatchCommitment**: Buffers next_block. Second new_head → abandons batch. Pre-existing issue.
- **new_head during WaitingAnnounceComputed (mini)**: Falls to DefaultProcessing → Initial. TODO: block-specific code/validator/reward commitments are lost. Chain commitment recovered by next block's collect_not_committed_predecessors. PRE-EXISTING on master too.
- **producer_delay=0**: Timer fires immediately. One poll, no TXs → batch. TXs found → one mini, then batch. Cap at 1 mini prevents tight loop.
- **mini_produced flag**: Caps depth at 2 (base + 1 mini). After mini computes, goes to AggregateBatchCommitment, not back to ReadyForMiniAnnounce.

## Subordinate State Machine

### Simplified Flow (2 states, not 3)
```
WaitingForAnnounce → receive announce → accept_announce → WaitingAnnounceComputed
  → computed → back to WaitingForAnnounce (loop for mini-announces)
  → receive VR in WaitingForAnnounce:
      head_computed? → Participant
      head not computed? → defer to pending
  → process_pending_after_compute replays deferred events oldest-first
```

### Edge Cases
- **Child arrives before parent (gossip reorder)**: accept_announce returns UnknownParent → defer to pending (if queue not full, else drop). Replayed after next announce computes.
- **VR arrives before mini computes**: VR deferred to pending. After mini computes, process_pending_after_compute retries it.
- **VR arrives in WaitingAnnounceComputed**: Saved to pending via DefaultProcessing. Replayed after compute finishes.
- **Pending queue overflow**: MAX_PENDING_EVENTS=10 enforced during create() AND during UnknownParent deferral. Byzantine producer cannot grow queue unbounded.
- **Non-validator receives VR**: Dropped silently in WaitingForAnnounce. Prevents recycle loop in pending replay.

## Compute Layer

### is_same_block Check
- When computing a mini-announce, its parent is from the SAME block
- Canonical events were already processed by the parent (base) announce
- The compute layer MUST skip canonical events for same-block announces
- Without this: events fire twice, program state corrupted, execution results wrong

### Announce Computation
- `prepare_executable_for_announce` reads parent announce's ProgramStates from DB
- Parent must be computed (has announce_program_states in DB)
- `collect_not_computed_predecessors` computes any missing predecessors first
- CAS/state blob writes are idempotent (content-addressed)
- Announce metadata writes (set_announce_outcome, set_announce_program_states, etc.) mark the announce as computed

## Batch Commitment

### What's in a Batch
- `chain_commitment`: announce chain transitions (RECOVERABLE via collect_not_committed_predecessors)
- `code_commitments`: validated codes from block's codes_queue (PER-BLOCK, NOT recoverable)
- `validators_commitment`: era validator set changes (PER-BLOCK, NOT recoverable)
- `rewards_commitment`: (PER-BLOCK, NOT recoverable)

### Loss Scenarios
- If batch commitment is never created for a block (new_head during WaitingAnnounceComputed), per-block commitments are permanently lost
- Chain transitions are recovered by the next block's batch (collect_not_committed_predecessors)
- This is a pre-existing issue on master, not introduced by mini-announces

### Expiry Calculation
- `calculate_batch_expiry`: walks announce chain counting block transitions
- Must examine the announce at the CDL boundary BEFORE breaking (check is_base, then break)
- `blocks_seen` starts at 1 (head announce's block)
- `expiry = blocks_to_check - oldest_not_base_depth`

## Network & Gossip

### Gossip Guarantees
- Gossipsub does NOT guarantee delivery order
- Gossipsub does NOT guarantee delivery at all (messages can be lost)
- Peer scoring provides some DoS resistance but Byzantine producers can still send valid-looking garbage
- The 12s ETH block interval is the recovery boundary. Any missed block is retried on the next one.

### TX Pool
- `InjectedTxPool` is volatile (starts empty on restart, repopulated from gossip)
- `HashSet` iteration is non-deterministic. Under TX pool overflow with limits, different validators may select different TX subsets. This is acceptable because the announce hash is deterministic from the selected set.
- TX validity is checked by `TxValidityChecker` against the parent announce's ProgramStates
- `select_for_announce` runs against the last computed predecessor's states

## Coordinator

### next_block Buffering
- First new_head during Coordinator: buffer it, continue waiting for signatures
- Second new_head: abandon batch, transition to Initial
- This is defensive. The batch is lost but the next block recovers chain commitments.

## Known TODOs (Pre-existing)

- #5342: Batch commitment lost when new_head arrives during WaitingAnnounceComputed
- #5343: Synced/prepared events lost when consensus state is not Initial
- #4641: Pending queue abuse (Byzantine producer floods fake events)
- Volatile TX pool: TXs lost on crash, repopulated from network gossip (by design)
- Non-deterministic TX selection order under overflow (HashSet iteration)
