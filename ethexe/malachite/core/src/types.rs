// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Core public types for [`crate::MalachiteService`].

use derive_where::derive_where;
pub use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};
use serde::{Deserialize, Serialize};
use std::fmt::Display;

use crate::externalities::BlockPayload;

/// 20-byte validator address.
///
/// Newtype around [`gsigner::schemes::secp256k1::Address`] so the
/// service's API and the typical application code (ethexe today,
/// arbitrary other consumers tomorrow) share a single address shape
/// without each side reaching across crate boundaries for the inner
/// representation.
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
/// chain-position fields the service needs (parent hash, height) and
/// a [`Self::reserved`] tail kept for future protocol extensions.
///
/// The block hash ([`Self::hash`]) is the [`gear_core::utils::hash`]
/// (Blake2b-256) over a SCALE-encoded
/// `(parent_hash, height, payload_hash, reserved)` tuple, where
/// `payload_hash = gear_core::utils::hash(payload.encode())`.
#[derive_where(Clone)]
#[derive(Encode, Decode)]
pub struct Block<P: BlockPayload> {
    pub parent_hash: H256,
    pub height: u64,
    pub payload: P,
    pub reserved: [u8; 64],
}

impl<P: BlockPayload> Block<P> {
    /// Construct a block with `reserved` zeroed out.
    pub fn new(parent_hash: H256, height: u64, payload: P) -> Self {
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
