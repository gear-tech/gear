// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! ethexe common types and traits.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod consensus;
pub mod db;
pub mod events;
pub mod gear;
mod hash;
pub mod injected;
pub mod network;
mod primitives;
mod utils;
mod validators;

#[cfg(feature = "std")]
pub mod futures;

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

/// Commitment delay limit in blocks.
/// This is the maximum number of blocks that can pass
/// since some not-base announce was created until it can be committed,
/// any not-base announce older than this limit must be discarded.
pub const COMMITMENT_DELAY_LIMIT: u32 = 3;
