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
    ecdsa::{SignedData, VerifiedData},
    injected::Promise,
};
use alloc::vec::Vec;
use core::{hash::Hash, num::NonZeroU32};
use parity_scale_codec::{Decode, Encode};
use sha3::Keccak256;

pub type ValidatorAnnounce = ValidatorMessage<Announce>;
pub type ValidatorRequest = ValidatorMessage<BatchCommitmentValidationRequest>;
pub type ValidatorReply = ValidatorMessage<BatchCommitmentValidationReply>;
pub type ValidatorPromise = ValidatorMessage<Promise>;

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
    Announce(SignedData<ValidatorAnnounce>),
    RequestBatchValidation(SignedData<ValidatorRequest>),
    ApproveBatch(SignedData<ValidatorReply>),
}

impl SignedValidatorMessage {
    pub fn into_verified(self) -> VerifiedValidatorMessage {
        match self {
            SignedValidatorMessage::Announce(announce) => announce.into_verified().into(),
            SignedValidatorMessage::RequestBatchValidation(request) => {
                request.into_verified().into()
            }
            SignedValidatorMessage::ApproveBatch(reply) => reply.into_verified().into(),
        }
    }
}

#[cfg_attr(feature = "serde", derive(Hash))]
#[derive(Debug, Clone, Eq, PartialEq, derive_more::Unwrap, derive_more::From)]
pub enum VerifiedValidatorMessage {
    Announce(VerifiedData<ValidatorAnnounce>),
    RequestBatchValidation(VerifiedData<ValidatorRequest>),
    ApproveBatch(VerifiedData<ValidatorReply>),
}

impl VerifiedValidatorMessage {
    pub fn era_index(&self) -> u64 {
        match self {
            VerifiedValidatorMessage::Announce(announce) => announce.data().era_index,
            VerifiedValidatorMessage::RequestBatchValidation(request) => request.data().era_index,
            VerifiedValidatorMessage::ApproveBatch(reply) => reply.data().era_index,
        }
    }

    pub fn address(&self) -> Address {
        match self {
            VerifiedValidatorMessage::Announce(announce) => announce.address(),
            VerifiedValidatorMessage::RequestBatchValidation(request) => request.address(),
            VerifiedValidatorMessage::ApproveBatch(reply) => reply.address(),
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
