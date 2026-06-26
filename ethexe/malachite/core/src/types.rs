// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Core public types for [`crate::MalachiteCore`].

use gear_core::limited::LimitedVec;
pub use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};
use serde::{Deserialize, Serialize};
use std::fmt::Display;

/// Hard cap on a block's encoded application payload. The whole [`Block`]
/// ships as a single gossipsub message, so the cap must stay well under
/// malachite's 4 MiB transport ceiling.
pub const MAX_BLOCK_PAYLOAD_BYTES: usize = 1024 * 1024;

/// Size-capped opaque application payload carried by [`Block::payload`].
pub type BlockPayload = LimitedVec<u8, MAX_BLOCK_PAYLOAD_BYTES>;

/// 20-byte validator address (newtype around
/// [`gsigner::schemes::secp256k1::Address`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Address(pub gsigner::schemes::secp256k1::Address);

impl Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "0x{}", hex::encode(self.0.0))
    }
}

impl Address {
    pub const fn from_inner(addr: gsigner::schemes::secp256k1::Address) -> Self {
        Self(addr)
    }

    pub fn as_bytes(&self) -> &[u8; 20] {
        &self.0.0
    }

    /// Derive an address from an ECDSA public key:
    /// `keccak256(uncompressed_pubkey[1..])[12..]`. Equivalent to
    /// the standard Ethereum address derivation.
    pub fn from_public_key(pk: &crate::signing::PublicKey) -> Self {
        Self(gsigner::schemes::secp256k1::Address(
            crate::signing::address_bytes_from_public_key(pk),
        ))
    }
}

/// Service-level block envelope: the application payload plus the
/// chain-position fields the service needs.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct Block {
    /// Hash of the parent block (zero for genesis).
    pub parent_hash: H256,
    /// Block height (parent height + 1).
    pub height: u64,
    /// Opaque application payload.
    pub payload: BlockPayload,
    /// Reserved tail for future protocol extensions; currently zeroed.
    pub reserved: [u8; 64],
}

impl Block {
    /// Construct a block with `reserved` zeroed out.
    pub fn new(parent_hash: H256, height: u64, payload: BlockPayload) -> Self {
        Self {
            parent_hash,
            height,
            payload,
            reserved: [0u8; 64],
        }
    }

    /// Canonical block hash: Blake2b-256 over the SCALE-encoded
    /// `(parent_hash, height, payload_hash, reserved)` tuple.
    pub fn hash(&self) -> H256 {
        let payload_bytes = self.payload.encode();
        let payload_hash: H256 = gear_core::utils::hash(&payload_bytes).into();
        let inner = (self.parent_hash, self.height, payload_hash, self.reserved).encode();
        gear_core::utils::hash(&inner).into()
    }
}

/// Quorum-signed certificate proving a height was finalized.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, Serialize, Deserialize)]
pub struct CommitCertificate {
    /// Finalized height.
    pub height: u64,
    /// Hash of the finalized block.
    pub block_hash: H256,
    /// Raw 64-byte secp256k1 signatures (`r || s`), in validator-set order.
    pub signatures: Vec<Vec<u8>>,
}
