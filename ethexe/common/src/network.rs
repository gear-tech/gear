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
    Address, Announce, ToDigest,
    consensus::{
        BatchCommitmentValidationReply, BatchCommitmentValidationRequest, SignedAnnounce,
        SignedValidationReply, SignedValidationRequest, VerifiedAnnounce, VerifiedReply,
        VerifiedRequest,
    },
    ecdsa::SignResult,
};
use parity_scale_codec::{Decode, Encode};
use sha3::Keccak256;

#[derive(Debug, Clone, Encode, Decode, derive_more::From, Eq, PartialEq, derive_more::Unwrap)]
pub enum ValidatorMessage {
    ProducerBlock(Announce),
    RequestBatchValidation(BatchCommitmentValidationRequest),
    ApproveBatch(BatchCommitmentValidationReply),
}

impl ToDigest for ValidatorMessage {
    fn update_hasher(&self, hasher: &mut Keccak256) {
        match self {
            ValidatorMessage::ProducerBlock(announce) => {
                announce.update_hasher(hasher);
            }
            ValidatorMessage::RequestBatchValidation(request) => {
                request.update_hasher(hasher);
            }
            ValidatorMessage::ApproveBatch(reply) => {
                reply.update_hasher(hasher);
            }
        }
    }
}

#[derive(Debug, Eq, Encode, Decode, PartialEq, Clone, derive_more::From)]
pub enum SignedValidatorMessage {
    ProducerBlock(SignedAnnounce),
    RequestBatchValidation(SignedValidationRequest),
    ApproveBatch(SignedValidationReply),
}

impl SignedValidatorMessage {
    pub fn verified(self) -> SignResult<VerifiedValidatorMessage> {
        match self {
            SignedValidatorMessage::ProducerBlock(announce) => announce
                .verified()
                .map(VerifiedValidatorMessage::ProducerBlock),
            SignedValidatorMessage::RequestBatchValidation(request) => request
                .verified()
                .map(VerifiedValidatorMessage::RequestBatchValidation),
            SignedValidatorMessage::ApproveBatch(reply) => {
                reply.verified().map(VerifiedValidatorMessage::ApproveBatch)
            }
        }
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum VerifiedValidatorMessage {
    ProducerBlock(VerifiedAnnounce),
    RequestBatchValidation(VerifiedRequest),
    ApproveBatch(VerifiedReply),
}

impl VerifiedValidatorMessage {
    pub fn address(&self) -> Address {
        match self {
            VerifiedValidatorMessage::ProducerBlock(announce) => announce.address(),
            VerifiedValidatorMessage::RequestBatchValidation(request) => request.address(),
            VerifiedValidatorMessage::ApproveBatch(reply) => reply.address(),
        }
    }
}
