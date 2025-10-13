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
    Announce, ToDigest,
    consensus::{
        BatchCommitmentValidationReply, BatchCommitmentValidationRequest, VerifiedAnnounce,
        VerifiedValidationReply, VerifiedValidationRequest,
    },
    ecdsa::SignedData,
};
use gprimitives::H256;
use k256::sha2::Digest;
use parity_scale_codec::{Decode, Encode};
use sha3::Keccak256;

pub type SignedValidatorMessage = SignedData<ValidatorMessage>;

#[derive(Debug, Clone, Encode, Decode, Eq, PartialEq)]
pub struct ValidatorMessage {
    pub block: H256,
    pub payload: ValidatorMessagePayload,
}

impl ToDigest for ValidatorMessage {
    fn update_hasher(&self, hasher: &mut Keccak256) {
        let Self { block, payload } = self;
        hasher.update(block.0);
        payload.update_hasher(hasher);
    }
}

#[derive(Debug, Clone, Encode, Decode, Eq, PartialEq, derive_more::Unwrap)]
pub enum ValidatorMessagePayload {
    ProducerBlock(Announce),
    RequestBatchValidation(BatchCommitmentValidationRequest),
    ApproveBatch(BatchCommitmentValidationReply),
}

impl ToDigest for ValidatorMessagePayload {
    fn update_hasher(&self, hasher: &mut Keccak256) {
        match self {
            ValidatorMessagePayload::ProducerBlock(payload) => payload.update_hasher(hasher),
            ValidatorMessagePayload::RequestBatchValidation(request) => {
                request.update_hasher(hasher)
            }
            ValidatorMessagePayload::ApproveBatch(reply) => reply.update_hasher(hasher),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, derive_more::Unwrap)]
pub enum VerifiedValidatorMessage {
    ProducerBlock(VerifiedAnnounce),
    RequestBatchValidation(VerifiedValidationRequest),
    ApproveBatch(VerifiedValidationReply),
}
