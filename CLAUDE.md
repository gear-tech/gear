# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Gear Protocol — a Substrate-based platform for running WebAssembly smart contracts (programs) with an actor-model message-passing architecture. The main network is **Vara**. The repo also contains **ethexe**, a layer that runs Gear programs on Ethereum.

- Rust edition 2024, MSRV 1.91, pinned nightly `nightly-2025-10-20` (see `rust-toolchain.toml`)
- 114-crate Cargo workspace; default member is `node/cli` (the Vara node binary)
- `ethexe/` is excluded from the main workspace and has its own Cargo.toml + Solidity contracts (Foundry/Forge)
- Sails framework lives in a separate repo (github.com/gear-tech/sails), not here

## Build & Development Commands

### Setup
```bash
make init              # Install wasm target + cargo tools (hakari, nextest, etc.)
```

### Build
```bash
make gear              # Build workspace (debug)
make node              # Build Vara node (debug)
make node-release      # Build Vara node (release)
make examples          # Build example WASM programs
```

### Test
```bash
# Workspace tests (uses cargo nextest):
make test                                          # All workspace tests (excludes gcli, gsdk, fuzzer)
cargo nextest run -p <crate-name>                  # Single crate
cargo nextest run -p <crate-name> -E 'test(name)'  # Single test by name filter
make test-pallet                                   # Pallet tests only
make test-doc                                      # Doc tests (uses cargo test, not nextest)

# Ethexe tests:
cargo nextest run -p "ethexe-*" --no-fail-fast

# gsdk/gcli tests require a built node:
make test-gsdk         # Builds node-release first
make test-gcli         # Builds node-release first
```

### Lint & Format
```bash
make fmt               # Format (rustfmt)
make fmt-check         # Check formatting
make clippy            # Clippy for workspace + examples
make typos             # Check for typos (requires typos-cli)
make pre-commit        # Full pre-commit: fmt, typos, hakari, clippy, test, check-runtime-imports
```

### Ethexe Contracts (Solidity/Forge)
```bash
make ethexe-contracts-pre-commit   # Clean, format, build, test, copy ABIs
forge test --root ethexe/contracts -vvv
```

### Workspace Hakari
```bash
make workspace-hack    # Regenerate workspace-hack (cargo hakari generate + post-process)
```

## Architecture

### Execution Flow

```
User extrinsic (upload_program / send_message / send_reply)
  → pallet-gear validates, reserves gas+value, queues StoredDispatch
    → run() inherent fires each block (mandatory, unsigned)
      → QueueRunner dequeues dispatches one by one
        → core-processor loads program, creates Ext context
          → core-backend instantiates WASM in sandbox (Wasmer/Wasmtime)
            → Program calls gstd/gcore syscalls → dispatched to host fns
            → lazy-pages intercepts SIGSEGV for on-demand memory paging
          → Execution produces journal notes (outgoing messages, state changes, gas burns)
        → pallet-gear applies journal: route messages, update waitlist/mailbox, transfer value
```

### Message Lifecycle

Messages have four dispatch kinds: **Init**, **Handle**, **Reply**, **Signal**.

Type progression through the system:
1. Created as packets (`InitPacket`, `HandlePacket`, `ReplyPacket`)
2. Converted to typed messages (`InitMessage`, `HandleMessage`, etc.)
3. Wrapped in `Dispatch` with `DispatchKind`
4. Stored as `StoredDispatch` (gasless, persistent) in the message queue
5. Dequeued as `IncomingDispatch` (with gas limit) for execution
6. `ContextStore` preserves execution state across async boundaries (initialized actors, reservation nonce, local nonce)

Reply/signal messages carry `MessageDetails` — either `ReplyDetails` (origin message + ReplyCode) or `SignalDetails` (origin message + SignalCode).

### Program Lifecycle

```
[not exists] → upload_program → Active(Uninitialized { message_id })
  → init succeeds → Active(Initialized)
  → init fails → Terminated(inheritor)
  → gr_exit called → Exited(inheritor)
  → expiration_block reached → removed
```

`ActiveProgram` fields: `code_id`, `state` (Uninitialized/Initialized), `allocations_tree_len`, `memory_infix` (lazy-page storage namespace), `gas_reservation_map`, `expiration_block`.

### Gas Model

UTXO-like gas tree (`GasTree`) where each message owns a gas node:
- `GasCounter` tracks `(left, burned)` — `charge()` deducts and records burn, `reduce()` deducts without burn (for sends)
- `ValueCounter` tracks value (u128) transfers alongside gas
- `GasAllowanceCounter` enforces block-level gas budget
- Tree operations: `create`, `split`, `split_with_value`, `cut`, `consume`, `lock`/`unlock`
- Lock IDs: `Mailbox`, `Waitlist`, `Reservation`, `DispatchStash`
- Imbalance types (`PositiveImbalance`/`NegativeImbalance`) for accounting correctness
- `ReservableTree` trait adds gas reservation management (programs can reserve gas for future use)

### Queues: Waitlist, Mailbox, TaskPool

**Message Queue** (gear-messenger): FIFO linked-list of `StoredDispatch` — processed each block by `run()`.

**Waitlist** (gear-messenger): Programs waiting for a reply. Keyed by `(ActorId, MessageId) → StoredDispatch`. When a reply arrives, the dispatch is woken and re-queued.

**Mailbox** (gear-messenger): Messages addressed to user accounts. Keyed by `(AccountId, MessageId) → UserStoredMessage`. Users call `claim_value` extrinsic to collect value. Messages have rent (gas-based TTL via `MailboxThreshold`).

**TaskPool** (gear-scheduler): Delayed actions keyed by `(BlockNumber, Task)`. Task types: delayed message send, wake from waitlist, mailbox expiry removal, reservation cleanup, program stall timeout.

**Interaction**: Queue → execution → may create Waitlist entry (async reply) or Mailbox entry (user-bound message) → TaskPool schedules cleanup/wake at future block.

### Lazy Pages

Signal-based demand paging for WASM program memory:
1. `initialize_for_program()` protects non-stack WASM pages via `mprotect`
2. When program accesses a protected page → SIGSEGV fires
3. Signal handler loads page data from `LazyPagesStorage`, unprotects the page
4. Tracks `accessed_pages` and `write_accessed_pages` for state diff computation
5. `PagePrefix` ([u8; 32]) namespaces pages per program via `memory_infix`

This avoids loading entire program memory upfront — only touched pages are loaded from storage.

### Pallet-Gear Extrinsics

| Extrinsic | Purpose |
|-----------|---------|
| `upload_code` | Store WASM code blob (CodeId = blake2 hash) |
| `upload_program` | Create program + enqueue Init dispatch. ProgramId = blake2(code + salt) |
| `send_message` | Enqueue Handle dispatch to a program |
| `send_reply` | Reply to a message in caller's mailbox |
| `claim_value` | Remove message from mailbox, transfer attached value |
| `claim_value_to_inheritor` | Traverse exited/terminated inheritance chain to recover stuck balance |
| `run` | **Mandatory inherent** — processes message queue within block gas limits. Once per block. |

### Builtin Actors

Native Rust actors registered in `pallet-gear-builtin` — no WASM, no gas metering, instant execution. Implement `BuiltinActor` trait with `handle()` and `max_gas()`. Actor IDs derived from `blake2(b"built/in" ++ builtin_id)`.

Available builtins: BLS12-381 verification, staking proxy, message proxy, Ethereum bridge.

### Voucher System

`pallet-gear-voucher` lets sponsors pre-fund gas for specific users/programs. `VoucherInfo` stores: owner, spender, balance, optional program whitelist, duration, code_uploading flag. Voucher IDs are nonce-based H256. Vouchers can only pay for gas on whitelisted programs — not general transfers.

### Ethexe (Ethereum Execution Layer)

Separate Cargo workspace (`ethexe/Cargo.toml`) that runs Gear programs on Ethereum. This is the primary active development area.

#### Crate Map

| Crate | Path | Purpose |
|-------|------|---------|
| ethexe-cli | `ethexe/cli` | Binary. Commands: `run`, `key`, `tx`, `check` |
| ethexe-service | `ethexe/service` | Main orchestrator — binds observer, consensus, compute, network, RPC, prometheus into one async service |
| ethexe-consensus | `ethexe/consensus` | Validator state machine (Initial → Producer/Subordinate → Coordinator/Participant). `ConsensusService` trait with `ValidatorService` and `ConnectService` impls |
| ethexe-compute | `ethexe/compute` | Orchestrates code preparation and program execution. Emits `ComputeEvent`s |
| ethexe-processor | `ethexe/processor` | Executes Gear programs in Wasmtime. `ProcessorExt` trait with `process_programs()` and `execute_for_reply()` |
| ethexe-runtime | `ethexe/runtime` | Gear runtime compiled to WASM itself — the actual computation binary run by validators |
| ethexe-runtime-common | `ethexe/runtime/common` | Shared types: `TransitionController`, `ProgramState`, storage traits |
| ethexe-observer | `ethexe/observer` | Watches Ethereum blocks, syncs chain state, decodes Router contract events |
| ethexe-ethereum | `ethexe/ethereum` | Contract interaction layer: `Router`, `Mirror`, `Middleware`, `WVara` wrappers + event builders |
| ethexe-blob-loader | `ethexe/blob-loader` | Loads code blobs from Ethereum beacon chain (EIP-4844) |
| ethexe-network | `ethexe/network` | libp2p P2P: gossipsub (validator msgs), Kademlia DHT, request-response (db-sync). Port 20333, protocol `ethexe/0.1.0` |
| ethexe-db | `ethexe/db` | Storage abstraction: `CASDatabase` (content-addressed), `KVDatabase` (key-value). Impls: `RocksDatabase`, `MemDb` |
| ethexe-common | `ethexe/common` | Shared types: `BlockHeader`, `Announce`, `SimpleBlockData`, `InjectedTransaction`, `Promise`, crypto primitives |
| ethexe-rpc | `ethexe/rpc` | JSON-RPC 2.0 server (jsonrpsee): `BlockApi`, `CodeApi`, `ProgramApi`, `InjectedApi` |
| ethexe-sdk | `ethexe/sdk` | Rust SDK: `VaraEthApi`, contract wrappers for external consumers |
| ethexe-prometheus | `ethexe/prometheus` | Metrics exposure via Prometheus HTTP |

#### Ethexe Architecture

```
Ethereum Chain
    ↓ (blocks, events)
Observer ──→ ConsensusService
                ├── receive_new_chain_head()
                ├── receive_announce()
                └── receive_validation_request()
                      ↓ (ConsensusEvent)
              ComputeService
                ├── fetch codes (BlobLoader)
                ├── prepare block
                ├── execute programs (Processor → Runtime WASM)
                └── emit ComputeEvent
                      ↓
              ┌── Network (gossip announces to peers)
              ├── RPC (serve client queries)
              └── Ethereum (submit batch commitments via Router)
```

All services run concurrently in ethexe-service, communicating via async event channels.

#### Validator State Machine

```
Initial (per block)
    ├─→ Producer (elected) → Coordinator → Initial
    └─→ Subordinate → Participant → Initial
```

Transitions driven by block arrival, era changes, validation requests/replies, and network announces.

#### Solidity Contracts (`ethexe/contracts/`)

Foundry/Forge project, Solidity 0.8.33:

- **Router.sol**: Central co-processor contract (UUPS upgradeable). Validators submit batch commitments (`submitBatchCommitment`), once 2-of-3 threshold reached → `commitBatch` applies state. Manages code validation, program creation, validator set, eras.
- **Mirror.sol**: Per-program proxy. Stores `stateHash` + `nonce`. `sendMessage()` for user→program, `performStateTransition()` for validated state updates from Router.
- **WrappedVara.sol**: ERC20 wrapper for Vara token.
- **Middleware.sol** / **POAMiddleware.sol**: Validator election and permissions. Middleware integrates Symbiotic (operator/vault staking, slashing, rewards); POAMiddleware is a simpler fixed-set variant.
- **Libraries**: `Gear.sol` (core structs, FROST crypto), `SSTORE2.sol` (cheap code storage), `Clones.sol` / `ClonesSmall.sol` (Mirror proxy cloning).

Contract ABIs must be copied after Forge build: `make ethexe-contracts-pre-commit` (copies from `ethexe/contracts/out/` to `ethexe/ethereum/abi/`).

#### Key Types

```rust
// Block model
SimpleBlockData { hash: H256, header: BlockHeader }
BlockHeader { height: u32, timestamp: u64, parent_hash: H256 }

// Announces — block execution results committed by validators
Announce { block_hash, parent: HashOf<Announce>, gas_allowance, injected_transactions }

// Program state tracking
ProgramStates: BTreeMap<ActorId, StateHashWithQueueSize>

// Cross-chain messaging
InjectedTransaction { origin, payload, reply_to }
SignedInjectedTransaction { tx, signature }

// Block events from Ethereum
BlockEvent::Mirror { actor_id, event: MirrorEvent }
BlockEvent::Router { event: RouterEvent }
```

#### Database Schema

```
CASDatabase: read(hash) → Vec<u8>, write(data) → H256    (code blobs, state)
KVDatabase:  get(key) → Vec<u8>, put(key, data)           (metadata)

Key prefixes (enum):
  BlockSmallData(H256), BlockEvents(H256),
  AnnounceProgramStates(HashOf<Announce>), AnnounceSchedule(HashOf<Announce>),
  ProgramToCodeId(ActorId), InstrumentedCode(u32, CodeId),
  CodeMetadata(CodeId), CodeValid(CodeId),
  InjectedTransaction(HashOf<Tx>),
  Config, Globals, LatestEraValidatorsCommitted(H256)
```

Impls: `RocksDatabase` (persistent, snappy compression) and `MemDb` (in-memory/testing). Database has versioned migrations.

#### Network Protocol

libp2p stack: QUIC + TCP + DNS + TLS + Yamux muxing.
- **Gossipsub**: validator messages and promises (with peer scoring)
- **Kademlia DHT**: peer discovery
- **Request-response** (`/ethexe/db-sync/1.0.0`): sync announces and codes between peers
- Constants: `DEFAULT_LISTEN_PORT = 20333`, `MAX_ESTABLISHED_CONNECTIONS = 500`

#### CLI Usage

```bash
ethexe run --ethereum-rpc <URL> --router-address <ADDR>   # Start node
ethexe run --validator <PUBKEY_OR_RANDOM>                  # Validator mode
ethexe key new                                             # Generate keypair
ethexe tx submit-injected                                  # Submit injected tx
ethexe check --all                                         # Validate DB integrity
```

#### Ethexe Runtime Differences from Vara

The ethexe runtime (`ethexe/runtime/`) is Gear runtime compiled to `no_std` WASM for EVM context:
- No Substrate pallets
- No signaling syscalls, no reservation operations
- Simplified host function interface via `RuntimeInterface` trait
- `ethexe` feature flag in gstd/gcore disables unavailable syscalls

#### Ethexe Testing

```bash
cargo nextest run -p "ethexe-*" --no-fail-fast   # All ethexe tests
forge test --root ethexe/contracts -vvv           # Solidity contract tests
```

Integration tests in ethexe-service use Anvil (local Ethereum) with mock contracts. Demo programs for testing: `demo-ping`, `demo-async`, `demo-panic-payload`, `demo-value-sender-ethexe`, etc. Use `MemDb` / `create_initialized_empty_memory_db()` for test databases.

### no_std Boundary

```
WASM program (no_std): gstd → gcore → gsys, gprimitives, gear-core-errors
                            ↕ syscall FFI boundary
Native runtime (std):   core-backend → core-processor → pallets → runtime/vara
```

`no_std` crates use `extern crate alloc` for Vec/BTreeMap/Arc. `std` feature is excluded by `wasm-builder` during program compilation. `ethexe` feature flag in gstd/gcore disables certain syscalls unavailable in the Ethereum context.

### WASM Builder

`utils/wasm-builder` hooks into `build.rs` to compile Gear programs:
1. Reads `CARGO_MANIFEST_DIR` and feature flags
2. Compiles to `wasm32-unknown-unknown` with nightly toolchain
3. Applies `wasm-instrument` for gas metering and stack height checks
4. Optimizes via `wasm-optimizer`
5. Set `__GEAR_WASM_BUILDER_NO_BUILD=1` or `SKIP_WASM_BUILD=1` to skip (used in doc tests)

### Testing (gtest)

`gtest` simulates a local blockchain. Key abstractions:
- `System` — singleton runtime environment (one per thread)
- `Program` — deployed program handle with `send_bytes()`, `read_state()`
- `System::run_next_block()` — increments block, processes TaskPool then message Queue
- `Log` — message log builder for assertions
- `BlockRunResult` — contains succeed/failed/panicked message ID sets
- `WasmProgram` trait — mock programs with custom `init()`/`handle()`/`state()` for unit testing without WASM

### Key Error Categories

`gear-core-errors` defines `ExtError` with sub-enums:
- `ExecutionError` (100s): NotEnoughGas, NotEnoughValue, NoReplyContext, NoSignalContext
- `MemoryError` (200s): RuntimeAllocOutOfBounds, AccessOutOfBounds
- `MessageError` (300s): MaxMessageSizeExceed, DuplicateReply, DuplicateWaking, OutgoingMessagesAmountLimitExceeded, InsufficientValue/GasLimit
- `ReservationError` (500s): InvalidReservationId, ReservationsLimitReached, ZeroReservationDuration

Errors are encoded as little-endian u32. Code `0xffff` is reserved for SyscallUsage, `600` is deprecated (ProgramRent).

## Code Conventions

- `rustfmt.toml`: `imports_granularity = "Crate"`, `format_code_in_doc_comments = true`
- CI uses `[skip-ci]` (not `[skip ci]`) in commit messages to get green status checks while skipping workflows
- `cargo nextest` is the test runner (not `cargo test`), except for doc tests
- `cargo hakari` manages workspace dependency deduplication — run `make workspace-hack` after dependency changes

## GitHub PR Review

When asked to review a PR (e.g. `@claude review` in a PR comment):

1. Read `.gemini/styleguide.md` for review priorities, guidelines, and anti-noise rules. Follow them strictly.
2. Use the `/code-review:code-review` command with `--comment` flag to perform the review and post inline comments on specific lines.
   Example: `/code-review:code-review OWNER/REPO/pull/NUMBER --comment`
