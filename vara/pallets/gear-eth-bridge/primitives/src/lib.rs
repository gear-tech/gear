// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Gear Ethereum Bridge Primitives.

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
#![doc(html_logo_url = "https://gear-tech.io/logo.png")]
#![doc(html_favicon_url = "https://gear-tech.io/favicon.ico")]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

use alloc::vec::Vec;
use binary_merkle_tree::MerkleProof;
use gprimitives::{ActorId, U256};
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

/// Ethereum address used by the bridge.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Encode,
    Decode,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    TypeInfo,
    MaxEncodedLen,
)]
#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
pub struct H160(gprimitives::H160);

impl DecodeWithMemTracking for H160 {}

impl H160 {
    /// Zero address.
    pub fn zero() -> Self {
        Self(gprimitives::H160::zero())
    }

    /// Returns bytes of the address.
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl From<gprimitives::H160> for H160 {
    fn from(value: gprimitives::H160) -> Self {
        Self(value)
    }
}

impl From<H160> for gprimitives::H160 {
    fn from(value: H160) -> Self {
        value.0
    }
}

/// Ethereum/Gear bridge hash.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Encode,
    Decode,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    TypeInfo,
    MaxEncodedLen,
)]
#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
pub struct H256(gprimitives::H256);

impl DecodeWithMemTracking for H256 {}

impl H256 {
    /// Zero hash.
    pub fn zero() -> Self {
        Self(gprimitives::H256::zero())
    }

    /// Returns the hash as bytes.
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    /// Returns the underlying fixed-size byte array.
    pub fn to_fixed_bytes(self) -> [u8; 32] {
        self.0.to_fixed_bytes()
    }
}

impl AsRef<[u8]> for H256 {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl From<gprimitives::H256> for H256 {
    fn from(value: gprimitives::H256) -> Self {
        Self(value)
    }
}

impl From<H256> for gprimitives::H256 {
    fn from(value: H256) -> Self {
        value.0
    }
}

impl From<[u8; 32]> for H256 {
    fn from(value: [u8; 32]) -> Self {
        Self(value.into())
    }
}

/// Type representing merkle proof of message's inclusion into bridging queue.
#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq, PartialOrd, Ord, TypeInfo)]
#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
pub struct Proof {
    /// Merkle root of the tree this proof associated with.
    pub root: H256,
    /// Proof itself: collection of hashes required for verification.
    pub proof: Vec<H256>,
    /// Number of leaves in the tree.
    pub number_of_leaves: u64,
    /// Leaf index we're proving inclusion.
    pub leaf_index: u64,
    /// Leaf value for inclusion proving.
    pub leaf: H256,
}

impl DecodeWithMemTracking for Proof {}

impl From<MerkleProof<gprimitives::H256, gprimitives::H256>> for Proof {
    fn from(value: MerkleProof<gprimitives::H256, gprimitives::H256>) -> Self {
        Self {
            root: value.root.into(),
            proof: value.proof.into_iter().map(Into::into).collect(),
            number_of_leaves: value.number_of_leaves as u64,
            leaf_index: value.leaf_index as u64,
            leaf: value.leaf.into(),
        }
    }
}

/// Type representing message being bridged from gear to eth.
#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq, PartialOrd, Ord, TypeInfo)]
pub struct EthMessage {
    nonce: U256,
    source: H256,
    destination: H160,
    payload: Vec<u8>,
}

impl DecodeWithMemTracking for EthMessage {}

impl EthMessage {
    /// Creates a new [`EthMessage`] with unchecked parameters.
    ///
    /// # Safety
    ///
    /// `nonce` must be unique for each message, `source` must be valid ActorId,
    /// `destination` must be valid Ethereum address, and `payload` must not exceed
    /// the maximum allowed size for message.
    pub unsafe fn new_unchecked(
        nonce: U256,
        source: ActorId,
        destination: H160,
        payload: Vec<u8>,
    ) -> Self {
        Self {
            nonce,
            source: source.into_bytes().into(),
            destination,
            payload,
        }
    }

    /// Message's nonce getter.
    pub fn nonce(&self) -> U256 {
        self.nonce
    }

    /// Message's source getter.
    pub fn source(&self) -> H256 {
        self.source
    }

    /// Message's destination getter.
    pub fn destination(&self) -> H160 {
        self.destination
    }

    /// Message's payload bytes getter.
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}
