// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! ethexe common types and traits.

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
