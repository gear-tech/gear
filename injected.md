# How Injected Transactions Work in Ethexe

**Injected transactions** are ethexe's mechanism for sending messages to Gear programs *without* going through Ethereum on-chain transactions. Instead of calling `Mirror.sendMessage()` on Ethereum (which costs gas and waits for block confirmation), users submit transactions directly to ethexe validators via RPC or P2P network.

## The Full Flow (traced across 10 communities)

### 1. Entry Points — SDK & RPC

Users create injected transactions through two paths:

- **SDK path**: `Mirror<'a>.prepare_injected_transaction()` → `send_message_injected()` or `send_message_injected_and_watch()` (`ethexe/sdk/src/mirror.rs`)
- **RPC path**: `InjectedApi.send_transaction()` or `.send_transaction_and_watch()` (`ethexe/rpc/src/apis/injected.rs`) — also `.forward_transaction()` for relaying from other validators

The core type is **`InjectedTransaction`** (`ethexe/common/src/injected.rs`), which has methods: `.update_hasher()`, `.to_hash()`, `.to_message_id()`. It carries `origin`, `payload`, and `reply_to` fields. There's also `AddressedInjectedTransaction` which wraps it with a target program address, and `SignedInjectedTransaction` which adds a cryptographic signature.

### 2. Network Propagation

When a validator receives an injected tx via RPC, it gossips it to peers through the P2P network:
- `NetworkService.send_injected_transaction()` broadcasts via gossipsub
- Receiving validators fire `Event.unwrap_new_injected_transaction()`
- Acceptance is tracked via `InjectedTransactionAcceptance` and `unwrap_injected_transaction_acceptance()`

The `addressed_injected_tx()` helper (`ethexe/network/src/injected.rs`) handles routing to the correct program.

### 3. Consensus Validation

The validator consensus layer processes injected txs through a dedicated pipeline:

- `ValidatorService.receive_injected_transaction()` → `DefaultProcessing.injected_transaction()` → `ValidatorState.process_injected_transaction()`
- **Validation rules** (`ethexe/consensus/src/tx_validation.rs`):
  - Non-zero value transfers are rejected (`test_check_injected_transaction_non_zero_value`)
  - Cannot initialize actors (`test_check_injected_tx_can_not_initialize_actor`)
- Valid txs enter the **`InjectedTxPool<DB>`** (`ethexe/consensus/src/validator/tx_pool.rs`) where they wait until block processing

### 4. Storage

The database layer stores injected txs:
- `InjectedStorageRW` / `InjectedStorageRO` (`ethexe/common/src/db.rs`) — read/write traits
- `.set_injected_transaction()` / `.injected_transaction()` (`ethexe/db/src/database.rs`)

### 5. Inclusion in Announces

When a Producer builds an `Announce` for a block, injected transactions are included in the announce hash:
- `test_announce_hash_with_injected()` vs `test_announce_hash_no_injected()` (`ethexe/common/src/primitives.rs`) — proves the announce hash differs based on included injected txs
- This means all validators must agree on which injected txs are included in a block

### 6. Execution

During block computation:
- `Processor.handle_injected_and_events()` (`ethexe/processor/src/lib.rs`) — processes injected txs alongside chain events
- `ProcessingHandler.handle_injected_transaction()` (`ethexe/processor/src/handling/events.rs`) — handles the actual dispatch
- `parse_journal_for_injected_dispatch()` (`ethexe/runtime/common/src/lib.rs`) — extracts execution results
- `ComputeSubService` includes injected txs via `injected_tx()` (`ethexe/compute/src/compute.rs`)

Integration tests confirm the pipeline works end-to-end: `injected()`, `injected_ping_pong()`, `injected_prioritized_over_canonical()`.

### 7. Promise Resolution

`InjectedApi` also supports **promises**: `.send_promise()` and `.spawn_promise_waiter()` allow clients to wait for execution results. The `RpcOrNetworkInjectedTx` type distinguishes between locally-submitted and network-received transactions.

## Architecture Diagram

```
User/SDK                    P2P Network
   │                            │
   ▼                            ▼
Mirror.prepare_injected_tx()   NetworkService.handle_injected_event()
   │                            │
   ▼                            ▼
InjectedApi.send_transaction() ─┬─► addressed_injected_tx()
                                │
                                ▼
                    ValidatorService.receive_injected_transaction()
                                │
                                ▼
                    tx_validation (reject non-zero value, no init)
                                │
                                ▼
                    InjectedTxPool<DB> (stored in DB)
                                │
                                ▼
                    Announce includes injected txs in hash
                                │
                                ▼
                    Processor.handle_injected_and_events()
                                │
                                ▼
                    parse_journal_for_injected_dispatch()
                                │
                                ▼
                    State transitions → BatchCommitment → Ethereum
```

## Key Design Decisions

- **No value transfers** — injected txs can't carry token value (validated at consensus layer), preventing double-spend without on-chain settlement
- **Prioritized over canonical** — `injected_prioritized_over_canonical()` test shows injected txs are processed before on-chain events, giving them lower latency
- **Network consensus required** — injected txs are gossiped and must be agreed upon by validators before inclusion (the announce hash includes them)
