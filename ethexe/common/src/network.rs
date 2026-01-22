// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

use crate::{
    Address, Announce, HashOf, ToDigest,
    consensus::{BatchCommitmentValidationReply, BatchCommitmentValidationRequest},
    crypto::{
        dkg::{DkgComplaint, DkgJustification, DkgRound1, DkgRound2, DkgRound2Culprits},
        frost::{
            SignAggregate, SignCulprits, SignNonceCommit, SignNoncePackage, SignSessionRequest,
            SignShare,
        },
    },
    ecdsa::{SignedData, VerifiedData},
};
use alloc::vec::Vec;
use core::{hash::Hash, num::NonZeroU32};
use parity_scale_codec::{Decode, Encode};
use sha3::Keccak256;

pub type ValidatorAnnounce = ValidatorMessage<Announce>;
pub type ValidatorRequest = ValidatorMessage<BatchCommitmentValidationRequest>;
pub type ValidatorReply = ValidatorMessage<BatchCommitmentValidationReply>;

// DKG message types
pub type ValidatorDkgRound1 = ValidatorMessage<DkgRound1>;
pub type ValidatorDkgRound2 = ValidatorMessage<DkgRound2>;
pub type ValidatorDkgRound2Culprits = ValidatorMessage<DkgRound2Culprits>;
pub type ValidatorDkgComplaint = ValidatorMessage<DkgComplaint>;
pub type ValidatorDkgJustification = ValidatorMessage<DkgJustification>;

// ROAST/FROST message types
pub type ValidatorSignRequest = ValidatorMessage<SignSessionRequest>;
pub type ValidatorSignNonce = ValidatorMessage<SignNonceCommit>;
pub type ValidatorSignNoncePackage = ValidatorMessage<SignNoncePackage>;
pub type ValidatorSignPartial = ValidatorMessage<SignShare>;
pub type ValidatorSignCulprits = ValidatorMessage<SignCulprits>;
pub type ValidatorSignResult = ValidatorMessage<SignAggregate>;

#[derive(Debug, Clone, Encode, Decode, Eq, PartialEq, Hash)]
pub struct ValidatorMessage<T> {
    pub era_index: u64,
    pub payload: T,
}

impl<T: ToDigest> ToDigest for ValidatorMessage<T> {
    fn update_hasher(&self, hasher: &mut Keccak256) {
        let Self { era_index, payload } = self;
        era_index.to_be_bytes().update_hasher(hasher);
        payload.update_hasher(hasher);
    }
}

#[derive(Debug, Clone, Encode, Decode, Eq, PartialEq, derive_more::Unwrap, derive_more::From)]
pub enum SignedValidatorMessage {
    // Existing consensus messages
    Announce(SignedData<ValidatorAnnounce>),
    RequestBatchValidation(SignedData<ValidatorRequest>),
    ApproveBatch(SignedData<ValidatorReply>),

    // DKG protocol messages
    DkgRound1(SignedData<ValidatorDkgRound1>),
    DkgRound2(SignedData<ValidatorDkgRound2>),
    DkgRound2Culprits(SignedData<ValidatorDkgRound2Culprits>),
    DkgComplaint(SignedData<ValidatorDkgComplaint>),
    DkgJustification(SignedData<ValidatorDkgJustification>),

    // ROAST/FROST signing messages
    SignSessionRequest(SignedData<ValidatorSignRequest>),
    SignNonceCommit(SignedData<ValidatorSignNonce>),
    SignNoncePackage(SignedData<ValidatorSignNoncePackage>),
    SignShare(SignedData<ValidatorSignPartial>),
    SignCulprits(SignedData<ValidatorSignCulprits>),
    SignAggregate(SignedData<ValidatorSignResult>),
}

impl SignedValidatorMessage {
    pub fn into_verified(self) -> VerifiedValidatorMessage {
        match self {
            SignedValidatorMessage::Announce(announce) => announce.into_verified().into(),
            SignedValidatorMessage::RequestBatchValidation(request) => {
                request.into_verified().into()
            }
            SignedValidatorMessage::ApproveBatch(reply) => reply.into_verified().into(),

            // DKG messages
            SignedValidatorMessage::DkgRound1(msg) => msg.into_verified().into(),
            SignedValidatorMessage::DkgRound2(msg) => msg.into_verified().into(),
            SignedValidatorMessage::DkgRound2Culprits(msg) => msg.into_verified().into(),
            SignedValidatorMessage::DkgComplaint(msg) => msg.into_verified().into(),
            SignedValidatorMessage::DkgJustification(msg) => msg.into_verified().into(),

            // ROAST messages
            SignedValidatorMessage::SignSessionRequest(msg) => msg.into_verified().into(),
            SignedValidatorMessage::SignNonceCommit(msg) => msg.into_verified().into(),
            SignedValidatorMessage::SignNoncePackage(msg) => msg.into_verified().into(),
            SignedValidatorMessage::SignShare(msg) => msg.into_verified().into(),
            SignedValidatorMessage::SignCulprits(msg) => msg.into_verified().into(),
            SignedValidatorMessage::SignAggregate(msg) => msg.into_verified().into(),
        }
    }
}

#[cfg_attr(feature = "serde", derive(Hash))]
#[derive(Debug, Clone, Eq, PartialEq, derive_more::Unwrap, derive_more::From)]
pub enum VerifiedValidatorMessage {
    // Existing consensus messages
    Announce(VerifiedData<ValidatorAnnounce>),
    RequestBatchValidation(VerifiedData<ValidatorRequest>),
    ApproveBatch(VerifiedData<ValidatorReply>),

    // DKG protocol messages
    DkgRound1(VerifiedData<ValidatorDkgRound1>),
    DkgRound2(VerifiedData<ValidatorDkgRound2>),
    DkgRound2Culprits(VerifiedData<ValidatorDkgRound2Culprits>),
    DkgComplaint(VerifiedData<ValidatorDkgComplaint>),
    DkgJustification(VerifiedData<ValidatorDkgJustification>),

    // ROAST/FROST signing messages
    SignSessionRequest(VerifiedData<ValidatorSignRequest>),
    SignNonceCommit(VerifiedData<ValidatorSignNonce>),
    SignNoncePackage(VerifiedData<ValidatorSignNoncePackage>),
    SignShare(VerifiedData<ValidatorSignPartial>),
    SignCulprits(VerifiedData<ValidatorSignCulprits>),
    SignAggregate(VerifiedData<ValidatorSignResult>),
}

impl VerifiedValidatorMessage {
    pub fn era_index(&self) -> u64 {
        match self {
            VerifiedValidatorMessage::Announce(announce) => announce.data().era_index,
            VerifiedValidatorMessage::RequestBatchValidation(request) => request.data().era_index,
            VerifiedValidatorMessage::ApproveBatch(reply) => reply.data().era_index,

            // DKG messages
            VerifiedValidatorMessage::DkgRound1(msg) => msg.data().era_index,
            VerifiedValidatorMessage::DkgRound2(msg) => msg.data().era_index,
            VerifiedValidatorMessage::DkgRound2Culprits(msg) => msg.data().era_index,
            VerifiedValidatorMessage::DkgComplaint(msg) => msg.data().era_index,
            VerifiedValidatorMessage::DkgJustification(msg) => msg.data().era_index,

            // ROAST messages
            VerifiedValidatorMessage::SignSessionRequest(msg) => msg.data().era_index,
            VerifiedValidatorMessage::SignNonceCommit(msg) => msg.data().era_index,
            VerifiedValidatorMessage::SignNoncePackage(msg) => msg.data().era_index,
            VerifiedValidatorMessage::SignShare(msg) => msg.data().era_index,
            VerifiedValidatorMessage::SignCulprits(msg) => msg.data().era_index,
            VerifiedValidatorMessage::SignAggregate(msg) => msg.data().era_index,
        }
    }

    pub fn address(&self) -> Address {
        match self {
            VerifiedValidatorMessage::Announce(announce) => announce.address(),
            VerifiedValidatorMessage::RequestBatchValidation(request) => request.address(),
            VerifiedValidatorMessage::ApproveBatch(reply) => reply.address(),

            // DKG messages
            VerifiedValidatorMessage::DkgRound1(msg) => msg.address(),
            VerifiedValidatorMessage::DkgRound2(msg) => msg.address(),
            VerifiedValidatorMessage::DkgRound2Culprits(msg) => msg.address(),
            VerifiedValidatorMessage::DkgComplaint(msg) => msg.address(),
            VerifiedValidatorMessage::DkgJustification(msg) => msg.address(),

            // ROAST messages
            VerifiedValidatorMessage::SignSessionRequest(msg) => msg.address(),
            VerifiedValidatorMessage::SignNonceCommit(msg) => msg.address(),
            VerifiedValidatorMessage::SignNoncePackage(msg) => msg.address(),
            VerifiedValidatorMessage::SignShare(msg) => msg.address(),
            VerifiedValidatorMessage::SignCulprits(msg) => msg.address(),
            VerifiedValidatorMessage::SignAggregate(msg) => msg.address(),
        }
    }
}

/// Until condition for announces request (see [`AnnouncesRequest`]).
#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy, Encode, Decode, derive_more::From)]
pub enum AnnouncesRequestUntil {
    /// Request until a specific tail announce hash
    Tail(HashOf<Announce>),
    /// Request until a specific chain length
    ChainLen(NonZeroU32),
}

/// Request announces body (see [`Announce`]) chain from `head_announce_hash`,
/// to announce defined by `until` condition.
/// If `until` is `Tail`, then tail must not be included in the response.
#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy, Encode, Decode)]
pub struct AnnouncesRequest {
    /// Hash of the requested chain head announce
    pub head: HashOf<Announce>,
    /// Request until this condition is met
    pub until: AnnouncesRequestUntil,
}

/// Checked announces response ensuring that it matches the corresponding request.
#[derive(derive_more::Debug, Clone, Eq, PartialEq, derive_more::From)]
pub struct AnnouncesResponse {
    /// Corresponding request for this response
    request: AnnouncesRequest,
    /// List of announces
    announces: Vec<Announce>,
}

impl AnnouncesResponse {
    /// # Safety
    ///
    /// Response must be only created by network service
    pub unsafe fn from_parts(request: AnnouncesRequest, announces: Vec<Announce>) -> Self {
        Self { request, announces }
    }

    pub fn request(&self) -> &AnnouncesRequest {
        &self.request
    }

    pub fn announces(&self) -> &[Announce] {
        &self.announces
    }

    pub fn into_parts(self) -> (AnnouncesRequest, Vec<Announce>) {
        (self.request, self.announces)
    }
}
