// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! # ethexe-common
//!
//! Shared vocabulary crate for the ethexe execution layer: block model, on-chain
//! events, validator commitments, injected transactions, storage trait abstractions,
//! and protocol constants. It defines shapes and trait interfaces only — the
//! concrete storage backends live in `ethexe-db`. Being `no_std`-compatible, it
//! links into both the WASM runtime
//! (`ethexe-runtime`) and the native node binary.
//!
//! ## Role in the stack
//!
//! This crate depends on no other ethexe workspace member (it sits on `gear-core`,
//! `gprimitives`, and `gsigner`) and nearly every other ethexe crate depends on it,
//! making it a foundational leaf. For example, `ethexe-consensus` exchanges
//! [`consensus::BatchCommitmentValidationRequest`] /
//! [`consensus::BatchCommitmentValidationReply`] messages defined here, and
//! `ethexe-db` provides backends for the [`db`] storage traits declared here.
//!
//! ## Public API
//!
//! - [`consensus`] — Validation request/reply messages and timeline helpers for the batch commitment protocol.
//! - [`db`] — `*StorageRO` / `*StorageRW` trait abstractions and block-metadata types.
//! - [`events`] — On-chain event model: `BlockEvent` (Mirror/Router variants) and `WVaraEvent`.
//! - [`gear`] — Protocol commitments ([`gear::BatchCommitment`] and siblings) and [`gear::StateTransition`].
//! - [`injected`] — Injected transactions, promises, and receipts for inbound cross-chain messaging.
//! - [`malachite`] — Sequencer block-payload shape ([`malachite::Operations`], `Operation`).
//! - [`network`] — Validator network messages (`ValidatorMessage` and signed/verified variants).
//! - [`ecdsa`] — secp256k1 re-exports from `gsigner`.
//! - [`mock`] — Test helpers and proptest fixtures (feature `mock`).
//!
//! Flattened crate-root re-exports include [`HashOf`], [`MaybeHashOf`],
//! [`BlockHeader`], [`SimpleBlockData`], [`BlockData`], [`ValidatorsVec`],
//! [`EmptyValidatorsError`], and the `gsigner` crypto surface ([`Address`],
//! [`Digest`], [`PublicKey`], [`Signature`], [`SignedData`], [`ToDigest`],
//! [`VerifiedData`], …).
//!
//! Crate-root constants include the per-MB soft execution limits
//! ([`OUTGOING_MESSAGES_SOFT_LIMIT`], [`OUTGOING_MESSAGES_BYTES_SOFT_LIMIT`],
//! [`CALL_REPLY_SOFT_LIMIT`], [`PROGRAM_MODIFICATIONS_SOFT_LIMIT`],
//! [`MAX_TOUCHED_PROGRAMS_PER_MB`]) and [`DEFAULT_BLOCK_GAS_LIMIT`].
//!
//! ## Key types
//!
//! - [`HashOf<T>`] — phantom-typed `H256` wrapper preventing mixing of hashes of
//!   different payload kinds; [`MaybeHashOf<T>`] is its optional sibling.
//! - [`BlockHeader`] / [`SimpleBlockData`] / [`BlockData`] — the ethexe block model.
//! - [`gear::BatchCommitment`] and sibling commitment types — the validator-submitted
//!   commitment hierarchy; each implements [`ToDigest`] for Keccak256 hashing.
//! - [`gear::StateTransition`] — a single validated program state change.
//! - [`ValidatorsVec`] — a `NonEmpty<Address>` wrapper guaranteeing the validator set
//!   is never empty.
//! - [`injected::InjectedTransaction`] / [`injected::Promise`] — inbound cross-chain
//!   transaction and its promise/receipt lifecycle.
//!
//! ## Invariants
//!
//! - [`ValidatorsVec`] cannot be constructed from an empty collection; `try_from`
//!   returns [`EmptyValidatorsError`] on empty input.
//! - [`DEFAULT_COMMITMENT_DELAY_LIMIT`] is a coordinator-local knob, not a protocol
//!   constant — each coordinator selects its own value.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod consensus;
pub mod db;
pub mod events;
pub mod gear;
mod hash;
pub mod injected;
pub mod malachite;
pub mod network;
mod primitives;
mod utils;
mod validators;

#[cfg(feature = "mock")]
pub mod mock;

pub use gsigner::{
    Address, ContractSignature, Digest, FromActorIdError, PrivateKey, PublicKey, Signature,
    SignedData, SignedMessage, ToDigest, VerifiedData,
};
pub use validators::{EmptyValidatorsError, ValidatorsVec};
pub mod ecdsa {
    #[cfg(feature = "std")]
    pub use gsigner::secp256k1::Signer;
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
pub const OUTGOING_MESSAGES_SOFT_LIMIT: u32 = 128;
pub const OUTGOING_MESSAGES_BYTES_SOFT_LIMIT: u32 = 32 * 1024;
pub const CALL_REPLY_SOFT_LIMIT: u32 = 4;
pub const PROGRAM_MODIFICATIONS_SOFT_LIMIT: u32 = MAX_TOUCHED_PROGRAMS_PER_MB / 2;

/// Old mailbox validity version (in Ethereum blocks).
pub const MAILBOX_VALIDITY_VERSION_1: core::num::NonZero<u32> =
    core::num::NonZero::new(54_000).expect("54_000 != 0");

/// New mailbox validity version (in Ethereum blocks). 15 minutes at 12s block time.
pub const MAILBOX_VALIDITY_VERSION_2: core::num::NonZero<u32> =
    core::num::NonZero::new(15 * 60 / 12).expect("75 != 0");
