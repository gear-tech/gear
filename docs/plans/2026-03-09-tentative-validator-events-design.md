# ETHEXE Tentative Validator Event Stream Design

## Scope

Design `v1` support for local validator-side subscriptions that expose tentative execution results early enough for an external client to try sending a follow-up injected transaction into the same Ethereum block.

In scope:

1. Local validator only.
2. External consumer over RPC/WebSocket.
3. Best-effort same-block follow-up flow.
4. Tentative, non-canonical execution data.
5. Reply and outgoing-message style signals derived from local compute.

Out of scope:

1. Observer-based canonical subscriptions.
2. Confirmation or cancellation events.
3. Cross-validator agreement on tentative events.
4. Light-client storage filtering from the second idea.
5. Hard latency guarantees that a follow-up transaction lands in the same block.

## Confirmed Decisions

1. Consumer model: external RPC/WebSocket client.
2. Primary API shape: `subscribe_events(filter)` style subscription.
3. Timing model: `best effort`; producer does not intentionally wait for the client.
4. Stability model: `tentative` only in `v1`; client handles instability.
5. Initial source of truth: local validator compute results, not observer `block_events`.

## Problem Statement

Current `ethexe` behavior already supports:

1. Sending injected transactions over RPC.
2. Watching for a signed `promise` reply to an injected transaction.
3. Querying synced block events after observer processing.

This is not enough for the requested flow:

1. Observer `block_events` arrive too late for same-block reaction.
2. Current producer state creates one announce per prepared block and then moves toward batch commitment.
3. Follower validator states are also optimized for a single producer announce per block before validation.

As a result, even if the local validator can see an interesting reply during compute, there is no subscription path that exposes it immediately and no iterative announce loop that gives a follow-up transaction another chance to land in the same block.

## Existing Constraints From the Code

1. `ComputeEvent::AnnounceComputed(ComputedAnnounce)` is emitted after local execution and before batch commitment.
2. `ComputedAnnounce` already carries `promises`, which contain `tx_hash` plus `ReplyInfo` for injected transaction replies.
3. The local database already stores `announce_outcome(announce_hash)`, whose `StateTransition.messages` capture outgoing messages produced during execution.
4. Injected transactions are validated against `reference_block`, which is expected to be the currently prepared block on the active branch.
5. `InjectedTxPool::select_for_announce` can naturally avoid reincluding transactions already present in the parent announce chain.

These constraints make local compute the earliest viable hook for `v1`.

## Architecture

## New Shared Types

Add shared types in `ethexe-common` for the tentative event stream:

1. `TentativeEventKind`
   1. `InjectedReply`
   2. `OutgoingMessage`
   3. `OutgoingReply`
2. `TentativeEventsFilter`
   1. `kinds`
   2. `emitter`
   3. `destination`
   4. `message_id`
   5. `reply_to`
   6. `tx_hash`
3. `TentativeEvent`
   1. `InjectedReply { tx_hash, reply }`
   2. `OutgoingMessage { emitter, message }`
   3. `OutgoingReply { emitter, message }`
4. `TentativeEventEnvelope`
   1. `block_hash`
   2. `announce_hash`
   3. `sequence`
   4. `tentative: true`
   5. `event`

The filter semantics are:

1. `AND` across populated fields.
2. `OR` within `kinds`.
3. Empty `kinds` means "all kinds".

## RPC Surface

Expose a new RPC namespace dedicated to local validator execution:

1. Namespace: `validator`
2. Subscription: `subscribeTentativeEvents(filter)`
3. Unsubscribe: generated pair from `jsonrpsee`

Why a new namespace instead of extending `block` or `program`:

1. The data is local and tentative, not chain-canonical.
2. The semantics are push-based, not query-based.
3. The API needs explicit warning by naming, so clients do not confuse it with observer-backed results.

## Event Source

The source of tentative events is the local service loop on `ComputeEvent::AnnounceComputed`.

For each computed announce:

1. Read `computed_data.promises`.
2. Read `db.announce_outcome(computed_data.announce_hash)`.
3. Convert both into a `TentativeExecutionBatch` / vector of `TentativeEventEnvelope`.
4. Publish the batch to RPC subscribers immediately.

This intentionally avoids using observer `BlockEvent` in `v1`, because the same-block latency target depends on data already known during local compute.

## Event Conversion Rules

### Injected replies

Each `ComputedAnnounce.promise` becomes:

1. `TentativeEvent::InjectedReply`
2. `tx_hash` comes from the promise.
3. `reply` comes from `ReplyInfo`.

### Outgoing messages

Each `StateTransition.message` from `announce_outcome` becomes:

1. `TentativeEvent::OutgoingReply` if `message.reply_details.is_some()`.
2. `TentativeEvent::OutgoingMessage` otherwise.

The emitting program is `StateTransition.actor_id`.

This is a local execution view, not a canonical event log.

## Producer Loop Changes

The producer must become iterative for a single prepared `block_hash`.

New producer flow:

1. Build announce from the current best parent plus valid injected transactions.
2. Publish and compute it.
3. On `ComputedAnnounce`:
   1. publish tentative events;
   2. sign and publish promises as today;
   3. check whether newly arrived injected transactions are now selectable for the same `block_hash` using the just-computed announce as parent context.
4. If new transactions are available, create a new announce for the same `block_hash`.
5. If no new transactions are available, proceed to batch aggregation and commitment.

There is no hold timer or explicit client acknowledgement in `v1`.

## Follower Validator Changes

Supporting repeated producer announces for one prepared block also affects non-producer validators.

`v1` therefore requires follower states to keep accepting additional producer announces for the same block until the final validation request is processed.

Implications:

1. `Subordinate` must not assume exactly one producer announce before transition.
2. `Participant` must not ignore later producer announces that advance the best head for the current block.
3. Validation must happen against the latest computed producer head, not just the first one observed.

Without this, the producer could locally advance the announce head while other validators still validate an older head.

## Data Flow

1. External client opens `validator.subscribeTentativeEvents(filter)`.
2. Producer computes an announce for the current prepared block.
3. Service receives `ComputeEvent::AnnounceComputed`.
4. Service builds tentative envelopes from:
   1. `ComputedAnnounce.promises`
   2. `announce_outcome.transitions[*].messages`
5. RPC broadcasts matching envelopes to subscribers.
6. Client reacts and sends a follow-up injected transaction with `reference_block = block_hash` from the envelope.
7. If the transaction reaches the validator before the producer closes the current iteration, it can be included into the next announce for the same block.
8. Once no more new injected transactions are selected, normal batch commitment flow continues.

## Error Handling and Semantics

1. Subscription delivery failure only affects that subscriber; consensus flow continues.
2. If no subscriber is connected, the node still computes and commits normally.
3. If a client disconnects after receiving a tentative event, no retry is attempted in `v1`.
4. If a follow-up transaction arrives too late, it is simply eligible for a later block.
5. If an announce is later superseded or the chain reorgs, the previously emitted tentative event remains an observed local execution fact; `v1` does not emit correction events.

## Testing Strategy

## Unit tests

1. Filter matching for each event kind and field combination.
2. Conversion from promise to `InjectedReply`.
3. Conversion from transition messages to `OutgoingMessage` / `OutgoingReply`.
4. RPC subscription cleanup and filtering behavior.

## Consensus tests

1. Producer emits a second announce for the same block when new injected transactions arrive after the first tentative publication.
2. Producer stops iterating when no new transactions are available.
3. Follower validator computes successive producer announces for the same block and validates the latest head.

## Service / integration tests

1. Tentative subscription receives local reply information before observer `block_synced`.
2. Follow-up injected transaction sent after a tentative event can land in a later announce of the same block.
3. Filtered subscriptions only receive matching events.

## Risks and Trade-offs

1. Best-effort timing means "same block" is opportunistic, not guaranteed.
2. Repeated announce rounds may increase local work for hot blocks with reactive clients.
3. Tentative events can diverge from eventual canonical chain history.
4. Broad subscriptions could create noisy local traffic; filters are required to keep `v1` practical.

## Rollout Notes

1. Ship `v1` as explicitly tentative and validator-local.
2. Keep observer-backed canonical subscriptions as a separate future feature.
3. Do not add confirmation/cancellation semantics until there is a clear consumer need.
