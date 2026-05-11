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
    Address, ToDigest,
    consensus::{BatchCommitmentValidationReply, BatchCommitmentValidationRequest},
    ecdsa::{SignedData, VerifiedData},
};
use core::hash::Hash;
use parity_scale_codec::{Decode, Encode};
use sha3::Keccak256;

pub type ValidatorRequest = ValidatorMessage<BatchCommitmentValidationRequest>;
pub type ValidatorReply = ValidatorMessage<BatchCommitmentValidationReply>;

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
    RequestBatchValidation(SignedData<ValidatorRequest>),
    ApproveBatch(SignedData<ValidatorReply>),
}

impl SignedValidatorMessage {
    pub fn into_verified(self) -> VerifiedValidatorMessage {
        match self {
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
    RequestBatchValidation(VerifiedData<ValidatorRequest>),
    ApproveBatch(VerifiedData<ValidatorReply>),
}

impl VerifiedValidatorMessage {
    pub fn era_index(&self) -> u64 {
        match self {
            VerifiedValidatorMessage::RequestBatchValidation(request) => request.data().era_index,
            VerifiedValidatorMessage::ApproveBatch(reply) => reply.data().era_index,
        }
    }

    pub fn address(&self) -> Address {
        match self {
            VerifiedValidatorMessage::RequestBatchValidation(request) => request.address(),
            VerifiedValidatorMessage::ApproveBatch(reply) => reply.address(),
        }
    }
}
