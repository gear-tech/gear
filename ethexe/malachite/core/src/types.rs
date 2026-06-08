// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Core public types for [`crate::MalachiteService`].

use gear_core::limited::LimitedVec;
pub use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};
use serde::{Deserialize, Serialize};

/// Hard cap on a block's encoded application payload — the SCALE-encoded
/// application operation list carried in [`Block::payload`] (the service treats
/// it as opaque bytes; the schema lives in the application crate).
///
/// The whole [`Block`] ships as a single gossipsub message: the proposer
/// streams it as one `Data` proposal part, and the value-sync path fetches a
/// finalized block in one request-response round. Malachite's `pubsub_max_size`
/// (the gossipsub `max_transmit_size`) defaults to 4 MiB, so the encoded block
/// must stay well under that. Realistic content is ~127 KiB
/// (`MAX_INJECTED_TRANSACTIONS_SIZE_PER_MB` plus three protocol operations); the
/// 1 MiB cap leaves ~8x headroom for future operation variants while staying
/// ~4x under the 4 MiB transport ceiling (block envelope + SCALE / stream
/// framing fit comfortably in the remaining margin).
pub const MAX_BLOCK_PAYLOAD_BYTES: usize = 1024 * 1024;

/// Size-capped opaque application payload carried by [`Block::payload`].
pub type BlockPayload = LimitedVec<u8, MAX_BLOCK_PAYLOAD_BYTES>;

/// 20-byte validator address.
///
/// Newtype around [`gsigner::schemes::secp256k1::Address`] so the
/// service's API and the typical application code (ethexe today,
/// arbitrary other consumers tomorrow) share a single address shape
/// without each side reaching across crate boundaries for the inner
/// representation.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    derive_more::Display,
)]
#[display("0x{}", hex::encode(_0.0))]
pub struct Address(pub gsigner::schemes::secp256k1::Address);

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
/// chain-position fields the service needs (parent hash, height) and
/// a [`Self::reserved`] tail kept for future protocol extensions.
///
/// The block hash ([`Self::hash`]) is the [`gear_core::utils::hash`]
/// (Blake2b-256) over a SCALE-encoded
/// `(parent_hash, height, payload_hash, reserved)` tuple, where
/// `payload_hash = gear_core::utils::hash(payload.encode())`.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct Block {
    pub parent_hash: H256,
    pub height: u64,
    pub payload: BlockPayload,
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

    /// Compute the canonical 32-byte block hash. Deterministic — two
    /// nodes with the same `(parent_hash, height, payload, reserved)`
    /// produce the same hash.
    pub fn hash(&self) -> H256 {
        let payload_bytes = self.payload.encode();
        let payload_hash: H256 = gear_core::utils::hash(&payload_bytes).into();
        let inner = (self.parent_hash, self.height, payload_hash, self.reserved).encode();
        gear_core::utils::hash(&inner).into()
    }
}

/// Quorum-signed certificate proving a height was finalized.
///
/// `signatures` is a parallel-to-validators vector of raw 64-byte
/// secp256k1 signatures (`r || s`); the application is responsible
/// for reconstructing the validator-set ordering when verifying it on
/// chain (or wherever else).
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, Serialize, Deserialize)]
pub struct CommitCertificate {
    pub height: u64,
    pub block_hash: H256,
    pub signatures: Vec<Vec<u8>>,
}
