// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! # ethexe-common
//!
//! Shared vocabulary crate for the ethexe execution layer: block model, on-chain
//! events, validator commitments, injected transactions, storage trait abstractions,
//! and protocol constants used by every other ethexe crate.
//!
//! ## Responsibilities
//!
//! This crate defines *shapes* and trait *interfaces* only — no execution logic,
//! no Ethereum I/O, no networking, and no database backend implementation. Concrete
//! implementations live in `ethexe-db`, `ethexe-observer`, `ethexe-processor`, and
//! so on. Because it is `no_std`-compatible (`#![cfg_attr(not(feature = "std"), no_std)]`),
//! it can be linked into the WASM runtime (`ethexe-runtime`) as well as the native
//! node binary.
//!
//! ## Role in the Stack
//!
//! `ethexe-common` depends on no other ethexe workspace member. Every other ethexe
//! crate (ethexe-service, ethexe-consensus, ethexe-compute, ethexe-processor,
//! ethexe-network, ethexe-ethereum, ethexe-observer, ethexe-db, …) depends on it
//! directly, making it a flat leaf:
//!
//! ```text
//! ethexe-service  ethexe-consensus  ethexe-compute  ethexe-processor
//! ethexe-network  ethexe-ethereum   ethexe-observer  ethexe-db  …
//!         \               |                |               /
//!                     ethexe-common  (leaf)
//!                   gear-core / gprimitives / gsigner
//! ```
//!
//! `ethexe-consensus` exchanges [`consensus::BatchCommitmentValidationRequest`] /
//! [`consensus::BatchCommitmentValidationReply`] messages whose types originate here.
//! `ethexe-processor` accepts [`malachite::Transactions`] without importing the
//! consensus layer because the Malachite payload types are defined here.
//! `ethexe-db` provides backends for the [`db`] storage traits declared here.
//!
//! ## Public API
//!
//! Public modules:
//!
//! - [`consensus`] — validation request/reply messages and timeline helpers for
//!   the 2-of-3 batch commitment protocol.
//! - [`db`] — `*StorageRO` / `*StorageRW` trait abstractions (`HashStorageRO`,
//!   `BlockMetaStorageRO`, `CodesStorageRW`, `OnChainStorageRW`, `InjectedStorageRW`,
//!   `MbStorageRW`, …) and block-metadata types.
//! - [`events`] — on-chain event model: `BlockEvent` (Mirror/Router variants) and the
//!   separate `WVaraEvent` token-event type.
//! - [`gear`] — protocol commitments (`BatchCommitment`, `ChainCommitment`,
//!   `CodeCommitment`, `ValidatorsCommitment`, `RewardsCommitment`) and `StateTransition`.
//! - [`injected`] — injected transactions, promises, and receipts for inbound
//!   cross-chain messaging.
//! - [`malachite`] — sequencer block-payload shape (`Transactions`, `Transaction`).
//! - [`network`] — validator network messages (`ValidatorMessage` and signed/verified
//!   variants).
//! - [`ecdsa`] — secp256k1 re-exports from `gsigner`.
//! - [`mock`] (feature `mock`) — test helpers and proptest fixtures.
//!
//! Flattened re-exports at crate root: [`HashOf`], [`MaybeHashOf`], [`BlockHeader`],
//! [`SimpleBlockData`], [`BlockData`], [`ValidatorsVec`], [`EmptyValidatorsError`],
//! and the `gsigner` crypto surface (`Address`, `Digest`, `PublicKey`, `Signature`,
//! `SignedData`, `ToDigest`, `VerifiedData`, …).
//!
//! Crate-root constants include per-MB soft execution limits
//! (`OUTGOING_MESSAGES_SOFT_LIMIT`, `OUTGOING_MESSAGES_BYTES_SOFT_LIMIT`,
//! `CALL_REPLY_SOFT_LIMIT`, `PROGRAM_MODIFICATIONS_SOFT_LIMIT`,
//! `MAX_TOUCHED_PROGRAMS_PER_MB`) alongside `DEFAULT_BLOCK_GAS_LIMIT`.
//!
//! ## Key Types
//!
//! - [`HashOf<T>`] — a phantom-typed `H256` wrapper that prevents mixing hashes of
//!   different payload kinds (e.g. `HashOf<BatchCommitment>` vs `HashOf<CodeCommitment>`).
//!   [`MaybeHashOf<T>`] is its optional sibling.
//! - [`BlockHeader`] / [`SimpleBlockData`] / [`BlockData`] — the ethexe block model
//!   (`height`, `timestamp`, `parent_hash`, events, derived data).
//! - [`gear::BatchCommitment`] and sibling commitment types — the validator-submitted
//!   commitment hierarchy; each implements [`ToDigest`] for Keccak256 hashing into the
//!   EVM Router contract.
//! - [`gear::StateTransition`] — a single validated program state change applied via
//!   the Mirror contract.
//! - [`ValidatorsVec`] — a `NonEmpty<Address>` wrapper that adds `Encode`/`Decode`
//!   and guarantees the validator set is never empty.
//! - [`injected::InjectedTransaction`] / [`injected::Promise`] — inbound cross-chain
//!   transaction and its promise/receipt lifecycle.
//!
//! ## Invariants
//!
//! - [`ValidatorsVec`] cannot be constructed from an empty collection;
//!   `ValidatorsVec::try_from` returns [`EmptyValidatorsError`] on empty input.
//! - [`HashOf`] requires a non-empty type name at construction time (panics otherwise).
//! - `CodeAndId::from_unchecked` panics if the supplied `code_id` does not match the
//!   hash of the provided code bytes.
//! - [`DEFAULT_COMMITMENT_DELAY_LIMIT`] is a coordinator-local knob, not a protocol
//!   constant — each coordinator selects its own value.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

/// Validation request/reply messages and timeline helpers for the 2-of-3 batch commitment protocol.
pub mod consensus;
/// Storage trait abstractions (`*StorageRO` / `*StorageRW`) and block-metadata types; concrete backends live in `ethexe-db`.
pub mod db;
/// On-chain event model: `BlockEvent` (Mirror/Router variants) and the `WVaraEvent` token-event type.
pub mod events;
/// Protocol commitment types (`BatchCommitment`, `ChainCommitment`, `CodeCommitment`, etc.) and `StateTransition`.
pub mod gear;
mod hash;
/// Injected transactions, promises, and receipts for inbound cross-chain messaging.
pub mod injected;
/// Sequencer block-payload shape (`Transactions`, `Transaction`) consumed by the compute layer.
pub mod malachite;
/// Validator network messages (`ValidatorMessage` and signed/verified variants).
pub mod network;
mod primitives;
mod utils;
mod validators;

/// Test helpers and proptest fixtures (enabled by the `mock` feature).
#[cfg(feature = "mock")]
pub mod mock;

pub use gsigner::{
    Address, ContractSignature, Digest, FromActorIdError, PrivateKey, PublicKey, Signature,
    SignedData, SignedMessage, ToDigest, VerifiedData,
};
pub use validators::{EmptyValidatorsError, ValidatorsVec};
/// secp256k1 crypto re-exports from `gsigner` for use without the full `gsigner` dependency surface.
pub mod ecdsa {
    pub use gsigner::secp256k1::{
        ContractSignature, PrivateKey, PublicKey, Signature, SignedData, SignedMessage,
        VerifiedData,
    };
}
pub use gear_core;
pub use gprimitives;
pub use hash::*;
pub use k256;
pub use primitives::*;
pub use sha3;
pub use utils::*;

/// Default block gas limit for the node.
pub const DEFAULT_BLOCK_GAS_LIMIT: u64 = 4_000_000_000_000;

/// Default `commitment_delay_limit` (in Ethereum blocks). Coordinator-local
/// knob: how many EBs a `BatchCommitment` stays valid past its target block.
/// Not a protocol constant — every coordinator picks its own value.
pub const DEFAULT_COMMITMENT_DELAY_LIMIT: core::num::NonZero<u8> =
    core::num::NonZero::new(16).expect("16 != 0");

/// Maximum number of touched programs per MB.
pub const MAX_TOUCHED_PROGRAMS_PER_MB: u32 = 128;

// Soft limits for one MB processing. Stops execution if any of them is exceeded.
/// Maximum number of outgoing messages produced by a single MB before execution stops.
pub const OUTGOING_MESSAGES_SOFT_LIMIT: u32 = 128;
/// Maximum total byte size of outgoing messages produced by a single MB before execution stops.
pub const OUTGOING_MESSAGES_BYTES_SOFT_LIMIT: u32 = 32 * 1024;
/// Maximum number of call-reply messages produced by a single MB before execution stops.
pub const CALL_REPLY_SOFT_LIMIT: u32 = 4;
/// Maximum number of program state modifications allowed per MB; half of [`MAX_TOUCHED_PROGRAMS_PER_MB`].
pub const PROGRAM_MODIFICATIONS_SOFT_LIMIT: u32 = MAX_TOUCHED_PROGRAMS_PER_MB / 2;
