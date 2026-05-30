// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{
    Address, ToDigest,
    consensus::{BatchCommitmentValidationReply, BatchCommitmentValidationRequest},
    ecdsa::{SignedData, VerifiedData},
};
use core::hash::Hash;
use parity_scale_codec::{Decode, Encode};
use sha3::Keccak256;

/// A validator network message carrying a [`BatchCommitmentValidationRequest`] payload.
pub type ValidatorRequest = ValidatorMessage<BatchCommitmentValidationRequest>;
/// A validator network message carrying a [`BatchCommitmentValidationReply`] payload.
pub type ValidatorReply = ValidatorMessage<BatchCommitmentValidationReply>;

/// A typed validator network message scoped to a consensus era.
///
/// Wraps an arbitrary payload `T` together with the era index so that
/// receivers can reject stale messages from a previous era.
#[derive(Debug, Clone, Encode, Decode, Eq, PartialEq, Hash)]
pub struct ValidatorMessage<T> {
    /// The consensus era this message belongs to.
    pub era_index: u64,
    /// The inner message payload.
    pub payload: T,
}

impl<T: ToDigest> ToDigest for ValidatorMessage<T> {
    fn update_hasher(&self, hasher: &mut Keccak256) {
        let Self { era_index, payload } = self;
        era_index.to_be_bytes().update_hasher(hasher);
        payload.update_hasher(hasher);
    }
}

/// A signed, unverified validator message exchanged over the P2P network.
///
/// Variants correspond to the two validator message kinds. Signatures must be
/// verified via [`SignedValidatorMessage::into_verified`] before acting on the
/// contained data.
#[derive(Debug, Clone, Encode, Decode, Eq, PartialEq, derive_more::Unwrap, derive_more::From)]
pub enum SignedValidatorMessage {
    /// A validator's request for peers to validate a batch commitment.
    RequestBatchValidation(SignedData<ValidatorRequest>),
    /// A validator's approval reply for a batch commitment validation request.
    ApproveBatch(SignedData<ValidatorReply>),
}

impl SignedValidatorMessage {
    /// Converts this signed message into a [`VerifiedValidatorMessage`], asserting that the ECDSA
    /// signature was already verified at construction time; this call performs no cryptographic work.
    pub fn into_verified(self) -> VerifiedValidatorMessage {
        match self {
            SignedValidatorMessage::RequestBatchValidation(request) => {
                request.into_verified().into()
            }
            SignedValidatorMessage::ApproveBatch(reply) => reply.into_verified().into(),
        }
    }
}

/// A validator message whose ECDSA signature has been verified.
///
/// Callers can trust that [`VerifiedValidatorMessage::address`] returns the
/// actual signer address and that the payload has not been tampered with.
#[cfg_attr(feature = "serde", derive(Hash))]
#[derive(Debug, Clone, Eq, PartialEq, derive_more::Unwrap, derive_more::From)]
pub enum VerifiedValidatorMessage {
    /// A verified batch-validation request from a coordinator.
    RequestBatchValidation(VerifiedData<ValidatorRequest>),
    /// A verified batch-approval reply from a subordinate validator.
    ApproveBatch(VerifiedData<ValidatorReply>),
}

impl VerifiedValidatorMessage {
    /// Returns the consensus era index carried by this message.
    pub fn era_index(&self) -> u64 {
        match self {
            VerifiedValidatorMessage::RequestBatchValidation(request) => request.data().era_index,
            VerifiedValidatorMessage::ApproveBatch(reply) => reply.data().era_index,
        }
    }

    /// Returns the Ethereum address of the validator who signed this message.
    pub fn address(&self) -> Address {
        match self {
            VerifiedValidatorMessage::RequestBatchValidation(request) => request.address(),
            VerifiedValidatorMessage::ApproveBatch(reply) => reply.address(),
        }
    }
}
