# ETHEXE Tentative Validator Event Stream Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a validator-local WebSocket subscription that streams tentative execution results early enough for clients to try sending follow-up injected transactions into another announce for the same prepared Ethereum block.

**Architecture:** Introduce shared tentative-event/filter types in `ethexe-common`, expose them through a new `validator.subscribeTentativeEvents` RPC subscription, emit those events from the service on `ComputeEvent::AnnounceComputed`, and extend validator consensus states so the producer and followers can process multiple announces for the same block before the final validation request/commit path.

**Tech Stack:** Rust 2024, `jsonrpsee`, `tokio`, `dashmap`, existing `ethexe-*` crates (`common`, `rpc`, `service`, `consensus`, `db`), existing service integration test harness.

---

## Execution Notes

1. Skills to apply during execution:
   1. `@superpowers/test-driven-development`
   2. `@ethexe-consensus`
   3. `@superpowers/verification-before-completion`
2. Worktree note:
   1. If you want isolation before implementation, create a dedicated worktree before Task 1.
3. Commit policy during execution:
   1. Use frequent commits for code changes.
   2. Do not commit the plan docs unless explicitly requested.

### Task 1: Add Shared Tentative Event Types and Filter Matching

**Files:**

- Create: `/Users/ukintvs/Documents/projects/gear/ethexe/common/src/tentative.rs`
- Modify: `/Users/ukintvs/Documents/projects/gear/ethexe/common/src/lib.rs`
- Test: `/Users/ukintvs/Documents/projects/gear/ethexe/common/src/tentative.rs`

**Step 1: Write the failing tests**

Add unit tests for:

```rust
#[test]
fn filter_matches_injected_reply_by_tx_hash() {
    let filter = TentativeEventsFilter {
        tx_hash: Some(HashOf::zero()),
        ..Default::default()
    };

    let event = TentativeEventEnvelope::injected_reply(
        H256::repeat_byte(1),
        HashOf::zero(),
        0,
        HashOf::zero(),
        ReplyInfo {
            payload: vec![],
            code: ReplyCode::Success(SuccessReplyReason::Auto),
            value: 0,
        },
    );

    assert!(filter.matches(&event));
}

#[test]
fn outgoing_reply_requires_reply_to_match() {
    let filter = TentativeEventsFilter {
        reply_to: Some(MessageId::from([7; 32])),
        ..Default::default()
    };

    let event = make_outgoing_reply(MessageId::from([8; 32]));
    assert!(!filter.matches(&event));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p ethexe-common tentative::tests:: -- --nocapture`
Expected: FAIL because tentative types and match helpers do not exist yet.

**Step 3: Write minimal implementation**

Add shared types and helpers:

```rust
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum TentativeEventKind {
    InjectedReply,
    OutgoingMessage,
    OutgoingReply,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct TentativeEventsFilter {
    pub kinds: Vec<TentativeEventKind>,
    pub emitter: Option<ActorId>,
    pub destination: Option<ActorId>,
    pub message_id: Option<MessageId>,
    pub reply_to: Option<MessageId>,
    pub tx_hash: Option<HashOf<InjectedTransaction>>,
}

impl TentativeEventsFilter {
    pub fn matches(&self, event: &TentativeEventEnvelope) -> bool { /* field-by-field match */ }
}
```

Add `TentativeEvent`, `TentativeEventEnvelope`, constructors, and `serde` derives under `std`.

**Step 4: Run test to verify it passes**

Run: `cargo test -p ethexe-common tentative::tests:: -- --nocapture`
Expected: PASS.

**Step 5: Commit**

```bash
git add ethexe/common/src/lib.rs ethexe/common/src/tentative.rs
git commit -m "feat(ethexe): add tentative validator event types"
```

### Task 2: Add RPC Subscription API and Subscription Broadcaster

**Files:**

- Create: `/Users/ukintvs/Documents/projects/gear/ethexe/rpc/src/apis/validator.rs`
- Modify: `/Users/ukintvs/Documents/projects/gear/ethexe/rpc/src/apis/mod.rs`
- Modify: `/Users/ukintvs/Documents/projects/gear/ethexe/rpc/src/lib.rs`
- Modify: `/Users/ukintvs/Documents/projects/gear/ethexe/rpc/src/tests.rs`

**Step 1: Write the failing RPC test**

Add a test that subscribes, injects one tentative event through the RPC service, and expects exactly one matching payload:

```rust
#[tokio::test]
async fn tentative_subscription_filters_events() {
    let client = WsClientBuilder::new().build(format!("ws://{listen_addr}")).await.unwrap();
    let mut sub = client
        .subscribe_tentative_events(TentativeEventsFilter {
            tx_hash: Some(expected_hash),
            ..Default::default()
        })
        .await
        .unwrap();

    mock_service.rpc.provide_tentative_events(vec![matching(), non_matching()]);

    let item = sub.next().await.unwrap().unwrap();
    assert_eq!(item.event.kind(), TentativeEventKind::InjectedReply);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p ethexe-rpc tentative_subscription_filters_events -- --nocapture`
Expected: FAIL because the validator RPC API and broadcaster do not exist.

**Step 3: Write minimal implementation**

Add:

```rust
#[cfg_attr(not(feature = "client"), rpc(server, namespace = "validator"))]
#[cfg_attr(feature = "client", rpc(server, client, namespace = "validator"))]
pub trait Validator {
    #[subscription(
        name = "subscribeTentativeEvents",
        unsubscribe = "unsubscribeTentativeEvents",
        item = TentativeEventEnvelope
    )]
    async fn subscribe_tentative_events(
        &self,
        filter: TentativeEventsFilter,
    ) -> SubscriptionResult;
}
```

Mirror the existing `InjectedApi` subscription lifecycle:

1. accept sink,
2. spawn per-subscriber task,
3. filter events before sending,
4. clean up subscriber state on disconnect.

Extend `RpcService` with:

```rust
pub fn provide_tentative_events(&self, events: Vec<TentativeEventEnvelope>) {
    self.validator_api.send_tentative_events(events);
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p ethexe-rpc tentative_subscription_filters_events -- --nocapture`
Expected: PASS.

**Step 5: Commit**

```bash
git add ethexe/rpc/src/apis/mod.rs ethexe/rpc/src/apis/validator.rs ethexe/rpc/src/lib.rs ethexe/rpc/src/tests.rs
git commit -m "feat(ethexe-rpc): add tentative validator event subscription"
```

### Task 3: Emit Tentative Events From Local Compute Results

**Files:**

- Modify: `/Users/ukintvs/Documents/projects/gear/ethexe/service/src/lib.rs`
- Modify: `/Users/ukintvs/Documents/projects/gear/ethexe/common/src/tentative.rs`
- Test: `/Users/ukintvs/Documents/projects/gear/ethexe/service/src/tests/mod.rs`

**Step 1: Write the failing service test**

Add a service integration test that:

1. starts one validator with RPC,
2. subscribes to tentative events,
3. sends an injected transaction that produces a reply,
4. asserts the tentative subscription receives the reply before the observer-driven wait helper completes.

Skeleton:

```rust
#[tokio::test(flavor = "multi_thread")]
async fn tentative_reply_arrives_before_block_synced_reply() {
    let mut sub = node
        .rpc_ws_client()
        .unwrap()
        .subscribe_tentative_events(TentativeEventsFilter {
            tx_hash: Some(tx.to_hash()),
            ..Default::default()
        })
        .await
        .unwrap();

    let wait_for_reply = env.send_message(program_id, b"PING").await.unwrap();

    let tentative = sub.next().await.unwrap().unwrap();
    assert!(matches!(tentative.event, TentativeEvent::InjectedReply { .. }));

    let synced = wait_for_reply.wait_for().await.unwrap();
    assert_eq!(synced.message_id, tx.to_message_id());
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p ethexe-service tentative_reply_arrives_before_block_synced_reply -- --nocapture`
Expected: FAIL because the service does not publish tentative batches yet.

**Step 3: Write minimal implementation**

On `Event::Compute(ComputeEvent::AnnounceComputed(computed_data))`:

1. read `db.announce_outcome(computed_data.announce_hash)`,
2. build ordered envelopes for:
   1. `computed_data.promises`,
   2. `transition.messages`,
3. set `block_hash`, `announce_hash`, and incrementing `sequence`,
4. call `rpc.provide_tentative_events(...)` before the producer moves on.

Use constructors in `ethexe_common::tentative` to keep the mapping logic out of the service loop.

**Step 4: Run test to verify it passes**

Run: `cargo test -p ethexe-service tentative_reply_arrives_before_block_synced_reply -- --nocapture`
Expected: PASS.

**Step 5: Commit**

```bash
git add ethexe/common/src/tentative.rs ethexe/service/src/lib.rs ethexe/service/src/tests/mod.rs
git commit -m "feat(ethexe-service): publish tentative compute events"
```

### Task 4: Extend Validator Consensus To Support Multiple Announces Per Prepared Block

**Files:**

- Modify: `/Users/ukintvs/Documents/projects/gear/ethexe/consensus/src/validator/producer.rs`
- Modify: `/Users/ukintvs/Documents/projects/gear/ethexe/consensus/src/validator/subordinate.rs`
- Modify: `/Users/ukintvs/Documents/projects/gear/ethexe/consensus/src/validator/participant.rs`
- Modify: `/Users/ukintvs/Documents/projects/gear/ethexe/consensus/src/validator/mod.rs`
- Test: `/Users/ukintvs/Documents/projects/gear/ethexe/consensus/src/validator/producer.rs`
- Test: `/Users/ukintvs/Documents/projects/gear/ethexe/consensus/src/validator/subordinate.rs`
- Test: `/Users/ukintvs/Documents/projects/gear/ethexe/consensus/src/validator/participant.rs`

**Step 1: Write the failing consensus tests**

Add focused tests for:

```rust
#[tokio::test]
async fn producer_restarts_same_block_when_new_tx_arrives_after_first_compute() { /* ... */ }

#[test]
fn subordinate_accepts_later_producer_announce_for_same_block() { /* ... */ }

#[test]
fn participant_uses_latest_computed_head_for_validation() { /* ... */ }
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p ethexe-consensus producer_restarts_same_block_when_new_tx_arrives_after_first_compute -- --nocapture`
Expected: FAIL because producer transitions to batch aggregation immediately after first compute.

Run: `cargo test -p ethexe-consensus subordinate_accepts_later_producer_announce_for_same_block -- --nocapture`
Expected: FAIL because follower states assume one announce per block.

**Step 3: Write minimal implementation**

Change producer post-compute logic to:

```rust
if self.ctx.core.injected_pool.has_selectable_txs(self.block.hash, computed_data.announce_hash)? {
    self.state = State::Delay { timer: None };
    return self.produce_announce().map(|state| (Poll::Ready(()), state));
}

self.state = State::AggregateBatchCommitment { /* existing path */ };
```

Do not add a sleep/hold window. Just re-run announce production immediately when new selectable transactions exist.

Update follower states so they can keep advancing the current block head:

1. `Subordinate` should compute later producer announces for the same `block.hash`.
2. `Participant` should accept and compute later producer announces while waiting for validation request.
3. Validation should continue to target the latest computed head.

**Step 4: Run tests to verify they pass**

Run: `cargo test -p ethexe-consensus producer_restarts_same_block_when_new_tx_arrives_after_first_compute -- --nocapture`
Expected: PASS.

Run: `cargo test -p ethexe-consensus subordinate_accepts_later_producer_announce_for_same_block -- --nocapture`
Expected: PASS.

Run: `cargo test -p ethexe-consensus participant_uses_latest_computed_head_for_validation -- --nocapture`
Expected: PASS.

**Step 5: Commit**

```bash
git add ethexe/consensus/src/validator/mod.rs ethexe/consensus/src/validator/producer.rs ethexe/consensus/src/validator/subordinate.rs ethexe/consensus/src/validator/participant.rs
git commit -m "feat(ethexe-consensus): support iterative same-block announces"
```

### Task 5: Add End-to-End Same-Block Reactive Flow Test

**Files:**

- Modify: `/Users/ukintvs/Documents/projects/gear/ethexe/service/src/tests/mod.rs`
- Modify: `/Users/ukintvs/Documents/projects/gear/ethexe/service/src/tests/utils/env.rs` (only if a helper is needed for tentative subscriptions)

**Step 1: Write the failing integration test**

Add one integration test that proves the intended user story:

1. subscribe to tentative events for a target reply,
2. trigger the first transaction,
3. on tentative reply receipt, immediately send a follow-up injected transaction with `reference_block` equal to the envelope `block_hash`,
4. assert both transactions were included under the same prepared block but in different announces.

Skeleton:

```rust
#[tokio::test(flavor = "multi_thread")]
async fn follow_up_tx_can_land_in_same_block_after_tentative_reply() {
    let first = send_and_watch_initial_tx(...).await;
    let tentative = sub.next().await.unwrap().unwrap();
    let same_block = tentative.block_hash;

    send_follow_up_tx(reference_block = same_block).await.unwrap();

    let promises = node
        .events()
        .find_map(|event| match event {
            TestingEvent::Consensus(ConsensusEvent::Promises(promises)) if promises.len() >= 2 => Some(promises),
            _ => None,
        })
        .await;

    assert_same_block(promises, same_block);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p ethexe-service follow_up_tx_can_land_in_same_block_after_tentative_reply -- --nocapture`
Expected: FAIL until producer iteration and tentative subscription are both wired.

**Step 3: Implement the minimal missing pieces**

Fill any remaining gaps in:

1. event ordering,
2. follow-up transaction acceptance timing,
3. test helpers for WebSocket subscription and block/announce assertions.

Keep the assertion focused on "same prepared block, later announce", not on wall-clock timing.

**Step 4: Run test to verify it passes**

Run: `cargo test -p ethexe-service follow_up_tx_can_land_in_same_block_after_tentative_reply -- --nocapture`
Expected: PASS.

**Step 5: Commit**

```bash
git add ethexe/service/src/tests/mod.rs ethexe/service/src/tests/utils/env.rs
git commit -m "test(ethexe-service): cover same-block tentative reaction flow"
```

### Task 6: Document the New RPC and Run Final Verification

**Files:**

- Modify: `/Users/ukintvs/Documents/projects/gear/ethexe/README.md`

**Step 1: Add failing doc checklist**

Create a short checklist in the working notes:

1. RPC method name is documented.
2. Tentative semantics are explicitly called out.
3. Client must use `reference_block = envelope.block_hash`.

Treat missing documentation as a failure for this task.

**Step 2: Write the docs**

Add a section like:

```md
## Tentative validator subscriptions

`validator.subscribeTentativeEvents` streams local execution results from the connected validator.

These events are tentative:

1. they are not observer-confirmed,
2. they may disappear on reorg or if a later announce supersedes them,
3. they exist to let a client react quickly and attempt a follow-up injected transaction.
```

Include one short request/response example.

**Step 3: Run verification commands**

Run: `cargo test -p ethexe-common tentative::tests:: -- --nocapture`
Expected: PASS.

Run: `cargo test -p ethexe-rpc tentative_subscription_filters_events -- --nocapture`
Expected: PASS.

Run: `cargo test -p ethexe-consensus producer_restarts_same_block_when_new_tx_arrives_after_first_compute -- --nocapture`
Expected: PASS.

Run: `cargo test -p ethexe-service tentative_reply_arrives_before_block_synced_reply -- --nocapture`
Expected: PASS.

Run: `cargo test -p ethexe-service follow_up_tx_can_land_in_same_block_after_tentative_reply -- --nocapture`
Expected: PASS.

**Step 4: Commit**

```bash
git add ethexe/README.md
git commit -m "docs(ethexe): document tentative validator event subscription"
```
