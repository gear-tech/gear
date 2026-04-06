# Gear Gemini Review Style Guide

## Purpose

Review pull requests for the whole `gear` workspace with a balanced posture: prioritize correctness, safety-critical behavior, and verification gaps before maintainability comments.

## Repository Context

> **Note:** The canonical source for architecture documentation is [CLAUDE.md](../CLAUDE.md).
> This section is a subset kept in sync for Gemini Code Assist, which only reads `styleguide.md`.

Gear Protocol — a Substrate-based platform for running WebAssembly smart contracts (programs) with an actor-model message-passing architecture. The main network is **Vara**. The repo also contains **ethexe**, a layer that runs Gear programs on Ethereum.

- Rust edition 2024, MSRV 1.91, pinned nightly `nightly-2025-10-20` (see `rust-toolchain.toml`)
- 114-crate Cargo workspace; default member is `node/cli` (the Vara node binary)
- `ethexe/` is excluded from the main workspace and has its own Cargo.toml + Solidity contracts (Foundry/Forge)
- Sails framework lives in a separate repo (github.com/gear-tech/sails), not here

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

Type progression: packets → typed messages → `Dispatch` → `StoredDispatch` (gasless, in queue) → `IncomingDispatch` (with gas, for execution). `ContextStore` preserves async execution state. Reply/signal messages carry `MessageDetails` with origin message ID and result code.

### Program Lifecycle

```
[not exists] → upload_program → Active(Uninitialized { message_id })
  → init succeeds → Active(Initialized)
  → init fails → Terminated(inheritor)
  → gr_exit called → Exited(inheritor)
  → expiration_block reached → removed
```

`ActiveProgram` fields: `code_id`, `state`, `allocations_tree_len`, `memory_infix` (lazy-page storage namespace), `gas_reservation_map`, `expiration_block`.

### Gas Model

UTXO-like gas tree (`GasTree`) where each message owns a gas node. `GasCounter` tracks `(left, burned)`. Tree operations: `create`, `split`, `split_with_value`, `cut`, `consume`, `lock`/`unlock`. Lock IDs: `Mailbox`, `Waitlist`, `Reservation`, `DispatchStash`. `GasAllowanceCounter` enforces block-level gas budget. `ReservableTree` trait adds gas reservation management.

### Queues: Waitlist, Mailbox, TaskPool

**Message Queue** (gear-messenger): FIFO linked-list of `StoredDispatch` — processed each block by `run()`.

**Waitlist** (gear-messenger): Programs waiting for a reply. Keyed by `(ActorId, MessageId) → StoredDispatch`.

**Mailbox** (gear-messenger): Messages to user accounts. Keyed by `(AccountId, MessageId) → UserStoredMessage`. Rent-based TTL via `MailboxThreshold`.

**TaskPool** (gear-scheduler): Delayed actions keyed by `(BlockNumber, Task)`. Types: delayed send, wake, mailbox expiry, reservation cleanup, stall timeout.

### Lazy Pages

Signal-based demand paging: `mprotect` on WASM pages → SIGSEGV handler loads from `LazyPagesStorage` → tracks accessed/write-accessed pages for state diff. `PagePrefix` ([u8; 32]) namespaces per program via `memory_infix`.

### Key Layers

**Program-facing libraries** (`no_std`, compiled to WASM):
- `gstd` — high-level standard library (async runtime, message API, sync primitives)
- `gcore` — low-level syscall bindings
- `gsys` — raw system-level constants and FFI
- `gprimitives` — shared types (ActorId, MessageId, CodeId)

**Core engine** (native):
- `core/` (`gear-core`) — fundamental types: messages, programs, gas, memory pages, code
- `core/processor/` (`gear-core-processor`) — executes programs, manages context and state transitions
- `core/backend/` (`gear-core-backend`) — bridges processor to WASM sandbox, implements syscall dispatch

**Runtime integration**:
- `pallets/gear/` — main Substrate pallet; manages message queue, mailbox, waitlist, program/code storage
- `pallets/gear-messenger/` — message queue and mailbox storage
- `pallets/gear-scheduler/` — task scheduling across blocks
- `pallets/gear-bank/` — gas and balance accounting
- `pallets/gear-voucher/` — pre-paid call vouchers (sponsor gas for specific users/programs)
- `pallets/gear-builtin/` — native Rust actors (BLS12-381, staking, proxy, eth-bridge) — no WASM, no gas metering
- `runtime/vara/` — Vara network runtime configuration

**WASM tooling**:
- `utils/wasm-builder/` — compiles Rust programs to WASM with gas metering injection
- `utils/wasm-instrument/` — bytecode instrumentation (gas metering, stack checks)
- `sandbox/` — WASM VM abstraction (Wasmer/Wasmtime backends)
- `lazy-pages/` — demand-paged memory (signal-based, OS-specific)

**Ethereum execution layer** (`ethexe/`):
- Separate Cargo workspace; runs Gear programs on Ethereum. Primary active development area.
- Key crates: `ethexe-service` (main orchestrator), `ethexe-consensus` (validator state machine: Initial → Producer/Subordinate → Coordinator/Participant), `ethexe-compute` (orchestrates execution), `ethexe-processor` (Wasmtime program execution), `ethexe-runtime` (Gear runtime compiled to WASM), `ethexe-runtime-common` (shared types/traits)
- Chain integration: `ethexe-observer` (Ethereum block sync), `ethexe-ethereum` (contract wrappers: Router, Mirror, Middleware, WVara), `ethexe-blob-loader` (EIP-4844 code blobs from beacon chain)
- Infrastructure: `ethexe-network` (libp2p P2P — gossipsub, Kademlia, db-sync), `ethexe-db` (CASDatabase + KVDatabase — RocksDB/MemDb), `ethexe-rpc` (JSON-RPC 2.0: BlockApi, CodeApi, ProgramApi, InjectedApi), `ethexe-prometheus` (metrics)
- Solidity contracts (`ethexe/contracts/`, Foundry/Forge, Solidity 0.8.33):
  - **Router.sol** — UUPS upgradeable co-processor. Validators submit batch commitments, 2-of-3 threshold → `commitBatch`. Manages code validation, program creation, validator set, eras.
  - **Mirror.sol** — per-program proxy. Stores `stateHash` + `nonce`. `sendMessage()` for user→program, `performStateTransition()` for validated state updates.
  - **WrappedVara.sol** — ERC20 wrapper for Vara token.
  - **Middleware.sol** / **POAMiddleware.sol** — validator election and permissions (Symbiotic staking or fixed POA set).
  - Libraries: `Gear.sol` (core structs, FROST crypto), `SSTORE2.sol` (cheap storage), `Clones.sol`/`ClonesSmall.sol` (Mirror cloning).
- Key types: `SimpleBlockData`, `BlockHeader`, `Announce` (block execution results), `InjectedTransaction`/`SignedInjectedTransaction` (cross-chain msgs), `ProgramStates`, `BlockEvent`
- Runtime differences from Vara: no Substrate pallets, no signaling syscalls, no reservation operations; `RuntimeInterface` trait for host functions; `ethexe` feature flag in gstd/gcore disables unavailable syscalls
- Contract ABIs in `ethexe/ethereum/abi/` must be regenerated after contract changes (`make ethexe-contracts-pre-commit`)

**Testing**:
- `gtest` — local blockchain simulator (`System`, `Program`, `run_next_block()`)
- `gsdk` — Rust SDK for running node interaction
- `gcli` — CLI client

### no_std Boundary

```
WASM program (no_std): gstd → gcore → gsys, gprimitives, gear-core-errors
                            ↕ syscall FFI boundary
Native runtime (std):   core-backend → core-processor → pallets → runtime/vara
```

`ethexe` feature flag in gstd/gcore disables syscalls unavailable in the Ethereum context.

### Key Error Categories

`ExtError` sub-enums: `ExecutionError` (100s), `MemoryError` (200s), `MessageError` (300s), `ReservationError` (500s). Encoded as little-endian u32. Code `0xffff` reserved for SyscallUsage.

### Code Conventions

- `rustfmt.toml`: `imports_granularity = "Crate"`, `format_code_in_doc_comments = true`
- CI uses `[skip-ci]` (not `[skip ci]`) in commit messages for green status checks
- `cargo nextest` is the test runner (not `cargo test`), except for doc tests
- `cargo hakari` manages workspace dependency deduplication

## Review Priorities

1. Correctness and protocol behavior.
2. Safety-critical changes.
3. Verification gaps.
4. Maintainability only when it materially affects safety, debugging, or future regressions.

## Correctness And Protocol Behavior

Focus first on:

1. Logic errors and broken edge cases.
2. Invalid state transitions.
3. Event handling mistakes.
4. Ordering assumptions that can break under real execution.
5. Incorrect API, RPC, CLI, or tool usage.
6. Accidental behavior changes in runtime, protocol, batching, queueing, or externally visible flows.
7. Concurrency or race risks when code changes touch async, scheduling, or parallel processing behavior.

## Safety-Critical Changes

Treat these as high-attention areas:

1. Contract upgrades and migrations.
2. Storage compatibility.
3. Access control regressions.
4. Consensus-sensitive logic.
5. Validator, batching, or commitment limits.
6. ABI compatibility and source-to-generated artifact drift.
7. Changes that alter externally visible behavior without clear verification.

## Verification Expectations

Prefer comments about missing verification over comments about code style.

Look for:

1. Behavior changes without tests.
2. New invariants without CI or check coverage.
3. Source changes that appear to require regenerated ABI or artifact updates.
4. Deployment, workflow, or script changes without corresponding source-of-truth updates.
5. Workspace or toolchain changes without validation of cross-workspace effects.

## Anti-Noise Rules

Do not:

1. Comment on formatting already enforced by `rustfmt`, `forge fmt`, or repository linting.
2. Review generated JSON or ABI files directly.
3. Suggest broad refactors unless they have a clear correctness, verification, or maintenance benefit.
4. Flood the pull request with many small comments when one high-signal comment is enough.
5. Focus on naming, wording, or docs style unless the change makes behavior misleading or incomplete.
6. Present speculative concerns as findings without tying them to changed code.

## Repository-Specific Cues

1. Prefer reviewing source-of-truth files over generated artifacts.
2. If `ethexe/contracts/src/` changes, consider whether tests, ABI files, scripts, or relevant README instructions should also change.
3. If `.github/workflows/` changes, focus on weakened enforcement, reduced coverage, or accidental bypasses.
4. If `Cargo.toml`, `rust-toolchain.toml`, or workspace patches change, focus on toolchain alignment, version pinning, and cross-workspace effects.
5. If protocol behavior changes, expect deterministic verification paths and concrete evidence rather than reasoning alone.
6. Prefer comments on missing tests or missing regeneration steps over comments on superficial code organization.

## Generated Files

Generated files are not primary review targets. It is acceptable to comment that a source-of-truth change appears to require regenerated artifacts or committed outputs, but do not review the generated files themselves for formatting or style.
